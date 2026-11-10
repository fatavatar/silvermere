//! Message triggers: MegaMUD's `Messages.md`, made from the realm's own records.
//!
//! MegaMUD (and Majordomo after it) keeps a table of the game's sentences that matter: the line
//! that says a spell took hold or a condition began, the line that says it ended, and what the
//! condition is (blind, poisoned, held, ...). Players built theirs by hand, or used one shipped
//! for the stock game; a realm whose spells say other things matched nothing. Every one of those
//! sentences is in the realm's records, so the table can be made from them:
//!
//! - a spell's *descriptive message* (the `DescMsg` ability) begins a lasting effect with its
//!   third line ("You feel perceptive!") and ends it with its first ("The effects of alertness
//!   wear off!"): one row, its condition read from the spell's abilities;
//! - a spell's own message and its *starting message* (`StartMsg`) are the caster's lines when a
//!   player casts it, and their second lines what a monster's target reads: one row each, the
//!   game's `%s` and `%d` written as Majordomo's `{target}`, `{source}` and `{dmg}`.
//!
//! The rows go into the realm's MME database as the table [`TABLE`] (MME does not read it;
//! Majordomo does), with the columns of a `Messages.md` row.

use std::collections::{BTreeMap, HashSet};

use silvermere_types::enums::{Ability, Magery, MonsterAttackType, SpellTarget};
use silvermere_types::flags::SpellFlags;
use silvermere_types::records::{Message, Spell};

use crate::nmr::Content;
use crate::table::{Row, Value};

/// The table's name in the database.
pub const TABLE: &str = "Messages";

/// The table's columns, in the order of the table its definition is copied from (MME's `Info`:
/// five text columns, two numbers, one text).
pub const COLUMNS: [&str; 8] = [
    "Name",
    "Message",
    "EndMessage",
    "Response",
    "Source",
    "Flags",
    "Action",
    "Kind",
];

/// The table whose definition [`TABLE`] copies.
pub const LIKE: &str = "Info";

/// MegaMUD's effect bits (Majordomo `MEGAMUD_EFFECT_BITS`).
pub mod effect {
    pub const BLIND: i32 = 0x0001;
    pub const CONFUSED: i32 = 0x0002;
    pub const POISONED: i32 = 0x0004;
    pub const LOSING_HP: i32 = 0x0008;
    pub const HELD: i32 = 0x0010;
    pub const HP_REGEN: i32 = 0x0080;
    pub const MANA_REGEN: i32 = 0x0200;
}

/// The rows of [`TABLE`] for the realm's spells, in spell number order.
pub fn message_triggers(content: &Content) -> Vec<Row> {
    let messages: BTreeMap<i32, &Message> =
        content.messages.iter().map(|m| (m.number.0, m)).collect();
    let text = |n: i32, line: usize| -> String {
        messages
            .get(&n)
            .map(|m| clean(&m.msgs[line]))
            .unwrap_or_default()
    };
    let mut spells: Vec<&Spell> = content.spells.iter().collect();
    spells.sort_by_key(|s| s.number.0);

    // the spells monsters cast: in attacks, on hits, and between rounds
    let mut monster_spells: HashSet<i32> = HashSet::new();
    for m in &content.monsters {
        for a in &m.attacks {
            if a.kind == MonsterAttackType::Spell {
                monster_spells.insert(i32::from(a.accuracy));
            }
            if a.hitspell.0 > 0 {
                monster_spells.insert(i32::from(a.hitspell.0));
            }
        }
        for s in &m.spells {
            if s.spell.0 > 0 {
                monster_spells.insert(i32::from(s.spell.0));
            }
        }
    }

    let mut rows = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut push = |rows: &mut Vec<Row>,
                    spell: &Spell,
                    kind: &str,
                    message: String,
                    end: String,
                    flags: i32| {
        let name = spell.name.trim().to_owned();
        if message.is_empty()
            || name.is_empty()
            || !seen.insert((name.to_lowercase(), message.clone()))
        {
            return;
        }
        let mut row = Row::new();
        row.insert("Name".into(), Value::Text(name));
        row.insert("Message".into(), Value::Text(message));
        row.insert("EndMessage".into(), Value::Text(end));
        row.insert("Response".into(), Value::Text(String::new()));
        row.insert(
            "Source".into(),
            Value::Text(format!("Spell #{}", spell.number.0)),
        );
        row.insert("Flags".into(), Value::Long(flags));
        row.insert("Action".into(), Value::Long(0));
        row.insert("Kind".into(), Value::Text(kind.to_owned()));
        rows.push(row);
    };

    for spell in spells {
        let ability = |a: Ability| -> Option<i16> {
            spell
                .abilities
                .iter()
                .find(|(x, _)| *x == a)
                .map(|(_, v)| *v)
        };
        // the condition: begins with the third line, ends with the first
        if let Some(n) = ability(Ability::DescMsg).filter(|n| *n > 0) {
            let n = i32::from(n);
            push(
                &mut rows,
                spell,
                "effect",
                text(n, 2),
                text(n, 0),
                effects(spell),
            );
        }
        // a spell a player casts: the caster's lines (an area spell names no target)
        let target = if names_no_target(spell.target) {
            ""
        } else {
            "{target}"
        };
        if player_cast(spell, &monster_spells) {
            if let Some(n) = ability(Ability::StartMsg).filter(|n| *n > 0) {
                push(
                    &mut rows,
                    spell,
                    "start",
                    fill(&text(i32::from(n), 0), &[target]),
                    String::new(),
                    0,
                );
            }
            if spell.msgnum.0 > 0 {
                let caster = text(spell.msgnum.0, 0);
                let filled = if spell.flags.contains(SpellFlags::ALTMSG) {
                    fill(&caster, &[target, "{dmg}"])
                } else {
                    fill(&caster, &[&spell.name, target, "{dmg}"])
                };
                push(&mut rows, spell, "cast", filled, String::new(), 0);
            }
        }
        // a spell a monster casts: what its target reads (silvermere-engine
        // `monsters::spell_support::monster_spell_texts`)
        if monster_spells.contains(&i32::from(spell.number.0)) {
            if let Some(n) = ability(Ability::StartMsg).filter(|n| *n > 0) {
                push(
                    &mut rows,
                    spell,
                    "hit",
                    fill(&text(i32::from(n), 1), &["{source}"]),
                    String::new(),
                    0,
                );
            }
            if spell.msgnum.0 > 0 {
                let line = text(spell.msgnum.0, 1);
                let filled = if spell.flags.contains(SpellFlags::ALTMSG) {
                    fill(&line, &["{dmg}"])
                } else {
                    fill(&line, &["{source}", &spell.name, "{dmg}"])
                };
                push(
                    &mut rows,
                    spell,
                    "hit",
                    upper_first(&filled),
                    String::new(),
                    0,
                );
            }
        }
    }
    rows
}

