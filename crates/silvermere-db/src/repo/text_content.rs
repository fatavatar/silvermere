//! The realm's text content beside the record files: help topics (`help_topics`,
//! WCCMMHLP.MSG), the `FILE DESCRIPTION` text files (`desc_files`, the gang houses'
//! `WCCnnnnn.HSE` and the emblem, banner, statue and stone files) and the game texts the
//! message files set (`game_texts`, WCCMMUD.MSG and WCCTEXT.MSG blocks).
//!
//! The import fills them from its source's files, overlays add and change them, and the
//! server reads them whole at startup. Each is keyed by a name: the topic's message name,
//! the file name (upper case) and the text's message name.

use crate::Result;
use crate::pool::Db;
use crate::repo::{Args, Batch, Get, select_sql};

const HELP_COLS: &[&str] = &["topic", "position", "words", "text"];
const DESC_COLS: &[&str] = &["name", "text"];
const GAME_TEXT_COLS: &[&str] = &["msg_key", "text"];

/// One help topic: the overview (`HL1B0`, no words) or a topic and its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelpTopicRow {
    /// The message name: `HL1B0` the overview, `HL1Tn` a stock topic, or an overlay's own.
    pub topic: String,
    /// The order HELP tries the topics in; the highest is the `Help ?` end marker.
    pub position: i64,
    /// What a player's topic is matched against; empty for the overview.
    pub words: String,
    /// The text as the `.MSG` file holds it (`%c` for the say and yell characters).
    pub text: String,
}

/// One `FILE DESCRIPTION` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescFileRow {
    /// The file name, upper case.
    pub name: String,
    /// The file's text, CP437 decoded, line ends as the file holds them.
    pub text: String,
}

/// One game text a message file sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameTextRow {
    /// The message name (`USRLOGIN`).
    pub key: String,
    /// The text as the `.MSG` file holds it.
    pub text: String,
}

fn help_args(r: &HelpTopicRow) -> Args {
    let mut a = Args::new();
    a.add(&r.topic).add(r.position).add(&r.words).add(&r.text);
    a
}

fn desc_args(r: &DescFileRow) -> Args {
    let mut a = Args::new();
    a.add(r.name.to_ascii_uppercase()).add(&r.text);
    a
}

fn game_text_args(r: &GameTextRow) -> Args {
    let mut a = Args::new();
    a.add(&r.key).add(&r.text);
    a
}

/// Help topics (WCCMMHLP.MSG).
#[derive(Debug, Clone, Copy)]
pub struct HelpTopics<'a> {
    db: &'a Db,
}

impl<'a> HelpTopics<'a> {
    pub(crate) fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Every topic in `position` order (the overview first).
    pub async fn all(&self) -> Result<Vec<HelpTopicRow>> {
        let rows = Args::new()
            .query(select_sql(
                "help_topics",
                HELP_COLS,
                "ORDER BY position, topic",
            ))
            .fetch_all(&self.db.pool())
            .await?;
        rows.iter()
            .map(|row| {
                let g = Get::new(row, "help_topics");
                Ok(HelpTopicRow {
                    topic: g.text("topic")?,
                    position: g.i64("position")?,
                    words: g.text("words")?,
                    text: g.text("text")?,
                })
            })
            .collect()
    }

    /// The topic named `topic`.
    pub async fn get(&self, topic: &str) -> Result<Option<HelpTopicRow>> {
        Ok(self.all().await?.into_iter().find(|r| r.topic == topic))
    }

    /// Writes many topics in one transaction (upsert by `topic`).
    pub async fn put_many(&self, rows: &[HelpTopicRow]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        let mut b = Batch::default();
        for r in rows {
            b.upsert("help_topics", HELP_COLS, &["topic"], &r.topic, help_args(r));
        }
        b.flush(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The `FILE DESCRIPTION` files.
#[derive(Debug, Clone, Copy)]
pub struct DescFiles<'a> {
    db: &'a Db,
}

impl<'a> DescFiles<'a> {
    pub(crate) fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Every file in name order.
    pub async fn all(&self) -> Result<Vec<DescFileRow>> {
        let rows = Args::new()
            .query(select_sql("desc_files", DESC_COLS, "ORDER BY name"))
            .fetch_all(&self.db.pool())
            .await?;
        rows.iter()
            .map(|row| {
                let g = Get::new(row, "desc_files");
                Ok(DescFileRow {
                    name: g.text("name")?,
                    text: g.text("text")?,
                })
            })
            .collect()
    }

    /// The file named `name`, any case.
    pub async fn get(&self, name: &str) -> Result<Option<DescFileRow>> {
        let mut a = Args::new();
        a.add(name.trim().to_ascii_uppercase());
        let rows = a
            .query(select_sql("desc_files", DESC_COLS, "WHERE name = $1"))
            .fetch_all(&self.db.pool())
            .await?;
        rows.first()
            .map(|row| {
                let g = Get::new(row, "desc_files");
                Ok(DescFileRow {
                    name: g.text("name")?,
                    text: g.text("text")?,
                })
            })
            .transpose()
    }

    /// Writes many files in one transaction (upsert by name; names are stored upper case).
    pub async fn put_many(&self, rows: &[DescFileRow]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        let mut b = Batch::default();
        for r in rows {
            b.upsert(
                "desc_files",
                DESC_COLS,
                &["name"],
                r.name.to_ascii_uppercase(),
                desc_args(r),
            );
        }
        b.flush(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The game texts the message files set.
#[derive(Debug, Clone, Copy)]
pub struct GameTexts<'a> {
    db: &'a Db,
}

impl<'a> GameTexts<'a> {
    pub(crate) fn new(db: &'a Db) -> Self {
        Self { db }
    }

    /// Every text in key order.
    pub async fn all(&self) -> Result<Vec<GameTextRow>> {
        let rows = Args::new()
            .query(select_sql("game_texts", GAME_TEXT_COLS, "ORDER BY msg_key"))
            .fetch_all(&self.db.pool())
            .await?;
        rows.iter()
            .map(|row| {
                let g = Get::new(row, "game_texts");
                Ok(GameTextRow {
                    key: g.text("msg_key")?,
                    text: g.text("text")?,
                })
            })
            .collect()
    }

    /// The text named `key`.
    pub async fn get(&self, key: &str) -> Result<Option<GameTextRow>> {
        Ok(self.all().await?.into_iter().find(|r| r.key == key))
    }

    /// Writes many texts in one transaction (upsert by key).
    pub async fn put_many(&self, rows: &[GameTextRow]) -> Result<()> {
        let mut tx = self.db.pool().begin().await?;
        let mut b = Batch::default();
        for r in rows {
            b.upsert(
                "game_texts",
                GAME_TEXT_COLS,
                &["msg_key"],
                &r.key,
                game_text_args(r),
            );
        }
        b.flush(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
