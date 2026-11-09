//! Content overlays: new and changed content records written as TOML and applied to a world
//! that is already in the database (`docs/overlays.md`).
//!
//! An overlay is a directory holding `overlay.toml` (its name) and any number of content
//! files. Each content file lists entries by kind: `[[room]]` creates a room,
//! `[[room_change]]` changes one that exists, and the same for monsters, items, spells, shops,
//! messages and text blocks. Field names are the record's own (`docs/schema.md`), so anything a
//! record holds can be set. A new record starts from a blank (rooms, messages, text) or from a
//! `from` template record; a change carries the values it expects to replace (`was`), so an
//! overlay written against one world refuses to overwrite a different one.
//!
//! [`load`] reads a directory, [`apply::plan`] builds every record against the database and
//! checks it (keys, owners, references), and [`apply::write`] writes the plan.

pub mod apply;
pub mod build;
pub mod export;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

pub use apply::{Plan, apply_dirs, plan, stored_record, write};
pub use export::{ExportReport, export};

/// The manifest file of an overlay directory.
pub const MANIFEST: &str = "overlay.toml";

/// A kind of record an overlay can create or change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// A room (`map`, `room`).
    Room,
    /// A known monster (`number`).
    Monster,
    /// An item (`number`).
    Item,
    /// A spell (`number`).
    Spell,
    /// A shop (`number`).
    Shop,
    /// A message (`number`).
    Message,
    /// A text block (`number`; every part).
    Text,
    /// A class (`number`).
    Class,
    /// A race (`number`).
    Race,
    /// An action, a social verb (`name`).
    Action,
}

impl Kind {
    /// Every kind, in the order a plan is reported.
    pub const ALL: [Kind; 10] = [
        Kind::Room,
        Kind::Monster,
        Kind::Item,
        Kind::Spell,
        Kind::Shop,
        Kind::Message,
        Kind::Text,
        Kind::Class,
        Kind::Race,
        Kind::Action,
    ];

    /// The entry name (`room`); a change is `room_change`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Room => "room",
            Kind::Monster => "monster",
            Kind::Item => "item",
            Kind::Spell => "spell",
            Kind::Shop => "shop",
            Kind::Message => "message",
            Kind::Text => "text",
            Kind::Class => "class",
            Kind::Race => "race",
            Kind::Action => "action",
        }
    }

    /// The kind named `name` (`room` .. `text`).
    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    fn from_entry_name(name: &str) -> Option<(Kind, bool)> {
        let (base, change) = match name.strip_suffix("_change") {
            Some(base) => (base, true),
            None => (name, false),
        };
        let kind = Kind::ALL.into_iter().find(|k| k.name() == base)?;
        // text blocks are replaced whole, never patched field by field
        (!(change && kind == Kind::Text)).then_some((kind, change))
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A record's key as the overlay names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    /// A room: map and room number.
    Room(u32, u32),
    /// An action: its verb, in lower case (the game looks verbs up in any case).
    Name(String),
    /// Every other kind: the record number.
    Number(i64),
}

impl Key {
    /// A key as written on a command line: `map/room` for a room, else a number.
    pub fn parse(kind: Kind, s: &str) -> Option<Key> {
        match (kind, s.split_once('/')) {
            (Kind::Room, Some((m, r))) => {
                Some(Key::Room(m.trim().parse().ok()?, r.trim().parse().ok()?))
            }
            (Kind::Room, None) => None,
            (Kind::Action, _) => Some(Key::Name(s.trim().to_ascii_lowercase())),
            (_, Some(_)) => None,
            (_, None) => s.trim().parse().ok().map(Key::Number),
        }
    }
}

impl fmt::Display for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Key::Room(m, r) => write!(f, "{m}/{r}"),
            Key::Name(n) => f.write_str(n),
            Key::Number(n) => write!(f, "{n}"),
        }
    }
}

