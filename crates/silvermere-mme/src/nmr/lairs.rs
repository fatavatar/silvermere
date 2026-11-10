//! Lairs: each generation group and index range lairs draw from, its monsters' averages, and
//! the per-monster lair experience and script value MME ranks hunting spots by.
//!
//! A lair is a lair room (room type 3 generating at most N monsters) of a group and an index
//! range; the averages are per monster over the range's monsters that regenerate (regeneration
//! time 0). A monster's script value weighs the experience of the lairs it appears in against
//! the damage taken and hit points cut through to clear them, and is scaled down when the
//! monster has too few lairs to hunt in a regeneration period.

use std::collections::HashMap;
use std::ops::RangeInclusive;

use super::records::MonsterStats;
use super::rooms::MonsterIndex;
use super::vb::{self, money, round, whole};
use super::world::World;
use crate::table::{Row, Value};

/// One group and index range ("6-0-5").
#[derive(Debug, Clone, Default)]
pub(super) struct LairInfo {
    pub index: String,
    /// The regenerating monsters, "1,2,3".
    mob_list: String,
    pub mobs: i32,
    /// Averages per monster.
    avg_exp: f64,
    avg_dmg: f64,
    avg_phys: f64,
    avg_spell: f64,
    avg_hp: i32,
    avg_ac: i32,
    avg_dr: i32,
    avg_mr: i32,
    avg_dodge: i32,
    /// The script value last worked out for this range (with some "most at once"); see
    /// [`LairBook::get`].
    script_value: f64,
    /// The sum of the lairs' delays until averaged.
    avg_delay: i32,
    total_lairs: i32,
    /// ",1/2,1/3," and the same for the range's rooms that are not lairs.
    lair_rooms: String,
    other_rooms: String,
}

impl LairInfo {
    /// Takes the range's regenerating monsters (the first lair seen does this).
    pub(super) fn add_mobs(
        &mut self,
        world: &World,
        index: &MonsterIndex,
        group: i32,
        range: RangeInclusive<i32>,
    ) {
        for i in range {
            for &n in index.get(group, i) {
                if world.monster_regen(n) == 0 {
                    self.mob_list.push_str(&format!("{n},"));
                    self.avg_exp = money(self.avg_exp + world.monster_exp(n));
                    self.mobs += 1;
                }
            }
        }
        if self.mobs > 0 {
            self.mob_list.pop();
            self.avg_exp = round(self.avg_exp / f64::from(self.mobs), 0);
        }
    }

    pub(super) fn add_lair(&mut self, delay: i16, room: &str) {
        self.avg_delay += if delay == 0 { 5 } else { i32::from(delay) };
        self.total_lairs += 1;
        add_room(&mut self.lair_rooms, room);
    }

    pub(super) fn add_other_room(&mut self, room: &str) {
        add_room(&mut self.other_rooms, room);
    }
}

fn add_room(list: &mut String, room: &str) {
    if list.is_empty() {
        *list = format!(",{room},");
    } else if !list.contains(&format!(",{room},")) {
        list.push_str(&format!("{room},"));
    }
}

/// The ranges in the order first seen.
#[derive(Default)]
pub(super) struct LairBook {
    lairs: Vec<LairInfo>,
    by_index: HashMap<String, usize>,
}

impl LairBook {
    /// "6-0-5" (or "6-0-5-3" with most at once) to its range's entry, made when new.
    fn slot(&mut self, key: &str) -> Option<(usize, i32)> {
        if key.len() < 5 {
            return None;
        }
        let parts: Vec<&str> = key.split('-').collect();
        let (index, max) = if parts.len() > 3 {
            (parts[..3].join("-"), whole(vb::val(parts[3].as_bytes())))
        } else {
            (key.to_string(), 0)
        };
        let next = self.lairs.len();
        let slot = *self.by_index.entry(index.clone()).or_insert(next);
        if slot == next {
            self.lairs.push(LairInfo {
                index,
                ..LairInfo::default()
            });
        }
        Some((slot, max))
    }

