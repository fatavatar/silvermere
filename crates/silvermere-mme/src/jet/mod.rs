//! Jet 4 (Access 2000–2003) databases: just enough to replace the rows of a template's tables.
//!
//! MME opens its database through DAO and looks records up with `Seek` on each table's index,
//! so a database we hand it must be one Jet itself would accept: rows on data pages listed in
//! the table's usage map, long text on LVAL pages, and every index a valid B-tree. Writing a
//! database from nothing would mean writing the system catalog as well, so we don't: we take a
//! database NMR made (its catalog, table definitions and indexes are already right), drop every
//! row of the tables we fill, and write ours in their place.
//!
//! The format follows the documentation of mdbtools (`HACKING.md`) and Jackcess's reader and
//! writer; each module names the parts it implements. Only the column types MME's tables use
//! are supported (numbers, text and memo), and only numeric index keys.

mod create;
mod index;
mod read;
mod rows;
mod tdef;
mod usage;

use std::collections::BTreeMap;

use crate::table::Row;

pub use create::add_table_like;
pub use read::read_table;

/// The size of a Jet 4 page.
pub(crate) const PAGE: usize = 4096;

/// Page types (the first byte of a page).
pub(crate) mod page_type {
    pub const DATA: u8 = 0x01;
    pub const TABLE_DEF: u8 = 0x02;
    pub const INDEX_NODE: u8 = 0x03;
    pub const INDEX_LEAF: u8 = 0x04;
    pub const USAGE_MAP: u8 = 0x05;
}

/// Why a database could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum JetError {
    /// The file is not a Jet 4 database, or a structure in it is not what the format says.
    #[error("not a usable Jet 4 database: {0}")]
    Format(String),
    /// The database has no table of this name.
    #[error("the database has no table {0:?}")]
    NoTable(String),
    /// The table uses something this writer does not support.
    #[error("table {table}: {reason}")]
    Unsupported { table: String, reason: String },
    /// A row does not fit its table.
    #[error("table {table}: {reason}")]
    Row { table: String, reason: String },
}

pub(crate) type Result<T> = std::result::Result<T, JetError>;

pub(crate) fn format_error(what: impl Into<String>) -> JetError {
    JetError::Format(what.into())
}

/// A row's place: its page and its number on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct RowId {
    pub page: u32,
    pub row: u8,
}

impl RowId {
    /// The 4-byte pointer a table definition uses for a row (row number in the low byte).
    pub fn from_pointer(pointer: u32) -> RowId {
        RowId {
            page: pointer >> 8,
            row: (pointer & 0xff) as u8,
        }
    }
}

/// A database held in memory, page by page.
pub(crate) struct Pages {
    pub bytes: Vec<u8>,
}

impl Pages {
    pub fn count(&self) -> u32 {
        (self.bytes.len() / PAGE) as u32
    }

    pub fn page(&self, n: u32) -> Result<&[u8]> {
        let start = n as usize * PAGE;
        self.bytes
            .get(start..start + PAGE)
            .ok_or_else(|| format_error(format!("page {n} is past the end of the file")))
    }

    pub fn page_mut(&mut self, n: u32) -> &mut [u8] {
        let start = n as usize * PAGE;
        if self.bytes.len() < start + PAGE {
            self.bytes.resize(start + PAGE, 0);
        }
        &mut self.bytes[start..start + PAGE]
    }

    /// The bytes of a row on a data page. A row runs from its offset to the start of the row
    /// before it on the page (rows are packed from the end of the page down).
    pub fn row(&self, id: RowId) -> Result<&[u8]> {
        let page = self.page(id.page)?;
        if page[0] != page_type::DATA {
            return Err(format_error(format!("page {} is not a data page", id.page)));
        }
        let rows = u16le(page, 12) as usize;
        let n = id.row as usize;
        if n >= rows {
            return Err(format_error(format!("page {} has no row {n}", id.page)));
        }
        let start = (u16le(page, 14 + 2 * n) & 0x1fff) as usize;
        let end = if n == 0 {
            PAGE
        } else {
            (u16le(page, 14 + 2 * (n - 1)) & 0x1fff) as usize
        };
        page.get(start..end)
            .ok_or_else(|| format_error(format!("row {n} of page {} is out of bounds", id.page)))
    }

    /// Overwrites a row in place; the new bytes must be exactly as long as the row.
    pub fn put_row(&mut self, id: RowId, bytes: &[u8]) -> Result<()> {
        let len = self.row(id)?.len();
        if len != bytes.len() {
            return Err(format_error(format!(
                "row {}/{} is {len} bytes, not {}",
                id.page,
                id.row,
                bytes.len()
            )));
        }
        let page = self.page(id.page)?;
        let start = (u16le(page, 14 + 2 * id.row as usize) & 0x1fff) as usize;
        self.page_mut(id.page)[start..start + len].copy_from_slice(bytes);
        Ok(())
    }
}

pub(crate) fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

pub(crate) fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

/// Hands out pages: first those freed from the tables being rewritten, lowest first, then new
/// pages at the end of the file.
pub(crate) struct Allocator {
    free: Vec<u32>,
    next_new: u32,
}

