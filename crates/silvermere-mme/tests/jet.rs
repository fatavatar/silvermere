//! The writer against MME's own database: every table read, written back, and read again.
//!
//! The template is not in the repository (it is MME's, 11 MB); the test runs when
//! `SILVERMERE_MME_TEMPLATE` names a copy (`silvermere site build` caches one in
//! `data/cache/mme-data-v1.11p.mdb`) and says it was skipped otherwise.

use silvermere_mme::{Row, jet};

const TABLES: [&str; 10] = [
    "Info", "Classes", "Races", "Items", "Spells", "Monsters", "Shops", "Rooms", "TBInfo", "Lairs",
];

fn key(r: &Row) -> String {
    format!(
        "{:?}{:?}{:?}{:?}",
        r.get("Number"),
        r.get("Map Number"),
        r.get("Room Number"),
        r.get("GroupIndex")
    )
}

#[test]
fn the_template_round_trips() {
    let Some(path) = std::env::var_os("SILVERMERE_MME_TEMPLATE") else {
        eprintln!("skipped: set SILVERMERE_MME_TEMPLATE to MME's data-v1.11p.mdb");
        return;
    };
    let template = std::fs::read(path).expect("read the template");
    let tables: Vec<(&str, Vec<Row>)> = TABLES
        .iter()
        .map(|t| (*t, jet::read_table(&template, t).expect("read")))
        .collect();
    let named: Vec<(&str, &[Row])> = tables.iter().map(|(n, r)| (*n, r.as_slice())).collect();
    let written = jet::write_tables(&template, &named).expect("write");
    for (name, rows) in &tables {
        let mut before = rows.clone();
        let mut after = jet::read_table(&written, name).expect("read back");
        before.sort_by_key(key);
        after.sort_by_key(key);
        assert_eq!(before.len(), after.len(), "{name}");
        assert!(before == after, "{name} differs after the round trip");
    }
    // fewer rows than before: the freed pages are blanked, the tables still read
    let few: Vec<(&str, &[Row])> = tables
        .iter()
        .map(|(n, r)| (*n, &r[..r.len().min(3)]))
        .collect();
    let small = jet::write_tables(&template, &few).expect("write a few");
    assert_eq!(jet::read_table(&small, "Rooms").expect("rooms").len(), 3);
}

#[test]
fn a_file_that_is_not_jet_is_refused() {
    assert!(jet::write_tables(&[0u8; 4096 * 4], &[]).is_err());
    assert!(jet::write_tables(b"short", &[]).is_err());
}

#[test]
fn a_table_added_to_the_template_reads_back() {
    let Some(path) = std::env::var_os("SILVERMERE_MME_TEMPLATE") else {
        eprintln!("skipped: set SILVERMERE_MME_TEMPLATE to MME's data-v1.11p.mdb");
        return;
    };
    let template = std::fs::read(path).expect("read the template");
    let names = silvermere_mme::triggers::COLUMNS;
    let added = jet::add_table_like(&template, "Messages", "Info", &names).expect("add");
    // the old tables are untouched
    assert_eq!(
        jet::read_table(&added, "Rooms").unwrap(),
        jet::read_table(&template, "Rooms").unwrap()
    );
    let rows: Vec<Row> = (0..300)
        .map(|i| {
            let mut r = Row::new();
            r.insert(
                "Name".into(),
                silvermere_mme::Value::Text(format!("spell {i}")),
            );
            r.insert(
                "Message".into(),
                silvermere_mme::Value::Text(format!("You feel {i}!")),
            );
            r.insert("Flags".into(), silvermere_mme::Value::Long(i));
            r
        })
        .collect();
    let out = jet::write_tables(&added, &[("Messages", rows.as_slice())]).expect("fill");
    let back = jet::read_table(&out, "Messages").expect("read");
    assert_eq!(back.len(), 300);
    assert!(
        back.iter()
            .any(|r| r.get("Message") == Some(&silvermere_mme::Value::Text("You feel 7!".into())))
    );
    // twice is refused
    assert!(jet::add_table_like(&out, "Messages", "Info", &names).is_err());
}