    /// The range's entry. With a "most at once" the entry's script value is then worked out
    /// for it, and without one cleared; what is returned is the value from before, so a
    /// monster's script value uses the one worked out for the lair seen before it.
    pub(super) fn get(&mut self, key: &str) -> LairInfo {
        let Some((slot, max)) = self.slot(key) else {
            return LairInfo::default();
        };
        let info = self.lairs[slot].clone();
        let lair = &mut self.lairs[slot];
        lair.script_value = if max > 0 && lair.avg_exp > 0.0 {
            let m = f64::from(max);
            if f64::from(lair.avg_hp) + lair.avg_dmg <= 0.0 {
                money(lair.avg_exp * m * 100.0)
            } else {
                let cost = f64::from(lair.avg_hp) * m + lair.avg_dmg * 2.0 * (m * (m + 1.0) / 2.0);
                money(round(lair.avg_exp * m / cost * 100.0, 0))
            }
        } else {
            0.0
        };
        info
    }

    /// Stores what the room scan found.
    pub(super) fn set(&mut self, info: &LairInfo) {
        let Some((slot, _)) = self.slot(&info.index) else {
            return;
        };
        let script_value = self.lairs[slot].script_value;
        self.lairs[slot] = LairInfo {
            script_value,
            ..info.clone()
        };
    }

    /// Works out each range's averages from its monsters' exported columns.
    pub(super) fn average(&mut self, stats: &HashMap<i32, MonsterStats>) {
        for lair in self.lairs.iter_mut().filter(|l| l.mobs > 0) {
            let mut sum = [0.0; 8];
            for n in lair
                .mob_list
                .split(',')
                .map(|n| whole(vb::val(n.as_bytes())))
            {
                let Some(s) = (n > 0).then(|| stats.get(&n)).flatten() else {
                    continue;
                };
                let values = [s.damage, s.phys, s.spell, s.hp, s.ac, s.dr, s.mr, s.dodge];
                for (total, v) in sum.iter_mut().zip(values) {
                    *total = money(*total + f64::from(v));
                }
            }
            let mobs = f64::from(lair.mobs);
            let avg = |v: f64| round(v / mobs, 0);
            lair.avg_dmg = avg(sum[0]);
            lair.avg_phys = avg(sum[1]);
            lair.avg_spell = avg(sum[2]);
            lair.avg_hp = avg(sum[3]) as i32;
            lair.avg_ac = avg(sum[4]) as i32;
            lair.avg_dr = avg(sum[5]) as i32;
            lair.avg_mr = avg(sum[6]) as i32;
            lair.avg_dodge = avg(sum[7]) as i32;
            if lair.total_lairs > 0 && lair.avg_delay > 0 {
                lair.avg_delay = whole(f64::from(lair.avg_delay) / f64::from(lair.total_lairs));
            }
        }
    }

    /// A monster's AvgLairExp and ScriptValue from the lairs its Summoned By lists, when it
    /// regenerates and has any.
    pub(super) fn monster_value(&mut self, summoned_by: &str) -> Option<(f64, f64)> {
        let lairs = lair_entries(summoned_by);
        if lairs.is_empty() {
            return None;
        }
        let count = lairs.len() as f64;
        let (mut exp, mut value, mut mobs) = (0.0, 0.0, 0);
        for (index, max) in &lairs {
            if *max <= 0 {
                continue;
            }
            if index.len() > 4 {
                let info = self.get(&format!("{index}-{max}"));
                exp = money(exp + info.avg_exp * f64::from(*max));
                value = money(value + info.script_value);
            }
            mobs += max;
        }
        value = money(round(value / count, 0));
        // lairs a hunter could clear in a regeneration period, fewer the more each holds
        let per_lair = whole(round(f64::from(mobs) / count, 1));
        let mut lairs_per_period = 36.0;
        if per_lair > 1 {
            lairs_per_period = money(round(36.0 / f64::from(per_lair) + 1.0, 2));
        }
        if count < lairs_per_period {
            let share = money(round(count / lairs_per_period, 2));
            value = money(round(value * share, 0));
        }
        (value >= 1.0 || exp > 0.0).then(|| (round(exp / count, 0), value))
    }

