//! Rows: encoding a [`Row`] for a table, and packing rows and long values onto pages.
//!
//! A Jet 4 row (mdbtools `HACKING.md`, "Data Rows"; Jackcess `TableImpl.createRow`) is the
//! column count, the fixed-length columns at their offsets, the variable-length columns end to
//! end, then a trailer read from the end: the null mask, the number of variable columns, their
//! offsets (last first) and the offset where variable data ends. Memo values live in the row
//! when short (at most 64 bytes) and otherwise on LVAL pages, the row holding a 12-byte pointer.

use std::collections::BTreeMap;

use super::tdef::{ColType, Column, TableDef};
use super::{Allocator, JetError, PAGE, Pages, Result, RowId, page_type};
use crate::table::{Row, Value};

/// The largest row Jet 4 stores on a data page.
pub(crate) const MAX_ROW: usize = 4060;
/// A data page's header before the row offset table.
const PAGE_HEADER: usize = 14;
/// Memo values up to this many bytes may stay in the row.
const MAX_INLINE_LONG_VALUE: usize = 64;
/// The largest long value row on an LVAL page; longer values chain over several.
const MAX_LONG_VALUE_ROW: usize = 4076;
const LONG_VALUE_DEF: usize = 12;
/// Rows a data page may hold (the row number in a pointer is one byte).
const MAX_ROWS_PER_PAGE: usize = 255;
/// Text up to this many characters is stored compressed when it can be (Jackcess
/// `JetFormat.MAX_COMPRESSED_UNICODE_SIZE`).
const MAX_COMPRESSED_CHARS: usize = 1024;

/// One field, encoded for its column.
#[derive(Debug, Clone)]
enum Field {
    Null,
    Fixed(Vec<u8>),
    Bool(bool),
    Var(Vec<u8>),
    Memo(Vec<u8>),
}

/// A row encoded for a table, its memo values not yet placed.
#[derive(Debug, Clone)]
pub(crate) struct Encoded {
    /// By position in the table's column list.
    fields: Vec<Field>,
}

impl Encoded {
    pub fn new(def: &TableDef, row: &Row) -> Result<Encoded> {
        for name in row.keys() {
            if !def.columns.iter().any(|c| &c.name == name) {
                return Err(JetError::Row {
                    table: def.name.clone(),
                    reason: format!("no column {name:?}"),
                });
            }
        }
        let mut fields = Vec::with_capacity(def.columns.len());
        for column in &def.columns {
            let value = row.get(&column.name);
            fields.push(encode(def, column, value)?);
        }
        Ok(Encoded { fields })
    }

    /// The text of a text column (for index keys), or `None` when it is null.
    pub fn text(&self, def: &TableDef, number: u16) -> Option<String> {
        let i = def.columns.iter().position(|c| c.number == number)?;
        match &self.fields[i] {
            Field::Var(bytes) => Some(super::read::decode_text(bytes)),
            _ => None,
        }
    }

    /// The bytes of a fixed-length column (for index keys), or `None` when it is null.
    pub fn fixed(&self, def: &TableDef, number: u16) -> Option<&[u8]> {
        let i = def.columns.iter().position(|c| c.number == number)?;
        match &self.fields[i] {
            Field::Fixed(bytes) => Some(bytes),
            _ => None,
        }
    }
}

