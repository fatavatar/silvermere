//! Reads every MME table of a database and writes it back through the writer:
//! `cargo run -p silvermere-mme --example jet_roundtrip -- IN.mdb OUT.mdb`.

use silvermere_mme::jet;

const TABLES: [&str; 10] = [
    "Info", "Classes", "Races", "Items", "Spells", "Monsters", "Shops", "Rooms", "TBInfo", "Lairs",
];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = std::fs::read(&args[1]).expect("read the database");
    let mut tables = Vec::new();
    for name in TABLES {
        let rows = jet::read_table(&input, name).expect("read a table");
        eprintln!("{name}: {} rows", rows.len());
        tables.push((name, rows));
    }
    let refs: Vec<(&str, &[silvermere_mme::Row])> =
        tables.iter().map(|(n, r)| (*n, r.as_slice())).collect();
    let out = jet::write_tables(&input, &refs).expect("write");
    std::fs::write(&args[2], &out).expect("write the file");
    for (name, rows) in &tables {
        let back = jet::read_table(&out, name).expect("read back");
        let mut a = rows.clone();
        let mut b = back.clone();
        let key = |r: &silvermere_mme::Row| {
            format!(
                "{:?}{:?}{:?}",
                r.get("Number"),
                r.get("Map Number"),
                r.get("Room Number")
            )
        };
        a.sort_by_key(key);
        b.sort_by_key(key);
        let same = a == b;
        eprintln!(
            "{name}: wrote {} read {} identical={same}",
            rows.len(),
            back.len()
        );
    }
}