    /// The rows of the Lairs table.
    pub(super) fn rows(&self, world: &World) -> Vec<Row> {
        self.lairs
            .iter()
            .filter(|l| l.mobs > 0 && l.index.split('-').count() == 3)
            .map(|l| self.row(world, l))
            .collect()
    }

    fn row(&self, world: &World, l: &LairInfo) -> Row {
        let group = whole(vb::val(l.index.split('-').next().unwrap_or("").as_bytes()));
        let others = if l.other_rooms.is_empty() {
            0
        } else {
            l.other_rooms.trim_matches(',').split(',').count()
        };
        let rooms_per_lair = if l.total_lairs > 0 && others > 0 {
            round(
                (others as f64 + f64::from(l.total_lairs)) / f64::from(l.total_lairs),
                2,
            )
        } else {
            0.0
        };
        let lairs: Vec<(u32, u32)> = l
            .lair_rooms
            .trim_matches(',')
            .split(',')
            .filter_map(map_room)
            .collect();
        let mut walk = 0.0;
        if lairs.len() > 1 {
            let (mut total, mut count) = (0, 0);
            for &start in &lairs {
                let mut seen = vec![start];
                let (steps, found) = walk_to_lairs(world, start, group, &mut seen, 1);
                if found > 0 {
                    total = whole(f64::from(total) + f64::from(steps) / f64::from(found));
                    count += 1;
                }
            }
            if count > 0 {
                walk = round(f64::from(total) / f64::from(count), 2);
            }
        }
        let avg_walk = if rooms_per_lair > 0.0 && rooms_per_lair < walk {
            rooms_per_lair
        } else if walk > 0.0 {
            walk
        } else {
            rooms_per_lair
        };
        let mut row = Row::new();
        row.insert("GroupIndex".into(), l.index.clone().into());
        row.insert("TotalLairs".into(), Value::Long(l.total_lairs));
        row.insert("MobList".into(), l.mob_list.clone().into());
        row.insert("Mobs".into(), Value::Long(l.mobs));
        row.insert("AvgDelay".into(), Value::Long(l.avg_delay));
        row.insert("AvgExp".into(), Value::Double(l.avg_exp));
        row.insert("AvgDmgPhys".into(), Value::Double(l.avg_phys));
        row.insert("AvgDmgSpell".into(), Value::Double(l.avg_spell));
        row.insert("AvgDmg".into(), Value::Double(l.avg_dmg));
        row.insert("AvgHP".into(), Value::Double(l.avg_hp.into()));
        row.insert("AvgAC".into(), Value::Long(l.avg_ac));
        row.insert("AvgDR".into(), Value::Long(l.avg_dr));
        row.insert("AvgMR".into(), Value::Long(l.avg_mr));
        row.insert("AvgDodge".into(), Value::Long(l.avg_dodge));
        row.insert("AvgWalk".into(), Value::Double(avg_walk));
        row
    }
}

/// "1/2" to map and room.
fn map_room(text: &str) -> Option<(u32, u32)> {
    let (map, room) = text.split_once('/')?;
    let (map, room) = (map.trim().parse().ok()?, room.trim().parse().ok()?);
    (map > 0 && room > 0).then_some((map, room))
}

