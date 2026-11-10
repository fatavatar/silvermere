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
//!   wear off!"): one row, its condition read from the spell's abilities as the engine applies
//!   them (see [`effects`]); the target of a monster's spell reads that line right after the
//!   spell's own;
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
    pub const NO_ATTACK: i32 = 0x0020;
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
                effects(&spell.abilities, spell.minval, spell.maxval, spell.duration),
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
    agree_on_shared_lines(&mut rows);
    rows
}

/// A condition's line is matched by its words alone, whichever spell printed it: when several
/// spells begin with the same line, its rows take the conditions most of them give ("You feel
/// fast!" is speed's and the haste songs', not a monster's blinding mirrored shield's; "You are
/// stunned!" holds, though a few of its spells only confuse). When no one set of conditions is
/// the most given, the rows keep what those sets share.
fn agree_on_shared_lines(rows: &mut [Row]) {
    let effect_line = |row: &Row| match (row.get("Kind"), row.get("Message")) {
        (Some(Value::Text(kind)), Some(Value::Text(m))) if kind == "effect" => Some(m.clone()),
        _ => None,
    };
    let flags = |row: &Row| match row.get("Flags") {
        Some(Value::Long(f)) => *f,
        _ => 0,
    };
    let mut votes: BTreeMap<String, BTreeMap<i32, usize>> = BTreeMap::new();
    for row in rows.iter() {
        if let Some(line) = effect_line(row) {
            *votes
                .entry(line)
                .or_default()
                .entry(flags(row))
                .or_default() += 1;
        }
    }
    let agreed: BTreeMap<String, i32> = votes
        .into_iter()
        .map(|(line, counts)| {
            let most = counts.values().copied().max().unwrap_or(0);
            let flags = counts
                .iter()
                .filter(|(_, n)| **n == most)
                .fold(!0, |shared, (f, _)| shared & f);
            (line, flags)
        })
        .collect();
    for row in rows.iter_mut() {
        if let Some(line) = effect_line(row) {
            row.insert("Flags".into(), Value::Long(agreed[&line]));
        }
    }
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

/// The light at or below which a room is too dark to see in (silvermere-engine
/// `display::light::LIGHT_DARK`), as an Alter User Light on a character in an ordinary room.
const TOO_DARK: i32 = -150;

/// Confusion, as the chance in a hundred of losing each action, from which a condition is worth
/// telling: a form that loses one action in fifty is a buff with a price, not a condition to
/// cure.
const CONFUSION_FLAGGED: i32 = 10;

/// What a lasting spell does to whoever it holds, as MegaMUD's effect bits, from its abilities
/// as the engine applies them (silvermere-rules `abilities::dynamic`, the per-round code): an
/// ability without its own value takes the spell's roll, between `minval` and `maxval`.
///
/// - blind: Blind User (whatever its value: the flag is all it sets), Blinding Light, or an
///   Alter User Light that leaves an ordinary room too dark to see;
/// - held and no attack: Paralyze, or Confusion that loses every action; held alone: Hold
///   Person (no moving); no attack alone: Flee (afraid: no attacking or casting);
/// - confused: Confusion that loses at least one action in [`CONFUSION_FLAGGED`];
/// - poisoned: Poison; losing hit points: damage (with or without armour) each round;
/// - regenerating: Heal Rate or Mana Rate, or healing (hit points or mana) each round, on a
///   spell that does nothing else. MegaMUD (and
///   Majordomo) read these as "the regeneration spell is up" and stop recasting it, so a buff
///   that adds some regeneration beside other things (a bard's song, a cloak) gives none.
pub fn effects(abilities: &[(Ability, i16)], minval: i16, maxval: i16, duration: i16) -> i32 {
    let roll = (i32::from(minval) + i32::from(maxval)) / 2;
    let amount = |value: i16| {
        if value != 0 { i32::from(value) } else { roll }
    };
    let rolls_anything = minval != 0 || maxval != 0;
    let only_regen = abilities.iter().all(|(a, _)| {
        matches!(
            a,
            Ability::HealRate | Ability::ManaRate | Ability::Heal | Ability::HealMana
        ) || incidental(*a)
    });
    let lasting = duration != 0;
    let mut bits = 0;
    for &(ability, value) in abilities {
        bits |= match ability {
            Ability::BlindUser | Ability::BlindingLight => effect::BLIND,
            Ability::UserLight if amount(value) <= TOO_DARK => effect::BLIND,
            Ability::Paralyze => effect::HELD | effect::NO_ATTACK,
            Ability::HoldPerson => effect::HELD,
            Ability::Flee => effect::NO_ATTACK,
            Ability::Confusion if amount(value) >= 100 => effect::HELD | effect::NO_ATTACK,
            Ability::Confusion if amount(value) >= CONFUSION_FLAGGED => effect::CONFUSED,
            Ability::Poison => effect::POISONED,
            Ability::DamageNoArmour | Ability::DamageArmour
                if lasting && (value != 0 || rolls_anything) =>
            {
                effect::LOSING_HP
            }
            Ability::HealRate if only_regen && amount(value) > 0 => effect::HP_REGEN,
            Ability::Heal if only_regen && lasting && amount(value) > 0 => effect::HP_REGEN,
            Ability::ManaRate if only_regen && amount(value) > 0 => effect::MANA_REGEN,
            Ability::HealMana if only_regen && lasting && amount(value) > 0 => effect::MANA_REGEN,
            _ => 0,
        };
    }
    bits
}

/// An ability that says something about a spell rather than doing anything to whoever it
/// holds: its messages, who it may affect, how it ends.
fn incidental(a: Ability) -> bool {
    matches!(
        a,
        Ability::None
            | Ability::DescMsg
            | Ability::StartMsg
            | Ability::ShockMsg
            | Ability::ConfuseMsg
            | Ability::RemoveSpell
            | Ability::EvilInCombat
            | Ability::AffectsLiving
            | Ability::AffectsAnimal
            | Ability::Animal
            | Ability::NonLiving
            | Ability::Good
            | Ability::Evil
            | Ability::NotGood
            | Ability::NotEvil
            | Ability::Neutral
            | Ability::NotNeutral
            | Ability::Magical
            | Ability::NonMagical
            | Ability::MinLevel
            | Ability::MaxLevel
            | Ability::EndCast
            | Ability::TermSpell
            | Ability::KeepVisible
            | Ability::DelCleanup
            | Ability::RechargeCleanup
            | Ability::DelInvCleanup
    )
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
    fn conditions_are_read_as_the_engine_applies_them() {
        use Ability as A;
        let e = effects;
        // mirrored shield: Blind User at 0 still blinds
        assert_eq!(
            e(&[(A::BlindUser, 0), (A::DescMsg, 1)], 999, 999, 8),
            effect::BLIND
        );
        // black beam: dark enough to see nothing; a dim light is no blindness
        assert_eq!(e(&[(A::UserLight, 0)], -500, -100, 10), effect::BLIND);
        assert_eq!(e(&[(A::UserLight, -50)], 0, 0, 10), 0);
        // yellow fungus and sleep lose every action; song of dazzling a quarter; the monkey
        // form one in fifty
        let all = effect::HELD | effect::NO_ATTACK;
        assert_eq!(e(&[(A::Confusion, 0)], 100, 100, 6), all);
        assert_eq!(e(&[(A::Confusion, 100)], 1, 1, 5), all);
        assert_eq!(e(&[(A::Confusion, 0)], 25, 25, 10), effect::CONFUSED);
        assert_eq!(e(&[(A::Confusion, 2), (A::AvBless, 10)], 0, 0, 300), 0);
        // hold person, paralyze, terror
        assert_eq!(e(&[(A::HoldPerson, 0)], 100, 100, 4), effect::HELD);
        assert_eq!(e(&[(A::Paralyze, 0)], 100, 100, 4), all);
        assert_eq!(e(&[(A::Flee, 0)], 35, 90, 10), effect::NO_ATTACK);
        // a net: held, and two damage a round
        assert_eq!(
            e(&[(A::HoldPerson, 0), (A::DamageNoArmour, 2)], 1, 1, 6),
            effect::HELD | effect::LOSING_HP
        );
        // earthbind regenerates and nothing else; a bard's song of knowledge does more
        assert_eq!(
            e(
                &[
                    (A::HealRate, 0),
                    (A::ManaRate, 0),
                    (A::StartMsg, 1),
                    (A::Neutral, 0)
                ],
                10,
                15,
                80
            ),
            effect::HP_REGEN | effect::MANA_REGEN
        );
        assert_eq!(
            e(
                &[(A::ManaRate, 10), (A::Critical, 2), (A::MagicResist, 5)],
                0,
                0,
                40
            ),
            0
        );
        // regeneration heals each round; an instant heal is no condition
        assert_eq!(
            e(&[(A::Heal, 0), (A::DescMsg, 1)], 2, 4, 40),
            effect::HP_REGEN
        );
        assert_eq!(e(&[(A::Heal, 0), (A::DescMsg, 1)], 2, 4, 0), 0);
    }

    #[test]
    fn a_shared_line_keeps_what_its_spells_agree_on() {
        let row = |name: &str, flags: i32| {
            let mut r = Row::new();
            r.insert("Name".into(), Value::Text(name.into()));
            r.insert("Message".into(), Value::Text("You feel fast!".into()));
            r.insert("Kind".into(), Value::Text("effect".into()));
            r.insert("Flags".into(), Value::Long(flags));
            r
        };
        // the most given wins
        let mut rows = vec![
            row("speed", 0),
            row("wind of haste", 0),
            row("mirrored shield", effect::BLIND),
        ];
        agree_on_shared_lines(&mut rows);
        assert!(rows.iter().all(|r| r["Flags"] == Value::Long(0)));
        // a tie keeps what is shared
        let mut rows = vec![
            row("fungus cloud", effect::CONFUSED),
            row("spore cloud", effect::CONFUSED | effect::LOSING_HP),
        ];
        agree_on_shared_lines(&mut rows);
        assert!(
            rows.iter()
                .all(|r| r["Flags"] == Value::Long(effect::CONFUSED))
        );
    }

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
