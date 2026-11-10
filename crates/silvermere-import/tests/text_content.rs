//! Help topics, description files and game texts as content: the stock import fills them,
//! overlays add and change them, and an export carries them.

use std::path::{Path, PathBuf};

use silvermere_db::{Db, TestDb};
use silvermere_import::importer::{ImportOptions, Importer};
use silvermere_import::overlay::apply::Action;
use silvermere_import::overlay::{self, OverlayError};
use silvermere_import::text_content::{self, HELP_OVERVIEW};

use crate::stock as stock_data;

/// The stock world from the whole package: its help, gang houses and message files with the
/// record files (the `.VIR` files alone hold none of them).
async fn stock_world(db: &Db) {
    let opts = ImportOptions::new(stock_data::dir());
    Importer::new(opts, Some(db)).run().await.expect("import");
}

fn dir(name: &str) -> PathBuf {
    let d = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("text-content-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn overlay_dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let d = dir(name);
    std::fs::write(d.join("overlay.toml"), format!("name = \"{name}\"\n")).unwrap();
    for (f, text) in files {
        std::fs::write(d.join(f), text).unwrap();
    }
    d
}

async fn apply(db: &Db, d: &Path) -> Result<overlay::Plan, OverlayError> {
    let plan = overlay::plan(db, overlay::load(d)?).await?;
    overlay::write(db, &plan).await?;
    Ok(plan)
}

/// The three tables, for comparing two worlds.
async fn content(db: &Db) -> text_content::TextContent {
    text_content::TextContent {
        help: db.help_topics().all().await.unwrap(),
        desc_files: db.desc_files().all().await.unwrap(),
        game_texts: db.game_texts().all().await.unwrap(),
    }
}

const AREA: &str = r#"
[[help_change]]
topic = "HL1B0"
set = { text = "{ansi:1;32}Welcome to the realm.{ansi:0}\n" }

[[help]]
topic = "ashfall"
words = "Ashfall Valley"
text = "A valley under the Volcano. {{Ask Tamsin.}}\n"

[[desc_file]]
name = "wcc99915.hse"
text = "A dusty gang house.\r\n"

[[game_text_change]]
key = "USRLOGIN"
set = { text = "Fantasy awaits you in the realm, v%s!\n" }
"#;

#[tokio::test]
async fn the_stock_import_fills_help_description_files_and_game_texts() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let c = content(db).await;
    // the overview, then 131 topics, the last the `Help ?` end marker
    assert_eq!(c.help.len(), 132);
    assert_eq!(c.help[0].topic, HELP_OVERVIEW);
    assert_eq!(c.help.last().unwrap().words.trim(), "Help ?");
    // WCCHSE.ZIP's gang houses and the emblem, banner, statue and stone files
    assert!(
        c.desc_files.iter().any(|f| f.name == "WCC85715.HSE"),
        "{:?}",
        c.desc_files.len()
    );
    assert!(
        c.desc_files
            .iter()
            .all(|f| f.name == f.name.to_ascii_uppercase())
    );
    let keys: Vec<&str> = text_content::game_text_keys().collect();
    assert_eq!(c.game_texts.len(), keys.len());
    t.close().await;
}

