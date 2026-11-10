//! The MajorMUD Explorer export's inputs: the world as the database holds it, and the
//! settings NMR's export dialog would ask for, from the realm's configuration.

use silvermere_db::Db;
use silvermere_mme::nmr::{Content, Options};

use crate::config::Config;

/// Every record the export reads.
pub async fn content(db: &Db) -> anyhow::Result<Content> {
    Ok(Content {
        rooms: db.rooms().all().await?,
        monsters: db.monsters().all().await?,
        items: db.items().all().await?,
        spells: db.spells().all().await?,
        shops: db.shops().all().await?,
        classes: db.classes().all().await?,
        races: db.races().all().await?,
        messages: db.messages().all().await?,
        text_blocks: db.text_blocks().all().await?,
    })
}

/// NMR's defaults, named for this realm and dated now.
pub fn options(cfg: &Config) -> Options {
    let now = jiff::Zoned::now();
    let hour12 = match now.hour() % 12 {
        0 => 12,
        h => h,
    };
    Options {
        custom: if cfg.site.name.is_empty() {
            "silvermere".to_owned()
        } else {
            cfg.site.name.clone()
        },
        // as NMR writes them in a US locale: "9/15/2026" and "1:50:47 PM"
        date: format!("{}/{}/{}", now.month(), now.day(), now.year()),
        time: format!(
            "{hour12}:{:02}:{:02} {}",
            now.minute(),
            now.second(),
            if now.hour() < 12 { "AM" } else { "PM" }
        ),
        ..Options::default()
    }
}
