//! Reading text block scripts for the records they refer to.
//!
//! NMR does not parse scripts; it looks for each command word ("giveitem ", "cast ", "text ",
//! ":" for a topic ...) and takes the digits after it, and this module does the same, position
//! for position, because MME shows what that finds. A command on a line that first checks for
//! an item not in the game ("checkitem", "roomitem", "takeitem") is skipped, a learned spell
//! takes the class a "class N" check earlier on its line names, and a reference from a block of
//! random results carries its chance ("Textblock #12(25%)").

use super::vb::{Money, at, instr, lower, mid, mid_to_end, val, whole};
use super::xref::{Marks, Refers};

/// Items a script may give, with the chance of each: NMR's running list (chance given, chance
/// not), which starts with an unused entry.
pub(super) struct Drops(Vec<(i32, Money, Money)>);

impl Default for Drops {
    fn default() -> Self {
        Self(vec![(0, Money::default(), Money::default())])
    }
}

impl Drops {
    /// Adds an item given with `chance`, scaled by the chance of reaching the script.
    fn add(&mut self, item: i32, chance: Money, scale: Money) {
        if let Some(entry) = self.0.iter_mut().find(|e| e.0 == item) {
            entry.1 = entry.1 + entry.2.times(chance).times(scale);
            entry.2 = entry.2.times(Money::ONE - chance);
        } else {
            let given = chance.times(scale);
            self.0.push((item, given, Money::ONE - given));
        }
    }

    /// The items found, with their chance.
    pub(super) fn found(&self) -> impl Iterator<Item = (i32, Money)> + '_ {
        self.0.iter().filter(|e| e.0 > 0).map(|e| (e.0, e.1))
    }
}

/// Spells a script teaches, with the classes it teaches them to; starts with an unused entry.
pub(super) struct Lessons(Vec<(i32, String)>);

impl Default for Lessons {
    fn default() -> Self {
        Self(vec![(0, String::new())])
    }
}

impl Lessons {
    fn learn(&mut self, spell: i32, classes: String) {
        if let Some(entry) = self.0.iter_mut().find(|e| e.0 == spell) {
            entry.1 = if classes == "(*)" {
                classes
            } else {
                super::xref::merge_classes(&classes, &entry.1)
            };
        } else {
            self.0.push((spell, classes));
        }
    }

    /// The spells found, with their classes.
    pub(super) fn found(&self) -> impl Iterator<Item = (i32, String)> + '_ {
        self.0.iter().filter(|e| e.0 > 0).cloned()
    }
}

/// The number of digits at the 1-based position `from`.
pub(super) fn digits_at(text: &[u8], from: usize) -> usize {
    mid_to_end(text, from)
        .iter()
        .take_while(|b| b.is_ascii_digit())
        .count()
}

/// The 1-based position before the digits that end just before position `end` (0 when they
/// start the text).
fn digits_before(text: &[u8], end: usize) -> usize {
    let mut at_ = end - 1;
    while at_ != 0 && at(text, at_).is_some_and(|b| b.is_ascii_digit()) {
        at_ -= 1;
    }
    at_
}

/// The 1-based position where the line holding position `pos` starts.
fn line_start(text: &[u8], pos: usize) -> usize {
    let before = mid(text, 1, pos.saturating_sub(1));
    let mut x = 1;
    loop {
        let lf = instr(x, before, b"\n");
        if lf == 0 {
            return x;
        }
        x = lf + 1;
    }
}

/// The number after the first `search` in `text`, spaces before it skipped; 0 without one.
fn value_after(text: &[u8], search: &[u8]) -> i32 {
    let found = instr(1, text, search);
    if found == 0 {
        return 0;
    }
    let mut x = found + search.len();
    let mut y = x;
    while let Some(b) = at(text, y) {
        match b {
            b'0'..=b'9' => {}
            b' ' if y == x => x += 1,
            _ => break,
        }
        y += 1;
    }
    if y > x {
        whole(val(mid(text, x, y - x)))
    } else {
        0
    }
}