impl Allocator {
    fn new(mut free: Vec<u32>, file_pages: u32) -> Allocator {
        free.sort_unstable_by(|a, b| b.cmp(a));
        free.dedup();
        Allocator {
            free,
            next_new: file_pages,
        }
    }

    pub fn take(&mut self) -> u32 {
        self.free.pop().unwrap_or_else(|| {
            self.next_new += 1;
            self.next_new - 1
        })
    }

    /// Pages freed and not used again.
    fn leftover(self) -> Vec<u32> {
        self.free
    }
}

/// Replaces every row of the named tables in a Jet 4 database (`template`, the bytes of the
/// file) and returns the new file. Tables not named keep their rows. Each table's rows are
/// stored in the order of its first index, and every index is rebuilt.
pub fn write_tables(
    template: &[u8],
    tables: &[(&str, &[Row])],
) -> std::result::Result<Vec<u8>, JetError> {
    if template.len() < PAGE * 3 || !template.len().is_multiple_of(PAGE) {
        return Err(format_error(
            "the file is not a whole number of 4 KiB pages",
        ));
    }
    // byte 0x14 of the header is the engine version: 1 is Jet 4
    if template[0x14] != 1 {
        return Err(format_error(format!(
            "engine version {} (Jet 4 is 1)",
            template[0x14]
        )));
    }
    let mut pages = Pages {
        bytes: template.to_vec(),
    };
    let catalog = read::catalog(&pages)?;

    let mut defs = Vec::new();
    let mut freed = Vec::new();
    for (name, rows) in tables {
        let page = *catalog
            .get(*name)
            .ok_or_else(|| JetError::NoTable((*name).to_owned()))?;
        let def = tdef::TableDef::read(&pages, page, name)?;
        freed.extend(def.owned_pages(&pages)?);
        defs.push((def, *rows));
    }
    let mut alloc = Allocator::new(freed, pages.count());

    for (def, rows) in &defs {
        write_table(&mut pages, &mut alloc, def, rows)?;
    }

    // pages the old rows used and the new ones did not: blank them, so nothing stale is left
    for page in alloc.leftover() {
        pages.page_mut(page).fill(0);
    }
    Ok(pages.bytes)
}

/// Writes one table's rows, indexes and usage maps.
fn write_table(
    pages: &mut Pages,
    alloc: &mut Allocator,
    def: &tdef::TableDef,
    rows: &[Row],
) -> Result<()> {
    if !rows.is_empty() {
        for idx in &def.indexes {
            index::check(def, idx)?;
        }
    }
    let mut encoded = Vec::with_capacity(rows.len());
    for row in rows {
        let e = rows::Encoded::new(def, row)?;
        for idx in &def.indexes {
            index::check_row(def, idx, &e)?;
        }
        encoded.push(e);
    }
    // store rows in index order: MME reads some tables front to back, and `MoveLast` on the
    // monster table must land on the highest number
    if let Some(first) = def.indexes.first() {
        let mut keyed: Vec<(Vec<u8>, rows::Encoded)> = encoded
            .into_iter()
            .map(|e| (index::key(def, first, &e), e))
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        encoded = keyed.into_iter().map(|(_, e)| e).collect();
    }

    let mut written = rows::Writer::new(def);
    let mut ids = Vec::with_capacity(encoded.len());
    for e in &encoded {
        ids.push(written.add(pages, alloc, e)?);
    }
    let written = written.finish(pages);

    let mut index_pages = Vec::new();
    let mut roots = Vec::new();
    for idx in &def.indexes {
        let mut entries: Vec<Vec<u8>> = encoded
            .iter()
            .zip(&ids)
            .map(|(e, id)| {
                let mut entry = index::key(def, idx, e);
                index::push_row_id(&mut entry, *id);
                entry
            })
            .collect();
        entries.sort();
        let unique = {
            let mut keys: Vec<Vec<u8>> = encoded.iter().map(|e| index::key(def, idx, e)).collect();
            keys.dedup();
            keys.len() as u32
        };
        let built = index::build(pages, alloc, def.page, &entries)?;
        index_pages.push(built.pages);
        roots.push((built.root, unique));
    }

    let mut patch = def.patch(pages)?;
    patch.set_rows(encoded.len() as u32);
    for (i, (root, unique)) in roots.iter().enumerate() {
        patch.set_index(i, *root, *unique);
    }
    patch.write(pages)?;

    let mut maps: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    maps.insert(def.owned_map, written.data_pages.clone());
    maps.insert(def.free_map, Vec::new());
    for (idx, pages_of) in def.indexes.iter().zip(index_pages) {
        maps.insert(idx.usage_map, pages_of);
    }
    for memo in &def.long_value_maps {
        let pages_of = written
            .lval_pages
            .get(&memo.column)
            .cloned()
            .unwrap_or_default();
        maps.insert(memo.owned, pages_of);
        maps.insert(memo.free, Vec::new());
    }
    for (pointer, pages_of) in maps {
        usage::write(pages, alloc, RowId::from_pointer(pointer), &pages_of)?;
    }
    Ok(())
}
