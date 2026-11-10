//! Content overlays (docs/overlays.md) on the stock world: a small overlay with one record
//! of every kind is planned and written into an imported world, re-applied unchanged, edited
//! and re-applied, and refused where it would overwrite what it does not own.

use std::path::{Path, PathBuf};

use silvermere_db::{Db, TestDb};
use silvermere_import::importer::{ImportOptions, Importer};
use silvermere_import::overlay::apply::Action;
use silvermere_import::overlay::{self, OverlayError};
use silvermere_types::enums::ExitType;
use silvermere_types::ids::{ItemNo, MapId, MessageNo, MonsterNo, RoomId, RoomKey, TextBlockNo};

use crate::stock as stock_data;

async fn stock_world(db: &Db) {
    let opts = ImportOptions::new(stock_data::vir_dir());
    Importer::new(opts, Some(db)).run().await.expect("import");
}

fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("overlay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(d: &Path, file: &str, text: &str) {
    std::fs::write(d.join(file), text).unwrap();
}

/// An overlay named `name` in a fresh directory, with `files`.
fn overlay_dir(name: &str, files: &[(&str, String)]) -> PathBuf {
    let d = dir(name);
    write(&d, "overlay.toml", &format!("name = \"{name}\"\n"));
    for (f, text) in files {
        write(&d, f, text);
    }
    d
}

async fn apply(db: &Db, d: &Path) -> Result<overlay::Plan, OverlayError> {
    let o = overlay::load(d)?;
    let plan = overlay::plan(db, &o).await?;
    overlay::write(db, &plan).await?;
    Ok(plan)
}

fn actions(plan: &overlay::Plan) -> Vec<(String, Action)> {
    plan.records
        .iter()
        .map(|p| (format!("{} {}", p.kind, p.key), p.action))
        .collect()
}

/// The area: two rooms on a new map behind a map-change exit from stock room 1/1, a monster,
/// item, spell, shop, message and text, each new one from a stock template.
fn area(entrance_name: &str, rat_hp: u32, t: Templates) -> Vec<(&'static str, String)> {
    vec![
        (
            "rooms.toml",
            format!(
                r#"
[defaults.room]
minlevel = 1
maxlevel = 2
mongroup = 1
maxmonsters = 2

[[room]]
map = 90
room = 1
name = "Test Vent"
desc = """
A narrow vent drops into warm dark.
"""
exits = {{ up = {{ target = 1, kind = "MapChange", par1 = 1 }}, north = 2 }}
cmdtext = 9901

[[room]]
map = 90
room = 2
name = "Test Basin"
desc = "Ferns."
exits = {{ south = 1 }}
specmon = 9001

[[room_change]]
map = 1
room = 1
why = "the way down"
set = {{ exits = {{ down = {{ target = 1, kind = "MapChange", par1 = 90 }} }} }}
was = {{ name = "{entrance_name}" }}
"#
            ),
        ),
        (
            "things.toml",
            format!(
                r#"
[[monster]]
number = 9001
from = 1
name = "test raptor"
maxhp = {rat_hp}
desc = "A small, quick lizard."
abilities = [["DeathText", 9902]]
deathspell = 0

[[item]]
number = 9001
from = {item}
name = "raptor claw"
desc = "A hooked claw."

[[spell]]
number = 9001
from = 1
name = "vent heat"
msgnum = 9901

[[shop]]
number = 9001
from = {shop}
name = "Test Alchemist"
stock = [{{ item = 9001, maxqty = 5, qty = 5 }}]

[[message]]
number = 9901
msgs = ["You feel the vent's heat.", "", "Heat rolls out of the vent."]

[[text]]
number = 9901
text = """
go vent:message 9901:teleport 1 1
"""

[[text]]
number = 9902
text = "{{ansi:1;32}}The raptor{{ansi:0;32}} falls.\n"
"#,
                item = t.item,
                shop = t.shop,
            ),
        ),
    ]
}

/// Stock records to copy: the lowest-numbered item and shop.
#[derive(Clone, Copy)]
struct Templates {
    item: i32,
    shop: i32,
}

async fn templates(db: &Db) -> Templates {
    Templates {
        item: db.items().all().await.unwrap()[0].number.0,
        shop: db.shops().all().await.unwrap()[0].number.0,
    }
}

