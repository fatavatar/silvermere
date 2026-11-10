//! The game's own help screens for classes and races (`HELP NECROLYTES`), read for the site.
//!
//! The data says what a class's numbers are; the help says what the class *is*: a Necrolyte's
//! record has priest magery, but its help calls it Pagan-3, and that is what players know it
//! by. So the pages lead with the help, and fall back to the data only for a class or race the
//! help does not cover.
//!
//! A help screen is a box drawn in line characters. A class's has its name and experience, a
//! description, a line of `Key: value` facts and a line of abilities (with notes under it). A
//! race's description runs down the right of a table of stat ranges and abilities.

/// One class's or race's help screen, read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HelpEntry {
    /// The experience the header shows (`200%`, `+30%`).
    pub exp: Option<String>,
    /// The description, as one paragraph.
    pub description: String,
    /// `Key: value` facts, in order (`HP/Level`, `Magic`, `Combat`, `Weapons`, `Armour`).
    pub facts: Vec<(String, String)>,
    /// The abilities, one per entry.
    pub abilities: Vec<String>,
    /// Notes under the abilities.
    pub notes: Vec<String>,
    /// A race's stat ranges: (stat, range).
    pub stats: Vec<(String, String)>,
}

impl HelpEntry {
    /// A fact by key, ignoring case.
    pub fn fact(&self, key: &str) -> Option<&str> {
        self.facts
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }
}

/// The text of the topic for `name` (a class's or race's): the topic one of whose words, or
/// run of words, is the name or its plural, the one naming it earliest winning.
pub fn find<'a>(topics: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let name = name.trim().to_lowercase();
    let mut forms = vec![name.clone(), format!("{name}s"), format!("{name}es")];
    if let Some(stem) = name.strip_suffix('y') {
        forms.push(format!("{stem}ies"));
    }
    if let Some(stem) = name.strip_suffix('f') {
        forms.push(format!("{stem}ves"));
    }
    let mut best: Option<(usize, &str)> = None;
    for (words, text) in topics {
        let tokens: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
        for form in &forms {
            let want: Vec<&str> = form.split_whitespace().collect();
            if want.is_empty() {
                continue;
            }
            let at = tokens
                .windows(want.len())
                .position(|w| w.iter().zip(&want).all(|(a, b)| a == b));
            if let Some(at) = at
                && best.is_none_or(|(b, _)| at < b)
            {
                best = Some((at, text.as_str()));
            }
        }
    }
    best.map(|(_, t)| t)
}

