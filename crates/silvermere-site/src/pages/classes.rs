//! The classes, side by side, and the experience calculator.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use silvermere_rules::progression::exp_needed_for_level;
use silvermere_types::enums::Magery;
use silvermere_types::records::Class;

use super::abilities;
use crate::Site;
use crate::help;
use crate::html::{esc, shell};

/// The highest level the calculator shows.
pub const MAX_LEVEL: u16 = 250;

fn magic(class: &Class) -> String {
    if class.magery == Magery::None || class.magery.code() == 0 {
        "None".to_owned()
    } else {
        format!(
            "{}-{}",
            class.magery.name().unwrap_or("Magic"),
            class.magelvl
        )
    }
}

fn weapons(class: &Class) -> String {
    class.weapon.name().unwrap_or("?").to_owned()
}

fn armour(class: &Class) -> String {
    class.armour.name().unwrap_or("?").to_owned()
}

fn hp(class: &Class) -> String {
    if class.hplevel > 0 {
        format!("{}–{}", class.basehp, class.basehp + class.hplevel)
    } else {
        class.basehp.to_string()
    }
}

/// A class as the site shows it: its help screen's words where it has one, its record's
/// otherwise.
struct Profile {
    exp: String,
    /// For sorting: the experience percentage as a number.
    exp_n: i32,
    description: Option<String>,
    hp: String,
    magic: String,
    combat: String,
    weapons: String,
    armour: String,
    abilities: Vec<String>,
    notes: Vec<String>,
}

fn profile(site: &Site, class: &Class) -> Profile {
    let help = help::find(&site.help_topics, &class.name).and_then(help::parse);
    let fact = |key: &str| {
        help.as_ref()
            .and_then(|h| h.fact(key))
            .map(|v| v.replace('-', "–"))
    };
    let fact_raw = |key: &str| help.as_ref().and_then(|h| h.fact(key)).map(str::to_owned);
    let exp_n = 100 + i32::from(class.exp);
    Profile {
        exp: help
            .as_ref()
            .and_then(|h| h.exp.clone())
            .unwrap_or_else(|| format!("{exp_n}%")),
        exp_n,
        description: help
            .as_ref()
            .map(|h| h.description.clone())
            .filter(|d| !d.is_empty()),
        hp: fact("HP/Level").unwrap_or_else(|| hp(class)),
        magic: fact_raw("Magic").unwrap_or_else(|| magic(class)),
        combat: fact_raw("Combat").unwrap_or_else(|| class.combat.to_string()),
        weapons: fact_raw("Weapons").unwrap_or_else(|| weapons(class)),
        armour: fact_raw("Armour").unwrap_or_else(|| armour(class)),
        abilities: match &help {
            Some(h) => h.abilities.clone(),
            None => abilities::list(&class.abilities),
        },
        notes: help.map(|h| h.notes).unwrap_or_default(),
    }
}