fn encode(def: &TableDef, column: &Column, value: Option<&Value>) -> Result<Field> {
    let wrong = |what: &str| JetError::Row {
        table: def.name.clone(),
        reason: format!(
            "column {:?} ({:?}) cannot hold {what}",
            column.name, column.kind
        ),
    };
    // a column the row leaves out is zero or empty, as `Row` documents
    let default;
    let value = match value {
        Some(v) => v,
        None => {
            default = match column.kind {
                ColType::Text | ColType::Memo => Value::Text(String::new()),
                ColType::Binary | ColType::Ole => Value::Null,
                _ => Value::Long(0),
            };
            &default
        }
    };
    let number = |v: &Value| -> Option<f64> {
        match v {
            Value::Long(n) => Some(f64::from(*n)),
            Value::Double(d) => Some(*d),
            _ => None,
        }
    };
    Ok(match (column.kind, value) {
        (_, Value::Null) => Field::Null,
        (ColType::Bool, v) => Field::Bool(number(v).ok_or_else(|| wrong("text or bytes"))? != 0.0),
        (ColType::Byte | ColType::Int | ColType::Long, v) => {
            let n = number(v).ok_or_else(|| wrong("text"))?.round();
            let bytes = match column.kind {
                ColType::Byte if (0.0..=255.0).contains(&n) => vec![n as u8],
                ColType::Int if (f64::from(i16::MIN)..=f64::from(i16::MAX)).contains(&n) => {
                    (n as i16).to_le_bytes().to_vec()
                }
                ColType::Long if (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&n) => {
                    (n as i32).to_le_bytes().to_vec()
                }
                _ => return Err(wrong(&format!("{n}"))),
            };
            Field::Fixed(bytes)
        }
        (ColType::Float, v) => Field::Fixed(
            (number(v).ok_or_else(|| wrong("text"))? as f32)
                .to_le_bytes()
                .to_vec(),
        ),
        (ColType::Binary, Value::Bytes(b)) => {
            Field::Var(b.iter().take(usize::from(column.length)).copied().collect())
        }
        (ColType::Ole, Value::Bytes(b)) => Field::Memo(b.clone()),
        (ColType::Binary | ColType::Ole, _) => return Err(wrong("anything but bytes")),
        (ColType::Double | ColType::Date, v) => Field::Fixed(
            number(v)
                .ok_or_else(|| wrong("text"))?
                .to_le_bytes()
                .to_vec(),
        ),
        (ColType::Text, Value::Text(s)) => {
            let chars = usize::from(column.length / 2);
            let s: String = s.chars().take(chars).collect();
            Field::Var(encode_text(&s, column.compressed))
        }
        (ColType::Memo, Value::Text(s)) => Field::Memo(encode_text(s, column.compressed)),
        (ColType::Text | ColType::Memo, _) => return Err(wrong("a number or bytes")),
        (ColType::Other(code), _) => {
            return Err(def.unsupported(format!("column {:?} has type {code:#04x}", column.name)));
        }
    })
}

/// Text as Jet 4 stores it: "compressed unicode" (`FF FE` then one byte per character) when
/// every character fits a byte, else UTF-16LE (Jackcess `ColumnImpl.encodeTextValue`).
pub(crate) fn encode_text(s: &str, compressible: bool) -> Vec<u8> {
    let count = s.chars().count();
    if compressible
        && count > 0
        && count <= MAX_COMPRESSED_CHARS
        && s.chars().all(|c| ('\u{1}'..='\u{ff}').contains(&c))
    {
        let mut out = vec![0xff, 0xfe];
        out.extend(s.chars().map(|c| c as u32 as u8));
        out
    } else {
        s.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }
}

/// Writes rows onto data pages and long values onto LVAL pages.
pub(crate) struct Writer<'a> {
    def: &'a TableDef,
    data: PageBuilder,
    data_pages: Vec<u32>,
    lval: BTreeMap<u16, (PageBuilder, Vec<u32>)>,
}

pub(crate) struct Written {
    pub data_pages: Vec<u32>,
    /// LVAL pages by memo column number.
    pub lval_pages: BTreeMap<u16, Vec<u32>>,
}

