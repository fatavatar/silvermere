//! The tables that hold one record each: Info, Classes, Races, Items, Spells, Shops, Monsters.
//!
//! Numbers are written as NMR reads them from the record: a 16-bit field signed (a class
//! restriction of 65535 is -1), a byte field as its low byte (a spell's level cap), and a
//! monster's experience and level unsigned.

use std::collections::HashMap;

use silvermere_types::records::{AbilitySlot, KnownMonster};

use super::sim::Damage;
use super::vb::whole;
use super::world::{World, ansi, signed};
use super::xref::Marks;
use super::{ExportKind, Options};
use crate::table::{Row, Value};

/// Columns with a running number: `"Abil-"` and 3 is `"Abil-3"`.
fn col(prefix: &str, i: usize) -> String {
    format!("{prefix}{i}")
}

fn long(row: &mut Row, name: &str, v: impl Into<i32>) {
    row.insert(name.into(), Value::Long(v.into()));
}

fn text(row: &mut Row, name: &str, v: impl Into<String>) {
    row.insert(name.into(), Value::Text(v.into()));
}

fn flag(row: &mut Row, name: &str, v: bool) {
    long(row, name, i32::from(v));
}

fn abilities(row: &mut Row, slots: &[AbilitySlot]) {
    for (i, &(ability, value)) in slots.iter().enumerate() {
        long(row, &col("Abil-", i), signed(ability.code()));
        long(row, &col("AbilVal-", i), value);
    }
}

/// The Info table's row.
pub(super) fn info(options: &Options) -> Row {
    let mut row = Row::new();
    text(&mut row, "NMR Version", options.nmr_version.as_str());
    text(&mut row, "Dat File Version", options.dat_version.as_str());
    text(&mut row, "Date", options.date.as_str());
    text(&mut row, "Time", options.time.as_str());
    let (legit, url, custom) = match options.kind {
        ExportKind::Legit => (1, "http://www.mudinfo.net/mmudexp.php", "Default"),
        ExportKind::GreaterMud => (2, options.update_url.as_str(), options.custom.as_str()),
        ExportKind::Custom => (0, options.update_url.as_str(), options.custom.as_str()),
    };
    long(&mut row, "Legit", legit);
    flag(&mut row, "DisableKai", options.disable_kai);
    text(&mut row, "UpdateURL", url);
    text(&mut row, "Custom", custom);
    row
}

pub(super) fn classes(world: &World) -> Vec<Row> {
    world
        .classes
        .iter()
        .map(|c| {
            let mut row = Row::new();
            long(&mut row, "Number", signed(c.number.0));
            text(&mut row, "Name", ansi(&c.name));
            long(&mut row, "MinHits", c.basehp);
            long(&mut row, "MaxHits", c.hplevel);
            long(&mut row, "ExpTable", c.exp);
            long(&mut row, "MageryType", signed(c.magery.code()));
            long(&mut row, "MageryLVL", c.magelvl);
            long(&mut row, "WeaponType", signed(c.weapon.code()));
            long(&mut row, "ArmourType", signed(c.armour.code()));
            long(&mut row, "CombatLVL", c.combat);
            abilities(&mut row, &c.abilities);
            row
        })
        .collect()
}

pub(super) fn races(world: &World) -> Vec<Row> {
    world
        .races
        .iter()
        .map(|r| {
            let mut row = Row::new();
            long(&mut row, "Number", signed(r.number.0));
            text(&mut row, "Name", ansi(&r.name));
            for (name, v) in [
                ("mINT", r.minint),
                ("mWIL", r.minwil),
                ("mSTR", r.minstr),
                ("mHEA", r.minhea),
                ("mAGL", r.minagi),
                ("mCHM", r.mincha),
                ("xINT", r.maxint),
                ("xWIL", r.maxwil),
                ("xSTR", r.maxstr),
                ("xHEA", r.maxhea),
                ("xAGL", r.maxagi),
                ("xCHM", r.maxcha),
                ("HPPerLVL", r.hpbonus),
                ("ExpTable", r.exp),
                ("BaseCP", r.basecp),
            ] {
                long(&mut row, name, v);
            }
            abilities(&mut row, &r.abilities);
            row
        })
        .collect()
}

