//! Adding a table to a database: [`add_table_like`].
//!
//! MME's database has no table for anything MME does not read, and a table cannot be added to
//! an Access file without Access; so this does what Access would, as little as it can. The new
//! table's definition is a copy of an existing table's (column types, sizes and order), under
//! new column names; it gets usage maps of its own on a fresh page; and the system catalog
//! (`MSysObjects`) gets one row naming it, on a fresh page, with the catalog's indexes rebuilt
//! over all its rows. No existing catalog row, and no other table, is touched.
//!
//! The table has no index and no permissions rows (`MSysACEs`): a reader that walks the
//! catalog (mdb-reader, Jackcess, access_parser) reads it; Access itself would want both before
//! opening it in its own user interface.

use super::rows::Encoded;
use super::tdef::TableDef;
use super::{
    Allocator, JetError, PAGE, Pages, RowId, format_error, index, page_type, read, u16le, u32le,
    usage,
};
use crate::table::{Row, Value};

/// Header offsets of a table definition (see `tdef`).
const OFFSET_DEF_LEN: usize = 8;
const OFFSET_NUM_ROWS: usize = 16;
const OFFSET_NUM_COLS: usize = 45;
const OFFSET_NUM_REAL_INDEXES: usize = 51;
const OFFSET_OWNED_PAGES: usize = 55;
const OFFSET_FREE_PAGES: usize = 59;
const OFFSET_INDEX_COUNTS: usize = 63;
const SIZE_INDEX_COUNTS: usize = 12;
const SIZE_COLUMN: usize = 25;
/// The length of the usage map rows the new table gets: the length Access gives a small
/// table's.
const USAGE_MAP_ROW: usize = 69;
/// `MSysObjects.Type` of a table.
const TYPE_TABLE: i32 = 1;

