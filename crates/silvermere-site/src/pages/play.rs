//! How to play MajorMUD: the guide for someone who has never seen it, and a reminder for
//! someone who has not played in twenty years.

use super::telnet_command;
use crate::Site;
use crate::html::{esc, shell, slug};

/// The guide's sections: (heading, HTML).
const SECTIONS: &[(&str, &str)] = &[
    (
        "What this is",
        r#"<p>MajorMUD is a multi-player fantasy game played entirely in text. You move through tens of thousands of rooms by typing directions, fight what lives there, collect better equipment, learn spells and train your character up through the levels, alone or in a party.</p>"#,
    ),
    (
        "Getting in",
        r#"<p>The client we recommend is <a href="https://github.com/fatavatar/majordomo">Majordomo</a>, for Windows, Mac and Linux. It shows the game in a terminal surrounded by what you need at a glance: your vitals, the fight, your inventory, a map and the chat. Every character in it can be driven by a port of MegaMUD's own engine, so walking, looping, resting and fighting can be automatic, under the settings MegaMUD players already know. Give it <a href="tools.html#mme">this realm's database</a> for maps and routes. Download it from its <a href="https://github.com/fatavatar/majordomo/releases/latest">latest release</a>.</p>
<p>Any telnet client works too: <kbd>telnet</kbd> from a terminal, or Mudlet, SyncTERM or NetRunner. Set it to ANSI colour and an 80-column screen.</p>
<p>At the login prompt, type <kbd>new</kbd> to make an account, then follow the menu into the game. Your account can hold characters; each character is one adventurer.</p>"#,
    ),
    (
        "Making a character",
        r#"<p>You choose a <a href="races.html">race</a> and a <a href="classes.html">class</a>, then spend character points (CP) on six stats: strength, intellect, willpower, agility, health and charm. Your race sets the floor and ceiling of each stat; your class decides what you can equip and cast.</p>
<p>Some advice for a first character: a warrior-type (Warrior, Paladin, Witchunter) is the gentlest start, because it hits hard, wears heavy armour and needs no spells. Put points into strength and health. Mages and priests are rewarding but fragile early. Thieves and ninjas live by stealth and backstabs, which takes practice.</p>
<p>Each class and race also changes how much experience a level costs. The <a href="classes.html#experience">experience calculator</a> shows the totals for any pairing.</p>"#,
    ),
    (
        "Reading the screen",
        r#"<p>A room looks like this:</p>
{SCREEN}
<p>The bright cyan line is the room's name, and the indented paragraph under it describes it. <em>You notice</em> lists what's on the floor; <em>Also here</em> lists the players and creatures with you; <em>Obvious exits</em> are the ways out you can see. There can be others you can't see.</p>
<p>The prompt shows your hit points (HP) and mana (MA). When your hit points drop, stop and <kbd>rest</kbd>. While you rest the prompt says <em>(Resting)</em>.</p>"#,
    ),
    (
        "Moving and looking",
        r#"<p>Type a direction to walk: <kbd>n</kbd> <kbd>s</kbd> <kbd>e</kbd> <kbd>w</kbd> <kbd>ne</kbd> <kbd>nw</kbd> <kbd>se</kbd> <kbd>sw</kbd> <kbd>u</kbd> <kbd>d</kbd>. Pressing Enter on an empty line looks at the room again.</p>
<p><kbd>look</kbd> (or <kbd>l</kbd>) followed by something looks at it: a creature, an item, a direction. <kbd>search</kbd> finds hidden exits and things. <kbd>exits</kbd> lists the ways out.</p>
<p>Doors are opened with <kbd>open north</kbd> and so on. A locked one needs its key (<kbd>use key north</kbd>), a thief's <kbd>picklock north</kbd>, or a strong shoulder: <kbd>bash north</kbd>.</p>"#,
    ),
    (
        "Fighting",
        r#"<p><kbd>attack</kbd> (or <kbd>a</kbd>) followed by a name starts a fight; you keep swinging each round until it dies, you die, or you <kbd>break</kbd> off. <kbd>bash</kbd> and <kbd>smash</kbd> trade accuracy for heavier blows. A hidden thief opens with <kbd>backstab</kbd> (<kbd>bs</kbd>).</p>
<p>To cast a spell, type its short name, and the target when it needs one: <kbd>mmis rat</kbd> casts magic missile at the rat, and <kbd>mend</kbd> alone heals yourself (<kbd>mend Thornwyn</kbd> heals Thornwyn). <kbd>spells</kbd> lists the ones you know, with their short names.</p>
<p>Leaving is a fight's escape hatch: walk out of the room. Not everything lets you go.</p>
<p>Killing earns experience. Creatures carry and drop coins and items; <kbd>get</kbd> them before someone else does.</p>"#,
    ),
    (
        "Dying",
        r#"<p>At zero hit points you fall unconscious. In this realm you die only when your hit points reach {DEATH_HP}; until then you are out cold, and can still be saved. Dying drops everything you were carrying, coins and equipment, where you fell, costs you a life, and wakes you in the temple with full health. Go back for your things before someone else does, and don't go back the way that killed you.</p>
<p><kbd>status</kbd> shows how many lives you have left. A character with no lives left is gone for good.</p>"#,
    ),
    (
        "Experience and training",
        r#"<p>Experience doesn't raise your level by itself. When <kbd>exp</kbd> says you have enough, go to a training room (your class's guild, or the town's) and type <kbd>train</kbd>. Training a level raises your hit points and gives you character points; <kbd>train stats</kbd> in a training room spends them.</p>
<p>Don't put training off for long: past a point the game stops giving you experience until you train.</p>"#,
    ),
    (
        "Equipment and money",
        r#"<p><kbd>i</kbd> shows what you carry and what you have on. <kbd>eq</kbd> equips anything: <kbd>eq leather boots</kbd>, <kbd>eq shortsword</kbd>, <kbd>eq torch</kbd>; <kbd>remove</kbd> takes it off again. Carry a light: the dark is darker than you expect. <kbd>use</kbd>, <kbd>read</kbd>, <kbd>eat</kbd> and <kbd>drink</kbd> do what they say.</p>
<p>Shops: <kbd>list</kbd> shows the stock, then <kbd>buy</kbd> and <kbd>sell</kbd>. Coins come as copper, silver, gold, platinum and {TOP_COIN}; <kbd>wealth</kbd> shows yours. Coins are heavy and can be stolen, so <kbd>deposit</kbd> them at a bank, <kbd>withdraw</kbd> when you need them, and check your <kbd>balance</kbd>.</p>"#,
    ),
    (
        "Other players",
        r#"<p>Anything you type that isn't a command is said aloud to the room. <kbd>/name message</kbd> telepaths one player wherever they are; <kbd>gossip message</kbd> talks to everyone who has gossip on. <kbd>who</kbd> lists who's playing.</p>
<p>A party shares the fighting: <kbd>invite name</kbd> and they <kbd>join</kbd> you; <kbd>party</kbd> shows the group, <kbd>leave</kbd> leaves it. Gangs are longer-lived; ask around.</p>"#,
    ),
];