impl<'a> Writer<'a> {
    pub fn new(def: &'a TableDef) -> Writer<'a> {
        Writer {
            def,
            data: PageBuilder::new(Owner::Table(def.page)),
            data_pages: Vec::new(),
            lval: BTreeMap::new(),
        }
    }

    /// Stores one row, and its long values; returns where the row is.
    pub fn add(
        &mut self,
        pages: &mut Pages,
        alloc: &mut Allocator,
        row: &Encoded,
    ) -> Result<RowId> {
        let bytes = self.assemble(pages, alloc, row)?;
        if bytes.len() > MAX_ROW {
            return Err(JetError::Row {
                table: self.def.name.clone(),
                reason: format!("a row of {} bytes is over Jet's {MAX_ROW}", bytes.len()),
            });
        }
        Ok(self.data.add(pages, alloc, &bytes, &mut self.data_pages))
    }

    pub fn finish(mut self, pages: &mut Pages) -> Written {
        self.data.flush(pages);
        let mut lval_pages = BTreeMap::new();
        for (column, (mut builder, list)) in self.lval {
            builder.flush(pages);
            lval_pages.insert(column, list);
        }
        Written {
            data_pages: self.data_pages,
            lval_pages,
        }
    }

    fn assemble(
        &mut self,
        pages: &mut Pages,
        alloc: &mut Allocator,
        row: &Encoded,
    ) -> Result<Vec<u8>> {
        let def = self.def;
        let max_cols = usize::from(def.max_cols);
        let mask_len = max_cols.div_ceil(8);
        let mut null_mask = vec![0u8; mask_len];
        let mut mark = |number: u16| {
            let n = usize::from(number);
            if n < max_cols {
                null_mask[n / 8] |= 1 << (n % 8);
            }
        };

        let mut out = Vec::with_capacity(256);
        out.extend_from_slice(&def.max_cols.to_le_bytes());
        let fixed_end = def
            .columns
            .iter()
            .filter(|c| c.fixed && c.kind != ColType::Bool)
            .map(|c| usize::from(c.fixed_offset) + usize::from(c.length))
            .max()
            .unwrap_or(0);
        out.resize(2 + fixed_end, 0);
        for (column, field) in def.columns.iter().zip(&row.fields) {
            match field {
                Field::Fixed(bytes) if column.fixed => {
                    let at = 2 + usize::from(column.fixed_offset);
                    out[at..at + bytes.len()].copy_from_slice(bytes);
                    mark(column.number);
                }
                // a yes/no column is its null-mask bit
                Field::Bool(true) => mark(column.number),
                _ => {}
            }
        }

        // variable columns in the order of their offset table
        let mut vars: Vec<(&Column, &Field)> = def
            .columns
            .iter()
            .zip(&row.fields)
            .filter(|(c, _)| !c.fixed)
            .collect();
        vars.sort_by_key(|(c, _)| c.var_index);
        let mut offsets = vec![0u16; vars.len()];
        for (slot, (column, field)) in offsets.iter_mut().zip(&vars) {
            *slot = out.len() as u16;
            match field {
                Field::Var(bytes) => {
                    out.extend_from_slice(bytes);
                    mark(column.number);
                }
                Field::Memo(bytes) => {
                    let room = MAX_ROW.saturating_sub(out.len() + mask_len + 4 + vars.len() * 2);
                    let def_bytes = self.long_value(pages, alloc, column.number, bytes, room)?;
                    out.extend_from_slice(&def_bytes);
                    mark(column.number);
                }
                _ => {}
            }
        }
        let eod = out.len() as u16;
        out.extend_from_slice(&eod.to_le_bytes());
        for offset in offsets.iter().rev() {
            out.extend_from_slice(&offset.to_le_bytes());
        }
        out.extend_from_slice(&(vars.len() as u16).to_le_bytes());
        out.extend_from_slice(&null_mask);
        Ok(out)
    }

    /// The 12-byte (or longer, when inline) definition of a memo value, writing the value to
    /// LVAL pages when it does not stay in the row (Jackcess `LongValueColumnImpl.writeLongValue`).
    fn long_value(
        &mut self,
        pages: &mut Pages,
        alloc: &mut Allocator,
        column: u16,
        value: &[u8],
        room: usize,
    ) -> Result<Vec<u8>> {
        let len = value.len() as u32;
        let mut def = Vec::with_capacity(LONG_VALUE_DEF + value.len());
        if value.len() <= MAX_INLINE_LONG_VALUE && LONG_VALUE_DEF + value.len() <= room {
            def.extend_from_slice(&(len | 0x8000_0000).to_le_bytes());
            def.extend_from_slice(&[0; 8]);
            def.extend_from_slice(value);
            return Ok(def);
        }
        let (builder, list) = self
            .lval
            .entry(column)
            .or_insert_with(|| (PageBuilder::new(Owner::LongValue), Vec::new()));
        let first = if value.len() <= MAX_LONG_VALUE_ROW {
            def.extend_from_slice(&(len | 0x4000_0000).to_le_bytes());
            builder.add(pages, alloc, value, list)
        } else {
            def.extend_from_slice(&len.to_le_bytes());
            // a chain: each part starts with a pointer to the next; write back to front so
            // every part knows where the next one went
            let chunk = MAX_LONG_VALUE_ROW - 4;
            let parts: Vec<&[u8]> = value.chunks(chunk).collect();
            let mut next: Option<RowId> = None;
            for part in parts.iter().rev() {
                let mut bytes = Vec::with_capacity(part.len() + 4);
                bytes.extend_from_slice(&pointer(next));
                bytes.extend_from_slice(part);
                // each part on a page of its own, so no part is squeezed
                builder.flush(pages);
                next = Some(builder.add(pages, alloc, &bytes, list));
            }
            next.expect("a long value has at least one part")
        };
        def.extend_from_slice(&pointer(Some(first)));
        def.extend_from_slice(&[0; 4]);
        Ok(def)
    }
}

/// A row pointer as long values store it: the row number, then the page in three bytes.
fn pointer(id: Option<RowId>) -> [u8; 4] {
    match id {
        None => [0; 4],
        Some(id) => {
            let p = id.page.to_le_bytes();
            [id.row, p[0], p[1], p[2]]
        }
    }
}

/// What a data page belongs to: its header names the table, or says `LVAL`.
#[derive(Debug, Clone, Copy)]
enum Owner {
    Table(u32),
    LongValue,
}

/// Fills one data page at a time.
struct PageBuilder {
    owner: Owner,
    page: Option<u32>,
    rows: Vec<Vec<u8>>,
    used: usize,
}

impl PageBuilder {
    fn new(owner: Owner) -> PageBuilder {
        PageBuilder {
            owner,
            page: None,
            rows: Vec::new(),
            used: 0,
        }
    }