pub(super) fn items(world: &World, marks: &Marks) -> Vec<Row> {
    world
        .items
        .iter()
        .map(|i| {
            let mut row = Row::new();
            long(&mut row, "Number", i.number.0);
            text(&mut row, "Name", ansi(&i.name));
            long(&mut row, "Limit", signed(i.limit));
            long(&mut row, "Encum", i.weight);
            long(&mut row, "ItemType", signed(i.item_type.code()));
            long(&mut row, "UseCount", i.charges);
            long(&mut row, "Price", signed(i.cost));
            long(&mut row, "Currency", i32::from(i.currency.code() & 0xFF));
            long(&mut row, "Min", i.mindmg);
            long(&mut row, "Max", i.maxdmg);
            long(&mut row, "ArmourClass", i.ac);
            long(&mut row, "DamageResist", i.dr);
            long(&mut row, "WeaponType", signed(i.wpntype.code()));
            long(&mut row, "ArmourType", signed(i.armtype.code()));
            long(&mut row, "Worn", signed(i.worn.code()));
            long(&mut row, "Accy", i.accuracy);
            long(&mut row, "Gettable", i.gettable & 0xFF);
            long(&mut row, "StrReq", i.strreq);
            long(&mut row, "Speed", i.speed);
            long(&mut row, "Not Droppable", i.nodrop);
            long(&mut row, "Destroy On Death", i.destroy);
            long(&mut row, "Retain After Uses", i.retain);
            for x in 0..10 {
                long(&mut row, &col("ClassRest-", x), signed(i.classes[x].0));
                long(&mut row, &col("RaceRest-", x), signed(i.races[x].0));
                long(&mut row, &col("NegateSpell-", x), i.negates[x].0);
            }
            abilities(&mut row, &i.abilities);
            let m = &marks.items[&i.number.0];
            flag(&mut row, "In Game", m.in_game);
            text(&mut row, "Obtained From", m.obtained_from.text());
            text(&mut row, "References", m.references.text());
            row
        })
        .collect()
}

pub(super) fn spells(world: &World, marks: &Marks) -> Vec<Row> {
    world
        .spells
        .iter()
        .map(|s| {
            let mut row = Row::new();
            long(&mut row, "Number", s.number.0);
            text(&mut row, "Name", ansi(&s.name));
            text(&mut row, "Short", ansi(&s.shortnm));
            long(&mut row, "ReqLevel", signed(s.level));
            long(&mut row, "EnergyCost", signed(s.energy));
            long(&mut row, "ManaCost", s.mana);
            long(&mut row, "MinBase", s.minval);
            long(&mut row, "MaxBase", s.maxval);
            long(&mut row, "Diff", s.diff);
            long(&mut row, "TypeOfResists", signed(s.resmode.code()));
            long(&mut row, "Targets", signed(s.target.code()));
            long(&mut row, "Dur", s.duration);
            long(&mut row, "AttType", signed(s.element.code()));
            long(&mut row, "Magery", signed(s.magery.code()));
            long(&mut row, "MageryLVL", s.magelvl);
            long(&mut row, "Cap", s.levelcap & 0xFF);
            long(&mut row, "MaxIncLVLs", s.maxlvls);
            long(&mut row, "MaxInc", s.maxinc);
            long(&mut row, "MinIncLVLs", s.minlvls);
            long(&mut row, "MinInc", s.mininc);
            long(&mut row, "DurIncLVLs", s.durlvls);
            long(&mut row, "DurInc", s.durinc);
            abilities(&mut row, &s.abilities);
            let m = &marks.spells[&i32::from(s.number.0)];
            flag(&mut row, "Learnable", m.learnable);
            text(&mut row, "Learned From", m.learned_from.text());
            text(&mut row, "Casted By", m.casted_by.text());
            text(&mut row, "Classes", m.classes.as_str());
            row
        })
        .collect()
}

