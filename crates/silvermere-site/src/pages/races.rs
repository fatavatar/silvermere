//! The races: their stat ranges, what they add, and what their levels cost.

use std::fmt::Write as _;

use silvermere_types::records::Race;

use super::abilities;
use crate::Site;
use crate::help;
use crate::html::{esc, percent, shell};

/// The six stats in the order the game lists them: (short, name).
const STATS: [(&str, &str); 6] = [
    ("Str", "Strength"),
    ("Int", "Intellect"),
    ("Wil", "Willpower"),
    ("Agl", "Agility"),
    ("Hea", "Health"),
    ("Cha", "Charm"),
];

/// A race's stats in [`STATS`] order: (short, name, minimum, maximum).
fn stats(r: &Race) -> [(&'static str, &'static str, i16, i16); 6] {
    let range = [
        (r.minstr, r.maxstr),
        (r.minint, r.maxint),
        (r.minwil, r.maxwil),
        (r.minagi, r.maxagi),
        (r.minhea, r.maxhea),
        (r.mincha, r.maxcha),
    ];
    std::array::from_fn(|i| (STATS[i].0, STATS[i].1, range[i].0, range[i].1))
}

pub fn render(site: &Site) -> String {
    let mut body = String::new();
    body.push_str(
        "<section id=\"stats\">\n<h2>Stats side by side</h2>\n<p>Each stat starts at the race's minimum and can be trained up to its maximum. The bar shows the range on a common scale.</p>\n<div class=\"table-wrap\"><table class=\"sortable races\">\n<thead><tr><th scope=\"col\">Race</th>",
    );
    for (short, name) in STATS {
        let _ = write!(
            body,
            "<th scope=\"col\" data-type=\"num\"><abbr title=\"{name}\">{short}</abbr></th>"
        );
    }
    body.push_str("<th scope=\"col\" data-type=\"num\">HP/level</th><th scope=\"col\" data-type=\"num\">Exp</th><th scope=\"col\" data-type=\"num\">CP</th></tr></thead>\n<tbody>\n");
    let top = site
        .races
        .iter()
        .flat_map(|r| stats(r).map(|s| s.3))
        .max()
        .unwrap_or(250)
        .max(1);
    for r in &site.races {
        let _ = write!(
            body,
            "<tr><th scope=\"row\"><a href=\"#race-{}\">{}</a></th>",
            r.number.0,
            esc(&r.name)
        );
        for (_, name, min, max) in stats(r) {
            let from = i32::from(min) * 100 / i32::from(top);
            let to = i32::from(max) * 100 / i32::from(top);
            let _ = write!(
                body,
                "<td data-sort=\"{max}\"><span class=\"range\" aria-label=\"{name} {min} to {max}\">{min}–{max}</span><span class=\"bar\" style=\"--from:{from}%;--to:{to}%\" aria-hidden=\"true\"></span></td>"
            );
        }
        let _ = writeln!(
            body,
            "<td data-sort=\"{hp}\">{hps}</td><td data-sort=\"{exp}\">{exps}</td><td data-sort=\"{cp}\">{cp}</td></tr>",
            hp = r.hpbonus,
            hps = signed(r.hpbonus),
            exp = r.exp,
            exps = percent(i32::from(r.exp)),
            cp = r.basecp,
        );
    }
    body.push_str("</tbody></table></div>\n</section>\n");

    let mut cards =
        String::from("<section id=\"all\" class=\"entries\" aria-label=\"The races\">\n");
    for r in &site.races {
        let help = help::find(&site.help_topics, &r.name).and_then(help::parse);
        let abil = match &help {
            Some(h) if !h.abilities.is_empty() => h.abilities.clone(),
            _ => abilities::list(&r.abilities)
                .iter()
                .map(|a| capitalise(a))
                .collect(),
        };
        let exp = help
            .as_ref()
            .and_then(|h| h.exp.clone())
            .unwrap_or_else(|| percent(i32::from(r.exp)));
        let desc = help
            .as_ref()
            .map(|h| h.description.clone())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| format!("Strongest in {}.", best(r)));
        let _ = write!(
            cards,
            r#"<article class="entry" id="race-{n}">
<div class="entry-head"><h2>{name}</h2><span class="exp" title="Experience a level costs">Exp {exp}</span></div>
<p>{desc}</p>
<dl class="stats">
<div><dt>HP/level</dt><dd>{hp}</dd></div>
<div><dt>Start</dt><dd>{cp} CP</dd></div>
</dl>
{abil}</article>
"#,
            n = r.number.0,
            name = esc(&r.name),
            exp = esc(&exp),
            desc = esc(&desc),
            hp = signed(r.hpbonus),
            cp = r.basecp,
            abil = if abil.is_empty() {
                String::new()
            } else {
                format!(
                    "<p class=\"abilities\"><span class=\"label\">Abilities</span>{}</p>\n",
                    esc(&abil.join(", "))
                )
            },
        );
    }
    cards.push_str("</section>\n");
    let body = cards + &body;

    shell(
        site,
        "races.html",
        "Races",
        "Every race in this realm, and its stat ranges.",
        &[
            ("all".into(), "the races".into()),
            ("stats".into(), "stats side by side".into()),
        ],
        &body,
    )
}

/// The race's highest maximums, in words.
fn best(r: &Race) -> String {
    let all = stats(r);
    let top = all.iter().map(|s| s.3).max().unwrap_or(0);
    let names: Vec<&str> = all.iter().filter(|s| s.3 == top).map(|s| s.1).collect();
    let lower: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    match lower.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

fn signed(n: i16) -> String {
    if n >= 0 {
        format!("+{n}")
    } else {
        format!("−{}", -n)
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}
