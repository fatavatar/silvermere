//! Writing the difference between two worlds as an overlay: every record the world holds that
//! the base does not (as a new entry), and every record both hold that differs (as a change
//! whose `was` is the base's values). Applied to the base, the overlay gives the world's content
//! back. What play changes (a room's floor and monsters, a monster's counts, an item's copies in
//! play) is not content and is left out.
//!
//! The entries are as short as the loader reads them back exactly: only the fields that differ
//! from a blank record (new) or from the base (changed), descriptions as one string, exits by
//! direction, lists without their empty tail, and texts in their stored parts.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde_json::{Map, Value, json};
use silvermere_db::Db;

use super::apply::{blank, design};
#[cfg(test)]
use super::build::direction;
use super::build::{empty_slot, is_line_field};
use super::{Key, Kind, OverlayError};

/// What an export found, per kind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Records the world holds and the base does not.
    pub created: BTreeMap<Kind, u32>,
    /// Records both hold that differ.
    pub changed: BTreeMap<Kind, u32>,
    /// Records both hold alike.
    pub same: BTreeMap<Kind, u32>,
}

impl ExportReport {
    /// Whether the world and the base hold the same content.
    pub fn is_empty(&self) -> bool {
        self.created.values().sum::<u32>() + self.changed.values().sum::<u32>() == 0
    }
}

impl fmt::Display for ExportReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let get = |m: &BTreeMap<Kind, u32>, k| m.get(&k).copied().unwrap_or(0);
        for kind in Kind::ALL {
            writeln!(
                f,
                "export {:<8} {} new, {} changed, {} alike",
                kind.name(),
                get(&self.created, kind),
                get(&self.changed, kind),
                get(&self.same, kind)
            )?;
        }
        Ok(())
    }
}

fn to_value<T: serde::Serialize>(r: &T) -> Value {
    serde_json::to_value(r).unwrap_or(Value::Null)
}