pub(super) fn shops(world: &World, marks: &Marks) -> Vec<Row> {
    world
        .shops
        .iter()
        .map(|s| {
            let mut row = Row::new();
            long(&mut row, "Number", s.number.0);
            text(&mut row, "Name", ansi(&s.name));
            long(&mut row, "ShopType", signed(s.shop_type.code()));
            long(&mut row, "MinLVL", signed(s.minlevel));
            long(&mut row, "MaxLVL", signed(s.maxlevel));
            long(&mut row, "Markup%", signed(s.markup));
            long(&mut row, "ClassRest", i32::from(s.trainclass.0 & 0xFF));
            // a gang shop's slots hold its owner and asking price, not stock
            let gang = s.shop_type.code() == 11;
            for (x, slot) in s.stock.iter().enumerate() {
                let values = if gang {
                    [0; 5]
                } else {
                    [
                        slot.item.0,
                        signed(slot.maxqty).into(),
                        signed(slot.regentime).into(),
                        slot.regennum.into(),
                        slot.regenpct.into(),
                    ]
                };
                for (prefix, v) in ["Item-", "Max-", "Time-", "Amount-", "%-"]
                    .iter()
                    .zip(values)
                {
                    long(&mut row, &col(prefix, x), v);
                }
            }
            let m = &marks.shops[&s.number.0];
            flag(&mut row, "In Game", m.in_game);
            text(&mut row, "Assigned To", m.assigned_to.text());
            row
        })
        .collect()
}

/// The exported columns of a monster that its lairs' averages are taken from.
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MonsterStats {
    pub damage: i32,
    pub phys: i32,
    pub spell: i32,
    pub hp: i32,
    pub ac: i32,
    pub dr: i32,
    pub mr: i32,
    pub dodge: i32,
}

impl MonsterStats {
    pub(super) fn of(m: &KnownMonster, damage: &Damage) -> Self {
        Self {
            damage: whole(damage.total),
            phys: whole(damage.phys),
            spell: whole(damage.spell),
            hp: m.maxhp.into(),
            ac: m.ac.into(),
            dr: m.dr.into(),
            mr: m.magicres.into(),
            // the last dodge slot counts
            dodge: m
                .abilities
                .iter()
                .rev()
                .find(|(a, _)| a.code() == 34)
                .map_or(0, |&(_, v)| v.into()),
        }
    }
}

/// A monster's row; `lair` is its AvgLairExp and ScriptValue.
pub(super) fn monster(
    world: &World,
    marks: &Marks,
    m: &KnownMonster,
    damage: &Damage,
    lair: Option<(f64, f64)>,
) -> Row {
    let mut row = Row::new();
    long(&mut row, "Number", m.number.0);
    text(&mut row, "Name", ansi(&m.name));
    long(&mut row, "Weapon", m.weapon.0);
    long(&mut row, "ArmourClass", m.ac);
    long(&mut row, "DamageResist", m.dr);
    long(&mut row, "Follow%", m.followpct);
    long(&mut row, "MagicRes", m.magicres);
    long(&mut row, "BSDefense", m.bsdefense);
    row.insert("EXP".into(), Value::Double(f64::from(m.exp as u32)));
    row.insert("ExpMulti".into(), Value::Double(f64::from(m.level)));
    long(&mut row, "HP", m.maxhp);
    long(&mut row, "Energy", signed(m.energy));
    row.insert("AvgDmg".into(), Value::Double(damage.total));
    row.insert("AvgDmgPhys".into(), Value::Double(damage.phys));
    row.insert("AvgDmgSpell".into(), Value::Double(damage.spell));
    long(&mut row, "GreetTXT", m.talktext.0);
    long(&mut row, "HPRegen", m.hpregen);
    long(&mut row, "CharmLVL", signed(m.charmlvl));
    long(&mut row, "Type", signed(m.grouprole.code()));
    long(&mut row, "Undead", m.undead);
    long(&mut row, "Align", signed(m.alignment.code()));
    long(&mut row, "RegenTime", m.regenhrs);
    long(&mut row, "GameLimit", signed(m.maxalive));
    let (lair_exp, script_value) = lair.unwrap_or_default();
    row.insert("AvgLairExp".into(), Value::Double(lair_exp));
    row.insert("ScriptValue".into(), Value::Double(script_value));
    let cash = &m.cash;
    for (name, v) in [
        ("R", cash.runic),
        ("P", cash.platinum),
        ("G", cash.gold),
        ("S", cash.silver),
        ("C", cash.copper),
    ] {
        long(&mut row, name, v as i32);
    }
    long(&mut row, "DeathSpell", m.deathspell.0);
    long(&mut row, "CreateSpell", m.createspell.0);
    for (x, (a, s)) in m.attacks.iter().zip(&m.spells).enumerate() {
        text(&mut row, &col("AttName-", x), attack_name(world, m, x));
        long(
            &mut row,
            &col("AttType-", x),
            i32::from(a.kind.code() & 0xFF),
        );
        long(&mut row, &col("AttAcc-", x), a.accuracy);
        long(&mut row, &col("Att%-", x), a.pct);
        row.insert(col("AttTrue%-", x), Value::Double(damage.share[x]));
        long(&mut row, &col("AttMin-", x), a.min);
        long(&mut row, &col("AttMax-", x), a.max);
        long(&mut row, &col("AttEnergy-", x), a.energy);
        long(&mut row, &col("AttHitSpell-", x), a.hitspell.0);
        long(&mut row, &col("MidSpell-", x), s.spell.0);
        long(&mut row, &col("MidSpell%-", x), s.pct);
        long(&mut row, &col("MidSpellLVL-", x), s.level);
    }
    for (x, drop) in m.items.iter().enumerate() {
        long(&mut row, &col("DropItem-", x), drop.item.0);
        long(&mut row, &col("DropItem%-", x), drop.pct);
    }
    abilities(&mut row, &m.abilities);
    let marks = &marks.monsters[&m.number.0];
    flag(&mut row, "In Game", marks.in_game);
    text(&mut row, "Summoned By", marks.summoned_by.text());
    row
}