/// The chance of the random result that holds position `pos`, "(25%)": its "NN:" less the one
/// before it, or "(?%)" when it cannot tell.
fn chance_at(text: &[u8], pos: usize) -> String {
    let before = mid(text, 1, pos - 1);
    let mut x = 1;
    let mut last = 0;
    while instr(x, before, b"\n") != 0 {
        let colon = instr(x, before, b":");
        if colon == 0 {
            return "(?%)".into();
        }
        let start = digits_before(before, colon);
        last = whole(val(mid(before, start + 1, colon - start + 1)));
        x = instr(x, before, b"\n") + 1;
    }
    let line = mid(text, x, pos - x);
    let colon = instr(1, line, b":");
    if colon == 0 {
        return "(?%)".into();
    }
    let start = digits_before(line, colon);
    let chance = whole(val(mid(line, start + 1, colon - start + 1))) - last;
    if chance == 0 {
        "(?%)".into()
    } else {
        format!("({chance}%)")
    }
}

/// The class a "class N" check earlier on the line of position `pos` restricts to, "(N)", or
/// "(*)" for any class.
fn class_at(text: &[u8], pos: usize) -> String {
    let start = line_start(text, pos);
    let line = mid(text, start, pos - start);
    let mut x = 1;
    loop {
        let found = instr(x, line, b"class ");
        if found == 0 {
            return "(*)".into();
        }
        x = found;
        // part of a longer word; NMR never gets past one of these, the port skips it
        if at(line, x - 1).is_some_and(|b| b.is_ascii_lowercase()) {
            x += 1;
            continue;
        }
        let y1 = x + b"class ".len();
        let digits = digits_at(line, y1);
        if digits == 0 {
            x = y1;
            continue;
        }
        return format!("({})", String::from_utf8_lossy(mid(line, y1, digits)));
    }
}

impl Marks<'_, '_> {
    /// Whether the line holding position `pos` first checks for an item not in the game.
    fn item_missing(&self, text: &[u8], pos: usize) -> bool {
        let start = line_start(text, pos);
        let line = mid(text, start, pos - start);
        // "failitem" is left out: it fails when the item is there
        for look in [&b"checkitem "[..], b"roomitem ", b"takeitem "] {
            let mut x = 1;
            loop {
                let found = instr(x, line, look);
                if found == 0 {
                    break;
                }
                x = found;
                if look == b"roomitem " && x > 4 && mid(line, x - 4, 4) == b"fail" {
                    x += 1;
                    continue;
                }
                let y1 = x + look.len();
                let digits = digits_at(line, y1);
                if digits == 0 {
                    x = y1;
                    continue;
                }
                if !self.items_seen.contains(&whole(val(mid(line, y1, digits)))) {
                    return true;
                }
                x = y1;
            }
        }
        false
    }

