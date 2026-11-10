//! Table definitions (TDEF pages): a table's columns, indexes and usage maps.
//!
//! Offsets are Jet 4's, from mdbtools `HACKING.md` ("Table Definition") and Jackcess
//! `JetFormat.Jet4Format`. A definition longer than a page continues on the page named at
//! offset 4; each continuation page carries 8 bytes of header before its share of the data.

use super::{JetError, PAGE, Pages, Result, RowId, format_error, page_type, u16le, u32le, usage};

const OFFSET_NUM_ROWS: usize = 16;
const OFFSET_MAX_COLS: usize = 41;
const OFFSET_NUM_VAR_COLS: usize = 43;
const OFFSET_NUM_COLS: usize = 45;
const OFFSET_NUM_INDEXES: usize = 47;
const OFFSET_NUM_REAL_INDEXES: usize = 51;
const OFFSET_OWNED_PAGES: usize = 55;
const OFFSET_FREE_PAGES: usize = 59;
const OFFSET_INDEX_COUNTS: usize = 63;
const SIZE_INDEX_COUNTS: usize = 12;
const SIZE_COLUMN: usize = 25;
const SIZE_REAL_INDEX: usize = 52;
const SIZE_LOGICAL_INDEX: usize = 28;
const INDEX_COLUMNS: usize = 10;

/// A column's storage type (mdbtools `HACKING.md`, "Column Types").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColType {
    Bool,
    Byte,
    Int,
    Long,
    Float,
    Double,
    Text,
    Memo,
    /// Date/time: a `Double` of days since 1899-12-30.
    Date,
    /// Binary, up to 255 bytes, in the row.
    Binary,
    /// OLE object: bytes stored as a memo is.
    Ole,
    /// Anything else (currency, GUID, ...): kept by the reader as nothing, refused by the
    /// writer.
    Other(u8),
}

