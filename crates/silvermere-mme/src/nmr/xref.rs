//! Cross-referencing: which records are in the game, and where each is found.
//!
//! NMR starts from the rooms (what is placed, sold, generated, cast or run there) and the shops
//! those rooms hold, then makes at least ten passes over monsters, spells, text blocks and items,
//! each record marking what it summons, casts, teaches, gives or runs, until a pass marks nothing
//! new (at most nineteen). Text blocks are read as scripts (see [`super::script`]). Last it opens
//! the containers and asks the NPCs about every topic, for the items and spells those give.
//!
//! Each "where from" column lists entries joined with ", " and skips an entry the column already
//! contains as text (so "Room 1/23" is not added once "Room 1/234" is there); a column that
//! outgrows its limit is cut and ends with "+".

use std::collections::{BTreeMap, HashMap, HashSet};

use silvermere_types::records::Room;

use super::lairs::LairBook;
use super::rooms::{Exclusions, MonsterIndex};
use super::script::{Drops, Lessons, digits_at};
use super::vb::{self, Money};
use super::world::{World, signed};

/// A "where from" column, `"\0"` while empty as NMR stores it.
#[derive(Debug, Clone)]
pub(super) struct FromList {
    text: String,
    /// The longest the text may grow before it is cut.
    max: usize,
    /// How much of it a cut keeps.
    keep: usize,
}

impl FromList {
    fn new(max: usize, keep: usize) -> Self {
        Self {
            text: "\0".into(),
            max,
            keep,
        }
    }

    /// Whether the column's text contains `from`.
    fn holds(&self, from: &str) -> bool {
        !self.text.is_empty() && self.text.contains(from)
    }

    /// Adds `from` unless the text already contains it; whether it was added.
    fn add(&mut self, from: &str) -> bool {
        if self.holds(from) {
            return false;
        }
        let mut text = if self.text == "\0" {
            String::new()
        } else {
            format!("{}, ", self.text)
        };
        text.push_str(from);
        if text.chars().count() > self.max {
            text = text.chars().take(self.keep).collect();
            text.push_str("+\0");
        }
        self.text = text;
        true
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }
}

/// An item's derived columns.
pub(super) struct ItemMarks {
    pub in_game: bool,
    pub obtained_from: FromList,
    pub references: FromList,
}

/// A monster's derived columns.
pub(super) struct MonsterMarks {
    pub in_game: bool,
    pub summoned_by: FromList,
}

/// A spell's derived columns.
pub(super) struct SpellMarks {
    pub learnable: bool,
    pub learned_from: FromList,
    pub casted_by: FromList,
    /// "(1),(4)": the classes that can learn it, "(*)" any.
    pub classes: String,
}

/// A shop's derived columns.
pub(super) struct ShopMarks {
    pub in_game: bool,
    pub assigned_to: FromList,
}

/// A TBInfo row: a text block something runs.
pub(super) struct TextMarks {
    pub link_to: i32,
    /// The script, when the block holds commands; `"\0"` otherwise.
    pub action: Vec<u8>,
    pub called_from: FromList,
}

/// What a text block script refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refers {
    Spell,
    Item,
    Monster,
    Text,
}

/// The cross-referencing state.
pub(super) struct Marks<'w, 'a> {
    pub(super) world: &'w World<'a>,
    pub items: HashMap<i32, ItemMarks>,
    pub monsters: HashMap<i32, MonsterMarks>,
    pub spells: HashMap<i32, SpellMarks>,
    pub shops: HashMap<i32, ShopMarks>,
    pub text: BTreeMap<i32, TextMarks>,
    pub lairs: LairBook,
    /// Records found in the game, which the passes then follow. An item only sold or only
    /// referenced is followed without counting as in the game in its column.
    pub(super) items_seen: HashSet<i32>,
    monsters_seen: HashSet<i32>,
    pub(super) spells_seen: HashSet<i32>,
    something_new: bool,
    /// Blocks run as random results.
    pub(super) text_random: HashSet<i32>,
    /// Blocks whose text is commands (their TBInfo action is written).
    pub(super) text_commands: HashSet<i32>,
    /// Blocks reached from an NPC's topics; what they give or teach is credited to the NPC
    /// by the topic scan rather than to the block.
    pub(super) text_from_npc: HashSet<i32>,
    /// Block parts whose every reference was followed.
    text_done: HashSet<(i32, i16)>,
    /// Per generation group and index, the rooms that generate it ("Group: 1/2" and
    /// "[6-0-5][2]Group(lair): 1/3"), comma-joined.
    groups: HashMap<(i32, i32), String>,
}