    /// Follows every `look` command in a part of block `tb`; false when one was skipped for a
    /// record not in the game yet (the part is then read again next pass).
    pub(super) fn follow(&mut self, tb: i32, text: &[u8], look: &[u8], refers: Refers) -> bool {
        let text = lower(text);
        let learn = look.starts_with(b"learn");
        let random = look.starts_with(b"random");
        let test_skill = look.starts_with(b"testskill");
        let check_spell = look.starts_with(b"checkspell");
        let give = look.starts_with(b"giveitem");
        let mut cleared = true;
        let mut x = 1;
        loop {
            let found = instr(x, &text, look);
            if found == 0 {
                return cleared;
            }
            x = found;
            let mut y1 = x + look.len();
            if test_skill {
                // "testskill <skill> <amount> <block>", all on one command
                let first = instr(y1, &text, b" ");
                let second = if first == 0 {
                    0
                } else {
                    instr(first + 1, &text, b" ")
                };
                if second == 0 {
                    x = y1;
                    continue;
                }
                x = second + 1;
                let colon = instr(y1, &text, b":");
                let lf = instr(y1, &text, b"\n");
                if (colon > 0 && colon < x) || (lf > 0 && lf < x) {
                    x = y1;
                    continue;
                }
                y1 = x;
            }
            let digits = digits_at(&text, y1);
            if digits == 0 {
                x = y1;
                continue;
            }
            let mut number = whole(val(mid(&text, y1, digits)));
            if check_spell {
                // "checkspell <spell> <block>": the block counts when the spell is in the game
                if number == 0 {
                    x = y1;
                    continue;
                }
                y1 += digits + 1;
                let digits = digits_at(&text, y1);
                let block = if digits == 0 {
                    0
                } else {
                    whole(val(mid(&text, y1, digits)))
                };
                if block == 0 {
                    x = y1;
                    continue;
                }
                if !self.spells_seen.contains(&number) {
                    cleared = false;
                    x = y1;
                    continue;
                }
                number = block;
            }
            if self.item_missing(&text, x) {
                cleared = false;
            } else {
                let chance = if self.text_random.contains(&tb) {
                    chance_at(&text, x)
                } else {
                    String::new()
                };
                self.text_commands.insert(tb);
                let from = format!("Textblock #{tb}{chance}");
                let from_npc = self.text_from_npc.contains(&tb);
                match refers {
                    Refers::Spell if learn && from_npc => self.mark_spell(number, "", None),
                    Refers::Spell if learn => {
                        let classes = class_at(&text, x);
                        self.mark_spell(number, &from, Some(&classes));
                    }
                    Refers::Spell => self.mark_spell(number, &from, None),
                    Refers::Item if give && from_npc => self.mark_item(number, "", false, false),
                    Refers::Item if give => self.mark_item(number, &from, false, false),
                    // takeitem, checkitem ...: the item is needed, not obtained
                    Refers::Item => self.mark_item(number, &from, true, true),
                    Refers::Monster => self.mark_monster(number, &from),
                    Refers::Text => {
                        if from_npc {
                            self.text_from_npc.insert(number);
                        }
                        if random {
                            let from = format!("Textblock(rndm) #{tb}{chance}");
                            self.mark_text(number, &from, true);
                            self.text_commands.insert(number);
                        } else {
                            self.mark_text(number, &from, false);
                            self.text_commands.insert(self.world.text_link(number));
                        }
                    }
                }
            }
            x = y1;
        }
    }

    /// The items (and, with `spells`, the spells) block `n` gives: "giveitem" lines count once
    /// each, "random" lines add the results of that block, nested at most six deep.
    pub(super) fn block_drops(
        &self,
        drops: &mut Drops,
        n: i32,
        nest: &mut i32,
        mut spells: Option<&mut Lessons>,
    ) {
        let Some(data) = self.script(n, *nest) else {
            return;
        };
        *nest += 1;
        let mut given: Vec<(i32, i32)> = vec![(0, 0)];
        let mut pos = 1;
        while pos < data.len() {
            let end = match instr(pos, &data, b"\n") {
                0 => data.len(),
                lf => lf,
            };
            let line = lower(mid(&data, pos, end - pos));
            let start = pos;
            pos = end + 1;
            if self.item_missing(&data, pos - 1) {
                continue;
            }
            for item in commands(&line, b"giveitem ") {
                match given.iter_mut().find(|g| g.0 == item.1) {
                    Some(g) => g.1 += 1,
                    None => given.push((item.1, 1)),
                }
            }
            if let Some(lessons) = spells.as_deref_mut() {
                for (y, spell) in commands(&line, b"learnspell ") {
                    lessons.learn(spell, class_at(&data, start + y));
                }
            }
            for (_, block) in commands(&line, b"random ") {
                if block > 0 {
                    self.random_drops(drops, block, nest, Money::ONE, spells.as_deref_mut());
                }
            }
        }
        for (item, count) in given.into_iter().filter(|g| g.0 > 0) {
            drops.add(item, Money::from_f64(count.into()), Money::ONE);
        }
        *nest -= 1;
    }

