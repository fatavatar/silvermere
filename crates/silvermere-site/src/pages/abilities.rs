//! Abilities in players' words.
//!
//! The ability table's names are the editor's ("Alter Defence Value"); a player knows these
//! as armour class, stealth, picklocks. The common ones a class or race carries get a short
//! phrase here; any other keeps the table's name and its value.

use silvermere_types::enums::Ability;

/// A class's or race's ability, as a phrase; `None` for an empty slot.
pub fn describe(ability: Ability, value: i16) -> Option<String> {
    let code = ability.code();
    if code == 0 {
        return None;
    }
    let signed = |n: i16| {
        if n >= 0 {
            format!("+{n}")
        } else {
            n.to_string()
        }
    };
    let text = match code {
        2 => format!("{} armour class", signed(value)),
        3 => format!("{}% cold resistance", signed(value)),
        4 => format!("{} damage", signed(value)),
        5 => format!("{}% fire resistance", signed(value)),
        7 => format!("{} damage resistance", signed(value)),
        13 => format!("{} sight in darkness", signed(value)),
        21 => "immune to poison".to_owned(),
        22 => format!("{} accuracy", signed(value)),
        29 => "punches".to_owned(),
        30 => "kicks".to_owned(),
        31 => "bashes doors".to_owned(),
        34 => format!("{} dodge", signed(value)),
        35 => "jumpkicks".to_owned(),
        36 => format!("{} magic resistance", signed(value)),
        37 if value != 0 => format!("picks locks ({})", signed(value)),
        37 => "picks locks".to_owned(),
        38 => "tracks".to_owned(),
        40 => "finds traps".to_owned(),
        51 => "anti-magic".to_owned(),
        57 => "sees hidden things".to_owned(),
        58 => format!("{}% critical hits", signed(value)),
        // the item a druid may use beyond its armour limit: an internal detail
        59 => return None,
        69 => format!("{} mana", signed(value)),
        77 => format!("{} perception", signed(value)),
        96 => format!("{}% carrying capacity", signed(value)),
        102 | 103 => "stealth".to_owned(),
        106 => format!("{} accuracy", signed(value)),
        116 => format!("{} backstab accuracy", signed(value)),
        117 => format!("{} minimum backstab damage", signed(value)),
        118 => format!("{} maximum backstab damage", signed(value)),
        123 => format!("{}% healing", signed(value)),
        142 if value > 0 => format!("hits magical creatures (level {value})"),
        142 => "hits magical creatures".to_owned(),
        145 => format!("{}% mana regeneration", signed(value)),
        147 => format!("{}% water resistance", signed(value)),
        186 => "perfect stealth".to_owned(),
        187 => "meditates".to_owned(),
        _ => {
            let name = ability.name().unwrap_or("Ability");
            if value == 0 {
                name.to_owned()
            } else {
                format!("{name} {}", signed(value))
            }
        }
    };
    Some(text)
}

/// Every ability of a list, in order, without empty slots or repeats.
pub fn list(slots: &[(Ability, i16)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &(a, v) in slots {
        if let Some(s) = describe(a, v)
            && !out.contains(&s)
        {
            out.push(s);
        }
    }
    out
}