#[tokio::test]
async fn an_overlay_creates_changes_reapplies_and_refuses() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let entrance = db
        .rooms()
        .get(RoomKey::new(1, 1))
        .await
        .unwrap()
        .expect("stock room 1/1");

    // first apply: everything created, the stock room changed
    let tpl = templates(db).await;
    let d = overlay_dir("area", &area(&entrance.name, 400, tpl));
    let plan = apply(db, &d).await.expect("apply");
    let mut acts = actions(&plan);
    acts.sort();
    assert_eq!(
        acts,
        vec![
            ("item 9001".to_string(), Action::Create),
            ("message 9901".to_string(), Action::Create),
            ("monster 9001".to_string(), Action::Create),
            ("room 1/1".to_string(), Action::Change),
            ("room 90/1".to_string(), Action::Create),
            ("room 90/2".to_string(), Action::Create),
            ("shop 9001".to_string(), Action::Create),
            ("spell 9001".to_string(), Action::Create),
            ("text 9901".to_string(), Action::Create),
            ("text 9902".to_string(), Action::Create),
        ]
    );
    let vent = db.rooms().get(RoomKey::new(90, 1)).await.unwrap().unwrap();
    assert_eq!(vent.name, "Test Vent");
    assert_eq!((vent.minlevel, vent.maxlevel, vent.maxmonsters), (1, 2, 2));
    assert_eq!(vent.exits[8].kind, ExitType::MapChange);
    assert_eq!((vent.exits[8].target, vent.exits[8].par1), (RoomId(1), 1));
    let down = db
        .rooms()
        .get(RoomKey::new(1, 1))
        .await
        .unwrap()
        .unwrap()
        .exits[9];
    assert_eq!(
        (down.target, down.kind, down.par1),
        (RoomId(1), ExitType::MapChange, 90)
    );
    assert!(
        db.room_state()
            .get(RoomKey::new(90, 2))
            .await
            .unwrap()
            .is_some()
    );
    let rat = db.monsters().get(MonsterNo(9001)).await.unwrap().unwrap();
    let template = db.monsters().get(MonsterNo(1)).await.unwrap().unwrap();
    assert_eq!((rat.name.as_str(), rat.maxhp), ("test raptor", 400));
    assert_eq!(rat.desc[0], "A small, quick lizard.");
    assert_eq!(rat.desc[1], "");
    // the template lends what the entry does not set
    assert_eq!((rat.exp, rat.attacks), (template.exp, template.attacks));
    assert!(
        db.monster_state()
            .get(MonsterNo(9001))
            .await
            .unwrap()
            .is_some()
    );
    assert_eq!(
        db.items().get(ItemNo(9001)).await.unwrap().unwrap().name,
        "raptor claw"
    );
    let msg = db.messages().get(MessageNo(9901)).await.unwrap().unwrap();
    assert_eq!(msg.msgs[2], "Heat rolls out of the vent.");
    let text = db
        .text_blocks()
        .block(0, TextBlockNo(9902))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(text.text, "\u{1b}[1;32mThe raptor\u{1b}[0;32m falls.\n");

    // the same overlay again: nothing to write
    let again = apply(db, &d).await.expect("reapply");
    assert!(
        again.records.iter().all(|p| p.action == Action::Unchanged),
        "{again}"
    );

    // edited: its own records are rewritten
    let d2 = overlay_dir("area", &area(&entrance.name, 450, tpl));
    let edited = apply(db, &d2).await.expect("edited");
    let rewritten: Vec<_> = actions(&edited)
        .into_iter()
        .filter(|(_, a)| *a != Action::Unchanged)
        .collect();
    assert_eq!(
        rewritten,
        vec![("monster 9001".to_string(), Action::Rewrite)]
    );
    assert_eq!(
        db.monsters()
            .get(MonsterNo(9001))
            .await
            .unwrap()
            .unwrap()
            .maxhp,
        450
    );

    // a change may be revised while the record holds what the overlay last set, and not once
    // someone else has changed it
    let light = db
        .rooms()
        .get(RoomKey::new(1, 2))
        .await
        .unwrap()
        .unwrap()
        .light;
    let tune = |name: &str, to: i16, was: i16| {
        overlay_dir(
            name,
            &[(
                "a.toml",
                format!(
                    "[[room_change]]\nmap = 1\nroom = 2\nset = {{ light = {to} }}\nwas = {{ light = {was} }}\n"
                ),
            )],
        )
    };
    let first = apply(db, &tune("tune", light + 1, light))
        .await
        .expect("tune");
    assert_eq!(
        actions(&first),
        vec![("room 1/2".to_string(), Action::Change)]
    );
    let revised = apply(db, &tune("tune", light + 2, light))
        .await
        .expect("revised");
    assert_eq!(
        actions(&revised),
        vec![("room 1/2".to_string(), Action::Change)]
    );
    apply(db, &tune("meddler", light + 9, light + 2))
        .await
        .expect("another overlay's change");
    let err = apply(db, &tune("tune", light + 3, light))
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("not as expected"), "{err}");

    // another overlay may not take those records, and nobody may take the realm's
    let thief = overlay_dir(
        "thief",
        &[(
            "a.toml",
            "[[monster]]\nnumber = 9001\nfrom = 1\n\n[[monster]]\nnumber = 2\nfrom = 1\n".into(),
        )],
    );
    let err = apply(db, &thief).await.unwrap_err().to_string();
    assert!(err.contains("created by the overlay `area`"), "{err}");
    assert!(err.contains("exists in the realm"), "{err}");

    // a change that finds other values than it expects, and a missing number, stop it all
    let wrong = overlay_dir(
        "wrong",
        &[(
            "a.toml",
            "[[room_change]]\nmap = 1\nroom = 1\nset = { light = 5 }\nwas = { name = \"Elsewhere\" }\n\n\
             [[room]]\nmap = 91\nroom = 1\nname = \"Dead end\"\nspecmon = 99999\n"
                .into(),
        )],
    );
    let err = apply(db, &wrong).await.unwrap_err().to_string();
    assert!(err.contains("not as expected"), "{err}");
    assert!(err.contains("monster 99999, which does not exist"), "{err}");
    assert!(db.rooms().get(RoomKey::new(91, 1)).await.unwrap().is_none());
    assert_eq!(
        db.rooms()
            .get(RoomKey {
                map: MapId(1),
                room: RoomId(1)
            })
            .await
            .unwrap()
            .unwrap()
            .light,
        entrance.light
    );
    t.close().await;
}