    /// The items (and spells) a block of random results gives, each "NN:" line's chance being
    /// its number less the line before's, scaled by `scale`, the chance of reaching the block.
    pub(super) fn random_drops(
        &self,
        drops: &mut Drops,
        n: i32,
        nest: &mut i32,
        scale: Money,
        mut spells: Option<&mut Lessons>,
    ) {
        let Some(data) = self.script(n, *nest) else {
            return;
        };
        *nest += 1;
        let scale = if scale <= Money::default() {
            Money::ONE
        } else {
            scale
        };
        let mut given: Vec<(i32, Money)> = vec![(0, Money::default())];
        let (mut pos, mut previous) = (1, 0);
        while pos < data.len() {
            let colon = instr(pos, &data, b":");
            if colon == 0 {
                break;
            }
            if colon == pos {
                pos += 1;
                continue;
            }
            let upto = whole(val(mid(&data, pos, colon - pos)));
            let chance = Money::from_f64(f64::from(upto - previous) / 100.0);
            previous = upto;
            pos = colon + 1;
            let end = match instr(pos, &data, b"\n") {
                0 => data.len(),
                lf => lf,
            };
            let line = lower(mid(&data, pos, end - pos));
            let start = pos;
            pos = end;
            for (_, item) in commands(&line, b"giveitem ") {
                match given.iter_mut().find(|g| g.0 == item) {
                    Some(g) => g.1 = g.1 + chance,
                    None => given.push((item, chance)),
                }
            }
            if let Some(lessons) = spells.as_deref_mut() {
                for (y, spell) in commands(&line, b"learnspell ") {
                    lessons.learn(spell, class_at(&data, start + y));
                }
            }
            for (_, block) in commands(&line, b"random ") {
                if block > 0 {
                    let reach = chance.times(scale);
                    self.random_drops(drops, block, nest, reach, spells.as_deref_mut());
                }
            }
        }
        for (item, chance) in given.into_iter().filter(|g| g.0 > 0) {
            drops.add(item, chance, scale);
        }
        *nest -= 1;
    }

    /// Block `n`'s script when it can be read at this depth: it exists, nesting is at most six
    /// deep, and its first part has at least five characters.
    fn script(&self, n: i32, nest: i32) -> Option<Vec<u8>> {
        let parts = self.world.text_parts(n);
        let first = parts.first()?;
        if nest > 5 || first.data.len() < 5 {
            return None;
        }
        Some(parts.iter().flat_map(|p| p.data.iter().copied()).collect())
    }
}

/// Each `look` command in a line: its 1-based position and the number after it.
fn commands(line: &[u8], look: &[u8]) -> Vec<(usize, i32)> {
    let mut found = Vec::new();
    let mut y = 1;
    loop {
        y = instr(y, line, look);
        if y == 0 {
            return found;
        }
        found.push((y, value_after(mid_to_end(line, y), look)));
        y += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_random_result_has_its_own_share() {
        let text = b"25:giveitem 5\n75:giveitem 6\n";
        assert_eq!(chance_at(text, 4), "(25%)");
        assert_eq!(chance_at(text, 18), "(50%)");
        assert_eq!(chance_at(b"giveitem 5", 1), "(?%)");
    }

    #[test]
    fn a_class_check_on_the_line_restricts() {
        let text = b"class 3 learnspell 9\nlearnspell 4";
        assert_eq!(class_at(text, 9), "(3)");
        assert_eq!(class_at(text, 23), "(*)");
        assert_eq!(class_at(b"subclass 3 learnspell 9", 12), "(*)");
    }

    #[test]
    fn command_numbers_skip_spaces() {
        assert_eq!(value_after(b"giveitem  12 x", b"giveitem "), 12);
        assert_eq!(value_after(b"giveitem x", b"giveitem "), 0);
        assert_eq!(
            commands(b"giveitem 1 giveitem 2", b"giveitem "),
            vec![(1, 1), (12, 2)]
        );
    }

    #[test]
    fn drops_combine_like_the_original() {
        let mut drops = Drops::default();
        drops.add(7, Money::from_f64(0.5), Money::ONE);
        drops.add(7, Money::from_f64(0.5), Money::ONE);
        let found: Vec<_> = drops.found().collect();
        assert_eq!(found, vec![(7, Money::from_f64(0.75))]);
    }
}