impl ColType {
    fn from_code(code: u8) -> ColType {
        match code {
            0x01 => ColType::Bool,
            0x02 => ColType::Byte,
            0x03 => ColType::Int,
            0x04 => ColType::Long,
            0x06 => ColType::Float,
            0x07 => ColType::Double,
            0x08 => ColType::Date,
            0x09 => ColType::Binary,
            0x0a => ColType::Text,
            0x0b => ColType::Ole,
            0x0c => ColType::Memo,
            other => ColType::Other(other),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Column {
    pub name: String,
    pub kind: ColType,
    /// The column's number: its bit in a row's null mask.
    pub number: u16,
    /// Fixed-length columns live at `fixed_offset` (after the row's column count); the others
    /// are numbered by `var_index` in the row's variable-length table.
    pub fixed: bool,
    pub fixed_offset: u16,
    pub var_index: u16,
    /// Bytes: 4 for a long, twice the characters for text, 0 for memo.
    pub length: u16,
    /// Text may be stored as "compressed unicode" (one byte per character).
    pub compressed: bool,
}

/// A real index: the B-tree one or more logical indexes share.
#[derive(Debug, Clone)]
pub(crate) struct Index {
    /// Logical name, for messages (the first logical index on this real one).
    pub name: String,
    /// Column numbers and whether each sorts ascending.
    pub columns: Vec<(u16, bool)>,
    /// The usage map listing the index's pages.
    pub usage_map: u32,
    /// Where in the definition its root page number is.
    first_page_at: usize,
}

/// The usage maps of one memo column's LVAL pages.
#[derive(Debug, Clone)]
pub(crate) struct LongValueMaps {
    pub column: u16,
    pub owned: u32,
    pub free: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct TableDef {
    pub name: String,
    pub page: u32,
    pub max_cols: u16,
    pub columns: Vec<Column>,
    pub indexes: Vec<Index>,
    pub owned_map: u32,
    pub free_map: u32,
    pub long_value_maps: Vec<LongValueMaps>,
}

impl TableDef {
    /// Reads the definition on `page` (and its continuation pages).
    pub fn read(pages: &Pages, page: u32, name: &str) -> Result<TableDef> {
        let bytes = definition_bytes(pages, page)?;
        let t = &bytes.data;
        let need = |at: usize, len: usize| -> Result<()> {
            if at + len > t.len() {
                Err(format_error(format!(
                    "table {name}: its definition ends early"
                )))
            } else {
                Ok(())
            }
        };
        need(0, OFFSET_INDEX_COUNTS)?;
        let max_cols = u16le(t, OFFSET_MAX_COLS);
        let num_var = u16le(t, OFFSET_NUM_VAR_COLS);
        let num_cols = u16le(t, OFFSET_NUM_COLS) as usize;
        let num_idx = u32le(t, OFFSET_NUM_INDEXES) as usize;
        let num_real = u32le(t, OFFSET_NUM_REAL_INDEXES) as usize;
        let owned_map = u32le(t, OFFSET_OWNED_PAGES);
        let free_map = u32le(t, OFFSET_FREE_PAGES);

        let mut at = OFFSET_INDEX_COUNTS + num_real * SIZE_INDEX_COUNTS;
        need(at, num_cols * SIZE_COLUMN)?;
        let mut columns = Vec::with_capacity(num_cols);
        for _ in 0..num_cols {
            let c = &t[at..at + SIZE_COLUMN];
            let flags = c[15];
            columns.push(Column {
                name: String::new(),
                kind: ColType::from_code(c[0]),
                number: u16le(c, 5),
                fixed: flags & 0x01 != 0,
                fixed_offset: u16le(c, 21),
                var_index: u16le(c, 7),
                length: u16le(c, 23),
                compressed: c[16] & 0x01 != 0,
            });
            at += SIZE_COLUMN;
        }
        for column in &mut columns {
            column.name = read_name(t, &mut at, name)?;
        }
        if columns.iter().filter(|c| !c.fixed).count() != num_var as usize {
            return Err(format_error(format!(
                "table {name}: {num_var} variable columns declared, the columns say otherwise"
            )));
        }

        need(at, num_real * SIZE_REAL_INDEX)?;
        let mut indexes = Vec::with_capacity(num_real);
        for _ in 0..num_real {
            let r = &t[at..at + SIZE_REAL_INDEX];
            let mut cols = Vec::new();
            for k in 0..INDEX_COLUMNS {
                let number = u16le(r, 4 + 3 * k);
                if number != 0xffff {
                    cols.push((number, r[6 + 3 * k] == 0x01));
                }
            }
            indexes.push(Index {
                name: String::new(),
                columns: cols,
                usage_map: u32le(r, 34),
                first_page_at: at + 38,
            });
            at += SIZE_REAL_INDEX;
        }
        need(at, num_idx * SIZE_LOGICAL_INDEX)?;
        let mut logical_real = Vec::with_capacity(num_idx);
        for _ in 0..num_idx {
            logical_real.push(u32le(t, at + 8) as usize);
            at += SIZE_LOGICAL_INDEX;
        }
        for real in logical_real {
            let index_name = read_name(t, &mut at, name)?;
            if let Some(index) = indexes.get_mut(real)
                && index.name.is_empty()
            {
                index.name = index_name;
            }
        }

        let mut long_value_maps = Vec::new();
        while at + 2 <= t.len() {
            let column = u16le(t, at);
            if column == 0xffff {
                break;
            }
            need(at, 10)?;
            long_value_maps.push(LongValueMaps {
                column,
                owned: u32le(t, at + 2),
                free: u32le(t, at + 6),
            });
            at += 10;
        }

        Ok(TableDef {
            name: name.to_owned(),
            page,
            max_cols,
            columns,
            indexes,
            owned_map,
            free_map,
            long_value_maps,
        })
    }

    pub fn column(&self, number: u16) -> Option<&Column> {
        self.columns.iter().find(|c| c.number == number)
    }

    pub fn unsupported(&self, reason: impl Into<String>) -> JetError {
        JetError::Unsupported {
            table: self.name.clone(),
            reason: reason.into(),
        }
    }

    /// Every page the table's rows, indexes and long values occupy (and the map pages of any
    /// of its usage maps that are kept on pages of their own).
    pub fn owned_pages(&self, pages: &Pages) -> Result<Vec<u32>> {
        let mut maps = vec![self.owned_map];
        maps.extend(self.indexes.iter().map(|i| i.usage_map));
        maps.extend(self.long_value_maps.iter().map(|m| m.owned));
        let mut all = Vec::new();
        for pointer in maps {
            let map = usage::read(pages, RowId::from_pointer(pointer))?;
            all.extend(map.pages);
            all.extend(map.map_pages);
        }
        for map in [self.free_map]
            .into_iter()
            .chain(self.long_value_maps.iter().map(|m| m.free))
        {
            all.extend(usage::read(pages, RowId::from_pointer(map))?.map_pages);
        }
        // a table's pages are its own: refuse to free anything else (a definition page, say)
        for &p in &all {
            let kind = pages.page(p)?[0];
            if !matches!(
                kind,
                page_type::DATA
                    | page_type::INDEX_NODE
                    | page_type::INDEX_LEAF
                    | page_type::USAGE_MAP
            ) {
                return Err(format_error(format!(
                    "table {}: page {p}, listed as its own, is of type {kind}",
                    self.name
                )));
            }
        }
        all.sort_unstable();
        all.dedup();
        Ok(all)
    }

    /// The definition's bytes, to change and write back.
    pub fn patch(&self, pages: &Pages) -> Result<Patch> {
        Ok(Patch {
            def: definition_bytes(pages, self.page)?,
            index_first: self.indexes.iter().map(|i| i.first_page_at).collect(),
        })
    }
}

/// A table definition being changed in place.
pub(crate) struct Patch {
    def: Definition,
    index_first: Vec<usize>,
}

impl Patch {
    pub fn set_rows(&mut self, rows: u32) {
        self.def.data[OFFSET_NUM_ROWS..OFFSET_NUM_ROWS + 4].copy_from_slice(&rows.to_le_bytes());
    }

    /// Sets a real index's root page and its count of distinct keys (Jackcess
    /// `IndexData.create`: the second word of the index's 12-byte count block, the first being
    /// the same count again in the databases NMR writes).
    pub fn set_index(&mut self, i: usize, root: u32, unique: u32) {
        let counts = OFFSET_INDEX_COUNTS + i * SIZE_INDEX_COUNTS;
        // the first word is the count again in user tables, and 0 in the system catalog's:
        // keep a 0
        if u32le(&self.def.data, counts) != 0 {
            self.def.data[counts..counts + 4].copy_from_slice(&unique.to_le_bytes());
        }
        self.def.data[counts + 4..counts + 8].copy_from_slice(&unique.to_le_bytes());
        let at = self.index_first[i];
        self.def.data[at..at + 4].copy_from_slice(&root.to_le_bytes());
    }

    pub fn write(&self, pages: &mut Pages) -> Result<()> {
        for (i, &page) in self.def.pages.iter().enumerate() {
            let (from, to) = self.def.span(i);
            let at = if i == 0 { 0 } else { 8 };
            let dest = pages.page_mut(page);
            dest[at..at + (to - from)].copy_from_slice(&self.def.data[from..to]);
        }
        Ok(())
    }
}

/// A definition's bytes laid end to end: the first page whole, then each continuation page
/// after its 8-byte header.
struct Definition {
    data: Vec<u8>,
    pages: Vec<u32>,
}

impl Definition {
    /// The range of `data` page `i` holds.
    fn span(&self, i: usize) -> (usize, usize) {
        if i == 0 {
            (0, PAGE)
        } else {
            let from = PAGE + (i - 1) * (PAGE - 8);
            (from, from + PAGE - 8)
        }
    }
}

fn definition_bytes(pages: &Pages, first: u32) -> Result<Definition> {
    let mut data = Vec::new();
    let mut list = Vec::new();
    let mut page = first;
    while page != 0 {
        if list.contains(&page) || list.len() > 64 {
            return Err(format_error(format!(
                "the definition at page {first} loops"
            )));
        }
        let p = pages.page(page)?;
        if p[0] != page_type::TABLE_DEF {
            return Err(format_error(format!(
                "page {page} is not a table definition"
            )));
        }
        data.extend_from_slice(if list.is_empty() { p } else { &p[8..] });
        list.push(page);
        page = u32le(p, 4);
    }
    Ok(Definition { data, pages: list })
}

fn read_name(t: &[u8], at: &mut usize, table: &str) -> Result<String> {
    if *at + 2 > t.len() {
        return Err(format_error(format!(
            "table {table}: its definition ends early"
        )));
    }
    let len = u16le(t, *at) as usize;
    *at += 2;
    let raw = t
        .get(*at..*at + len)
        .ok_or_else(|| format_error(format!("table {table}: a name runs past the definition")))?;
    *at += len;
    let units: Vec<u16> = raw
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    Ok(String::from_utf16_lossy(&units))
}