/// Reads a class's or race's help screen; `None` when it is not one (no header box).
pub fn parse(text: &str) -> Option<HelpEntry> {
    let text = strip_escapes(text);
    let mut entry = HelpEntry::default();
    let mut section = 0usize;
    let mut header_seen = false;
    let mut race_table = false;
    let mut desc: Vec<String> = Vec::new();
    let mut abilities_line: Option<String> = None;
    for line in text.lines() {
        if !line.contains('║') && !line.contains('╠') && !line.contains('╚') && !line.contains('╔')
        {
            continue;
        }
        let cells: Vec<String> = line
            .split(['║', '╠', '╣', '╦', '╩', '╔', '╗', '╚', '╝'])
            .map(str::trim)
            .filter(|c| c.is_empty() || !c.chars().all(|ch| ch == '═'))
            .map(str::to_owned)
            .collect();
        // the line's text cells, without the empty ends a split leaves
        let inner: Vec<&str> = trim_ends(&cells);
        let divider = inner.iter().all(|c| c.is_empty());
        if divider {
            if line.contains('╠') && header_seen {
                section += 1;
            }
            continue;
        }
        if !header_seen {
            header_seen = true;
            if let Some(i) = inner[0].find("EXP:") {
                entry.exp = Some(inner[0][i + 4..].trim().to_owned());
            }
            continue;
        }
        if race_table || inner.len() >= 3 {
            race_table = true;
            match inner.as_slice() {
                [stat, ability, rest] => {
                    if let Some((name, range)) = stat.rsplit_once(' ') {
                        entry
                            .stats
                            .push((name.trim().to_owned(), range.trim().to_owned()));
                    }
                    if ability.starts_with('(')
                        && let Some(last) = entry.abilities.last_mut()
                    {
                        // an ability's detail wrapped onto the next line
                        last.push(' ');
                        last.push_str(ability);
                    } else if !ability.is_empty() {
                        entry.abilities.push((*ability).to_owned());
                    }
                    desc.push((*rest).to_owned());
                }
                [_heading, rest] => desc.push((*rest).to_owned()),
                [rest] => desc.push((*rest).to_owned()),
                _ => {}
            }
            continue;
        }
        let cell = inner.join(" ");
        if inner.len() == 2 {
            // the line where a race's description meets its table's heading
            race_table = true;
            desc.push(inner[1].to_owned());
            continue;
        }
        // the divider under the header opens section 1, the description
        match section {
            0 | 1 => desc.push(cell),
            _ if cell.contains("HP/Level:")
                || (entry.facts.is_empty() && looks_like_facts(&cell)) =>
            {
                entry.facts = facts(&cell);
            }
            _ if cell.starts_with("Abilities:") => {
                abilities_line = Some(cell["Abilities:".len()..].to_owned())
            }
            _ if abilities_line.is_some() => entry.notes.push(cell),
            _ => desc.push(cell),
        }
    }
    if !header_seen {
        return None;
    }
    if let Some(line) = abilities_line {
        let line = line.trim().trim_end_matches('.');
        if !line.eq_ignore_ascii_case("none") && !line.is_empty() {
            entry.abilities = line
                .split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect();
        }
    }
    entry.description = join_lines(&desc);
    // a note runs on over several lines: "Note: ..." starts one
    let mut notes: Vec<String> = Vec::new();
    for n in entry.notes.drain(..) {
        match notes.last_mut() {
            Some(last) if !n.starts_with("Note") => {
                last.push(' ');
                last.push_str(&n);
            }
            _ => notes.push(n),
        }
    }
    entry.notes = notes;
    Some(entry)
}

fn trim_ends(cells: &[String]) -> Vec<&str> {
    let mut v: Vec<&str> = cells.iter().map(String::as_str).collect();
    while v.first().is_some_and(|c| c.is_empty()) {
        v.remove(0);
    }
    while v.last().is_some_and(|c| c.is_empty()) {
        v.pop();
    }
    v
}

fn looks_like_facts(cell: &str) -> bool {
    cell.matches(": ").count() >= 3
}

/// `Key: value` pairs: a key is the word or words (with `/`) before a colon.
fn facts(cell: &str) -> Vec<(String, String)> {
    let mut keys: Vec<(usize, usize)> = Vec::new(); // (key start, value start)
    let bytes = cell.as_bytes();
    for (i, _) in cell.match_indices(':') {
        let mut start = i;
        while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'/') {
            start -= 1;
        }
        if start < i {
            keys.push((start, i + 1));
        }
    }
    let mut out = Vec::new();
    for (n, &(start, value)) in keys.iter().enumerate() {
        let end = keys.get(n + 1).map_or(cell.len(), |k| k.0);
        let v = cell[value..end].trim().trim_end_matches('.').to_owned();
        out.push((cell[start..value - 1].to_owned(), v));
    }
    out
}