/// Where an entry came from, for messages: `rooms.toml, room #3`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The content file's name.
    pub file: String,
    /// The entry name (`room`, `monster_change`).
    pub entry: String,
    /// 1-based position among that file's entries of that name.
    pub index: usize,
}

impl fmt::Display for Origin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}, {} #{}", self.file, self.entry, self.index)
    }
}

/// One `[[kind]]` or `[[kind_change]]` entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// What it makes or changes.
    pub kind: Kind,
    /// `[[kind_change]]`: changes a record that exists.
    pub change: bool,
    /// The record's key.
    pub key: Key,
    /// A new record's template (`from`): the record it starts as.
    pub from: Option<Key>,
    /// A new record may take over an existing record no overlay created (`replace = true`).
    pub replace: bool,
    /// The values: a new record's fields, or a change's `set`.
    pub values: Map<String, Value>,
    /// A change's `was`: the values it expects to find.
    pub was: Map<String, Value>,
    /// Where it was written.
    pub origin: Origin,
}

/// An overlay directory, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Overlay {
    /// `name` in `overlay.toml`: the owner recorded for every record it creates.
    pub name: String,
    /// `description` in `overlay.toml`.
    pub description: String,
    /// `after`: overlays this one loads after (they must be applied first or with it).
    pub after: Vec<String>,
    /// `overrides`: overlays whose records and changes this one may change in turn.
    pub overrides: Vec<String>,
    /// `references = "warn"`: a number a record points at that exists nowhere is a warning,
    /// not a problem. For a copy of an existing world, whose dangling numbers are its own.
    pub references_warn: bool,
    /// The directory.
    pub dir: PathBuf,
    /// The entries, file by file in name order, each file in its own order.
    pub entries: Vec<Entry>,
}

/// Why an overlay could not be read, planned or written.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// A file could not be read.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// A file is not TOML, or not an overlay's shape.
    #[error("{path}: {reason}")]
    Format {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        reason: String,
    },
    /// Entries that cannot be built or would be wrong to write, one line each.
    #[error("the overlay has {} problem(s):\n  {}", .0.len(), .0.join("\n  "))]
    Invalid(Vec<String>),
    /// The database.
    #[error("database: {0}")]
    Db(#[from] silvermere_db::DbError),
}

/// An overlay's `after` naming one not among those given: (the overlay, the one it names).
pub type Outside = (String, String);

/// `overlays` in the order they load: each after the overlays its `after` names that are among
/// them, otherwise in the order given. The names it does not find among them come back too,
/// for the caller to check were applied before.
pub fn order(overlays: Vec<Overlay>) -> Result<(Vec<Overlay>, Vec<Outside>), OverlayError> {
    let mut seen = std::collections::HashSet::new();
    for o in &overlays {
        if !seen.insert(o.name.clone()) {
            return Err(OverlayError::Invalid(vec![format!(
                "two overlays are named `{}`",
                o.name
            )]));
        }
    }
    let mut outside = Vec::new();
    for o in &overlays {
        for a in &o.after {
            if !seen.contains(a) {
                outside.push((o.name.clone(), a.clone()));
            }
        }
    }
    let mut left: Vec<Overlay> = overlays;
    let mut done: Vec<Overlay> = Vec::new();
    while !left.is_empty() {
        let ready = left.iter().position(|o| {
            o.after
                .iter()
                .all(|a| !seen.contains(a) || done.iter().any(|d| &d.name == a))
        });
        match ready {
            Some(i) => done.push(left.remove(i)),
            None => {
                let names: Vec<_> = left.iter().map(|o| o.name.as_str()).collect();
                return Err(OverlayError::Invalid(vec![format!(
                    "these overlays each wait for another: {}",
                    names.join(", ")
                )]));
            }
        }
    }
    Ok((done, outside))
}

fn format_err(path: &Path, reason: impl Into<String>) -> OverlayError {
    OverlayError::Format {
        path: path.to_path_buf(),
        reason: reason.into(),
    }
}

