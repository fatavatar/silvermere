//! The realm's text content kept beside the record files, read out of a folder of its files
//! (an import's source, or an older database's `<data_dir>/assets`): the help topics of
//! `WCCMMHLP.MSG`, the `FILE DESCRIPTION` files (`.HSE`, `.EMB`, `.BAN`, `.STA`, `.STO`) and
//! the game texts of `WCCMMUD.MSG` and `WCCTEXT.MSG` the server shows. The database keeps
//! them (`help_topics`, `desc_files`, `game_texts`) and overlays add and change them.

use std::collections::BTreeMap;
use std::path::Path;

use silvermere_db::Db;
use silvermere_db::repo::{DescFileRow, GameTextRow, HelpTopicRow};
use silvermere_text::cp437;
use silvermere_text::package::{
    HELP_FILE, MsgEntry, WCCMMUD_FILE, WCCMMUD_TEXTS, WCCTEXT_FILE, WCCTEXT_TEXTS, parse_msg,
};

/// The extensions of the files `FILE DESCRIPTION` rooms and items name.
pub const DESC_FILE_EXTENSIONS: &[&str] = &["HSE", "EMB", "BAN", "STA", "STO"];

/// The overview's message name.
pub const HELP_OVERVIEW: &str = "HL1B0";

/// Gap between stock topics' positions, so overlays' topics fit before the end marker.
pub const HELP_POSITION_STEP: i64 = 1000;

/// The three kinds, as one folder of files holds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextContent {
    pub help: Vec<HelpTopicRow>,
    pub desc_files: Vec<DescFileRow>,
    pub game_texts: Vec<GameTextRow>,
}

impl TextContent {
    /// Nothing at all was found.
    pub fn is_empty(&self) -> bool {
        self.help.is_empty() && self.desc_files.is_empty() && self.game_texts.is_empty()
    }

    /// Writes all three, upserting by key.
    pub async fn write(&self, db: &Db) -> Result<(), silvermere_db::DbError> {
        db.help_topics().put_many(&self.help).await?;
        db.desc_files().put_many(&self.desc_files).await?;
        db.game_texts().put_many(&self.game_texts).await
    }
}

/// The message names of the game texts the server shows from the message files.
pub fn game_text_keys() -> impl Iterator<Item = &'static str> {
    WCCMMUD_TEXTS
        .iter()
        .chain(WCCTEXT_TEXTS.iter())
        .map(|t| t.key)
}

/// Reads the three kinds from the files in `dir` (names in any case): an older database's
/// `<data_dir>/assets`.
pub fn read_dir(dir: &Path) -> Result<TextContent, String> {
    let mut files = BTreeMap::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file()) {
            let name = entry.file_name().to_string_lossy().to_ascii_uppercase();
            let bytes = std::fs::read(entry.path())
                .map_err(|e| format!("{}: {e}", entry.path().display()))?;
            files.insert(name, bytes);
        }
    }
    from_files(&files)
}

/// Reads the three kinds from `files` (upper-case name to bytes, as
/// [`crate::assets::source_files`] gives an import's source). A missing message file leaves
/// its part empty; a message file that does not parse is an error.
pub fn from_files(files: &BTreeMap<String, Vec<u8>>) -> Result<TextContent, String> {
    let read = |name: &str| -> Result<Option<Vec<u8>>, String> { Ok(files.get(name).cloned()) };
    let parse = |name: &str| -> Result<Option<Vec<MsgEntry>>, String> {
        read(name)?
            .map(|bytes| parse_msg(name, &bytes).map_err(|e| e.to_string()))
            .transpose()
    };
    let mut out = TextContent::default();
    if let Some(entries) = parse(HELP_FILE)? {
        out.help = help_rows(&entries)?;
    }
    let keys: Vec<&str> = game_text_keys().collect();
    for file in [WCCMMUD_FILE, WCCTEXT_FILE] {
        for e in parse(file)?.unwrap_or_default() {
            if keys.contains(&e.key.as_str()) && !out.game_texts.iter().any(|t| t.key == e.key) {
                out.game_texts.push(GameTextRow {
                    key: e.key,
                    text: e.value,
                });
            }
        }
    }
    let mut names: Vec<&String> = files
        .keys()
        .filter(|n| {
            n.rsplit_once('.')
                .is_some_and(|(_, ext)| DESC_FILE_EXTENSIONS.contains(&ext))
        })
        .collect();
    names.sort();
    for name in names {
        let bytes = read(name)?.unwrap_or_default();
        out.desc_files.push(DescFileRow {
            name: name.clone(),
            text: cp437::decode(&bytes),
        });
    }
    Ok(out)
}