/// Every record of `kind` in `db`, by key, as an overlay names its fields (content only).
async fn all(db: &Db, kind: Kind) -> Result<BTreeMap<Key, Value>, OverlayError> {
    let mut out = BTreeMap::new();
    let mut put = |key: Key, v: Value| {
        out.insert(key, design(kind, &v));
    };
    match kind {
        Kind::Room => {
            for r in db.rooms().all().await? {
                put(Key::Room(r.key.map.0, r.key.room.0), to_value(&r));
            }
        }
        Kind::Monster => {
            for r in db.monsters().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Item => {
            for r in db.items().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Spell => {
            for r in db.spells().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Shop => {
            for r in db.shops().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Message => {
            for r in db.messages().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Class => {
            for r in db.classes().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Race => {
            for r in db.races().all().await? {
                put(Key::Number(r.number.0.into()), to_value(&r));
            }
        }
        Kind::Action => {
            for r in db.actions().all().await? {
                put(Key::Name(r.name.to_ascii_lowercase()), to_value(&r));
            }
        }
        Kind::Help => {
            for r in db.help_topics().all().await? {
                put(
                    kind.name_key(&r.topic),
                    json!({"words": r.words, "text": r.text}),
                );
            }
        }
        Kind::DescFile => {
            for r in db.desc_files().all().await? {
                put(kind.name_key(&r.name), json!({"text": r.text}));
            }
        }
        Kind::GameText => {
            for r in db.game_texts().all().await? {
                put(kind.name_key(&r.key), json!({"text": r.text}));
            }
        }
        Kind::Text => {
            let mut blocks: BTreeMap<i32, (i32, Vec<String>)> = BTreeMap::new();
            for p in db.text_blocks().all().await? {
                let e = blocks.entry(p.number.0).or_insert((p.link.0, Vec::new()));
                e.1.push(p.text);
            }
            for (n, (link, parts)) in blocks {
                put(Key::Number(n.into()), json!({"link": link, "parts": parts}));
            }
        }
    }
    Ok(out)
}

/// The key fields of an entry.
fn key_fields(kind: Kind, key: &Key) -> Map<String, Value> {
    let mut m = Map::new();
    match key {
        Key::Room(map, room) => {
            m.insert("map".into(), json!(map));
            m.insert("room".into(), json!(room));
        }
        Key::Number(n) => {
            m.insert("number".into(), json!(n));
        }
        // an action's verb is its `name` field, written with the rest
        Key::Name(_) if kind == Kind::Action => {}
        Key::Name(n) => {
            m.insert(kind.name_field().unwrap_or("name").into(), json!(n));
        }
    }
    m
}

/// The string values of a text kind's fields with their braces doubled, so the loader's
/// markup leaves them as they are.
fn escaped(kind: Kind, mut m: Map<String, Value>) -> Map<String, Value> {
    if kind.text_fields().is_some() {
        for v in m.values_mut() {
            if let Value::String(s) = v {
                *s = s.replace('{', "{{").replace('}', "}}");
            }
        }
    }
    m
}

/// A list without the empty slots after its last used one (the loader empties them again).
fn trimmed(kind: Kind, field: &str, list: &[Value]) -> Value {
    let first = list.first().cloned().unwrap_or(Value::Null);
    let empty = empty_slot(kind, field, &first);
    let used = list.iter().rposition(|v| *v != empty).map_or(0, |i| i + 1);
    Value::Array(list[..used].to_vec())
}

/// A plain exit (a normal exit with no parameters) as its room number, else the whole exit.
fn exit_value(e: &Value) -> Value {
    let plain = e["kind"] == json!("Normal")
        && ["par1", "par2", "par3", "par4"]
            .iter()
            .all(|p| e[*p] == json!(0));
    if plain {
        e["target"].clone()
    } else {
        e.clone()
    }
}

/// `field` of a record as an entry writes it.
fn entry_value(kind: Kind, field: &str, v: &Value) -> Value {
    match v {
        Value::Array(lines)
            if is_line_field(kind, field) && lines.iter().all(|l| l.as_str().is_some()) =>
        {
            let used = lines
                .iter()
                .rposition(|l| l.as_str() != Some(""))
                .map_or(0, |i| i + 1);
            Value::String(
                lines[..used]
                    .iter()
                    .map(|l| l.as_str().unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        Value::Array(list) => trimmed(kind, field, list),
        other => other.clone(),
    }
}

/// The fields of `after` that differ from `before` (both records of `kind`), as an entry writes
/// them, with `before`'s values for a `was`. Exits are compared and written by direction.
fn differing(
    kind: Kind,
    before: &Value,
    after: &Value,
) -> (Map<String, Value>, Map<String, Value>) {
    let mut set = Map::new();
    let mut was = Map::new();
    let (Value::Object(b), Value::Object(a)) = (before, after) else {
        return (set, was);
    };
    for (field, v) in a {
        if matches!(field.as_str(), "key" | "number") || b.get(field) == Some(v) {
            continue;
        }
        let old = b.get(field).cloned().unwrap_or(Value::Null);
        if kind == Kind::Room && field == "exits" {
            let (Value::Array(new), Value::Array(prev)) = (v, &old) else {
                continue;
            };
            let mut s = Map::new();
            let mut w = Map::new();
            for (i, (n, p)) in new.iter().zip(prev).enumerate() {
                if n != p {
                    let dir = DIRS[i].to_owned();
                    s.insert(dir.clone(), exit_value(n));
                    w.insert(dir, exit_value(p));
                }
            }
            set.insert(field.clone(), Value::Object(s));
            was.insert(field.clone(), Value::Object(w));
            continue;
        }
        set.insert(field.clone(), entry_value(kind, field, v));
        was.insert(field.clone(), entry_value(kind, field, &old));
    }
    (set, was)
}

/// The direction names, in exit order.
const DIRS: [&str; 10] = [
    "north",
    "south",
    "east",
    "west",
    "northeast",
    "northwest",
    "southeast",
    "southwest",
    "up",
    "down",
];

/// A text's parts with their braces doubled, so the loader's markup leaves them as they are.
fn escaped_parts(v: &Value) -> Value {
    Value::Array(
        v["parts"]
            .as_array()
            .map(|ps| {
                ps.iter()
                    .map(|p| {
                        json!(
                            p.as_str()
                                .unwrap_or_default()
                                .replace('{', "{{")
                                .replace('}', "}}")
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
    )
}

/// Writes the overlay `name` into `out`: what `db` holds that `base` does not, or holds
/// differently. Files: `overlay.toml` (kept when `out` already has this overlay's),
/// `rooms-<map>.toml` per map, and one file per other kind.
pub async fn export(
    db: &Db,
    base: &Db,
    name: &str,
    out: &Path,
) -> Result<ExportReport, OverlayError> {
    let mut report = ExportReport::default();
    // file name -> entry name -> entries
    let mut files: BTreeMap<String, BTreeMap<String, Vec<Value>>> = BTreeMap::new();
    for kind in Kind::ALL {
        let world = all(db, kind).await?;
        let old = all(base, kind).await?;
        for (key, v) in &world {
            let file = match key {
                Key::Room(map, _) => format!("rooms-{map:03}.toml"),
                _ => format!("{}.toml", plural(kind)),
            };
            let mut entry = key_fields(kind, key);
            let entry_name = match old.get(key) {
                None => {
                    *report.created.entry(kind).or_default() += 1;
                    if kind == Kind::Text {
                        entry.insert("link".into(), v["link"].clone());
                        entry.insert("parts".into(), escaped_parts(v));
                    } else {
                        let start = blank(kind, key).map_err(|e| OverlayError::Invalid(vec![e]))?;
                        let (set, _) = differing(kind, &design(kind, &start), v);
                        entry.extend(escaped(kind, set));
                        if kind == Kind::Action {
                            entry.insert("name".into(), v["name"].clone());
                        }
                    }
                    kind.name().to_owned()
                }
                Some(prev) if prev == v => {
                    *report.same.entry(kind).or_default() += 1;
                    continue;
                }
                Some(prev) => {
                    *report.changed.entry(kind).or_default() += 1;
                    if kind == Kind::Text {
                        // a text is replaced whole
                        entry.insert("replace".into(), json!(true));
                        entry.insert("link".into(), v["link"].clone());
                        entry.insert("parts".into(), escaped_parts(v));
                        kind.name().to_owned()
                    } else {
                        let (set, was) = differing(kind, prev, v);
                        if kind == Kind::Action {
                            entry.insert("name".into(), v["name"].clone());
                        }
                        entry.insert("set".into(), Value::Object(escaped(kind, set)));
                        entry.insert("was".into(), Value::Object(escaped(kind, was)));
                        format!("{}_change", kind.name())
                    }
                }
            };
            files
                .entry(file)
                .or_default()
                .entry(entry_name)
                .or_default()
                .push(Value::Object(entry));
        }
    }
    std::fs::create_dir_all(out).map_err(|source| OverlayError::Io {
        path: out.to_path_buf(),
        source,
    })?;
    let write = |path: &Path, text: String| {
        std::fs::write(path, text).map_err(|source| OverlayError::Io {
            path: path.to_path_buf(),
            source,
        })
    };
    // a manifest already there (its description, credits and news are written by hand) stays,
    // as long as it is this overlay's
    let manifest = out.join(super::MANIFEST);
    let keep = match super::manifest(out) {
        Ok(m) if m.name == name => true,
        Ok(m) => {
            return Err(super::format_err(
                &manifest,
                format!("is overlay {:?}'s, not {name:?}", m.name),
            ));
        }
        Err(_) => false,
    };
    if !keep {
        write(
            &manifest,
            format!(
                "name = {}\n\
             # A copy of an existing world: the numbers it points at that exist nowhere are that\n\
             # world's own, so they are warnings rather than problems.\n\
             references = \"warn\"\n",
                toml_string(name)
            ),
        )?;
    }
    for (file, entries) in files {
        let mut table = Map::new();
        for (entry_name, list) in entries {
            table.insert(entry_name, Value::Array(list));
        }
        let text = toml::to_string(&Value::Object(table)).map_err(|e| OverlayError::Format {
            path: out.join(&file),
            reason: e.to_string(),
        })?;
        write(&out.join(&file), text)?;
    }
    Ok(report)
}

fn plural(kind: Kind) -> String {
    match kind {
        Kind::Class => "classes".into(),
        Kind::Help => "help".into(),
        other => format!("{}s", other.name()),
    }
}

fn toml_string(s: &str) -> String {
    toml::Value::String(s.to_owned()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_names_are_the_loaders() {
        for (i, d) in DIRS.iter().enumerate() {
            assert_eq!(direction(d), Some(i));
        }
    }

    #[test]
    fn lines_and_lists_lose_their_empty_tail() {
        let desc = json!(["one", "", "three", "", "", "", ""]);
        assert_eq!(
            entry_value(Kind::Room, "desc", &desc),
            json!("one\n\nthree")
        );
        let ab = json!([["Damage", 5], ["None", 0], ["None", 0]]);
        assert_eq!(
            entry_value(Kind::Item, "abilities", &ab),
            json!([["Damage", 5]])
        );
    }
}