    fn add(
        &mut self,
        pages: &mut Pages,
        alloc: &mut Allocator,
        row: &[u8],
        list: &mut Vec<u32>,
    ) -> RowId {
        let fits =
            self.used + row.len() + 2 <= PAGE - PAGE_HEADER && self.rows.len() < MAX_ROWS_PER_PAGE;
        if self.page.is_none() || !fits {
            self.flush(pages);
            let page = alloc.take();
            list.push(page);
            self.page = Some(page);
        }
        self.rows.push(row.to_vec());
        self.used += row.len() + 2;
        RowId {
            page: self.page.expect("a page was just taken"),
            row: (self.rows.len() - 1) as u8,
        }
    }

    /// Writes the page being filled; the next row starts a new one.
    fn flush(&mut self, pages: &mut Pages) {
        let Some(page) = self.page.take() else {
            return;
        };
        let bytes = pages.page_mut(page);
        bytes.fill(0);
        bytes[0] = page_type::DATA;
        bytes[1] = 0x01;
        let free = (PAGE - PAGE_HEADER - self.used) as u16;
        bytes[2..4].copy_from_slice(&free.to_le_bytes());
        match self.owner {
            Owner::Table(tdef) => bytes[4..8].copy_from_slice(&tdef.to_le_bytes()),
            Owner::LongValue => bytes[4..8].copy_from_slice(b"LVAL"),
        }
        bytes[12..14].copy_from_slice(&(self.rows.len() as u16).to_le_bytes());
        let mut end = PAGE;
        for (i, row) in self.rows.iter().enumerate() {
            let start = end - row.len();
            bytes[start..end].copy_from_slice(row);
            bytes[14 + 2 * i..16 + 2 * i].copy_from_slice(&(start as u16).to_le_bytes());
            end = start;
        }
        self.rows.clear();
        self.used = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jet::read::decode_text;

    #[test]
    fn text_round_trips_compressed_or_not() {
        assert_eq!(encode_text("Warrior", true), b"\xff\xfeWarrior");
        // a NUL (NMR's empty marker) cannot be compressed
        assert_eq!(
            encode_text("939,\0", true),
            "939,\0"
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        );
        assert_eq!(encode_text("", true), b"");
        for s in ["Warrior", "939,\0", "café", "€uro", ""] {
            assert_eq!(decode_text(&encode_text(s, true)), s);
            assert_eq!(decode_text(&encode_text(s, false)), s);
        }
    }
}
