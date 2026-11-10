//! Reading tables back: the catalog (to find a table's definition) and rows, for checking what
//! was written.

use std::collections::{BTreeMap, BTreeSet};

use super::tdef::{ColType, Column, TableDef};
use super::{JetError, PAGE, Pages, Result, RowId, format_error, page_type, u16le, u32le, usage};
use crate::table::{Row, Value};

/// The system catalog's definition page.
pub(crate) const CATALOG_PAGE: u32 = 2;
/// `MSysObjects.Type` of a table.
const TYPE_TABLE: i32 = 1;

const DELETED: u16 = 0x8000;
const OVERFLOW: u16 = 0x4000;

/// Every table's definition page, by name.
pub(crate) fn catalog(pages: &Pages) -> Result<BTreeMap<String, u32>> {
    let def = TableDef::read(pages, CATALOG_PAGE, "MSysObjects")?;
    let mut out = BTreeMap::new();
    for row in rows(pages, &def)? {
        let (Some(Value::Text(name)), Some(Value::Long(id)), Some(Value::Long(kind))) =
            (row.get("Name"), row.get("Id"), row.get("Type"))
        else {
            continue;
        };
        if *kind == TYPE_TABLE {
            out.insert(name.clone(), (*id as u32) & 0x00ff_ffff);
        }
    }
    Ok(out)
}

/// Every row of a table in a database file.
pub fn read_table(file: &[u8], name: &str) -> std::result::Result<Vec<Row>, JetError> {
    let pages = Pages {
        bytes: file.to_vec(),
    };
    let page = *catalog(&pages)?
        .get(name)
        .ok_or_else(|| JetError::NoTable(name.to_owned()))?;
    let def = TableDef::read(&pages, page, name)?;
    rows(&pages, &def)
}

fn rows(pages: &Pages, def: &TableDef) -> Result<Vec<Row>> {
    Ok(rows_with_ids(pages, def)?
        .into_iter()
        .map(|(_, row)| row)
        .collect())
}

/// Every row of a table with where it is: a moved row by its first place, which its index
/// entries name.
pub(crate) fn rows_with_ids(pages: &Pages, def: &TableDef) -> Result<Vec<(RowId, Row)>> {
    let owned = usage::read(pages, RowId::from_pointer(def.owned_map))?;
    let mut raw = Vec::new();
    let mut targets = BTreeSet::new();
    for &page in &owned.pages {
        let p = pages.page(page)?;
        if p[0] != page_type::DATA || u32le(p, 4) != def.page {
            continue;
        }
        for n in 0..u16le(p, 12) {
            let offset = u16le(p, 14 + 2 * usize::from(n));
            if offset & DELETED != 0 {
                continue;
            }
            let id = RowId { page, row: n as u8 };
            let mut bytes = pages.row(id)?;
            if offset & OVERFLOW != 0 {
                // the row moved: this one points to where it is now
                let target = long_pointer(bytes);
                targets.insert(target);
                bytes = pages.row(target)?;
            }
            raw.push((id, bytes));
        }
    }
    raw.into_iter()
        .filter(|(id, _)| !targets.contains(id))
        .map(|(id, bytes)| Ok((id, decode(pages, def, bytes)?)))
        .collect()
}

fn long_pointer(b: &[u8]) -> RowId {
    RowId {
        row: b[0],
        page: u32::from(b[1]) | u32::from(b[2]) << 8 | u32::from(b[3]) << 16,
    }
}

