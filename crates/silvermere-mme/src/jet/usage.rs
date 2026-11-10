//! Usage maps: which pages a table, index or memo column owns.
//!
//! A map is a row on a data page (mdbtools `HACKING.md`, "Usage Maps"; Jackcess `UsageMap`).
//! An inline map (type 0) holds a start page and a bitmap of the pages after it; a reference
//! map (type 1) holds the numbers of map pages, each a bitmap of 32,736 pages. The row's
//! length is fixed when the database makes it, so we rewrite it at the same length: inline
//! when the pages fit its window, by reference when they don't.

use super::{Allocator, PAGE, Pages, Result, RowId, format_error, page_type, u32le};

/// Pages one map page covers: its bitmap starts after a 4-byte header.
const PAGES_PER_MAP_PAGE: u32 = ((PAGE - 4) * 8) as u32;

pub(crate) struct UsageMap {
    /// The pages the map lists.
    pub pages: Vec<u32>,
    /// The pages holding the map itself, for a reference map.
    pub map_pages: Vec<u32>,
}

pub(crate) fn read(pages: &Pages, at: RowId) -> Result<UsageMap> {
    let row = pages.row(at)?;
    let mut map = UsageMap {
        pages: Vec::new(),
        map_pages: Vec::new(),
    };
    match row.first() {
        Some(0) => {
            if row.len() < 5 {
                return Err(format_error("an inline usage map is too short"));
            }
            let start = u32le(row, 1);
            collect(&row[5..], start, &mut map.pages);
        }
        Some(1) => {
            for (j, pointer) in row[1..].as_chunks::<4>().0.iter().enumerate() {
                let page = u32::from_le_bytes(*pointer);
                if page == 0 {
                    continue;
                }
                let bitmap = pages.page(page)?;
                if bitmap[0] != page_type::USAGE_MAP {
                    return Err(format_error(format!("page {page} is not a usage map page")));
                }
                collect(&bitmap[4..], j as u32 * PAGES_PER_MAP_PAGE, &mut map.pages);
                map.map_pages.push(page);
            }
        }
        other => {
            return Err(format_error(format!("usage map of unknown type {other:?}")));
        }
    }
    Ok(map)
}

fn collect(bitmap: &[u8], start: u32, out: &mut Vec<u32>) {
    for (i, byte) in bitmap.iter().enumerate() {
        for bit in 0..8 {
            if byte >> bit & 1 == 1 {
                out.push(start + i as u32 * 8 + bit);
            }
        }
    }
}

/// Rewrites the map at `at` to list exactly `listed`.
pub(crate) fn write(
    pages: &mut Pages,
    alloc: &mut Allocator,
    at: RowId,
    listed: &[u32],
) -> Result<()> {
    let len = pages.row(at)?.len();
    if len < 5 {
        return Err(format_error("a usage map row is too short"));
    }
    let mut row = vec![0u8; len];
    let window = (len as u32 - 5) * 8;
    let (min, max) = match (listed.iter().min(), listed.iter().max()) {
        (Some(&min), Some(&max)) => (min, max),
        _ => {
            // empty: an inline map with no bits
            return pages.put_row(at, &row);
        }
    };
    if max - min < window {
        row[0] = 0;
        row[1..5].copy_from_slice(&min.to_le_bytes());
        for &p in listed {
            let bit = (p - min) as usize;
            row[5 + bit / 8] |= 1 << (bit % 8);
        }
        return pages.put_row(at, &row);
    }

    let needed = (max / PAGES_PER_MAP_PAGE + 1) as usize;
    if 1 + needed * 4 > len {
        return Err(format_error(format!(
            "a usage map row of {len} bytes cannot reach page {max}"
        )));
    }
    row[0] = 1;
    let mut bitmaps: Vec<Option<u32>> = vec![None; needed];
    for &p in listed {
        let j = (p / PAGES_PER_MAP_PAGE) as usize;
        let page = *bitmaps[j].get_or_insert_with(|| {
            let page = alloc.take();
            let bytes = pages.page_mut(page);
            bytes.fill(0);
            bytes[0] = page_type::USAGE_MAP;
            bytes[1] = 0x01;
            page
        });
        let bit = (p % PAGES_PER_MAP_PAGE) as usize;
        pages.page_mut(page)[4 + bit / 8] |= 1 << (bit % 8);
    }
    for (j, page) in bitmaps.iter().enumerate() {
        if let Some(page) = page {
            row[1 + 4 * j..5 + 4 * j].copy_from_slice(&page.to_le_bytes());
        }
    }
    pages.put_row(at, &row)
}