/// The help file's entries as rows: `HL1B0` the overview at position 0, then each topic
/// string (`HL1Tn`) and the text after it, paired by position as the game pairs them.
pub fn help_rows(entries: &[MsgEntry]) -> Result<Vec<HelpTopicRow>, String> {
    let is_help = |k: &str| k.starts_with("HL1T") || k.starts_with("HL1B");
    let heads: Vec<&MsgEntry> = entries.iter().filter(|e| is_help(&e.key)).collect();
    let Some((first, rest)) = heads.split_first().filter(|(f, _)| f.key == HELP_OVERVIEW) else {
        return Err(format!(
            "{HELP_FILE}: {HELP_OVERVIEW} (the overview) must come first"
        ));
    };
    if rest.len() % 2 != 0 {
        return Err(format!(
            "{HELP_FILE}: topic and text messages do not pair up"
        ));
    }
    let mut rows = vec![HelpTopicRow {
        topic: HELP_OVERVIEW.to_string(),
        position: 0,
        words: String::new(),
        text: first.value.clone(),
    }];
    for (i, pair) in rest.chunks(2).enumerate() {
        let (t, b) = (pair[0], pair[1]);
        if !t.key.starts_with("HL1T") || !b.key.starts_with("HL1B") {
            return Err(format!(
                "{HELP_FILE}: {}/{} is not a topic/text pair",
                t.key, b.key
            ));
        }
        rows.push(HelpTopicRow {
            topic: t.key.clone(),
            position: (i as i64 + 1) * HELP_POSITION_STEP,
            words: t.value.clone(),
            text: b.value.clone(),
        });
    }
    Ok(rows)
}

/// Rows back into help file entries, for the package's checks and parser: the overview,
/// then a topic string and a text per topic in position order. A topic not named `HL1Tn`
/// (an overlay's own) gets a name of that form.
pub fn help_entries(rows: &[HelpTopicRow]) -> Vec<MsgEntry> {
    let mut rows: Vec<&HelpTopicRow> = rows.iter().collect();
    rows.sort_by(|a, b| (a.position, &a.topic).cmp(&(b.position, &b.topic)));
    let mut out = Vec::with_capacity(rows.len() * 2);
    for (i, r) in rows.iter().enumerate() {
        if r.topic == HELP_OVERVIEW {
            out.insert(
                0,
                MsgEntry {
                    key: HELP_OVERVIEW.to_string(),
                    value: r.text.clone(),
                    kind: "T".to_string(),
                },
            );
            continue;
        }
        let suffix = match r.topic.strip_prefix("HL1T") {
            Some(s)
                if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') =>
            {
                s.to_string()
            }
            _ => format!("_X{i}"),
        };
        out.push(MsgEntry {
            key: format!("HL1T{suffix}"),
            value: r.words.clone(),
            kind: "S".to_string(),
        });
        out.push(MsgEntry {
            key: format!("HL1B{suffix}"),
            value: r.text.clone(),
            kind: "T".to_string(),
        });
    }
    out
}

/// The bytes of a description file's text.
pub fn desc_file_bytes(text: &str) -> Vec<u8> {
    cp437::encode(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desc_file_keeps_every_byte() {
        let bytes: Vec<u8> = (0..=255).collect();
        assert_eq!(desc_file_bytes(&cp437::decode(&bytes)), bytes);
    }

    #[test]
    fn help_rows_go_back_to_the_same_entries() {
        let msg = b"HL1B0 {Overview} T\nHL1T1 {Classes} S\nHL1B1 {About classes} T\nHL1T2 {Help ?} S\nHL1B2 {%c} T\n";
        let entries = parse_msg(HELP_FILE, msg).unwrap();
        let rows = help_rows(&entries).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].position, HELP_POSITION_STEP);
        let back = help_entries(&rows);
        let pairs = |e: &[MsgEntry]| {
            e.iter()
                .map(|x| (x.key.clone(), x.value.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(pairs(&back), pairs(&entries));
    }
}
