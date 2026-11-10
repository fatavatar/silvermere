//! The world's overlays: what each one is, what it holds, who made it, and its news.

use std::fmt::Write as _;

use super::news_item;
use crate::Site;
use crate::html::{esc, paragraphs, shell, slug, thousands};

/// A record kind as players count it.
fn plural(kind: &str, n: i64) -> String {
    let word = match (kind, n == 1) {
        ("room", true) => "room",
        ("room", false) => "rooms",
        ("monster", true) => "creature",
        ("monster", false) => "creatures",
        ("item", true) => "item",
        ("item", false) => "items",
        ("spell", true) => "spell",
        ("spell", false) => "spells",
        ("shop", true) => "shop",
        ("shop", false) => "shops",
        ("message", true) => "message",
        ("message", false) => "messages",
        ("text", true) => "text",
        ("text", false) => "texts",
        ("class", true) => "class",
        ("class", false) => "classes",
        ("race", true) => "race",
        ("race", false) => "races",
        ("action", true) => "action",
        ("action", false) => "actions",
        (other, _) => other,
    };
    format!("{} {word}", thousands(n.unsigned_abs()))
}

/// The kinds a player counts, in the order they care about them. Texts, messages and actions
/// are the game's own plumbing and are not listed.
const ORDER: [&str; 7] = ["room", "monster", "item", "spell", "shop", "class", "race"];

fn counts(list: &[(String, i64)]) -> String {
    let mut sorted: Vec<&(String, i64)> = list
        .iter()
        .filter(|(k, n)| *n > 0 && ORDER.contains(&k.as_str()))
        .collect();
    sorted.sort_by_key(|(k, _)| ORDER.iter().position(|o| o == k).unwrap_or(ORDER.len()));
    sorted
        .iter()
        .map(|(k, n)| plural(k, *n))
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn render(site: &Site) -> String {
    let mut body = String::from(
        "<p>The stock MajorMUD 1.11p world, with these overlays applied in order.</p>\n",
    );
    let mut sections = Vec::new();
    if site.overlays.is_empty() {
        body.push_str("<p class=\"note\">This realm runs the stock world, with no overlays.</p>\n");
    }
    for (i, o) in site.overlays.iter().enumerate() {
        let id = slug(&o.name);
        sections.push((id.clone(), o.title().to_owned()));
        let _ = write!(
            body,
            "<section class=\"overlay\" id=\"{id}\">\n<p class=\"layer\">Layer {n}</p>\n<h2 class=\"room-small\">{name}</h2>\n",
            n = i + 1,
            name = esc(o.title())
        );
        if !o.description.is_empty() {
            body.push_str(&paragraphs(&o.description));
        }
        let created = counts(&o.created);
        let changed = counts(&o.changed);
        if !created.is_empty() || !changed.is_empty() {
            body.push_str("<dl class=\"facts\">\n");
            if !created.is_empty() {
                let _ = writeln!(body, "<dt>Adds</dt><dd>{}</dd>", esc(&created));
            }
            if !changed.is_empty() {
                let _ = writeln!(body, "<dt>Changes</dt><dd>{}</dd>", esc(&changed));
            }
            body.push_str("</dl>\n");
        }
        if !o.credits.is_empty() {
            let _ = writeln!(
                body,
                "<div class=\"credits\"><h3>Credits</h3>{}</div>",
                paragraphs(&o.credits)
            );
        }
        for n in o.news.iter().take(2) {
            body.push_str(&news_item(o, n, "h3"));
        }
        if o.news.len() > 2 {
            body.push_str("<p class=\"more\"><a href=\"news.html\">More news</a></p>\n");
        }
        body.push_str("</section>\n");
    }
    shell(
        site,
        "world.html",
        "The world",
        "What this realm is built from.",
        &sections,
        &body,
    )
}