/// Passes over monsters, spells, text blocks and items always made.
const PASSES: u32 = 10;
/// The pass the cross-referencing stops before, whatever is still being marked.
const PASS_LIMIT: u32 = 20;

impl<'w, 'a> Marks<'w, 'a> {
    fn new(world: &'w World<'a>) -> Self {
        Self {
            items: world
                .items
                .iter()
                .map(|i| {
                    let marks = ItemMarks {
                        in_game: false,
                        obtained_from: FromList::new(2000, 1995),
                        references: FromList::new(2000, 1995),
                    };
                    (i.number.0, marks)
                })
                .collect(),
            monsters: world
                .monsters
                .iter()
                .map(|m| {
                    let marks = MonsterMarks {
                        in_game: false,
                        summoned_by: FromList::new(20_000, 19_999),
                    };
                    (m.number.0, marks)
                })
                .collect(),
            spells: world
                .spells
                .iter()
                .map(|s| {
                    let marks = SpellMarks {
                        learnable: false,
                        learned_from: FromList::new(250, 248),
                        casted_by: FromList::new(250, 248),
                        classes: "\0".into(),
                    };
                    (i32::from(s.number.0), marks)
                })
                .collect(),
            shops: world
                .shops
                .iter()
                .map(|s| {
                    let marks = ShopMarks {
                        in_game: false,
                        assigned_to: FromList::new(250, 248),
                    };
                    (s.number.0, marks)
                })
                .collect(),
            text: BTreeMap::new(),
            lairs: LairBook::default(),
            items_seen: HashSet::new(),
            monsters_seen: HashSet::new(),
            spells_seen: HashSet::new(),
            something_new: false,
            text_random: HashSet::new(),
            text_commands: HashSet::new(),
            text_from_npc: HashSet::new(),
            text_done: HashSet::new(),
            groups: HashMap::new(),
            world,
        }
    }

    /// Cross-references the world.
    pub(super) fn locate(world: &'w World<'a>, mobs: &MonsterIndex, out: &Exclusions) -> Self {
        let mut marks = Self::new(world);
        marks.scan_rooms(mobs, out);
        marks.scan_shops();
        let mut pass = 1;
        while (pass <= PASSES || marks.something_new) && pass < PASS_LIMIT {
            marks.something_new = false;
            marks.scan_monsters();
            marks.scan_spells();
            marks.scan_text();
            marks.scan_items();
            pass += 1;
        }
        marks.scan_containers();
        marks.scan_topics();
        marks.fill_actions();
        marks
    }

    pub(super) fn mark_item(
        &mut self,
        n: i32,
        from: &str,
        sold_or_referenced: bool,
        reference: bool,
    ) {
        if n <= 0 {
            return;
        }
        self.something_new |= self.items_seen.insert(n);
        let Some(item) = self.items.get_mut(&n) else {
            return;
        };
        if !from.is_empty() {
            let list = if reference {
                &mut item.references
            } else {
                &mut item.obtained_from
            };
            if !list.add(from) {
                return;
            }
        }
        if !sold_or_referenced {
            item.in_game = true;
        }
    }

    pub(super) fn mark_monster(&mut self, n: i32, from: &str) {
        if n <= 0 {
            return;
        }
        self.something_new |= self.monsters_seen.insert(n);
        if let Some(monster) = self.monsters.get_mut(&n)
            && monster.summoned_by.add(from)
        {
            monster.in_game = true;
        }
    }

