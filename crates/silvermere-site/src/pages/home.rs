//! The home page: where to begin, the first room a new character sees (as boxed text, the
//! way a module gives what is read aloud), how to connect, and the latest of the chronicle.

use std::fmt::Write as _;

use super::{MAJORDOMO_RELEASES, all_news, news_item, telnet_command, telnet_url};
use crate::Site;
use crate::html::{esc, inline, shell};

pub fn render(site: &Site) -> String {
    let mut main = String::new();
    main.push_str(&boxed_room(site));
    main.push_str(
        r#"<h2>Where to begin</h2>
<ol class="begin">
<li><div><strong><a href="play.html">Learn how to play</a></strong><p>How the game works, and the commands to start with.</p></div></li>
<li><div><strong><a href="classes.html">Choose who you'll be</a></strong><p>Every class and race, and what each costs in experience.</p></div></li>
<li><div><strong><a href="tools.html">Get a client</a></strong><p>Majordomo, and this realm's database for it and for MajorMUD Explorer.</p></div></li>
</ol>
"#,
    );
    let news = all_news(site);
    if !news.is_empty() {
        main.push_str("<h2 id=\"chronicle\">From the chronicle</h2>\n");
        for (overlay, n) in news.iter().take(4) {
            main.push_str(&news_item(overlay, n, "h3"));
        }
        main.push_str("<p class=\"more\"><a href=\"news.html\">The whole chronicle</a></p>\n");
    }

    let body = format!(
        "<div class=\"columns\">\n<div>\n{main}</div>\n{}</div>\n",
        sidebar(site)
    );
    let lede = if site.tagline.is_empty() {
        "Everything a player needs."
    } else {
        &site.tagline
    };
    shell(site, "index.html", "Players' handbook", lede, &[], &body)
}

/// The start room as a new character first sees it, in the game's own output.
fn boxed_room(site: &Site) -> String {
    let Some(room) = &site.start_room else {
        return String::new();
    };
    let exits = room.exits.join(", ");
    let screen = crate::screen::room(&crate::screen::RoomScreen {
        name: &room.name,
        description: &room.description,
        items: None,
        players: None,
        exits: (!exits.is_empty()).then_some(exits.as_str()),
        prompt: (20, 8),
    });
    format!(
        "<figure class=\"screen\">\n<figcaption>The first room you will see</figcaption>\n{screen}\n</figure>\n"
    )
}

/// How to play here: the client, the address, the account.
fn sidebar(site: &Site) -> String {
    let mut s = String::from(
        "<aside class=\"sidebar\" aria-labelledby=\"to-play\">\n<h2 id=\"to-play\">To play</h2>\n",
    );
    let _ = write!(
        s,
        r#"<p>Use <a href="tools.html#majordomo">Majordomo</a>, a MajorMUD client for Windows, Mac and Linux.</p>
<div class="buttons"><a class="button" href="{MAJORDOMO_RELEASES}">Get Majordomo</a></div>
"#
    );
    match &site.connect {
        Some(connect) => {
            let cmd = telnet_command(connect);
            let _ = write!(
                s,
                r#"<p>Point it at this realm, or use any telnet client:</p>
<code class="command" id="connect-cmd">{cmd}</code>
<div class="buttons"><a class="button quiet" href="{url}">Open in telnet</a><button class="button quiet" type="button" data-copy="connect-cmd">Copy</button></div>
"#,
                cmd = esc(&cmd),
                url = esc(&telnet_url(connect)),
            );
        }
        None => s.push_str("<p class=\"note\">Ask the sysop for this realm's address.</p>\n"),
    }
    if site.open_signup {
        let _ = writeln!(
            s,
            "<p>{}</p>",
            inline("At the login prompt, type `new` to make an account.")
        );
    } else {
        s.push_str("<p>Ask the sysop for an account.</p>\n");
    }
    s.push_str("</aside>\n");
    s
}