/// An overlay directory with a full `overlay.toml`.
fn overlay_with(name: &str, manifest: &str, files: &[(&str, String)]) -> PathBuf {
    let d = dir(name);
    write(
        &d,
        "overlay.toml",
        &format!("name = \"{name}\"\n{manifest}"),
    );
    for (f, text) in files {
        write(&d, f, text);
    }
    d
}

#[tokio::test]
async fn overlays_load_in_order_and_an_addon_overrides_its_base() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let light = db
        .rooms()
        .get(RoomKey::new(1, 3))
        .await
        .unwrap()
        .unwrap()
        .light;

    // the base: new records with no template (a monster, a class, a race, an action), and a
    // change to a realm room
    let base = overlay_with(
        "base",
        "",
        &[(
            "a.toml",
            format!(
                "[[monster]]\nnumber = 9100\nname = \"blank beast\"\nmaxhp = 100\n\n\
                 [[class]]\nnumber = 99\nname = \"Tester\"\nbasehp = 5\n\n\
                 [[race]]\nnumber = 99\nname = \"Testkin\"\n\n\
                 [[action]]\nname = \"wiggle\"\ntexts = [\"You wiggle.\"]\n\n\
                 [[room_change]]\nmap = 1\nroom = 3\nset = {{ light = {} }}\nwas = {{ light = {light} }}\n",
                light + 1
            ),
        )],
    );
    // the add-on: loads after the base, changes the room the base changed and takes over its
    // monster
    let addon = overlay_with(
        "addon",
        "after = [\"base\"]\noverrides = [\"base\"]\n",
        &[(
            "a.toml",
            format!(
                "[[monster]]\nnumber = 9100\nname = \"tamed beast\"\nmaxhp = 200\n\n\
                 [[room_change]]\nmap = 1\nroom = 3\nset = {{ light = {} }}\nwas = {{ light = {light} }}\n",
                light + 2
            ),
        )],
    );
    // given add-on first, applied base first
    let plans = overlay::apply_dirs(db, &[addon.clone(), base.clone()], true)
        .await
        .expect("both");
    let names: Vec<&str> = plans.iter().map(|p| p.overlay.as_str()).collect();
    assert_eq!(names, vec!["base", "addon"]);
    assert_eq!(
        actions(&plans[1]),
        vec![
            ("monster 9100".to_string(), Action::TakeOver),
            ("room 1/3".to_string(), Action::Change),
        ]
    );
    let beast = db.monsters().get(MonsterNo(9100)).await.unwrap().unwrap();
    assert_eq!(
        (beast.name.as_str(), beast.maxhp, beast.exp),
        ("tamed beast", 200, 0)
    );
    assert_eq!(
        db.rooms()
            .get(RoomKey::new(1, 3))
            .await
            .unwrap()
            .unwrap()
            .light,
        light + 2
    );
    let class = db
        .classes()
        .get(silvermere_types::ids::ClassNo(99))
        .await
        .unwrap()
        .unwrap();
    assert_eq!((class.name.as_str(), class.basehp), ("Tester", 5));
    let wiggle = db.actions().by_name("WIGGLE").await.unwrap().unwrap();
    assert_eq!(wiggle.texts[0], "You wiggle.");

    // without `overrides`, the same change is refused: the room holds the add-on's value
    let rival = overlay_with(
        "rival",
        "",
        &[(
            "a.toml",
            format!(
                "[[room_change]]\nmap = 1\nroom = 3\nset = {{ light = {} }}\nwas = {{ light = {light} }}\n",
                light + 3
            ),
        )],
    );
    let err = overlay::apply_dirs(db, &[rival], true)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("not as expected"), "{err}");

    // an `after` that is neither given nor applied, and two overlays waiting for each other
    let lonely = overlay_with("lonely", "after = [\"nowhere\"]\n", &[]);
    let err = overlay::apply_dirs(db, &[lonely], false)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("neither given nor applied"), "{err}");
    let ping = overlay_with("ping", "after = [\"pong\"]\n", &[]);
    let pong = overlay_with("pong", "after = [\"ping\"]\n", &[]);
    let err = overlay::apply_dirs(db, &[ping, pong], false)
        .await
        .unwrap_err()
        .to_string();
    assert!(err.contains("wait for another"), "{err}");
    t.close().await;
}