#[tokio::test]
async fn an_overlay_adds_and_changes_help_description_files_and_game_texts() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let before = content(db).await;
    let d = overlay_dir("texts", &[("texts.toml", AREA)]);
    let plan = apply(db, &d).await.expect("apply");
    let got: Vec<(String, Action)> = plan
        .records
        .iter()
        .map(|p| (format!("{} {}", p.kind, p.key), p.action))
        .collect();
    // a file's entries come in entry-name order
    assert_eq!(
        got,
        vec![
            ("desc_file WCC99915.HSE".into(), Action::Create),
            ("game_text USRLOGIN".into(), Action::Change),
            ("help ASHFALL".into(), Action::Create),
            ("help HL1B0".into(), Action::Change),
        ]
    );
    let after = content(db).await;
    // the overview changed in place, with its markup made escape sequences
    assert_eq!(after.help[0].topic, HELP_OVERVIEW);
    assert_eq!(
        after.help[0].text,
        "\u{1b}[1;32mWelcome to the realm.\u{1b}[0m\n"
    );
    // the new topic comes after the stock ones and before the end marker
    let n = after.help.len();
    assert_eq!(n, before.help.len() + 1);
    assert_eq!(after.help[n - 2].topic, "ASHFALL");
    assert_eq!(
        after.help[n - 2].text,
        "A valley under the Volcano. {Ask Tamsin.}\n"
    );
    assert_eq!(after.help[n - 1], *before.help.last().unwrap());
    assert!(after.help[n - 2].position < after.help[n - 1].position);
    let file = db.desc_files().get("WCC99915.HSE").await.unwrap().unwrap();
    assert_eq!(file.text, "A dusty gang house.\r\n");
    let login = db.game_texts().get("USRLOGIN").await.unwrap().unwrap();
    assert_eq!(login.text, "Fantasy awaits you in the realm, v%s!\n");

    // applied again: nothing to do, and the new topic keeps its place
    let again = apply(db, &d).await.expect("reapply");
    assert!(
        again.records.iter().all(|p| p.action == Action::Unchanged),
        "{again}"
    );
    assert_eq!(content(db).await, after);
    t.close().await;
}

#[tokio::test]
async fn game_texts_are_only_changed_and_only_the_known_ones() {
    let made = overlay_dir(
        "new-text",
        &[(
            "t.toml",
            "[[game_text]]\nkey = \"USRLOGIN\"\ntext = \"x\"\n",
        )],
    );
    let err = overlay::load(&made).expect_err("a new game text");
    assert!(err.to_string().contains("[[game_text_change]]"), "{err}");
    let unknown = overlay_dir(
        "unknown-text",
        &[(
            "t.toml",
            "[[game_text_change]]\nkey = \"NOSUCH\"\nset = { text = \"x\" }\n",
        )],
    );
    let err = overlay::load(&unknown).expect_err("not a game text");
    assert!(err.to_string().contains("USRLOGIN"), "{err}");
}

#[tokio::test]
async fn a_room_naming_a_missing_description_file_is_a_warning() {
    let t = TestDb::from_env().await.expect("test database");
    let db = t.db();
    stock_world(db).await;
    let d = overlay_dir(
        "missing-file",
        &[(
            "rooms.toml",
            "[[room_change]]\nmap = 1\nroom = 1\nset = { desc = [\"FILE DESCRIPTION\", \"WCC00001.HSE\"] }\n",
        )],
    );
    let o = overlay::load(&d).expect("load");
    let plan = overlay::plan(db, o).await.expect("plan");
    assert!(
        plan.warnings
            .iter()
            .any(|w| w.contains("desc_file WCC00001.HSE")),
        "{:?}",
        plan.warnings
    );
    t.close().await;
}

#[tokio::test]
async fn an_export_carries_help_description_files_and_game_texts() {
    let world = TestDb::from_env().await.expect("test database");
    let base = TestDb::from_env().await.expect("test database");
    let copy = TestDb::from_env().await.expect("test database");
    stock_world(world.db()).await;
    stock_world(base.db()).await;
    stock_world(copy.db()).await;
    apply(
        world.db(),
        &overlay_dir("texts-src", &[("texts.toml", AREA)]),
    )
    .await
    .expect("apply");
    let out = dir("export");
    overlay::export(world.db(), base.db(), "texts-copy", &out)
        .await
        .expect("export");
    let plan = apply(copy.db(), &out).await.expect("apply the export");
    let kinds: Vec<String> = plan.records.iter().map(|p| p.kind.to_string()).collect();
    assert_eq!(kinds, ["desc_file", "game_text", "help", "help"]);
    assert_eq!(content(copy.db()).await, content(world.db()).await);
    // and an export of the copy against the world finds nothing
    let again = dir("export-again");
    let report = overlay::export(copy.db(), world.db(), "nothing", &again)
        .await
        .expect("export");
    assert!(
        report.created.is_empty() && report.changed.is_empty(),
        "{report}"
    );
    world.close().await;
    base.close().await;
    copy.close().await;
}