fn decode(pages: &Pages, def: &TableDef, row: &[u8]) -> Result<Row> {
    let bad = || format_error(format!("table {}: a row is malformed", def.name));
    if row.len() < 4 {
        return Err(bad());
    }
    let cols = usize::from(u16le(row, 0));
    let mask_len = cols.div_ceil(8);
    let mask_at = row.len().checked_sub(mask_len).ok_or_else(bad)?;
    let mask = &row[mask_at..];
    let present = |n: u16| usize::from(n) < cols && mask[usize::from(n) / 8] >> (n % 8) & 1 == 1;
    let var_count_at = mask_at.checked_sub(2).ok_or_else(bad)?;
    let var_count = usize::from(u16le(row, var_count_at));
    // offsets read backwards from the count; the one past the last is the end of the data
    let offset = |i: usize| -> Result<usize> {
        let at = var_count_at.checked_sub(2 * (i + 1)).ok_or_else(bad)?;
        Ok(usize::from(u16le(row, at)))
    };

    let mut out = Row::new();
    for column in &def.columns {
        let value = if column.kind == ColType::Bool {
            Value::Long(i32::from(present(column.number)))
        } else if !present(column.number) {
            Value::Null
        } else if column.fixed {
            let at = 2 + usize::from(column.fixed_offset);
            let bytes = row
                .get(at..at + usize::from(column.length))
                .ok_or_else(bad)?;
            fixed(column, bytes)
        } else {
            let i = usize::from(column.var_index);
            if i >= var_count {
                Value::Null
            } else {
                let (from, to) = (offset(i)?, offset(i + 1)?);
                let bytes = row.get(from..to).ok_or_else(bad)?;
                match column.kind {
                    ColType::Text => Value::Text(decode_text(bytes)),
                    ColType::Memo => Value::Text(decode_text(&long_value(pages, bytes)?)),
                    ColType::Binary => Value::Bytes(bytes.to_vec()),
                    ColType::Ole => Value::Bytes(long_value(pages, bytes)?),
                    _ => Value::Null,
                }
            }
        };
        out.insert(column.name.clone(), value);
    }
    Ok(out)
}

fn fixed(column: &Column, b: &[u8]) -> Value {
    match column.kind {
        ColType::Byte => Value::Long(i32::from(b[0])),
        ColType::Int => Value::Long(i32::from(i16::from_le_bytes([b[0], b[1]]))),
        ColType::Long => Value::Long(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
        ColType::Float => Value::Double(f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))),
        ColType::Double | ColType::Date => Value::Double(f64::from_le_bytes([
            b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7],
        ])),
        _ => Value::Null,
    }
}

/// A memo value from its definition in the row.
fn long_value(pages: &Pages, def: &[u8]) -> Result<Vec<u8>> {
    if def.len() < 12 {
        return Err(format_error("a memo definition is short"));
    }
    let word = u32le(def, 0);
    let len = (word & 0x3fff_ffff) as usize;
    match word >> 30 {
        2 => Ok(def[12..].to_vec()),
        1 => {
            let bytes = pages.row(long_pointer(&def[4..8]))?;
            Ok(bytes[..len.min(bytes.len())].to_vec())
        }
        0 => {
            let mut out = Vec::with_capacity(len);
            let mut next = long_pointer(&def[4..8]);
            while out.len() < len {
                let part = pages.row(next)?;
                if part.len() < 4 {
                    return Err(format_error("a memo part is short"));
                }
                out.extend_from_slice(&part[4..]);
                next = long_pointer(part);
                if next.page == 0 {
                    break;
                }
                if out.len() > 16 * 1024 * 1024 || next.page as usize * PAGE >= pages.bytes.len() {
                    return Err(format_error("a memo chain runs away"));
                }
            }
            out.truncate(len);
            Ok(out)
        }
        _ => Err(format_error("a memo definition of unknown type")),
    }
}

/// Text from either of Jet 4's encodings (see `rows::encode_text`). In compressed text a zero
/// byte switches between one byte and two per character.
pub(crate) fn decode_text(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xff && b[1] == 0xfe {
        let mut out = String::new();
        let mut compressed = true;
        let mut i = 2;
        while i < b.len() {
            if b[i] == 0 {
                compressed = !compressed;
                i += 1;
            } else if compressed {
                out.push(char::from(b[i]));
                i += 1;
            } else if i + 1 < b.len() {
                out.extend(
                    char::decode_utf16([u16::from_le_bytes([b[i], b[i + 1]])])
                        .map(|c| c.unwrap_or('\u{fffd}')),
                );
                i += 2;
            } else {
                break;
            }
        }
        out
    } else {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    }
}