/// Description lines as prose: hyphenations joined, spaces normalised.
fn join_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for l in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        if out.is_empty() {
            out.push_str(l);
        } else if out.ends_with('-') && !out.ends_with("--") && !out.ends_with(" -") {
            // a word broken over two lines ("immediately recognizable-- they" is not one)
            out.push_str(l);
        } else {
            out.push(' ');
            out.push_str(l);
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text without ANSI escape sequences, its lines ended by `\n`.
fn strip_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
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
        // the game ends its lines with a bare carriage return
        if c == '\r' {
            if chars.peek() != Some(&'\n') {
                out.push('\n');
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NECROLYTES: &str = "\u{1b}[0m\u{1b}[2J\r\u{1b}[34m╔═══════════════════════════════════════════════════════════════════════════╗\r║ \u{1b}[1;33mNECROLYTES                                                     \u{1b}[37mEXP: \u{1b}[0m200%  \u{1b}[34m║\r
╠═══════════════════════════════════════════════════════════════════════════╣
║ Necrolytes are the masters of the undead. The first Necrolytes were       ║
║ trained by the armies of the Dark One.                                    ║
╠═══════════════════════════════════════════════════════════════════════════╣
║ HP/Level: 3-6 Magic: Pagan-3 Combat: 2 Weapons: Blunt Armour: Robes       ║
╠═══════════════════════════════════════════════════════════════════════════╣
║ Abilities: Martial Arts, Dodge, Kai Powers.                               ║
║ Note: Mystics do not automatically get their Kai. Powers must be learned. ║
║       Kai-3 affects only the range of powers received.                    ║
╚═══════════════════════════════════════════════════════════════════════════╝
";

    const GAUNTS: &str = "\u{1b}[2J
╔════════════════════════════════════════════════════════════════════════════╗
║ GAUNT ONES                                                       EXP: +30% ║
╠════════════════════════════════════════════════════════════════════════════╣
║ Also known simply as 'Gaunts', this mysterious and reclusive folk are by   ║
║ far the least fathomable of the known races.                               ║
╠════════════════════════════════════╗ and suspicious of their motives.      ║
║ Statistics         Abilities       ║ Outsiders never see their concealed   ║
╠══════════════════╦═════════════════╣ homes.                                ║
║ Strength  40-160 ║ Dark Vision     ║ Gaunts' eyes are                      ║
║ Agility   50-180 ║                 ║ magical.                              ║
╚══════════════════╩═════════════════╩═══════════════════════════════════════╝
";

    #[test]
    fn a_class_screen() {
        let e = parse(NECROLYTES).expect("a screen");
        assert_eq!(e.exp.as_deref(), Some("200%"));
        assert!(
            e.description
                .starts_with("Necrolytes are the masters of the undead.")
        );
        assert!(e.description.ends_with("armies of the Dark One."));
        assert_eq!(e.fact("magic"), Some("Pagan-3"));
        assert_eq!(e.fact("HP/Level"), Some("3-6"));
        assert_eq!(e.fact("Armour"), Some("Robes"));
        assert_eq!(e.abilities, ["Martial Arts", "Dodge", "Kai Powers"]);
        assert_eq!(e.notes.len(), 1);
        assert!(e.notes[0].ends_with("range of powers received."));
    }

    #[test]
    fn a_race_screen() {
        let e = parse(GAUNTS).expect("a screen");
        assert_eq!(e.exp.as_deref(), Some("+30%"));
        assert_eq!(
            e.description,
            "Also known simply as 'Gaunts', this mysterious and reclusive folk are by far the least fathomable of the known races. and suspicious of their motives. Outsiders never see their concealed homes. Gaunts' eyes are magical."
        );
        assert_eq!(
            e.stats,
            [
                ("Strength".into(), "40-160".into()),
                ("Agility".into(), "50-180".into())
            ]
        );
        assert_eq!(e.abilities, ["Dark Vision"]);
    }

    #[test]
    fn topics_by_name_or_plural() {
        let topics: Vec<(String, String)> = [
            ("Kai Powers mystic powers", "kai"),
            ("Mystic mystics", "mystics"),
            ("Missionarys missionaries", "missionaries"),
            ("Gaunts Gaunt Ones", "gaunts"),
            ("Half-elfs halfelfs", "half-elves"),
            ("Elfs elves", "elves"),
            ("high druid", "boss"),
            ("Druids", "druids"),
        ]
        .iter()
        .map(|(w, t)| ((*w).to_owned(), (*t).to_owned()))
        .collect();
        assert_eq!(find(&topics, "Mystic"), Some("mystics"));
        assert_eq!(find(&topics, "Missionary"), Some("missionaries"));
        assert_eq!(find(&topics, "Gaunt One"), Some("gaunts"));
        assert_eq!(find(&topics, "Elf"), Some("elves"));
        assert_eq!(find(&topics, "Half-Elf"), Some("half-elves"));
        assert_eq!(find(&topics, "Druid"), Some("druids"));
        assert_eq!(find(&topics, "Ogre"), None);
    }
}