fn read_toml(path: &Path) -> Result<Map<String, Value>, OverlayError> {
    let text = std::fs::read_to_string(path).map_err(|source| OverlayError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let table: toml::Table = toml::from_str(&text).map_err(|e| format_err(path, e.to_string()))?;
    match serde_json::to_value(table) {
        Ok(Value::Object(map)) => Ok(map),
        Ok(_) => Err(format_err(path, "not a table")),
        Err(e) => Err(format_err(path, e.to_string())),
    }
}

/// Reads the overlay in `dir`: `overlay.toml`, then every other `*.toml` in name order.
pub fn load(dir: &Path) -> Result<Overlay, OverlayError> {
    let manifest_path = dir.join(MANIFEST);
    let manifest = read_toml(&manifest_path)?;
    let name = match manifest.get("name") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Err(format_err(&manifest_path, "needs `name = \"...\"`")),
    };
    let description = match manifest.get("description") {
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    };
    if let Some(k) = manifest.keys().find(|k| {
        !matches!(
            k.as_str(),
            "name" | "description" | "after" | "overrides" | "references"
        )
    }) {
        return Err(format_err(&manifest_path, format!("unknown key `{k}`")));
    }
    let names = |k: &str| -> Result<Vec<String>, OverlayError> {
        match manifest.get(k) {
            None => Ok(Vec::new()),
            Some(Value::Array(a)) => a
                .iter()
                .map(|v| v.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| {
                    format_err(&manifest_path, format!("`{k}` must list overlay names"))
                }),
            Some(_) => Err(format_err(
                &manifest_path,
                format!("`{k}` must list overlay names"),
            )),
        }
    };
    let references_warn = match manifest.get("references") {
        None => false,
        Some(Value::String(s)) if s == "error" => false,
        Some(Value::String(s)) if s == "warn" => true,
        Some(_) => {
            return Err(format_err(
                &manifest_path,
                "`references` is \"error\" (the default) or \"warn\"",
            ));
        }
    };
    let after = names("after")?;
    let overrides = names("overrides")?;
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|source| OverlayError::Io {
            path: dir.to_path_buf(),
            source,
        })?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "toml")
                && p.file_name().is_some_and(|n| n != MANIFEST)
        })
        .collect();
    files.sort();
    let mut entries = Vec::new();
    for path in &files {
        entries.extend(read_content_file(path)?);
    }
    Ok(Overlay {
        name,
        description,
        after,
        overrides,
        references_warn,
        dir: dir.to_path_buf(),
        entries,
    })
}

fn read_content_file(path: &Path) -> Result<Vec<Entry>, OverlayError> {
    let file = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut table = read_toml(path)?;
    // per-file defaults for new records of a kind: `[defaults.room]`
    let mut defaults: BTreeMap<Kind, Map<String, Value>> = BTreeMap::new();
    if let Some(d) = table.remove("defaults") {
        let Value::Object(d) = d else {
            return Err(format_err(path, "`defaults` must be a table"));
        };
        for (k, v) in d {
            let Some((kind, false)) = Kind::from_entry_name(&k) else {
                return Err(format_err(
                    path,
                    format!("`defaults.{k}`: not a record kind"),
                ));
            };
            let Value::Object(v) = v else {
                return Err(format_err(path, format!("`defaults.{k}` must be a table")));
            };
            defaults.insert(kind, v);
        }
    }
    let mut entries = Vec::new();
    for (name, list) in table {
        let Some((kind, change)) = Kind::from_entry_name(&name) else {
            return Err(format_err(path, format!("unknown entry `{name}`")));
        };
        let Value::Array(list) = list else {
            return Err(format_err(
                path,
                format!("`{name}` must be written [[{name}]]"),
            ));
        };
        for (i, v) in list.into_iter().enumerate() {
            let origin = Origin {
                file: file.clone(),
                entry: name.clone(),
                index: i + 1,
            };
            let Value::Object(map) = v else {
                return Err(format_err(path, format!("{origin}: not a table")));
            };
            let base = if change { None } else { defaults.get(&kind) };
            entries.push(
                entry(kind, change, map, base, origin.clone())
                    .map_err(|reason| format_err(path, format!("{origin}: {reason}")))?,
            );
        }
    }
    Ok(entries)
}

