//! Tools: the realm's MajorMUD Explorer database, and how to use it.

use std::fmt::Write as _;

use crate::Site;
use crate::html::{esc, shell, thousands};

pub fn render(site: &Site) -> String {
    let mut body = String::new();
    body.push_str(
        r#"<section id="majordomo">
<h2>Majordomo</h2>
<p><a href="https://github.com/fatavatar/majordomo">Majordomo</a> is a MajorMUD client for Windows, Mac and Linux. Around the game's own terminal it shows your vitals, the fight in progress, your inventory, a map and the chat, and every character in it can be driven by a port of MegaMUD's own engine: paths, loops, resting and combat under MegaMUD's settings. It brings over a MegaMUD player's settings, databases and paths.</p>
<div class="buttons"><a class="button" href="https://github.com/fatavatar/majordomo/releases/latest">Download Majordomo</a> <a class="button quiet" href="https://github.com/fatavatar/majordomo/blob/main/docs/getting-started.md">Getting started</a></div>
<p>Majordomo reads the same database as MajorMUD Explorer, below: give it this realm's as the realm's <em>World database</em> and it knows this world's rooms, for the map and for routes.</p>
<p class="note">Majordomo is a fork of Raymond Bennett's mudengine, and plays with a port of MegaMUD's engine; neither project made or endorses it.</p>
</section>
<section id="mme">
<h2>MajorMUD Explorer</h2>
<p><a href="https://github.com/syntax53/MMUD-Explorer">MajorMUD Explorer</a> (MME), by syntax53, lists every item, creature, spell, shop and room, with maps, a character planner and experience-per-hour estimates. It needs the realm's own database.</p>
"#,
    );
    match &site.mme {
        Some(m) => {
            let mb = m.bytes as f64 / (1024.0 * 1024.0);
            let _ = write!(
                body,
                r#"<p class="download"><a class="button" href="{file}" download>Download this realm's database</a> <span class="size">{file_shown}, {mb:.1} MB</span></p>
<p>Made from this server's world, and rebuilt each time the server starts.</p>
"#,
                file = esc(&m.file),
                file_shown = esc(&m.file),
            );
            if !m.tables.is_empty() {
                body.push_str("<dl class=\"facts inline\">\n");
                for (table, rows) in &m.tables {
                    let _ = writeln!(body, "<dt>{}</dt><dd>{}</dd>", esc(table), thousands(*rows as u64));
                }
                body.push_str("</dl>\n");
            }
        }
        None => body.push_str("<p class=\"note\">This realm's database is not available yet. The server makes it when it starts; check back shortly.</p>\n"),
    }
    body.push_str(
        r#"<h3>Using it</h3>
<ol class="steps">
<li>Install MajorMUD Explorer from <a href="https://github.com/syntax53/MMUD-Explorer/releases">its releases page</a>. It runs on Windows, and under Wine elsewhere.</li>
<li>Save the database above anywhere you like.</li>
<li>In MME, choose <em>File → Open Data File</em> and pick the file. MME remembers it under <em>Recent Databases</em>.</li>
</ol>
<p class="note">The database is made by silvermere, which follows Nightmare Redux, syntax53's own exporter. Derived figures such as average damage are worked out the same way, but may differ a little from a Nightmare Redux export.</p>
</section>
"#,
    );
    shell(
        site,
        "tools.html",
        "Tools",
        "A client, and this realm's database.",
        &[
            ("majordomo".into(), "Majordomo".into()),
            ("mme".into(), "MajorMUD Explorer".into()),
        ],
        &body,
    )
}
