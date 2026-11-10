//! The content in NMR's reading order, with the lookups the export makes.
//!
//! NMR walks each data file in key order (rooms by room number and then map, text blocks by
//! number and part, everything else by number) and looks records up by key as it goes; several
//! results depend on that order, so [`World`] keeps it. Text is held as the stored bytes (CP437), because NMR
//! scans scripts byte by byte and writes them to the database through the Windows code page.

use std::collections::HashMap;

use silvermere_types::fixed::char_to_cp437;
use silvermere_types::records::{
    AbilitySlot, Class, Item, KnownMonster, Message, Race, Room, Shop, Spell,
};

use super::Content;

/// One text block part, decoded the way NMR reads it.
pub(super) struct TextPart {
    pub number: i32,
    pub part: i16,
    pub link: i32,
    pub data: Vec<u8>,
}

/// The content, ordered and indexed.
pub(super) struct World<'a> {
    pub rooms: Vec<&'a Room>,
    pub monsters: Vec<&'a KnownMonster>,
    pub items: Vec<&'a Item>,
    pub spells: Vec<&'a Spell>,
    pub shops: Vec<&'a Shop>,
    pub classes: Vec<&'a Class>,
    pub races: Vec<&'a Race>,
    pub text: Vec<TextPart>,
    room_index: HashMap<(u32, u32), usize>,
    monster_index: HashMap<i32, usize>,
    item_index: HashMap<i32, usize>,
    spell_index: HashMap<i32, usize>,
    messages: HashMap<i32, &'a Message>,
    /// Text block number to the position of its part 0 in `text`.
    text_index: HashMap<i32, usize>,
}

impl<'a> World<'a> {
    pub(super) fn new(content: &'a Content) -> Self {
        let mut rooms: Vec<&Room> = content.rooms.iter().collect();
        // the room file's key is the map and room numbers read as one little-endian number,
        // so it runs by room number first
        rooms.sort_by_key(|r| (r.key.room.0, r.key.map.0));
        let mut monsters: Vec<&KnownMonster> = content.monsters.iter().collect();
        monsters.sort_by_key(|m| m.number.0);
        let mut items: Vec<&Item> = content.items.iter().collect();
        items.sort_by_key(|i| i.number.0);
        let mut spells: Vec<&Spell> = content.spells.iter().collect();
        spells.sort_by_key(|s| s.number.0);
        let mut shops: Vec<&Shop> = content.shops.iter().collect();
        shops.sort_by_key(|s| s.number.0);
        let mut classes: Vec<&Class> = content.classes.iter().collect();
        classes.sort_by_key(|c| c.number.0);
        let mut races: Vec<&Race> = content.races.iter().collect();
        races.sort_by_key(|r| r.number.0);
        let mut text: Vec<TextPart> = content
            .text_blocks
            .iter()
            .map(|t| TextPart {
                number: t.number.0,
                part: t.part,
                link: t.link.0,
                data: script_bytes(&t.text),
            })
            .collect();
        text.sort_by_key(|t| (t.number, t.part));

        let index = |keys: Vec<i32>| keys.into_iter().enumerate().map(|(i, k)| (k, i)).collect();
        Self {
            room_index: rooms
                .iter()
                .enumerate()
                .map(|(i, r)| ((r.key.map.0, r.key.room.0), i))
                .collect(),
            monster_index: index(monsters.iter().map(|m| m.number.0).collect()),
            item_index: index(items.iter().map(|i| i.number.0).collect()),
            spell_index: index(spells.iter().map(|s| i32::from(s.number.0)).collect()),
            messages: content.messages.iter().map(|m| (m.number.0, m)).collect(),
            text_index: text
                .iter()
                .enumerate()
                .filter(|(_, t)| t.part == 0)
                .map(|(i, t)| (t.number, i))
                .collect(),
            rooms,
            monsters,
            items,
            spells,
            shops,
            classes,
            races,
            text,
        }
    }

