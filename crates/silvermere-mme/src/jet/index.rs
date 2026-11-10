//! Indexes: each table's B-trees, built afresh from its rows.
//!
//! An index page (mdbtools `HACKING.md`, "Index Pages"; Jackcess `IndexData.writeDataPage`)
//! has a 27-byte header, a bitmap marking where each entry ends, and the entries from byte 480.
//! A leaf entry is the key and the row's pointer (page in three bytes, big-endian, then the row
//! number). A node entry adds the child page; it carries the last entry of that child. Where a
//! node's children are leaves, its last child is not an entry but the page's "child tail"
//! (Jackcess `IndexPageCache.promoteTail`). We write no entry prefixes: the format allows a
//! page without one, and every page then reads alike.
//!
//! Text keys follow Access's "General (legacy)" sort order (Jackcess
//! `GeneralLegacyIndexCodes`), for printable ASCII only: a character is either a code written
//! in line, or (the apostrophe and the hyphen) a code written after the text with its place.

use super::rows::Encoded;
use super::tdef::{ColType, Index, TableDef};
use super::{Allocator, JetError, PAGE, Pages, Result, RowId, page_type};

/// Where entries start on an index page.
const ENTRIES_AT: usize = 480;
const MASK_AT: usize = 27;
const ENTRY_SPACE: usize = PAGE - ENTRIES_AT;

/// The key of a row in an index: each column's flag byte and value, in the index's order.
pub(crate) fn key(def: &TableDef, index: &Index, row: &Encoded) -> Vec<u8> {
    let mut out = Vec::new();
    for &(number, ascending) in &index.columns {
        let kind = def.column(number).map(|c| c.kind);
        if kind == Some(ColType::Text) {
            match row.text(def, number) {
                None => out.push(if ascending { 0x00 } else { 0xff }),
                Some(text) => {
                    out.push(if ascending { 0x7f } else { 0x80 });
                    text_key(&text, ascending, &mut out);
                }
            }
            continue;
        }
        match row.fixed(def, number) {
            None => out.push(if ascending { 0x00 } else { 0xff }),
            Some(bytes) => {
                out.push(if ascending { 0x7f } else { 0x80 });
                let start = out.len();
                match kind {
                    // integers: big-endian, sign bit flipped, so they sort as bytes
                    Some(ColType::Byte) => out.push(bytes[0]),
                    Some(ColType::Int | ColType::Long) => {
                        out.extend(bytes.iter().rev());
                        out[start] ^= 0x80;
                    }
                    // floats: negative ones inverted whole, others with the sign bit flipped
                    Some(ColType::Float | ColType::Double) => {
                        out.extend(bytes.iter().rev());
                        if out[start] & 0x80 != 0 {
                            out[start..].iter_mut().for_each(|b| *b = !*b);
                        } else {
                            out[start] ^= 0x80;
                        }
                    }
                    _ => unreachable!("checked by `check`"),
                }
                if !ascending {
                    out[start..].iter_mut().for_each(|b| *b = !*b);
                }
            }
        }
    }
    out
}

/// Refuses a row whose text key this writer cannot encode (a character outside printable
/// ASCII in an indexed text column).
pub(crate) fn check_row(def: &TableDef, index: &Index, row: &Encoded) -> Result<()> {
    for &(number, _) in &index.columns {
        if def.column(number).map(|c| c.kind) == Some(ColType::Text)
            && let Some(text) = row.text(def, number)
            && let Some(c) = text.chars().find(|c| !(' '..='~').contains(c))
        {
            return Err(JetError::Row {
                table: def.name.clone(),
                reason: format!(
                    "index {} cannot key the character {c:?} in {text:?}",
                    index.name
                ),
            });
        }
    }
    Ok(())
}