/// An attack's name as MME lists it: a physical attack's verb from its hit message ("%s bites
/// you for ..." gives "bites you"), a spell attack's spell name.
fn attack_name(world: &World, m: &KnownMonster, x: usize) -> String {
    let a = &m.attacks[x];
    let name = match a.kind.code() {
        0 => "None".to_string(),
        1 => {
            let message = world.message(a.hitmsg.0, 1);
            verb(&message).unwrap_or_else(|| "Physical".into())
        }
        2 => match a.accuracy {
            0 => "none".to_string(),
            n => world
                .spell(n.into())
                .map_or_else(|| format!("Spell {n}"), |s| ansi(&s.name)),
        },
        _ => "Unknown".to_string(),
    };
    if name.chars().count() > 49 {
        format!("{}...", name.chars().take(46).collect::<String>())
    } else {
        name
    }
}

/// The words after "%s " up to " for " (or the next space), "all-out" taking the word after it.
fn verb(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();
    let start = lower.find("%s ")? + 3;
    let end = lower[start..]
        .find(" for ")
        .or_else(|| lower[start..].find(' '))?
        + start;
    if end <= start {
        return None;
    }
    let word = &message[start..end];
    if word == "all-out"
        && let Some(next) = lower
            .get(start + "all-out".len() + 1..)
            .and_then(|r| r.find(' '))
    {
        let end = start + "all-out".len() + 1 + next;
        return Some(message[start..end].to_string());
    }
    Some(word.to_string())
}

/// The stats each monster's lairs average.
pub(super) fn monster_stats(
    world: &World,
    damage: &HashMap<i32, Damage>,
) -> HashMap<i32, MonsterStats> {
    world
        .monsters
        .iter()
        .map(|m| (m.number.0, MonsterStats::of(m, &damage[&m.number.0])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_verb_comes_from_the_hit_message() {
        assert_eq!(
            verb("%s bites you for %d damage!").as_deref(),
            Some("bites you")
        );
        assert_eq!(verb("%s claws at you!").as_deref(), Some("claws"));
        assert_eq!(
            verb("%s all-out attacks you!").as_deref(),
            Some("all-out attacks")
        );
        assert_eq!(
            verb("%s all-out attacks you for %d!").as_deref(),
            Some("all-out attacks you")
        );
        assert_eq!(verb("You are hit."), None);
    }
}
