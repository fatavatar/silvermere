//! The realm's help topics, description files and game texts, read from the database at
//! startup (`help_topics`, `desc_files`, `game_texts`; the import and overlays write them).
//!
//! A database made before they were content has none: on its first start the server fills
//! them once from `<data_dir>/assets`, where the import put the realm's files, so the realm
//! keeps its own help and gang houses without importing again.

use std::path::Path;

use anyhow::Context;
use silvermere_db::Db;
use silvermere_import::text_content::{self, TextContent};
use silvermere_text::package::RealmText;

/// The three kinds from `db`, filled first from `<data_dir>/assets` when all three are empty.
pub async fn load(db: &Db, data_dir: &Path) -> anyhow::Result<TextContent> {
    let mut text = read(db).await?;
    if text.is_empty() {
        let assets = data_dir.join("assets");
        if assets.is_dir() {
            let found = text_content::read_dir(&assets)
                .map_err(anyhow::Error::msg)
                .context("reading the help, description files and game texts from the assets")?;
            if !found.is_empty() {
                found.write(db).await?;
                tracing::info!(
                    help = found.help.len(),
                    files = found.desc_files.len(),
                    texts = found.game_texts.len(),
                    "help, description files and game texts copied from {} into the database (once)",
                    assets.display()
                );
                text = read(db).await?;
            }
        }
    }
    Ok(text)
}

async fn read(db: &Db) -> anyhow::Result<TextContent> {
    Ok(TextContent {
        help: db.help_topics().all().await?,
        desc_files: db.desc_files().all().await?,
        game_texts: db.game_texts().all().await?,
    })
}

/// The database's help and texts in the form the text package replaces the files' with.
pub fn realm_text(text: &TextContent) -> RealmText {
    RealmText {
        help: (!text.help.is_empty()).then(|| text_content::help_entries(&text.help)),
        texts: text
            .game_texts
            .iter()
            .map(|t| (t.key.clone(), t.text.clone()))
            .collect(),
    }
}

/// The description files over the ones in the assets directory.
pub fn add_desc_files(assets: &mut silvermere_engine::display::Assets, text: &TextContent) {
    for f in &text.desc_files {
        assets.insert(&f.name, text_content::desc_file_bytes(&f.text));
    }
}