/// Keys an entry may carry for its reader; they are not record fields.
const NOTES: [&str; 3] = ["why", "note", "source"];

fn take_u32(map: &mut Map<String, Value>, k: &str) -> Result<u32, String> {
    match map.remove(k) {
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| format!("`{k}` must be a whole number from 0")),
        Some(_) => Err(format!("`{k}` must be a number")),
        None => Err(format!("needs `{k}`")),
    }
}

fn key_value(kind: Kind, v: Value) -> Result<Key, String> {
    match (kind, v) {
        (Kind::Room, Value::Array(a)) if a.len() == 2 => {
            let n = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok());
            match (n(&a[0]), n(&a[1])) {
                (Some(m), Some(r)) => Ok(Key::Room(m, r)),
                _ => Err("a room is `[map, room]`".into()),
            }
        }
        (Kind::Room, _) => Err("a room is `[map, room]`".into()),
        (Kind::Action, Value::String(n)) => Ok(Key::Name(n.to_ascii_lowercase())),
        (Kind::Action, _) => Err("an action is its verb".into()),
        (_, Value::Number(n)) => n
            .as_i64()
            .map(Key::Number)
            .ok_or_else(|| "a record number".into()),
        _ => Err("a record number".into()),
    }
}

fn take_key(kind: Kind, map: &mut Map<String, Value>) -> Result<Key, String> {
    match kind {
        Kind::Room => Ok(Key::Room(take_u32(map, "map")?, take_u32(map, "room")?)),
        Kind::Action => match map.get("name") {
            // the verb stays a field too: it is the record's name
            Some(Value::String(n)) => Ok(Key::Name(n.to_ascii_lowercase())),
            _ => Err("needs `name`, the verb".into()),
        },
        _ => Ok(Key::Number(i64::from(take_u32(map, "number")?))),
    }
}

fn entry(
    kind: Kind,
    change: bool,
    mut map: Map<String, Value>,
    defaults: Option<&Map<String, Value>>,
    origin: Origin,
) -> Result<Entry, String> {
    let key = take_key(kind, &mut map)?;
    if change && kind == Kind::Action {
        // an action's verb is its key; a change names it and sets the rest
        map.remove("name");
    }
    if change {
        let set = match map.remove("set") {
            Some(Value::Object(set)) => set,
            _ => return Err("needs `set = { ... }`".into()),
        };
        let was = match map.remove("was") {
            Some(Value::Object(was)) => was,
            None => Map::new(),
            Some(_) => return Err("`was` must be a table".into()),
        };
        for note in NOTES {
            map.remove(note);
        }
        if let Some(k) = map.keys().next() {
            return Err(format!(
                "unknown key `{k}` (a change holds `set` and `was`)"
            ));
        }
        return Ok(Entry {
            kind,
            change,
            key,
            from: None,
            replace: false,
            values: set,
            was,
            origin,
        });
    }
    let mut values = defaults.cloned().unwrap_or_default();
    let from = match map.remove("from").or_else(|| values.remove("from")) {
        Some(v) => Some(key_value(kind, v)?),
        None => None,
    };
    let replace = match map.remove("replace") {
        Some(Value::Bool(b)) => b,
        None => false,
        Some(_) => return Err("`replace` must be true or false".into()),
    };
    for note in NOTES {
        map.remove(note);
    }
    for (k, v) in map {
        values.insert(k, v);
    }
    Ok(Entry {
        kind,
        change,
        key,
        from,
        replace,
        values,
        was: Map::new(),
        origin,
    })
}
