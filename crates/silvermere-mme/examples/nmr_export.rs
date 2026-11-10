//! Exports a world's content as MME rows and writes them as JSON, one array of rows per table:
//! `cargo run -p silvermere-mme --example nmr_export -- sqlite:///path/world.db out.json`.
//!
//! The content is read through silvermere-db's repositories; the options are NMR's defaults
//! with the date, time and update page of the reference MudRev export.

use anyhow::Context;
use serde_json::{Map, Value as Json};
use silvermere_db::Db;
use silvermere_mme::nmr::{Content, Options, export};
use silvermere_mme::{Row, Value};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let (Some(url), Some(out)) = (args.get(1), args.get(2)) else {
        anyhow::bail!("usage: nmr_export DATABASE_URL OUT.json");
    };
    let db = Db::connect(url).await.context("open the database")?;
    let content = Content {
        rooms: db.rooms().all().await?,
        monsters: db.monsters().all().await?,
        items: db.items().all().await?,
        spells: db.spells().all().await?,
        shops: db.shops().all().await?,
        classes: db.classes().all().await?,
        races: db.races().all().await?,
        messages: db.messages().all().await?,
        text_blocks: db.text_blocks().all().await?,
    };
    let options = Options {
        date: "9/15/2026".into(),
        time: "1:50:47 PM".into(),
        update_url: "https://www.mudinfo.net/mmudexp.php".into(),
        ..Options::default()
    };
    let started = std::time::Instant::now();
    let tables = export(&content, &options);
    eprintln!("exported in {:.1}s", started.elapsed().as_secs_f64());
    let mut json = Map::new();
    for (name, rows) in tables.by_name() {
        eprintln!("{name}: {} rows", rows.len());
        json.insert(name.into(), Json::Array(rows.iter().map(row).collect()));
    }
    std::fs::write(out, serde_json::to_vec(&Json::Object(json))?).context("write the JSON")?;
    Ok(())
}

fn row(row: &Row) -> Json {
    row.iter()
        .map(|(column, value)| {
            let value = match value {
                Value::Long(v) => Json::from(*v),
                Value::Double(v) => Json::from(*v),
                Value::Text(v) => Json::from(v.as_str()),
                Value::Bytes(v) => Json::from(v.clone()),
                Value::Null => Json::Null,
            };
            (column.clone(), value)
        })
        .collect::<Map<_, _>>()
        .into()
}