/// A spell players cast: one with a school of magic, or one no monster casts (an item's, a
/// room's).
fn player_cast(spell: &Spell, monster_spells: &HashSet<i32>) -> bool {
    spell.magery != Magery::None || !monster_spells.contains(&i32::from(spell.number.0))
}

/// The area kinds whose texts leave the target out (silvermere-engine
/// `spells::messages::names_no_target`).
fn names_no_target(target: SpellTarget) -> bool {
    matches!(
        target,
        SpellTarget::DivArea
            | SpellTarget::DivAttack
            | SpellTarget::DivParty
            | SpellTarget::FullArea
            | SpellTarget::FullAttack
            | SpellTarget::FullParty
    )
}

/// The first letter raised, as the game raises a monster's lines.
fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// What a lasting spell does to whoever it holds, as MegaMUD's effect bits.
fn effects(spell: &Spell) -> i32 {
    let mut bits = 0;
    for &(ability, value) in &spell.abilities {
        // a value of 0 takes the spell's own amount
        let positive = value > 0 || (value == 0 && spell.maxval > 0);
        bits |= match ability {
            Ability::Poison => effect::POISONED,
            Ability::BlindUser | Ability::BlindingLight => effect::BLIND,
            Ability::Confusion => effect::CONFUSED,
            Ability::Paralyze => effect::HELD,
            Ability::DamageNoArmour if spell.duration > 0 => effect::LOSING_HP,
            Ability::HealRate if positive => effect::HP_REGEN,
            Ability::ManaRate if positive => effect::MANA_REGEN,
            _ => 0,
        };
    }
    bits
}

/// A game text with its `%s` and `%d` filled from `args` in order (a missing one as MegaMUD's
/// wildcard, `{1}`), and `%%` as `%`.
fn fill(text: &str, args: &[&str]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut next = 0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.peek() {
            Some('%') => {
                chars.next();
                out.push('%');
            }
            Some('s' | 'd') => {
                chars.next();
                out.push_str(args.get(next).copied().unwrap_or("{1}"));
                next += 1;
            }
            // `%'s`: a possessive the game fills with a name
            Some('\'') => {
                out.push_str("{1}");
            }
            _ => out.push('%'),
        }
    }
    out
}

/// A message line as it is matched: no colour codes, no line end, no edge spaces.
fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' && c != '\n' {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_texts_become_megamud_patterns() {
        assert_eq!(
            fill(
                "You cast %s on %s for %d damage!",
                &["agony", "{target}", "{dmg}"]
            ),
            "You cast agony on {target} for {dmg} damage!"
        );
        assert_eq!(fill("100%% sure, %s", &[]), "100% sure, {1}");
        assert_eq!(
            fill("%'s enemies are flung away!", &[]),
            "{1}'s enemies are flung away!"
        );
        assert_eq!(
            clean("\u{1b}[1;32mYou feel perceptive!\r"),
            "You feel perceptive!"
        );
    }
}