    pub(super) fn room(&self, map: i64, room: i64) -> Option<&'a Room> {
        let key = (u32::try_from(map).ok()?, u32::try_from(room).ok()?);
        self.room_index.get(&key).map(|&i| self.rooms[i])
    }

    pub(super) fn monster(&self, n: i32) -> Option<&'a KnownMonster> {
        self.monster_index.get(&n).map(|&i| self.monsters[i])
    }

    pub(super) fn item(&self, n: i32) -> Option<&'a Item> {
        self.item_index.get(&n).map(|&i| self.items[i])
    }

    pub(super) fn spell(&self, n: i32) -> Option<&'a Spell> {
        self.spell_index.get(&n).map(|&i| self.spells[i])
    }

    /// Text block `n`'s part 0 and the parts after it, in order.
    pub(super) fn text_parts(&self, n: i32) -> &[TextPart] {
        let Some(&first) = self.text_index.get(&n) else {
            return &[];
        };
        let len = self.text[first..]
            .iter()
            .take_while(|t| t.number == n)
            .count();
        &self.text[first..first + len]
    }

    /// The block text block `n` links to, 0 when it has none or does not exist.
    pub(super) fn text_link(&self, n: i32) -> i32 {
        self.text_parts(n).first().map_or(0, |t| t.link)
    }

    /// One line of message `n` (`which` 1 to 3), or all of its lines joined with ", " (`which`
    /// 0), as NMR shows a message: "none" for 0 and a marker for a missing one.
    pub(super) fn message(&self, n: i32, which: usize) -> String {
        if n == 0 {
            return "none".into();
        }
        let Some(message) = self.messages.get(&n) else {
            return "MSG NOT IN DATABASE!".into();
        };
        if which == 0 {
            let lines: Vec<String> = message
                .msgs
                .iter()
                .map(|l| ansi(l))
                .filter(|l| !l.is_empty())
                .collect();
            lines.join(", ")
        } else {
            message
                .msgs
                .get(which - 1)
                .map_or_else(String::new, |l| ansi(l))
        }
    }

    /// The value of ability `ability` in spell `n`'s first slot holding it, -1 without.
    pub(super) fn spell_ability(&self, n: i32, ability: u16) -> i32 {
        if n <= 0 {
            return -1;
        }
        self.spell(n)
            .map_or(-1, |s| slot_value(&s.abilities, ability))
    }

    /// The value of ability `ability` in item `n`'s first slot holding it, -1 without.
    pub(super) fn item_ability(&self, n: i32, ability: u16) -> i32 {
        if n <= 0 {
            return -1;
        }
        self.item(n)
            .map_or(-1, |i| slot_value(&i.abilities, ability))
    }

    /// Item `n`'s game limit, -1 when there is no such item.
    pub(super) fn item_limit(&self, n: i32) -> i32 {
        self.item(n).map_or(-1, |i| i32::from(signed(i.limit)))
    }

    /// Item `n`'s type code, -1 when there is no such item.
    pub(super) fn item_type(&self, n: i32) -> i32 {
        self.item(n)
            .map_or(-1, |i| i32::from(signed(i.item_type.code())))
    }

    /// Monster `n`'s regeneration time, -1 for none or a missing monster.
    pub(super) fn monster_regen(&self, n: i32) -> i32 {
        if n == 0 {
            return -1;
        }
        self.monster(n).map_or(-1, |m| i32::from(m.regenhrs))
    }

    /// The experience monster `n` gives (experience times level), 0 for a missing monster and
    /// when the product does not fit the 32-bit number NMR multiplies in (it gives up there).
    pub(super) fn monster_exp(&self, n: i32) -> f64 {
        self.monster(n).map_or(0.0, |m| {
            m.exp.checked_mul(m.level as i32).map_or(0.0, f64::from)
        })
    }

    /// A spell's minimum damage at cast level `level` (its base below level 1).
    pub(super) fn spell_min(&self, n: i32, level: i32) -> i32 {
        self.spell(n)
            .map_or(0, |s| scaled(s.minval, s.mininc, s.minlvls, level))
    }

    /// A spell's maximum damage at cast level `level`.
    pub(super) fn spell_max(&self, n: i32, level: i32) -> i32 {
        self.spell(n)
            .map_or(0, |s| scaled(s.maxval, s.maxinc, s.maxlvls, level))
    }

    /// A spell's duration at cast level `level`.
    pub(super) fn spell_duration(&self, n: i32, level: i32) -> i32 {
        self.spell(n)
            .map_or(0, |s| scaled(s.duration, s.durinc, s.durlvls, level))
    }
}

/// `base + trunc(inc / per * level)`, NMR's level scaling, which is in fractions where the game
/// divides whole numbers.
fn scaled(base: i16, inc: u8, per: u8, level: i32) -> i32 {
    if per == 0 || level <= 0 {
        i32::from(base)
    } else {
        i32::from(base) + (f64::from(inc) / f64::from(per) * f64::from(level)).trunc() as i32
    }
}

fn slot_value(slots: &[AbilitySlot], ability: u16) -> i32 {
    if ability == 0 {
        return -1;
    }
    slots
        .iter()
        .find(|(a, _)| a.code() == ability)
        .map_or(-1, |&(_, v)| i32::from(v))
}

/// A 16-bit field read as NMR reads it: signed.
pub(super) fn signed(v: u16) -> i16 {
    v as i16
}

/// The CP437 bytes of a decoded string.
pub(super) fn cp437(s: &str) -> Vec<u8> {
    s.chars().map(char_to_cp437).collect()
}

/// A text block's text as NMR decodes it: the stored bytes, without the ones NMR drops (those
/// stored below the 0x20 shift, which decode to 0xE0 and up).
fn script_bytes(text: &str) -> Vec<u8> {
    cp437(text).into_iter().filter(|&b| b < 0xE0).collect()
}

/// Windows-1252 for 0x80-0x9F; above that it is Latin-1.
const CP1252_HIGH: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Stored bytes as NMR puts them in the database: read through the Windows code page.
pub(super) fn ansi_bytes(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
            other => char::from(other),
        })
        .collect()
}

/// A record's text as NMR puts it in the database (see [`ansi_bytes`]).
pub(super) fn ansi(s: &str) -> String {
    ansi_bytes(&cp437(s))
}