/// A world that holds an overlay's content without its records (a realm imported from the files
/// the overlay was exported from) adopts it: ownership and changes recorded, nothing written,
/// and refused when the world does not hold what the overlay makes.
#[tokio::test]
async fn a_world_that_holds_an_overlay_adopts_it() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let entrance = db.rooms().get(RoomKey::new(1, 1)).await.unwrap().unwrap();
    let tpl = templates(db).await;
    let d = overlay_dir("adopted", &area(&entrance.name, 400, tpl));
    apply(db, &d).await.expect("apply");
    // forget it was applied: the content stays, the records go
    for sql in [
        "DELETE FROM overlay_records WHERE overlay = 'adopted'",
        "DELETE FROM overlay_changes WHERE overlay = 'adopted'",
    ] {
        sqlx::query(sql).execute(&db.pool()).await.unwrap();
    }
    assert!(
        !silvermere_db::overlay_records::applied(db, "adopted")
            .await
            .unwrap()
    );

    // the world differs from an edited overlay: nothing adopted
    // (its own directory: `overlay_dir` makes one per name)
    let edited = overlay_dir("adopted-edited", &area(&entrance.name, 450, tpl));
    let err = overlay::adopt_dir(db, &edited, true)
        .await
        .expect_err("differs");
    assert!(err.to_string().contains("monster 9001"), "{err}");
    assert!(
        !silvermere_db::overlay_records::applied(db, "adopted")
            .await
            .unwrap()
    );

    // a check writes nothing; the adoption records what an apply recorded
    let checked = overlay::adopt_dir(db, &d, false).await.expect("check");
    assert_eq!((checked.owned, checked.changed), (9, 1));
    assert!(
        !silvermere_db::overlay_records::applied(db, "adopted")
            .await
            .unwrap()
    );
    overlay::adopt_dir(db, &d, true).await.expect("adopt");
    assert_eq!(
        silvermere_db::overlay_records::owner(db, "monster", "9001")
            .await
            .unwrap(),
        silvermere_db::overlay_records::Owner::Overlay("adopted".into())
    );
    // and the overlay is now its own: applying it again changes nothing
    let again = apply(db, &d).await.expect("reapply");
    assert!(
        again.records.iter().all(|p| p.action == Action::Unchanged),
        "{again}"
    );
    t.close().await;
}