pub fn render(site: &Site) -> String {
    let mut body = String::new();
    let profiles: Vec<Profile> = site.classes.iter().map(|c| profile(site, c)).collect();

    body.push_str("<section id=\"all\" class=\"entries\" aria-label=\"The classes\">\n");
    for (c, p) in site.classes.iter().zip(&profiles) {
        let _ = write!(
            body,
            r#"<article class="entry" id="class-{n}">
<div class="entry-head"><h2>{name}</h2><span class="exp" title="Experience a level costs">Exp {exp}</span></div>
{desc}<dl class="stats">
<div><dt>HP/level</dt><dd>{hp}</dd></div>
<div><dt>Magic</dt><dd>{magic}</dd></div>
<div><dt>Combat</dt><dd>{combat}</dd></div>
<div><dt>Weapons</dt><dd>{weapons}</dd></div>
<div><dt>Armour</dt><dd>{armour}</dd></div>
</dl>
{abil}{notes}</article>
"#,
            n = c.number.0,
            name = esc(&c.name),
            exp = esc(&p.exp),
            desc = p
                .description
                .as_ref()
                .map(|d| format!("<p>{}</p>\n", esc(d)))
                .unwrap_or_default(),
            hp = esc(&p.hp),
            magic = esc(&p.magic),
            combat = esc(&p.combat),
            weapons = esc(&p.weapons),
            armour = esc(&p.armour),
            abil = if p.abilities.is_empty() {
                String::new()
            } else {
                format!(
                    "<p class=\"abilities\"><span class=\"label\">Abilities</span>{}</p>\n",
                    esc(&p.abilities.join(", "))
                )
            },
            notes = p
                .notes
                .iter()
                .map(|n| format!("<p class=\"small\">{}</p>\n", esc(n)))
                .collect::<String>(),
        );
    }
    body.push_str("</section>\n");

    body.push_str(
        "<section id=\"compare\">\n<h2>Side by side</h2>\n<p class=\"note\">Select a heading to sort by it.</p>\n<div class=\"table-wrap\"><table class=\"sortable\">\n<thead><tr><th scope=\"col\">Class</th><th scope=\"col\" data-type=\"num\">Exp</th><th scope=\"col\">HP/level</th><th scope=\"col\">Magic</th><th scope=\"col\">Combat</th><th scope=\"col\">Weapons</th><th scope=\"col\">Armour</th></tr></thead>\n<tbody>\n",
    );
    for (c, p) in site.classes.iter().zip(&profiles) {
        let _ = writeln!(
            body,
            "<tr><th scope=\"row\"><a href=\"#class-{n}\">{name}</a></th><td data-sort=\"{exp_n}\">{exp}</td><td>{hp}</td><td>{magic}</td><td>{combat}</td><td>{weapons}</td><td>{armour}</td></tr>",
            n = c.number.0,
            name = esc(&c.name),
            exp_n = p.exp_n,
            exp = esc(&p.exp),
            hp = esc(&p.hp),
            magic = esc(&p.magic),
            combat = esc(&p.combat),
            weapons = esc(&p.weapons),
            armour = esc(&p.armour),
        );
    }
    body.push_str("</tbody></table></div>\n</section>\n");

    body.push_str(&calculator(site));

    shell(
        site,
        "classes.html",
        "Classes",
        "Every class in this realm.",
        &[
            ("all".into(), "the classes".into()),
            ("compare".into(), "side by side".into()),
            ("experience".into(), "experience calculator".into()),
        ],
        &body,
    )
}

fn calculator(site: &Site) -> String {
    let mut s = String::from(
        r#"<section id="experience" class="calc">
<h2>Experience calculator</h2>
<p>A level costs more or less experience depending on your class and race, together. Pick both to see what each level needs.</p>
<form class="calc-form" onsubmit="return false">
<label>Class <select id="calc-class">"#,
    );
    for c in &site.classes {
        let _ = write!(s, "<option value=\"{}\">{}</option>", c.exp, esc(&c.name));
    }
    s.push_str("</select></label>\n<label>Race <select id=\"calc-race\">");
    for r in &site.races {
        let _ = write!(s, "<option value=\"{}\">{}</option>", r.exp, esc(&r.name));
    }
    let _ = write!(
        s,
        r#"</select></label>
<label>From level <input id="calc-from" type="number" min="1" max="{MAX_LEVEL}" value="1"></label>
<label>To level <input id="calc-to" type="number" min="1" max="{MAX_LEVEL}" value="30"></label>
</form>
<p class="calc-total" id="calc-total" aria-live="polite"></p>
<div class="table-wrap"><table class="calc-table"><thead><tr><th scope="col">Level</th><th scope="col">Experience to reach it</th><th scope="col">From the level before</th></tr></thead><tbody id="calc-rows"></tbody></table></div>
<noscript><p>The calculator needs JavaScript.</p></noscript>
<script src="assets/exp.js" defer></script>
</section>
"#
    );
    s
}

/// `assets/exp.js`: the experience each level needs, for every class and race modifier the
/// realm has, computed with the game's own formula.
pub fn exp_data(site: &Site) -> String {
    let mut mods = BTreeSet::new();
    for c in &site.classes {
        for r in &site.races {
            mods.insert(c.exp.wrapping_add(r.exp));
        }
    }
    let mut s = format!("window.SM_EXP={{max:{MAX_LEVEL},tables:{{");
    for (i, m) in mods.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        let _ = write!(s, "\"{m}\":[");
        for level in 0..=MAX_LEVEL {
            if level > 0 {
                s.push(',');
            }
            let _ = write!(s, "{}", exp_needed_for_level(level, *m));
        }
        s.push(']');
    }
    s.push_str("}};\n");
    s
}