/// The `sys` commands, for the realm's staff.
const SYS: &str = r#"<section class="guide" id="sys">
<h2>Sys commands</h2>
<p>The realm's sysops and game operators have commands of their own, each after <kbd>sys</kbd>. Players can't use them. Two matter most:</p>
<div class="commands"><div class="cmd-group"><dl>
<dt><kbd>sys map</kbd></dt><dd>draws a map of the rooms around you</dd>
<dt><kbd>sys goto place</kbd></dt><dd>takes you to a town: <kbd>newhaven</kbd>, <kbd>silvermere</kbd> (where the dead are reborn), <kbd>rhudaur</kbd>, <kbd>khazarad</kbd> or <kbd>lostcity</kbd>; the sysop has <kbd>support</kbd> too. Not in the middle of a fight.</dd>
</dl></div></div>
</section>
"#;

/// The guide's line on making an account, replaced where players cannot.
const SIGNUP: &str = "At the login prompt, type <kbd>new</kbd> to make an account, then follow the menu into the game.";

/// A reference of the commands a new player needs, by what they are for.
const COMMANDS: &[(&str, &[(&str, &str)])] = &[
    (
        "Getting around",
        &[
            ("n s e w ne nw se sw u d", "walk"),
            ("l, look thing", "look at the room, or at something"),
            ("search", "look for hidden things"),
            ("sneak, hide", "move or wait unseen"),
        ],
    ),
    (
        "Fighting and magic",
        &[
            ("a name", "attack"),
            ("bs name", "backstab, from hiding"),
            ("break", "stop fighting"),
            ("mmis rat", "a spell: its short name, then the target"),
            ("spells", "your spells"),
            ("rest, meditate", "recover hit points and mana"),
        ],
    ),
    (
        "You",
        &[
            ("st", "status: stats, lives"),
            ("exp", "experience, and what the next level needs"),
            ("he", "health"),
            ("i", "inventory"),
            ("train, train stats", "a level; your character points"),
        ],
    ),
    (
        "Things and money",
        &[
            ("get, drop, give", "pick up, put down, hand over"),
            (
                "eq item, remove item",
                "equip anything (a torch too); take it off",
            ),
            ("list, buy, sell", "in a shop"),
            ("deposit, withdraw, balance", "at a bank"),
        ],
    ),
    (
        "People",
        &[
            ("anything else", "said aloud"),
            ("/name message", "telepath"),
            ("gossip message", "everyone listening"),
            ("who", "who is playing"),
            ("invite, join, party", "a party"),
            ("help, ?", "the game's own help"),
            ("q", "quit"),
        ],
    ),
];