/// How a character sorts in a text key.
enum Code {
    /// Bytes written where the character is.
    Inline(&'static [u8]),
    /// Bytes written after the text, with the character's place.
    Unprintable(&'static [u8]),
}

/// Printable ASCII, from `' '`, in Access's legacy General order (Jackcess
/// `index_codes_genleg.txt`).
const CODES: [Code; 95] = [
    Code::Inline(&[0x07]),       // ' '
    Code::Inline(&[0x09]),       // '!'
    Code::Inline(&[0x0a]),       // '"'
    Code::Inline(&[0x0c]),       // '#'
    Code::Inline(&[0x0e]),       // '$'
    Code::Inline(&[0x10]),       // '%'
    Code::Inline(&[0x12]),       // '&'
    Code::Unprintable(&[0x80]),  // "'"
    Code::Inline(&[0x14]),       // '('
    Code::Inline(&[0x16]),       // ')'
    Code::Inline(&[0x18]),       // '*'
    Code::Inline(&[0x2c]),       // '+'
    Code::Inline(&[0x1a]),       // ','
    Code::Unprintable(&[0x82]),  // '-'
    Code::Inline(&[0x1c]),       // '.'
    Code::Inline(&[0x1e]),       // '/'
    Code::Inline(&[0x36]),       // '0'
    Code::Inline(&[0x38]),       // '1'
    Code::Inline(&[0x3a]),       // '2'
    Code::Inline(&[0x3c]),       // '3'
    Code::Inline(&[0x3e]),       // '4'
    Code::Inline(&[0x40]),       // '5'
    Code::Inline(&[0x42]),       // '6'
    Code::Inline(&[0x44]),       // '7'
    Code::Inline(&[0x46]),       // '8'
    Code::Inline(&[0x48]),       // '9'
    Code::Inline(&[0x20]),       // ':'
    Code::Inline(&[0x22]),       // ';'
    Code::Inline(&[0x2e]),       // '<'
    Code::Inline(&[0x30]),       // '='
    Code::Inline(&[0x32]),       // '>'
    Code::Inline(&[0x24]),       // '?'
    Code::Inline(&[0x26]),       // '@'
    Code::Inline(&[0x4a]),       // 'A'
    Code::Inline(&[0x4c]),       // 'B'
    Code::Inline(&[0x4d]),       // 'C'
    Code::Inline(&[0x4f]),       // 'D'
    Code::Inline(&[0x51]),       // 'E'
    Code::Inline(&[0x53]),       // 'F'
    Code::Inline(&[0x55]),       // 'G'
    Code::Inline(&[0x57]),       // 'H'
    Code::Inline(&[0x59]),       // 'I'
    Code::Inline(&[0x5b]),       // 'J'
    Code::Inline(&[0x5c]),       // 'K'
    Code::Inline(&[0x5e]),       // 'L'
    Code::Inline(&[0x60]),       // 'M'
    Code::Inline(&[0x62]),       // 'N'
    Code::Inline(&[0x64]),       // 'O'
    Code::Inline(&[0x66]),       // 'P'
    Code::Inline(&[0x68]),       // 'Q'
    Code::Inline(&[0x69]),       // 'R'
    Code::Inline(&[0x6b]),       // 'S'
    Code::Inline(&[0x6d]),       // 'T'
    Code::Inline(&[0x6f]),       // 'U'
    Code::Inline(&[0x71]),       // 'V'
    Code::Inline(&[0x73]),       // 'W'
    Code::Inline(&[0x75]),       // 'X'
    Code::Inline(&[0x76]),       // 'Y'
    Code::Inline(&[0x78]),       // 'Z'
    Code::Inline(&[0x27]),       // '['
    Code::Inline(&[0x29]),       // '\\'
    Code::Inline(&[0x2a]),       // ']'
    Code::Inline(&[0x2b, 0x02]), // '^'
    Code::Inline(&[0x2b, 0x03]), // '_'
    Code::Inline(&[0x2b, 0x07]), // '`'
    Code::Inline(&[0x4a]),       // 'a'
    Code::Inline(&[0x4c]),       // 'b'
    Code::Inline(&[0x4d]),       // 'c'
    Code::Inline(&[0x4f]),       // 'd'
    Code::Inline(&[0x51]),       // 'e'
    Code::Inline(&[0x53]),       // 'f'
    Code::Inline(&[0x55]),       // 'g'
    Code::Inline(&[0x57]),       // 'h'
    Code::Inline(&[0x59]),       // 'i'
    Code::Inline(&[0x5b]),       // 'j'
    Code::Inline(&[0x5c]),       // 'k'
    Code::Inline(&[0x5e]),       // 'l'
    Code::Inline(&[0x60]),       // 'm'
    Code::Inline(&[0x62]),       // 'n'
    Code::Inline(&[0x64]),       // 'o'
    Code::Inline(&[0x66]),       // 'p'
    Code::Inline(&[0x68]),       // 'q'
    Code::Inline(&[0x69]),       // 'r'
    Code::Inline(&[0x6b]),       // 's'
    Code::Inline(&[0x6d]),       // 't'
    Code::Inline(&[0x6f]),       // 'u'
    Code::Inline(&[0x71]),       // 'v'
    Code::Inline(&[0x73]),       // 'w'
    Code::Inline(&[0x75]),       // 'x'
    Code::Inline(&[0x76]),       // 'y'
    Code::Inline(&[0x78]),       // 'z'
    Code::Inline(&[0x2b, 0x09]), // '{'
    Code::Inline(&[0x2b, 0x0b]), // '|'
    Code::Inline(&[0x2b, 0x0d]), // '}'
    Code::Inline(&[0x2b, 0x0f]), // '~'
];

/// Index entries end a text value with this, and the extra sections with it too.
const END_TEXT: u8 = 0x01;
const END_EXTRA_TEXT: u8 = 0x00;

/// A text value's key (after its flag byte). `check_row` has refused anything outside
/// printable ASCII; Access indexes at most 255 characters.
fn text_key(text: &str, ascending: bool, out: &mut Vec<u8>) {
    let start = out.len();
    let mut unprintable = Vec::new();
    let mut place = 0u16;
    for c in text.chars().take(255) {
        let code = (c as u32)
            .checked_sub(0x20)
            .and_then(|i| CODES.get(i as usize));
        match code {
            Some(Code::Inline(bytes)) => {
                out.extend_from_slice(bytes);
                place += 1;
            }
            Some(Code::Unprintable(bytes)) => {
                let offset = (7 + 4 * place) | 0x8000;
                unprintable.extend_from_slice(&offset.to_be_bytes());
                unprintable.push(0x06);
                unprintable.extend_from_slice(bytes);
            }
            None => {}
        }
    }
    out.push(END_TEXT);
    if !unprintable.is_empty() {
        out.extend_from_slice(&[END_TEXT, END_TEXT, END_TEXT]);
        out.extend_from_slice(&unprintable);
    }
    if !ascending {
        out.push(END_EXTRA_TEXT);
        out[start..].iter_mut().for_each(|b| *b = !*b);
    }
    out.push(END_EXTRA_TEXT);
}

/// Refuses an index this writer cannot key (on text, say).
pub(crate) fn check(def: &TableDef, index: &Index) -> Result<()> {
    for &(number, _) in &index.columns {
        let column = def.column(number).ok_or_else(|| {
            def.unsupported(format!(
                "index {} names column {number}, which it lacks",
                index.name
            ))
        })?;
        if !matches!(
            column.kind,
            ColType::Byte
                | ColType::Int
                | ColType::Long
                | ColType::Float
                | ColType::Double
                | ColType::Text
        ) {
            return Err(JetError::Unsupported {
                table: def.name.clone(),
                reason: format!(
                    "index {} is on column {:?}, which is neither a number nor text",
                    index.name, column.name
                ),
            });
        }
    }
    Ok(())
}

pub(crate) fn push_row_id(entry: &mut Vec<u8>, id: RowId) {
    let page = id.page.to_be_bytes();
    entry.extend_from_slice(&page[1..]);
    entry.push(id.row);
}

pub(crate) struct Built {
    pub root: u32,
    pub pages: Vec<u32>,
}

/// One page of a level as it is built.
struct Pending {
    page: u32,
    entries: Vec<Vec<u8>>,
    /// The child tail (0 for none).
    tail: u32,
    /// The last leaf entry (key and row pointer) under this page.
    last: Vec<u8>,
}

/// Builds the tree over `entries` (sorted, each key + row pointer) and writes its pages.
pub(crate) fn build(
    pages: &mut Pages,
    alloc: &mut Allocator,
    table: u32,
    entries: &[Vec<u8>],
) -> Result<Built> {
    let mut all_pages = Vec::new();
    let mut take = |alloc: &mut Allocator| {
        let page = alloc.take();
        all_pages.push(page);
        page
    };

    let mut level: Vec<Pending> = leaf_pages(entries)
        .into_iter()
        .map(|group| Pending {
            page: take(alloc),
            last: group.last().cloned().unwrap_or_default(),
            entries: group,
            tail: 0,
        })
        .collect();
    if level.is_empty() {
        level.push(Pending {
            page: take(alloc),
            entries: Vec::new(),
            tail: 0,
            last: Vec::new(),
        });
    }

    let mut leaves = true;
    while level.len() > 1 {
        // a parent holds an entry per child: the child's last entry and its page
        let children: Vec<(Vec<u8>, u32)> =
            level.iter().map(|p| (p.last.clone(), p.page)).collect();
        let mut parents = Vec::new();
        for group in split_children(&children) {
            let last = group.last().map(|c| c.0.clone()).unwrap_or_default();
            let (members, tail) = match group.split_last() {
                Some((tail, rest)) if leaves && !rest.is_empty() => (rest.to_vec(), tail.1),
                _ => (group, 0),
            };
            parents.push(Pending {
                page: take(alloc),
                entries: members
                    .iter()
                    .map(|(e, child)| node_entry(e, *child))
                    .collect(),
                tail,
                last,
            });
        }
        write_level(pages, table, &level, leaves);
        level = parents;
        leaves = false;
    }
    write_level(pages, table, &level, leaves);
    Ok(Built {
        root: level[0].page,
        pages: all_pages,
    })
}

fn node_entry(last: &[u8], child: u32) -> Vec<u8> {
    let mut e = last.to_vec();
    e.extend_from_slice(&child.to_be_bytes());
    e
}

/// Splits sorted leaf entries into pages.
fn leaf_pages(entries: &[Vec<u8>]) -> Vec<Vec<Vec<u8>>> {
    let mut pages = Vec::new();
    let mut current: Vec<Vec<u8>> = Vec::new();
    let mut used = 0;
    for e in entries {
        if used + e.len() > ENTRY_SPACE && !current.is_empty() {
            pages.push(std::mem::take(&mut current));
            used = 0;
        }
        used += e.len();
        current.push(e.clone());
    }
    if !current.is_empty() {
        pages.push(current);
    }
    pages
}

/// Splits a level's children among parent pages, each entry being a child's last entry and
/// four bytes of page number.
fn split_children(children: &[(Vec<u8>, u32)]) -> Vec<Vec<(Vec<u8>, u32)>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    let mut used = 0;
    for child in children {
        let size = child.0.len() + 4;
        if used + size > ENTRY_SPACE && !current.is_empty() {
            groups.push(std::mem::take(&mut current));
            used = 0;
        }
        used += size;
        current.push(child.clone());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    groups
}

/// Writes a level's pages, linked to their neighbours.
fn write_level(pages: &mut Pages, table: u32, level: &[Pending], leaves: bool) {
    for (i, p) in level.iter().enumerate() {
        let prev = if i == 0 { 0 } else { level[i - 1].page };
        let next = level.get(i + 1).map_or(0, |n| n.page);
        let bytes = pages.page_mut(p.page);
        bytes.fill(0);
        bytes[0] = if leaves {
            page_type::INDEX_LEAF
        } else {
            page_type::INDEX_NODE
        };
        bytes[1] = 0x01;
        bytes[4..8].copy_from_slice(&table.to_le_bytes());
        bytes[12..16].copy_from_slice(&prev.to_le_bytes());
        bytes[16..20].copy_from_slice(&next.to_le_bytes());
        bytes[20..24].copy_from_slice(&p.tail.to_le_bytes());
        // bytes 24..26: entry prefix length, 0
        let mut at = ENTRIES_AT;
        for e in &p.entries {
            bytes[at..at + e.len()].copy_from_slice(e);
            at += e.len();
            let end = at - ENTRIES_AT;
            bytes[MASK_AT + end / 8] |= 1 << (end % 8);
        }
        let free = (PAGE - at) as u16;
        bytes[2..4].copy_from_slice(&free.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        text_key(text, true, &mut out);
        out
    }

    #[test]
    fn text_keys_are_access_legacy_general() {
        // from the template's own pkLairs index: "10-10-10"
        assert_eq!(
            key("10-10-10"),
            [
                0x38, 0x36, 0x38, 0x36, 0x38, 0x36, 0x01, 0x01, 0x01, 0x01, 0x80, 0x0f, 0x06, 0x82,
                0x80, 0x17, 0x06, 0x82, 0x00
            ]
        );
        // letters sort without case; plain text has no trailing sections
        assert_eq!(key("Ab"), key("aB"));
        assert_eq!(key("ab"), [0x4a, 0x4c, 0x01, 0x00]);
        assert!(key("a") < key("b") && key("1") < key("a"));
    }

    #[test]
    fn leaves_fill_pages_in_order() {
        let entries: Vec<Vec<u8>> = (0..1000u32).map(|n| n.to_be_bytes().repeat(3)).collect();
        let pages = leaf_pages(&entries);
        assert!(pages.len() > 1);
        assert!(
            pages
                .iter()
                .all(|p| p.iter().map(Vec::len).sum::<usize>() <= ENTRY_SPACE)
        );
        assert_eq!(pages.concat(), entries);
    }
}