    /// Marks a spell cast (`classes` `None`) or learned from `from`; a learned spell's classes
    /// are added to the ones it has ("(*)" wins).
    pub(super) fn mark_spell(&mut self, n: i32, from: &str, learned: Option<&str>) {
        if n <= 0 {
            return;
        }
        self.something_new |= self.spells_seen.insert(n);
        let Some(spell) = self.spells.get_mut(&n) else {
            return;
        };
        match learned {
            Some(classes) if !from.is_empty() => {
                if !spell.learned_from.add(from) {
                    return;
                }
                spell.learnable = true;
                if !classes.is_empty() && spell.classes != "(*)" {
                    spell.classes =
                        if classes == "(*)" || spell.classes.is_empty() || spell.classes == "\0" {
                            classes.to_string()
                        } else {
                            merge_classes(&spell.classes, classes)
                        };
                }
            }
            _ => {
                if !from.is_empty() {
                    spell.casted_by.add(from);
                }
            }
        }
    }

    fn mark_shop(&mut self, n: i32, from: &str) {
        if let Some(shop) = self.shops.get_mut(&n)
            && shop.assigned_to.add(from)
        {
            shop.in_game = true;
        }
    }

    /// Adds a TBInfo row for block `n` (whether or not the block exists) or adds `from` to it.
    pub(super) fn mark_text(&mut self, n: i32, from: &str, random: bool) {
        if n == 0 {
            return;
        }
        let row = self.text.entry(n).or_insert_with(|| {
            self.something_new = true;
            TextMarks {
                link_to: 0,
                action: b"\0".to_vec(),
                called_from: FromList::new(250, 248),
            }
        });
        if row.called_from.add(from) && random {
            self.text_random.insert(n);
        }
    }

    /// A block something runs as commands.
    pub(super) fn mark_command(&mut self, n: i32, from: &str) {
        self.text_commands.insert(n);
        self.mark_text(n, from, false);
    }

    fn scan_rooms(&mut self, mobs: &MonsterIndex, out: &Exclusions) {
        let world = self.world;
        for r in &world.rooms {
            if out.left_out(r) {
                continue;
            }
            let (map, number) = (r.key.map.0, r.key.room.0);
            let from = format!("Room {map}/{number}");
            for p in r.placed.iter().filter(|p| p.0 > 0) {
                self.mark_item(p.0, &from, false, false);
            }
            // signs are always hidden, being fixed in place
            for h in r.hiditems.iter().filter(|h| h.item.0 > 0) {
                if world.item_type(h.item.0) == 3 {
                    self.mark_item(h.item.0, &from, false, false);
                }
            }
            if r.room_type.code() == 1 && r.shopnum.0 > 0 {
                self.mark_shop(r.shopnum.0, &from);
            }
            if r.specmon.0 > 0 {
                self.mark_monster(r.specmon.0, &from);
            }
            if r.cmdtext.0 > 0 {
                self.mark_command(r.cmdtext.0, &from);
            }
            if r.roomspell > 0 {
                self.mark_spell(r.roomspell, &from, None);
            }
            for e in r.exits.iter().filter(|e| e.target.0 != 0) {
                match e.kind.code() {
                    // ticket, item and key exits need the item
                    2 | 3 | 17 => self.mark_item(e.par1 as i32, &from, true, true),
                    // door, gate and remote-action keys
                    7 | 11 | 12 => self.mark_item(e.par4, &from, true, true),
                    22 => {
                        self.mark_spell(e.par1 as i32, &from, None);
                        self.mark_spell(e.par2.into(), &from, None);
                    }
                    24 => self.mark_spell(e.par1 as i32, &from, None),
                    _ => {}
                }
            }
            self.room_groups(mobs, r);
        }
    }