pub fn render(site: &Site) -> String {
    let mut body = String::new();
    let mut sections: Vec<(String, String)> = Vec::new();
    for (heading, html) in SECTIONS {
        let id = slug(heading);
        sections.push((id.clone(), (*heading).to_lowercase()));
        let html = if site.open_signup {
            (*html).to_owned()
        } else {
            html.replace(
                SIGNUP,
                "Ask the sysop for an account, then log in and follow the menu into the game.",
            )
        };
        let html = html
            .replace("{SCREEN}", &sample_screen(site))
            .replace("{DEATH_HP}", &signed_thousands(site.death_hp))
            .replace("{TOP_COIN}", &esc(&site.top_coin));
        body.push_str(&format!(
            "<section class=\"guide\" id=\"{id}\">\n<h2>{}</h2>\n{html}\n</section>\n",
            esc(heading)
        ));
        if *heading == "Getting in"
            && let Some(connect) = &site.connect
        {
            body.push_str(&format!(
                "<p class=\"callout\">For this realm: <kbd>{}</kbd></p>\n",
                esc(&telnet_command(connect))
            ));
        }
    }
    sections.push(("commands".into(), "commands".into()));
    body.push_str("<section class=\"guide\" id=\"commands\">\n<h2>Commands to start with</h2>\n<p>Most commands can be shortened to their first letters. These are enough for a first evening; <kbd>help</kbd> in the game lists the rest.</p>\n<div class=\"commands\">\n");
    for (group, rows) in COMMANDS {
        body.push_str(&format!(
            "<div class=\"cmd-group\"><h3>{}</h3>\n<dl>\n",
            esc(group)
        ));
        for (cmd, what) in *rows {
            body.push_str(&format!(
                "<dt><kbd>{}</kbd></dt><dd>{}</dd>\n",
                esc(cmd),
                esc(what)
            ));
        }
        body.push_str("</dl></div>\n");
    }
    body.push_str("</div>\n</section>\n");
    sections.push(("sys".into(), "sys commands".into()));
    body.push_str(SYS);
    shell(
        site,
        "play.html",
        "How to play",
        "How the game works, and the commands to start with.",
        &sections,
        &body,
    )
}

/// A number as the guide writes it: thousands separated, a true minus sign.
fn signed_thousands(n: i32) -> String {
    let digits = crate::html::thousands(u64::from(n.unsigned_abs()));
    if n < 0 {
        format!("−{digits}")
    } else {
        digits
    }
}

/// A room as the game shows it, with something on the floor and someone else there: the realm's
/// starting room when it has one.
fn sample_screen(site: &Site) -> String {
    let fallback = vec!["You are standing in a small, cluttered shop.".to_owned()];
    let (name, description, exits) = match &site.start_room {
        Some(room) => (room.name.as_str(), &room.description, room.exits.join(", ")),
        None => ("Newhaven, Healer", &fallback, "north".to_owned()),
    };
    crate::screen::room(&crate::screen::RoomScreen {
        name,
        description,
        items: Some("a small potion"),
        players: Some("Thornwyn"),
        exits: (!exits.is_empty()).then_some(exits.as_str()),
        prompt: (24, 10),
    })
}
