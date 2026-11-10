//! Nightmare Redux's MME export, ported.
//!
//! Nightmare Redux (NMR) is syntax53's MajorMUD data editor, and its "Export to MMUD Explorer"
//! writes a realm's data into the Access database that MajorMUD Explorer (MME, also syntax53's)
//! reads. This module is that export, version 1.8.3, over silvermere's records instead of the
//! Btrieve files: [`export`] fills every table with every column NMR writes, the ones it works
//! out included. Credit for the export, its formulas and its heuristics goes to syntax53.
//!
//! Most columns are a record's fields. The rest come from cross-referencing the world
//! (`xref`): whether a record is in the game and where it comes from (an item's "Obtained
//! From" and "References", a spell's "Learned From", "Casted By" and classes, a monster's
//! "Summoned By", a shop's "Assigned To", the text blocks something runs), from a combat
//! simulation (`sim`: a monster's average damage), and from the lairs (`lairs`: each room's
//! lair, a monster's lair experience and script value, the Lairs table).
//!
//! The port keeps NMR's results where MME shows them, including its rounding and its quirks
//! (a column's text cut at its limit, a value carried from the record before), because MME
//! and its users read the database as NMR writes it. Where NMR draws random numbers or would
//! stop, the module notes what the port does instead.

mod lairs;
mod records;
mod rooms;
mod script;
mod sim;
mod vb;
mod world;
mod xref;

use std::collections::HashMap;

use silvermere_types::records::{
    Class, Item, KnownMonster, Message, Race, Room, Shop, Spell, TextBlock,
};

pub use rooms::DEFAULT_EXCLUDED_ROOMS;

use crate::table::Tables;
use world::{World, ansi_bytes};

/// The records the export reads: the world's content, in any order.
#[derive(Debug, Clone, Default)]
pub struct Content {
    pub rooms: Vec<Room>,
    pub monsters: Vec<KnownMonster>,
    pub items: Vec<Item>,
    pub spells: Vec<Spell>,
    pub shops: Vec<Shop>,
    pub classes: Vec<Class>,
    pub races: Vec<Race>,
    /// Exit texts and monster attack names come from these.
    pub messages: Vec<Message>,
    /// Every part of every block: scripts are read for what they refer to.
    pub text_blocks: Vec<TextBlock>,
}

/// What kind of database the export is (the Info table's `Legit`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportKind {
    /// A realm's own database.
    #[default]
    Custom,
    /// The stock database syntax53 publishes with MME: named "Default", pointing at MME's own
    /// update page, with the default rooms left out and no others.
    Legit,
    /// A realm running the GreaterMUD engine; MME adjusts its formulas to it.
    GreaterMud,
}

/// A range of room numbers on one map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RoomRange {
    pub map: u32,
    pub from: u32,
    pub to: u32,
}

impl RoomRange {
    pub const fn new(map: u32, from: u32, to: u32) -> Self {
        Self { map, from, to }
    }
}

/// The export's settings: NMR's export dialog. [`Options::default`] is NMR's defaults, with
/// the date and time left for the caller (the export does not read the clock).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The BBS or database name MME shows (Info `Custom`).
    pub custom: String,
    /// The data version or realm (Info `Dat File Version`).
    pub dat_version: String,
    /// The NMR version the database says it comes from (Info `NMR Version`); MME compares it.
    pub nmr_version: String,
    /// The export date as NMR writes it in a US locale, "9/15/2026" (Info `Date`).
    pub date: String,
    /// The export time, "1:50:47 PM" (Info `Time`).
    pub time: String,
    /// The page MME's update check opens (Info `UpdateURL`).
    pub update_url: String,
    pub kind: ExportKind,
    /// The realm has kai auto-learning turned off (Info `DisableKai`).
    pub disable_kai: bool,
    /// Leave out [`DEFAULT_EXCLUDED_ROOMS`]: they are not written and nothing in them counts
    /// as in the game.
    pub exclude_default_rooms: bool,
    /// More rooms to leave out the same way.
    pub excluded_rooms: Vec<RoomRange>,
    /// Write `excluded_rooms` by name only (no exits, so MME cannot map them) and keep what is
    /// in them in the game.
    pub hide_excluded_rooms: bool,
    /// Write no rooms at all (they are still cross-referenced).
    pub no_rooms: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            custom: "My BBS Name".into(),
            dat_version: "v1.11p".into(),
            nmr_version: "v1.8.3".into(),
            date: String::new(),
            time: String::new(),
            update_url: String::new(),
            kind: ExportKind::Custom,
            disable_kai: false,
            exclude_default_rooms: true,
            excluded_rooms: Vec::new(),
            hide_excluded_rooms: false,
            no_rooms: false,
        }
    }
}

/// The rows of every MME table for `content`.
pub fn export(content: &Content, options: &Options) -> Tables {
    let world = World::new(content);
    let out = rooms::Exclusions::new(options);
    let mobs = rooms::MonsterIndex::new(&world);

    // one simulator for all monsters, in number order: it carries values from one to the next
    let mut simulator = sim::Simulator::default();
    let damage: HashMap<i32, sim::Damage> = world
        .monsters
        .iter()
        .map(|m| (m.number.0, simulator.run(&world, m)))
        .collect();

    let mut marks = xref::Marks::locate(&world, &mobs, &out);
    marks
        .lairs
        .average(&records::monster_stats(&world, &damage));
    // in number order: each lair's script value is the one worked out for the monster before
    let mut lair_values = HashMap::new();
    for m in world.monsters.iter().filter(|m| m.regenhrs == 0) {
        let summoned_by = marks.monsters[&m.number.0].summoned_by.text();
        if summoned_by.contains("(lair)")
            && let Some(value) = marks.lairs.monster_value(summoned_by)
        {
            lair_values.insert(m.number.0, value);
        }
    }
    let monsters = world
        .monsters
        .iter()
        .map(|m| {
            let lair = lair_values.get(&m.number.0).copied();
            records::monster(&world, &marks, m, &damage[&m.number.0], lair)
        })
        .collect();

    Tables {
        info: vec![records::info(options)],
        classes: records::classes(&world),
        races: records::races(&world),
        items: records::items(&world, &marks),
        spells: records::spells(&world, &marks),
        monsters,
        shops: records::shops(&world, &marks),
        rooms: rooms::rows(&world, &mobs, &out, options),
        tbinfo: marks
            .text
            .iter()
            .map(|(&n, t)| {
                let mut row = crate::table::Row::new();
                row.insert("Number".into(), n.into());
                row.insert("LinkTo".into(), t.link_to.into());
                row.insert("Action".into(), ansi_bytes(&t.action).into());
                row.insert("Called From".into(), t.called_from.text().into());
                row
            })
            .collect(),
        lairs: marks.lairs.rows(&world),
    }
}