    /// Notes the generation group and indexes a room generates, and its lair.
    fn room_groups(&mut self, mobs: &MonsterIndex, r: &Room) {
        let (group, low, high) = (
            i32::from(r.mongroup),
            i32::from(r.minlevel),
            i32::from(r.maxlevel),
        );
        if !(low <= high && high > 0) {
            return;
        }
        let (map, number) = (r.key.map.0, r.key.room.0);
        let index = format!("{group}-{low}-{high}");
        let lair = r.maxmonsters > 0 && r.room_type.code() == 3;
        let mut info = self.lairs.get(&index);
        let here = format!("{map}/{number}");
        if lair {
            if info.mobs == 0 {
                info.add_mobs(self.world, mobs, group, low..=high);
            }
            info.add_lair(r.regendelay, &here);
        } else {
            info.add_other_room(&here);
        }
        self.lairs.set(&info);
        let entry = if lair {
            format!("[{index}][{}]Group(lair): {here}", r.maxmonsters)
        } else {
            format!("Group: {here}")
        };
        for i in low..=high {
            let list = self.groups.entry((group, i)).or_default();
            if !list.is_empty() {
                list.push(',');
            }
            list.push_str(&entry);
        }
    }

    /// The items the shops in the game sell; an item a shop only buys or never restocks is
    /// followed but not counted as in the game.
    fn scan_shops(&mut self) {
        let world = self.world;
        for shop in &world.shops {
            if !self.shops.get(&shop.number.0).is_some_and(|s| s.in_game) {
                continue;
            }
            for slot in shop.stock.iter().filter(|s| s.item.0 != 0) {
                let n = shop.number.0;
                if signed(slot.maxqty) == 0 {
                    self.mark_item(slot.item.0, &format!("Shop(sell) #{n}"), true, false);
                } else if slot.regenpct == 0 || slot.regennum == 0 {
                    self.mark_item(slot.item.0, &format!("Shop(nogen) #{n}"), true, false);
                } else {
                    self.mark_item(slot.item.0, &format!("Shop #{n}"), false, false);
                }
            }
        }
    }

    fn scan_monsters(&mut self) {
        let world = self.world;
        for m in &world.monsters {
            if m.name.to_ascii_lowercase().starts_with("sdf") {
                continue;
            }
            let n = m.number.0;
            let key = (i32::from(m.montype.code()), i32::from(m.genindex));
            if let Some(rooms) = self.groups.get(&key).cloned() {
                self.mark_monster(n, &rooms);
            }
            if !self.monsters_seen.contains(&n) {
                continue;
            }
            let from = format!("Monster #{n}");
            for drop in m.items.iter().filter(|d| d.item.0 != 0) {
                self.mark_item(
                    drop.item.0,
                    &format!("Monster #{n}({}%)", drop.pct),
                    false,
                    false,
                );
            }
            if m.talktext.0 != 0 {
                self.text_from_npc.insert(m.talktext.0);
                self.mark_command(m.talktext.0, &from);
            }
            if m.createspell.0 != 0 {
                self.mark_spell(m.createspell.0.into(), &from, None);
            }
            if m.deathspell.0 != 0 {
                self.mark_spell(m.deathspell.0.into(), &from, None);
            }
            for &(ability, value) in &m.abilities {
                let value = i32::from(value);
                match ability.code() {
                    12 => self.mark_monster(value, &from),
                    42 => self.mark_spell(value, &from, Some("")),
                    43 | 151 | 153 => self.mark_spell(value, &from, None),
                    148 | 155 => self.mark_command(value, &from),
                    _ => {}
                }
            }
            for (a, s) in m.attacks.iter().zip(&m.spells) {
                if a.kind.code() != 0 {
                    self.mark_spell(a.hitspell.0.into(), &from, None);
                }
                if a.kind.code() == 2 {
                    self.mark_spell(a.accuracy.into(), &from, None);
                }
                if s.spell.0 != 0 {
                    self.mark_spell(s.spell.0.into(), &from, None);
                }
            }
        }
    }

