//! The pages a small realm gets: every page there, the realm's words escaped, news in order,
//! and the experience tables the calculator reads.

use silvermere_import::overlay::build::{blank_class, blank_race};
use silvermere_site::{MmeFile, NewsKind, NewsView, OverlayView, Page, RoomView, Site, render};
use silvermere_types::enums::{Ability, Magery};

fn site() -> Site {
    let mut warrior = blank_class(1);
    warrior.name = "Warrior".into();
    warrior.basehp = 6;
    warrior.hplevel = 2;
    warrior.combat = 6;
    warrior.abilities[0] = (Ability::from_code(31), 0);
    let mut mage = blank_class(2);
    mage.name = "Mage".into();
    mage.exp = 50;
    mage.magery = Magery::from_code(1);
    mage.magelvl = 3;
    let mut human = blank_race(1);
    human.name = "Human".into();
    human.maxstr = 200;
    let mut quick = blank_race(2);
    quick.name = "Quickling".into();
    quick.hpbonus = -1;
    quick.exp = 50;
    let news = |date: &str, kind, title: &str| NewsView {
        date: date.into(),
        title: title.into(),
        kind,
        text: "First paragraph, `look`.\n\nSecond.".into(),
    };
    Site {
        realm: "Realm <Of> Legends".into(),
        tagline: "Since the dial-up days".into(),
        connect: Some("telnet://realm.example.org:2323".into()),
        open_signup: true,
        death_hp: -20000,
        top_coin: "Krabby Patties".into(),
        built: "2026-11-10".into(),
        version: "0.1.13".into(),
        start_room: Some(RoomView {
            name: "Newhaven, Village Entrance".into(),
            description: vec!["Welcome to".into(), "Newhaven!".into(), String::new()],
            exits: vec!["north".into(), "south".into()],
        }),
        classes: vec![warrior, mage],
        races: vec![human, quick],
        help_topics: vec![(
            "Mages magi".into(),
            "\u{1b}[2J
╔══════════════════════════════════════════╗
║ MAGES                          EXP: 150% ║
╠══════════════════════════════════════════╣
║ Mages study the <arcane> arts.           ║
╠══════════════════════════════════════════╣
║ HP/Level: 3-5 Magic: Mage-3 Combat: 1    ║
╠══════════════════════════════════════════╣
║ Abilities: None.                         ║
╚══════════════════════════════════════════╝"
                .into(),
        )],
        overlays: vec![
            OverlayView {
                name: "mudrev".into(),
                title: "MudRev".into(),
                description: "The world.".into(),
                news: vec![news("2026-11-09", NewsKind::Change, "Moved")],
                created: vec![("room".into(), 45_851), ("text".into(), 3)],
                ..Default::default()
            },
            OverlayView {
                name: "ashfall-valley".into(),
                news: vec![
                    news("2026-11-09", NewsKind::New, "Something stirs"),
                    news("2026-11-10", NewsKind::Fix, "Lairs"),
                ],
                ..Default::default()
            },
        ],
        mme: Some(MmeFile {
            file: "realm.mdb".into(),
            bytes: 18_000_000,
            tables: vec![("Items".into(), 3180)],
        }),
    }
}

fn page<'a>(pages: &'a [Page], path: &str) -> &'a str {
    let p = pages
        .iter()
        .find(|p| p.path == path)
        .unwrap_or_else(|| panic!("no {path}"));
    std::str::from_utf8(&p.bytes).expect("utf-8")
}

#[test]
fn every_page_is_made() {
    let pages = render(&site());
    for path in [
        "index.html",
        "play.html",
        "classes.html",
        "races.html",
        "world.html",
        "news.html",
        "tools.html",
        "assets/site.css",
        "assets/site.js",
        "assets/exp.js",
    ] {
        assert!(!page(&pages, path).is_empty(), "{path}");
    }
}

#[test]
fn the_home_page_shows_the_room_and_how_to_connect() {
    let pages = render(&site());
    let home = page(&pages, "index.html");
    assert!(home.contains("Realm &lt;Of&gt; Legends") && !home.contains("Realm <Of>"));
    assert!(
        home.contains("Welcome to Newhaven!"),
        "description lines joined"
    );
    assert!(home.contains("Obvious exits: north, south"));
    assert!(home.contains("telnet realm.example.org 2323"));
    assert!(home.contains("href=\"telnet://realm.example.org:2323\""));
    assert!(home.contains("<kbd>new</kbd>"));
    // the newest first, and on one day new places before changes
    let lairs = home.find("Lairs").expect("lairs");
    let stirs = home.find("Something stirs").expect("stirs");
    let moved = home.find("Moved").expect("moved");
    assert!(lairs < stirs && stirs < moved);
}

#[test]
fn without_open_signup_players_are_sent_to_the_sysop() {
    let mut s = site();
    s.open_signup = false;
    let pages = render(&s);
    assert!(!page(&pages, "index.html").contains("<kbd>new</kbd>"));
    assert!(!page(&pages, "play.html").contains("<kbd>new</kbd>"));
}

#[test]
fn classes_and_races_in_players_words() {
    let pages = render(&site());
    let classes = page(&pages, "classes.html");
    // the warrior has no help screen: its record speaks
    assert!(classes.contains("bashes doors"));
    assert!(classes.contains("<dd>6–8</dd>"));
    // the mage's help screen speaks for it, escaped
    assert!(classes.contains("Mages study the &lt;arcane&gt; arts."));
    assert!(classes.contains("<dd>Mage-3</dd>") && classes.contains("<dd>3–5</dd>"));
    assert!(classes.contains("Exp 150%"));
    let races = page(&pages, "races.html");
    assert!(races.contains("<dd>−1</dd>") && !races.contains("+-"));
    // the calculator has a table for each class and race sum: 0, 50 and 100
    let exp = page(&pages, "assets/exp.js");
    for m in ["\"0\":[", "\"50\":[", "\"100\":["] {
        assert!(exp.contains(m), "{m}");
    }
}

#[test]
fn the_world_lists_what_players_count() {
    let pages = render(&site());
    let world = page(&pages, "world.html");
    assert!(world.contains("MudRev") && world.contains("45,851 rooms"));
    assert!(!world.contains("3 texts"));
    assert!(world.contains("<kbd>look</kbd>"));
    let tools = page(&pages, "tools.html");
    assert!(tools.contains("href=\"realm.mdb\"") && tools.contains("3,180"));
}

#[test]
fn the_guide_uses_the_realms_numbers_and_screens() {
    let pages = render(&site());
    let play = page(&pages, "play.html");
    assert!(
        play.contains("reach −20,000"),
        "the realm's death hit points"
    );
    assert!(play.contains("<pre class=\"terminal\""));
    assert!(play.contains("You notice a small potion here."));
    assert!(play.contains("<kbd>mmis rat</kbd>") && !play.contains("<kbd>cast</kbd>"));
    assert!(play.contains("platinum and Krabby Patties") && !play.contains("runic"));
    assert!(play.contains("<kbd>eq torch</kbd>") && !play.contains("<kbd>wear</kbd>"));
    assert!(play.contains("<kbd>sys goto place</kbd>") && !play.contains("<kbd>map</kbd>"));
    let home = page(&pages, "index.html");
    assert!(
        home.contains("<pre class=\"terminal\"") && home.contains("Newhaven, Village Entrance")
    );
}