/// The walk from `start` to the other lairs, depth first through the rooms of the lair's
/// group: (sum of the steps to each lair reached, lairs reached). Rooms already walked through
/// are not entered again; map changes and remote actions are not taken.
fn walk_to_lairs(
    world: &World,
    start: (u32, u32),
    group: i32,
    seen: &mut Vec<(u32, u32)>,
    depth: i32,
) -> (i32, i32) {
    if depth > 99 {
        return (0, 0);
    }
    let Some(room) = world.room(start.0.into(), start.1.into()) else {
        return (0, 0);
    };
    if group > 0 && i32::from(room.mongroup) != group {
        return (0, 0);
    }
    let (mut steps, mut found) = (0, 0);
    let mut queue = Vec::new();
    for e in &room.exits {
        let kind = e.kind.code();
        if e.target.0 == 0 || kind == 12 || kind == 8 || (kind == 6 && e.par1 == 0) {
            continue;
        }
        let next = (start.0, e.target.0);
        if seen.contains(&next) {
            continue;
        }
        if is_lair(world, next) {
            steps += depth;
            found += 1;
        } else {
            queue.push(next);
        }
        seen.push(next);
    }
    for next in queue {
        let (s, f) = walk_to_lairs(world, next, group, seen, depth + 1);
        steps += s;
        found += f;
    }
    (steps, found)
}

fn is_lair(world: &World, (map, room): (u32, u32)) -> bool {
    world.room(map.into(), room.into()).is_some_and(|r| {
        r.minlevel <= r.maxlevel && r.maxlevel > 0 && r.maxmonsters > 0 && r.room_type.code() == 3
    })
}

/// The lairs a Summoned By lists: each "[6-0-5][3]Group(lair): 1/2" as ("6-0-5", 3).
fn lair_entries(summoned_by: &str) -> Vec<(String, i32)> {
    let mut found = Vec::new();
    let mut rest = summoned_by;
    while let Some(open) = rest.find('[') {
        rest = &rest[open..];
        match lair_entry(rest) {
            Some((entry, len)) => {
                found.push(entry);
                rest = &rest[len..];
            }
            None => rest = &rest[1..],
        }
    }
    found
}

/// One "[6-0-5][3]Group(lair): 1/2" at the start of `text`, and its length.
fn lair_entry(text: &str) -> Option<((String, i32), usize)> {
    let index_len = text[1..]
        .bytes()
        .take_while(|b| b.is_ascii_digit() || *b == b'-')
        .count();
    let after_index = text.get(1 + index_len..)?.strip_prefix("][")?;
    let max_len = after_index.bytes().take_while(u8::is_ascii_digit).count();
    let after_max = after_index.get(max_len..)?.strip_prefix("]Group(lair): ")?;
    let map_len = after_max.bytes().take_while(u8::is_ascii_digit).count();
    let after_map = after_max.get(map_len..)?.strip_prefix('/')?;
    let room_len = after_map.bytes().take_while(u8::is_ascii_digit).count();
    if index_len == 0 || max_len == 0 || map_len == 0 || room_len == 0 {
        return None;
    }
    let len = text.len() - after_map.len() + room_len;
    let max = whole(vb::val(&after_index.as_bytes()[..max_len]));
    Some(((text[1..=index_len].to_string(), max), len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lair_entries_are_read_from_summoned_by() {
        let text =
            "Group: 1/547,[6-0-5][1]Group(lair): 1/552,Group: 1/553,[6-1-1][3]Group(lair): 1/619";
        assert_eq!(
            lair_entries(text),
            vec![("6-0-5".to_string(), 1), ("6-1-1".to_string(), 3)]
        );
        assert!(lair_entries("[6-0-5][1]Group: 1/2").is_empty());
    }

    #[test]
    fn the_script_value_returned_is_the_one_from_before() {
        let mut book = LairBook::default();
        let mut info = book.get("6-0-5");
        info.mobs = 1;
        book.set(&info);
        book.lairs[0].avg_exp = 100.0;
        book.lairs[0].avg_hp = 10;
        book.lairs[0].avg_dmg = 5.0;
        assert_eq!(book.get("6-0-5-1").script_value, 0.0);
        // 100 / (10 + 5 * 2 * 1) * 100
        assert_eq!(book.get("6-0-5-2").script_value, 500.0);
    }
}