    /// What the spells in the game summon, teach, cast and run. An ability without a value
    /// takes the spell's range as the numbers (a spell that runs one of several blocks).
    fn scan_spells(&mut self) {
        let world = self.world;
        for s in &world.spells {
            let number = i32::from(s.number.0);
            if !self.spells_seen.contains(&number) {
                continue;
            }
            let from = format!("Spell #{number}");
            for &(ability, value) in &s.abilities {
                if signed(ability.code()) <= 0 {
                    continue;
                }
                let (low, high) = if value == 0 {
                    let (low, high) = (i32::from(s.minval), i32::from(s.maxval));
                    (low, high.max(low))
                } else {
                    (value.into(), value.into())
                };
                if low <= 0 || high <= 0 {
                    continue;
                }
                for n in low..=high {
                    match ability.code() {
                        12 => self.mark_monster(n, &from),
                        42 => self.mark_spell(n, &from, Some("")),
                        43 | 151 | 153 => self.mark_spell(n, &from, None),
                        148 | 155 => self.mark_command(n, &from),
                        _ => {}
                    }
                }
            }
        }
    }

    /// What the items in the game summon, teach (to the classes that may use the item), cast
    /// and run.
    fn scan_items(&mut self) {
        let world = self.world;
        for item in &world.items {
            let n = item.number.0;
            if !self.items_seen.contains(&n) {
                continue;
            }
            let mut classes: Vec<String> = Vec::new();
            for c in item.classes.iter().map(|c| signed(c.0)).filter(|&c| c > 0) {
                let class = format!("({c})");
                if !classes.contains(&class) {
                    classes.push(class);
                }
            }
            let classes = if classes.is_empty() {
                "(*)".to_string()
            } else {
                classes.join(",")
            };
            let from = format!("Item #{n}");
            for &(ability, value) in &item.abilities {
                let value = i32::from(value);
                if signed(ability.code()) <= 0 || value <= 0 {
                    continue;
                }
                match ability.code() {
                    12 => self.mark_monster(value, &from),
                    42 => self.mark_spell(value, &from, Some(&classes)),
                    43 | 151 | 153 => self.mark_spell(value, &from, None),
                    148 | 155 => self.mark_command(value, &from),
                    _ => {}
                }
            }
        }
    }

    /// Each block part once all its references were followed: a block nothing runs is not
    /// read, and a part that referred to a record not yet in the game is read again next pass.
    fn scan_text(&mut self) {
        let world = self.world;
        for part in &world.text {
            let (n, p) = (part.number, part.part);
            if self.text_done.contains(&(n, p)) {
                continue;
            }
            let Some(row) = self.text.get_mut(&n) else {
                continue;
            };
            if part.link > 0 {
                if row.link_to < 1 {
                    row.link_to = part.link;
                }
                if self.text_from_npc.contains(&n) {
                    self.text_from_npc.insert(part.link);
                }
                self.mark_text(part.link, &format!("Textblock #{n}"), false);
            }
            let data = &part.data;
            let mut done = true;
            for (look, refers) in [
                (&b"cast "[..], Refers::Spell),
                (b"learnspell ", Refers::Spell),
                (b"giveitem ", Refers::Item),
                (b"takeitem ", Refers::Item),
                (b"checkitem ", Refers::Item),
                (b"failitem ", Refers::Item),
                (b"failroomitem ", Refers::Item),
                (b"roomitem ", Refers::Item),
                (b"clearitem ", Refers::Item),
                (b"summon ", Refers::Monster),
                (b"random ", Refers::Text),
                (b"testskill ", Refers::Text),
                (b"text ", Refers::Text),
                (b"checkspell ", Refers::Text),
            ] {
                done &= self.follow(n, data, look, refers);
            }
            // a topic (":<block>") is followed but does not hold the part back
            self.follow(n, data, b":", Refers::Text);
            if done {
                self.text_done.insert((n, p));
            }
        }
    }