/// Adds the table `name` to the database (`file`) with the column types and order of the table
/// `like`, its columns named `columns` (one name per column of `like`, in its order). The new
/// table is empty; [`super::write_tables`] fills it. `like` must have no index and no memo
/// column, and its definition must fit one page.
pub fn add_table_like(
    file: &[u8],
    name: &str,
    like: &str,
    columns: &[&str],
) -> std::result::Result<Vec<u8>, JetError> {
    let mut pages = Pages {
        bytes: file.to_vec(),
    };
    let catalog = read::catalog(&pages)?;
    if catalog.contains_key(name) {
        return Err(format_error(format!(
            "the database already has a table {name:?}"
        )));
    }
    let like_page = *catalog
        .get(like)
        .ok_or_else(|| JetError::NoTable(like.to_owned()))?;
    let like_def = TableDef::read(&pages, like_page, like)?;
    if !like_def.indexes.is_empty() || !like_def.long_value_maps.is_empty() {
        return Err(like_def.unsupported("a table to copy must have no index and no memo column"));
    }
    if columns.len() != like_def.columns.len() {
        return Err(format_error(format!(
            "{like} has {} columns; {} names were given",
            like_def.columns.len(),
            columns.len()
        )));
    }
    let source = pages.page(like_page)?.to_vec();
    if u32le(&source, 4) != 0 {
        return Err(like_def.unsupported("its definition runs over more than one page"));
    }

    let mut alloc = Allocator::new(Vec::new(), pages.count());
    let tdef_page = alloc.take();
    let maps_page = alloc.take();

    // the definition: the copy, renamed, with maps of its own and no rows
    let num_cols = usize::from(u16le(&source, OFFSET_NUM_COLS));
    let num_real = u32le(&source, OFFSET_NUM_REAL_INDEXES) as usize;
    let names_at = OFFSET_INDEX_COUNTS + num_real * SIZE_INDEX_COUNTS + num_cols * SIZE_COLUMN;
    let mut names_end = names_at;
    for _ in 0..num_cols {
        names_end += 2 + usize::from(u16le(&source, names_end));
    }
    let old_len = u32le(&source, OFFSET_DEF_LEN) as usize;
    let mut def = source[..names_at].to_vec();
    // columns by number: the names go in the definition's column order, which `like_def`
    // keeps
    for column_name in columns {
        let units: Vec<u8> = column_name
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        def.extend_from_slice(&(units.len() as u16).to_le_bytes());
        def.extend_from_slice(&units);
    }
    def.extend_from_slice(&source[names_end..old_len]);
    if def.len() + 8 > PAGE {
        return Err(format_error("the new definition does not fit a page"));
    }
    let len = def.len() as u32;
    def[OFFSET_DEF_LEN..OFFSET_DEF_LEN + 4].copy_from_slice(&len.to_le_bytes());
    def[OFFSET_NUM_ROWS..OFFSET_NUM_ROWS + 4].copy_from_slice(&0u32.to_le_bytes());
    def[OFFSET_OWNED_PAGES..OFFSET_OWNED_PAGES + 4]
        .copy_from_slice(&(maps_page << 8).to_le_bytes());
    def[OFFSET_FREE_PAGES..OFFSET_FREE_PAGES + 4]
        .copy_from_slice(&(maps_page << 8 | 1).to_le_bytes());
    let free = (PAGE - def.len() - 8) as u16;
    def[2..4].copy_from_slice(&free.to_le_bytes());
    let page = pages.page_mut(tdef_page);
    page.fill(0);
    page[..def.len()].copy_from_slice(&def);

    // its usage maps: two empty inline maps on a page of their own, as Access keeps them
    write_rows(
        &mut pages,
        maps_page,
        0,
        &[vec![0; USAGE_MAP_ROW], vec![0; USAGE_MAP_ROW]],
    );

    // its row in the catalog
    let objects = TableDef::read(&pages, read::CATALOG_PAGE, "MSysObjects")?;
    let existing = read::rows_with_ids(&pages, &objects)?;
    let field = |row: &Row, k: &str| row.get(k).cloned().unwrap_or(Value::Null);
    let tables = existing
        .iter()
        .find(|(_, r)| {
            r.get("Name") == Some(&Value::Text("Tables".into()))
                && r.get("Type") == Some(&Value::Long(3))
        })
        .map(|(_, r)| field(r, "Id"))
        .ok_or_else(|| format_error("the catalog has no Tables container"))?;
    let like_row = existing
        .iter()
        .find(|(_, r)| r.get("Name") == Some(&Value::Text(like.into())))
        .map(|(_, r)| r.clone())
        .ok_or_else(|| format_error(format!("the catalog has no row for {like}")))?;
    let now = ole_date_now();
    let mut row = Row::new();
    row.insert("Id".into(), Value::Long(tdef_page as i32));
    row.insert("ParentId".into(), tables);
    row.insert("Name".into(), Value::Text(name.to_owned()));
    row.insert("Type".into(), Value::Long(TYPE_TABLE));
    row.insert("Flags".into(), Value::Long(0));
    row.insert("DateCreate".into(), Value::Double(now));
    row.insert("DateUpdate".into(), Value::Double(now));
    // owned as the table copied is
    row.insert("Owner".into(), field(&like_row, "Owner"));
    for column in &objects.columns {
        row.entry(column.name.clone()).or_insert(Value::Null);
    }
    let encoded = Encoded::new(&objects, &row)?;
    let mut writer = super::rows::Writer::new(&objects);
    let new_id = writer.add(&mut pages, &mut alloc, &encoded)?;
    let written = writer.finish(&mut pages);

    // the catalog owns the new page
    let owned_at = RowId::from_pointer(objects.owned_map);
    let mut owned = usage::read(&pages, owned_at)?.pages;
    owned.extend(&written.data_pages);
    usage::write(&mut pages, &mut alloc, owned_at, &owned)?;

    // its indexes, rebuilt over every row
    let mut rows: Vec<(RowId, Encoded)> = Vec::with_capacity(existing.len() + 1);
    for (id, r) in &existing {
        rows.push((*id, Encoded::new(&objects, r)?));
    }
    rows.push((new_id, encoded));
    let mut patch = objects.patch(&pages)?;
    patch.set_rows(rows.len() as u32);
    for (i, idx) in objects.indexes.iter().enumerate() {
        for (_, e) in &rows {
            index::check_row(&objects, idx, e)?;
        }
        let old = usage::read(&pages, RowId::from_pointer(idx.usage_map))?;
        for p in old.pages {
            pages.page_mut(p).fill(0);
        }
        let mut entries: Vec<Vec<u8>> = rows
            .iter()
            .map(|(id, e)| {
                let mut entry = index::key(&objects, idx, e);
                index::push_row_id(&mut entry, *id);
                entry
            })
            .collect();
        entries.sort();
        let mut keys: Vec<Vec<u8>> = rows
            .iter()
            .map(|(_, e)| index::key(&objects, idx, e))
            .collect();
        keys.sort();
        keys.dedup();
        let built = index::build(&mut pages, &mut alloc, objects.page, &entries)?;
        patch.set_index(i, built.root, keys.len() as u32);
        usage::write(
            &mut pages,
            &mut alloc,
            RowId::from_pointer(idx.usage_map),
            &built.pages,
        )?;
    }
    patch.write(&mut pages)?;
    Ok(pages.bytes)
}

/// Fills `page` as a data page belonging to `owner` (0: none, as usage map pages are) with
/// `rows`.
fn write_rows(pages: &mut Pages, page: u32, owner: u32, rows: &[Vec<u8>]) {
    let bytes = pages.page_mut(page);
    bytes.fill(0);
    bytes[0] = page_type::DATA;
    bytes[1] = 0x01;
    bytes[4..8].copy_from_slice(&owner.to_le_bytes());
    bytes[12..14].copy_from_slice(&(rows.len() as u16).to_le_bytes());
    let mut end = PAGE;
    let mut used = 0;
    for (i, row) in rows.iter().enumerate() {
        let start = end - row.len();
        bytes[start..end].copy_from_slice(row);
        bytes[14 + 2 * i..16 + 2 * i].copy_from_slice(&(start as u16).to_le_bytes());
        end = start;
        used += row.len() + 2;
    }
    let free = (PAGE - 14 - used) as u16;
    bytes[2..4].copy_from_slice(&free.to_le_bytes());
}

/// Now as Access keeps dates: days since 1899-12-30, the time of day as the fraction.
fn ole_date_now() -> f64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    25569.0 + secs / 86_400.0
}