    /// The items the containers in the game hold: a container that casts a spell running a
    /// block of random results gives each item with its chance ("Item #5(12.5%)").
    fn scan_containers(&mut self) {
        let world = self.world;
        for item in &world.items {
            let n = item.number.0;
            if !self.items_seen.contains(&n) || item.item_type.code() != 8 {
                continue;
            }
            for &(ability, value) in &item.abilities {
                if ability.code() != 43 || value <= 0 {
                    continue;
                }
                let Some(spell) = world.spell(value.into()) else {
                    continue;
                };
                for &(spell_ability, block) in &spell.abilities {
                    if spell_ability.code() != 148 {
                        continue;
                    }
                    let block = match block {
                        0 if spell.minval > 0 => spell.minval,
                        0 => spell.maxval,
                        b => b,
                    };
                    let Some(first) = world.text_parts(block.into()).first() else {
                        continue;
                    };
                    let data = &first.data;
                    let mut chest = Drops::default();
                    let mut at = 1;
                    loop {
                        let found = vb::instr(at, data, b"random ");
                        if found == 0 {
                            break;
                        }
                        at = found + b"random ".len();
                        let digits = digits_at(data, at);
                        if digits > 0 {
                            let n = vb::whole(vb::val(vb::mid(data, at, digits)));
                            self.random_drops(&mut chest, n, &mut 0, Money::ONE, None);
                        }
                    }
                    for (drop, chance) in chest.found() {
                        self.mark_item(
                            drop,
                            &format!("Item #{n}({}%)", chance.percent_text()),
                            false,
                            false,
                        );
                    }
                }
            }
        }
    }

    /// The items and spells NPCs give or teach when asked about a topic: each topic's block
    /// links to the block run, which is read for "giveitem", "learnspell" and random results.
    fn scan_topics(&mut self) {
        let world = self.world;
        for m in &world.monsters {
            let n = m.number.0;
            if !self.monsters_seen.contains(&n) || m.talktext.0 <= 0 {
                continue;
            }
            let parts = world.text_parts(m.talktext.0);
            if parts.is_empty() {
                continue;
            }
            let data: Vec<u8> = parts.iter().flat_map(|p| p.data.iter().copied()).collect();
            let mut drops = Drops::default();
            let mut spells = Lessons::default();
            let mut at = 1;
            loop {
                let found = vb::instr(at, &data, b":");
                if found == 0 {
                    break;
                }
                at = found + 1;
                let digits = digits_at(&data, at);
                if digits == 0 {
                    continue;
                }
                let topic = vb::whole(vb::val(vb::mid(&data, at, digits)));
                if world.text_parts(topic).is_empty() {
                    continue;
                }
                let block = world.text_link(topic);
                if block > 0 {
                    self.block_drops(&mut drops, block, &mut 0, Some(&mut spells));
                }
            }
            let from = format!("NPC #{n}");
            for (drop, _) in drops.found() {
                self.mark_item(drop, &from, false, false);
            }
            for (spell, classes) in spells.found() {
                self.mark_spell(spell, &from, Some(&classes));
            }
        }
    }

    /// Puts each commands block's script in its TBInfo row, parts joined.
    fn fill_actions(&mut self) {
        let world = self.world;
        for part in &world.text {
            if !self.text_commands.contains(&part.number) {
                continue;
            }
            if let Some(row) = self.text.get_mut(&part.number) {
                if part.part == 0 {
                    row.action.clone_from(&part.data);
                } else {
                    row.action.extend_from_slice(&part.data);
                }
            }
        }
    }
}

/// Two "(1),(2)" class lists as one: the longer list, then the other's classes it lacks.
pub(super) fn merge_classes(a: &str, b: &str) -> String {
    let (a, b) = (class_numbers(a), class_numbers(b));
    let (mut first, second) = if a.len() > b.len() { (a, b) } else { (b, a) };
    for c in second {
        if !first.contains(&c) {
            first.push(c);
        }
    }
    format!("({})", first.join("),("))
}

/// The numbers of a "(1),(2)" class list, as text.
fn class_numbers(list: &str) -> Vec<String> {
    list.split(',')
        .map(|c| vb::num(vb::val(c.replace(['(', ')'], "").as_bytes())))
        .collect()
}
