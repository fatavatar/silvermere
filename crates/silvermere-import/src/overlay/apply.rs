//! Planning an overlay against a database and writing the plan.
//!
//! [`plan`] builds every record the overlay describes and checks it before anything is written:
//! a new record's key is free (or the overlay's own, a room placeholder, or taken over with
//! `replace = true`), a change finds the values it expects (or finds itself already applied),
//! and every number a record or script points at exists in the world or in the overlay. Any
//! problem stops the whole overlay. [`write`] then writes the records kind by kind, creating
//! state for new rows (a room's floor, a monster's counts, a shop's stock) and keeping it for
//! rows that have it.

use std::collections::{BTreeMap, HashMap};
use std::fmt;

use serde_json::{Map, Value, json};
use silvermere_db::Db;
use silvermere_db::overlay_records::{self, Owner};
use silvermere_db::repo::StateWrite;
use silvermere_types::enums::{Ability, ExitType, MonsterAttackType, RoomType};
use silvermere_types::ids::{
    ClassNo, ItemNo, MapId, MessageNo, MonsterNo, RaceNo, RoomId, RoomKey, ShopNo, SpellNo,
    TextBlockNo,
};
use silvermere_types::records::{
    AbilitySlot, Action as ActionRecord, Class, Item, KnownMonster, Message, Race, Room, Shop,
    Spell, TextBlock,
};

use super::build::{self, TEXT_WIDTH};
use super::{Entry, Key, Kind, Origin, Overlay, OverlayError};

/// A room whose name starts so is an empty slot the realm's editor left, free to fill.
const PLACEHOLDER_ROOM: &str = "Buffer Room";

/// What writing a planned record does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    /// Creates a record that does not exist.
    Create,
    /// Fills a room placeholder, or takes over a record with `replace = true`.
    TakeOver,
    /// Rewrites a record this overlay created before.
    Rewrite,
    /// Changes fields of an existing record.
    Change,
    /// Already as the overlay describes it; nothing is written.
    Unchanged,
}

impl Action {
    fn label(self) -> &'static str {
        match self {
            Action::Create => "created",
            Action::TakeOver => "taken over",
            Action::Rewrite => "rewritten",
            Action::Change => "changed",
            Action::Unchanged => "unchanged",
        }
    }
}

/// A built record.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// A room.
    Room(Box<Room>),
    /// A known monster.
    Monster(Box<KnownMonster>),
    /// An item.
    Item(Box<Item>),
    /// A spell.
    Spell(Box<Spell>),
    /// A shop.
    Shop(Box<Shop>),
    /// A message.
    Message(Message),
    /// Every part of a text block.
    Text(Vec<TextBlock>),
    /// A class.
    Class(Box<Class>),
    /// A race.
    Race(Box<Race>),
    /// An action.
    Action(Box<ActionRecord>),
}

/// One record of a plan.
#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    /// Its kind.
    pub kind: Kind,
    /// Its key.
    pub key: Key,
    /// The entry that made it (the last one, when a change followed a create).
    pub origin: Origin,
    /// What writing it does.
    pub action: Action,
    /// Whether the overlay owns it: it was created by an entry rather than changed.
    pub created: bool,
    /// For a realm record the overlay changes: each field its changes set, as they leave it
    /// (recorded on write, so a revised change can tell the record still holds them).
    pub changed_fields: Option<Map<String, Value>>,
    /// The record.
    pub record: Record,
}

/// An overlay checked against a database: every record it writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The overlay's name.
    pub overlay: String,
    /// The records, in the order their entries came.
    pub records: Vec<Planned>,
    /// Things worth a look that do not stop the overlay (a line too wide, a script number
    /// not found).
    pub warnings: Vec<String>,
}

impl fmt::Display for Plan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut counts: BTreeMap<(Kind, Action), u32> = BTreeMap::new();
        for p in &self.records {
            *counts.entry((p.kind, p.action)).or_default() += 1;
        }
        for kind in Kind::ALL {
            let parts: Vec<String> = counts
                .iter()
                .filter(|((k, _), _)| *k == kind)
                .map(|((_, a), n)| format!("{n} {}", a.label()))
                .collect();
            if !parts.is_empty() {
                writeln!(
                    f,
                    "overlay [{}] {:<8} {}",
                    self.overlay,
                    kind.name(),
                    parts.join(", ")
                )?;
            }
        }
        // a copy of a whole world can carry hundreds; the first ones say what they are
        const SHOWN: usize = 20;
        for w in self.warnings.iter().take(SHOWN) {
            writeln!(f, "overlay [{}] warning: {w}", self.overlay)?;
        }
        if self.warnings.len() > SHOWN {
            writeln!(
                f,
                "overlay [{}] warning: ... and {} more",
                self.overlay,
                self.warnings.len() - SHOWN
            )?;
        }
        Ok(())
    }
}

/// The world as the plan sees it: the database, read once per key, under the overlay's own
/// records so far.
struct World<'a> {
    db: &'a Db,
    stored: HashMap<(Kind, Key), Option<Value>>,
    pending: HashMap<(Kind, Key), Value>,
}

fn db_err(e: silvermere_db::DbError) -> OverlayError {
    OverlayError::Db(e)
}

fn to_value<T: serde::Serialize>(r: &T) -> Value {
    serde_json::to_value(r).unwrap_or(Value::Null)
}

fn number<T: TryFrom<i64>>(key: Key) -> Option<T> {
    match key {
        Key::Number(n) => T::try_from(n).ok(),
        Key::Room(..) | Key::Name(_) => None,
    }
}

impl<'a> World<'a> {
    /// The stored record at `kind`/`key`, as its serde form (text: `{link, text}`).
    async fn stored(&mut self, kind: Kind, key: Key) -> Result<Option<Value>, OverlayError> {
        if let Some(v) = self.stored.get(&(kind, key.clone())) {
            return Ok(v.clone());
        }
        let db = self.db;
        let v = match (kind, key.clone()) {
            (Kind::Room, Key::Room(m, r)) => db
                .rooms()
                .get(RoomKey {
                    map: MapId(m),
                    room: RoomId(r),
                })
                .await
                .map_err(db_err)?
                .map(|r| to_value(&r)),
            (Kind::Monster, k) => match number::<i32>(k) {
                Some(n) => db
                    .monsters()
                    .get(MonsterNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Item, k) => match number::<i32>(k) {
                Some(n) => db
                    .items()
                    .get(ItemNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Spell, k) => match number::<i16>(k) {
                Some(n) => db
                    .spells()
                    .get(SpellNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Shop, k) => match number::<i32>(k) {
                Some(n) => db
                    .shops()
                    .get(ShopNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Message, k) => match number::<i32>(k) {
                Some(n) => db
                    .messages()
                    .get(MessageNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Text, k) => match number::<i32>(k) {
                Some(n) => {
                    let parts = db
                        .text_blocks()
                        .parts(TextBlockNo(n))
                        .await
                        .map_err(db_err)?;
                    parts.first().map(|first| {
                        json!({
                            "link": first.link.0,
                            "parts": parts.iter().map(|p| p.text.clone()).collect::<Vec<_>>(),
                        })
                    })
                }
                None => None,
            },
            (Kind::Class, k) => match number::<u16>(k) {
                Some(n) => db
                    .classes()
                    .get(ClassNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Race, k) => match number::<u16>(k) {
                Some(n) => db
                    .races()
                    .get(RaceNo(n))
                    .await
                    .map_err(db_err)?
                    .map(|r| to_value(&r)),
                None => None,
            },
            (Kind::Action, Key::Name(n)) => db
                .actions()
                .by_name(&n)
                .await
                .map_err(db_err)?
                .map(|r| to_value(&r)),
            _ => None,
        };
        self.stored.insert((kind, key), v.clone());
        Ok(v)
    }

    /// The record as the overlay leaves it so far: its own, else the stored one.
    async fn current(&mut self, kind: Kind, key: Key) -> Result<Option<Value>, OverlayError> {
        if let Some(v) = self.pending.get(&(kind, key.clone())) {
            return Ok(Some(v.clone()));
        }
        self.stored(kind, key).await
    }

    async fn exists(&mut self, kind: Kind, key: Key) -> Result<bool, OverlayError> {
        Ok(self.current(kind, key).await?.is_some())
    }
}

/// The stored record at `kind`/`key` as an overlay names its fields (a text as `{link, text}`),
/// `None` when there is none: what `silvermere overlay show` prints.
pub async fn stored_record(db: &Db, kind: Kind, key: Key) -> Result<Option<Value>, OverlayError> {
    let mut world = World {
        db,
        stored: HashMap::new(),
        pending: HashMap::new(),
    };
    world.stored(kind, key).await
}

/// A record without what play changes (a room's floor and monsters, a monster's counts, an
/// item's copies in play, a shop's stock on hand): what an overlay writes, for telling whether a rewrite changes
/// anything. Writing keeps the stored state either way.
pub(super) fn design(kind: Kind, v: &Value) -> Value {
    let mut v = v.clone();
    let Value::Object(m) = &mut v else { return v };
    let live: &[&str] = match kind {
        Kind::Room => &[
            "monsters",
            "items",
            "hiditems",
            "coins",
            "hidcoins",
            "owner",
            "lastkill",
            "foundmask",
            "areacount",
            "searchers",
            "nummonsters",
        ],
        Kind::Monster => &["curalive", "killed_at"],
        Kind::Item => &["active"],
        _ => &[],
    };
    for f in live {
        m.remove(*f);
    }
    // a shop's stock on hand is play; its slots' items and restocking are content
    if kind == Kind::Shop
        && let Some(Value::Array(stock)) = m.get_mut("stock")
    {
        for slot in stock {
            if let Value::Object(slot) = slot {
                slot.remove("qty");
            }
        }
    }
    // whether a room's own monster is alive is a flag bit, set and cleared by play
    if kind == Kind::Room
        && let Some(Value::String(flags)) = m.get_mut("flags")
    {
        *flags = flags
            .split('|')
            .map(str::trim)
            .filter(|f| !f.is_empty() && *f != "SPECALIVE")
            .collect::<Vec<_>>()
            .join(" | ");
    }
    v
}

/// `values` merged onto a copy of `base`, or the merge's error.
fn merged(kind: Kind, base: &Value, values: &Map<String, Value>) -> Result<Value, String> {
    let mut v = base.clone();
    build::merge(kind, &mut v, values)?;
    Ok(v)
}

/// The text entry's record form, `{link, parts}`, from its values: `text` split into parts at
/// line ends, or `parts` as given (an export keeps the stored parts exactly).
fn text_value(values: &Map<String, Value>) -> Result<Value, String> {
    for k in values.keys() {
        if !matches!(k.as_str(), "text" | "parts" | "link") {
            return Err(format!(
                "`{k}`: a text holds `text` (or `parts`) and `link`"
            ));
        }
    }
    let link = match values.get("link") {
        None => 0,
        Some(v) => v.as_i64().ok_or("`link` must be a text block number")?,
    };
    let parts: Vec<String> = match (values.get("text"), values.get("parts")) {
        (Some(Value::String(s)), None) => {
            let text = build::markup(s)?;
            build::text_parts(0, 0, &text)?
                .into_iter()
                .map(|p| p.text)
                .collect()
        }
        (None, Some(Value::Array(a))) => a
            .iter()
            .map(|p| match p {
                Value::String(s) => {
                    let s = build::markup(s)?;
                    let n = s.chars().count();
                    if n > build::TEXT_PART_CHARS {
                        return Err(format!("a part of {n} characters is too long"));
                    }
                    Ok(s)
                }
                _ => Err("`parts` lists strings".to_string()),
            })
            .collect::<Result<_, _>>()?,
        _ => return Err("needs `text = \"...\"` or `parts = [...]`".into()),
    };
    Ok(json!({"link": link, "parts": parts}))
}

fn typed(kind: Kind, key: Key, v: Value) -> Result<Record, String> {
    let e = |e: serde_json::Error| e.to_string();
    Ok(match kind {
        Kind::Room => Record::Room(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Monster => Record::Monster(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Item => Record::Item(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Spell => Record::Spell(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Shop => Record::Shop(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Message => Record::Message(serde_json::from_value(v).map_err(e)?),
        Kind::Class => Record::Class(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Race => Record::Race(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Action => Record::Action(Box::new(serde_json::from_value(v).map_err(e)?)),
        Kind::Text => {
            let Key::Number(n) = key else {
                return Err("a text is numbered".into());
            };
            let n = i32::try_from(n).map_err(|_| "text number out of range".to_string())?;
            let link = v["link"]
                .as_i64()
                .and_then(|l| i32::try_from(l).ok())
                .unwrap_or(0);
            let parts = v["parts"].as_array().cloned().unwrap_or_default();
            Record::Text(
                parts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| TextBlock {
                        part: i16::try_from(i).unwrap_or(i16::MAX),
                        number: TextBlockNo(n),
                        link: TextBlockNo(link),
                        text: p.as_str().unwrap_or_default().to_owned(),
                    })
                    .collect(),
            )
        }
    })
}

/// Puts the key into a record built from a template or blank.
fn set_key(kind: Kind, key: Key, v: &mut Value) {
    let Value::Object(m) = v else { return };
    match key {
        Key::Room(map, room) => {
            m.insert("key".into(), json!({"map": map, "room": room}));
        }
        Key::Number(n) if kind != Kind::Text => {
            m.insert("number".into(), json!(n));
        }
        Key::Number(_) => {}
        // an action's verb is its own `name` field, set by the entry
        Key::Name(_) => {}
    }
}

/// The base a new record starts from: a blank, or its `from` template.
/// A blank record of `kind` with `key`: every field zero or empty.
pub(super) fn blank(kind: Kind, key: &Key) -> Result<Value, String> {
    let range = || format!("{kind} {key}: the number is out of range");
    let n = |k: &Key| match k {
        Key::Number(n) => Some(*n),
        _ => None,
    };
    Ok(match (kind, key) {
        (Kind::Room, Key::Room(m, r)) => to_value(&build::blank_room(*m, *r)),
        (Kind::Action, Key::Name(name)) => to_value(&build::blank_action(name)),
        (Kind::Text, _) => json!({"link": 0, "parts": []}),
        (Kind::Monster, k) => to_value(&build::blank_monster(
            n(k).and_then(|n| i32::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Item, k) => to_value(&build::blank_item(
            n(k).and_then(|n| i32::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Spell, k) => to_value(&build::blank_spell(
            n(k).and_then(|n| i16::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Shop, k) => to_value(&build::blank_shop(
            n(k).and_then(|n| i32::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Message, k) => to_value(&build::blank_message(
            n(k).and_then(|n| i32::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Class, k) => to_value(&build::blank_class(
            n(k).and_then(|n| u16::try_from(n).ok()).ok_or_else(range)?,
        )),
        (Kind::Race, k) => to_value(&build::blank_race(
            n(k).and_then(|n| u16::try_from(n).ok()).ok_or_else(range)?,
        )),
        _ => return Err(format!("{kind} {key}: not a {kind} key")),
    })
}

async fn new_base(world: &mut World<'_>, e: &Entry) -> Result<Value, String> {
    match e.kind {
        Kind::Text => blank(e.kind, &e.key),
        kind => {
            let Some(from) = e.from.clone() else {
                return blank(kind, &e.key);
            };
            let mut base = world
                .current(kind, from.clone())
                .await
                .map_err(|err| err.to_string())?
                .ok_or_else(|| format!("`from`: {kind} {from} does not exist"))?;
            if kind == Kind::Room {
                // a template room lends its look and spawns, never its place in the world
                let Key::Room(m, r) = e.key else {
                    unreachable!("a room key")
                };
                let blank = to_value(&build::blank_room(m, r));
                for f in [
                    "exits",
                    "monsters",
                    "items",
                    "hiditems",
                    "coins",
                    "hidcoins",
                    "owner",
                    "lastkill",
                    "foundmask",
                    "areacount",
                    "searchers",
                    "nummonsters",
                    "placed",
                    "specmon",
                    "cmdtext",
                    "protmsg",
                    "shopnum",
                    "ctlroom",
                ] {
                    base[f] = blank[f].clone();
                }
            }
            build::fresh(kind, &mut base);
            Ok(base)
        }
    }
}

/// Builds and checks every record of `overlay` against `db`. Nothing is written.
pub async fn plan(db: &Db, overlay: &Overlay) -> Result<Plan, OverlayError> {
    let mut world = World {
        db,
        stored: HashMap::new(),
        pending: HashMap::new(),
    };
    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut order: Vec<(Kind, Key)> = Vec::new();
    let mut planned: HashMap<(Kind, Key), Planned> = HashMap::new();
    let mut created_here: HashMap<(Kind, Key), Origin> = HashMap::new();
    // what each record started as (its template, blank or stored self): the numbers it
    // already held are the realm's business, not the overlay's
    let mut starts: HashMap<(Kind, Key), Value> = HashMap::new();

    for e in &overlay.entries {
        let id = (e.kind, e.key.clone());
        let fail = |why: String| format!("{}: {} {}: {why}", e.origin, e.kind, e.key);
        let stored = world.stored(e.kind, e.key.clone()).await?;
        let mut changed_fields: Option<Map<String, Value>> = None;
        let (value, action, created) = if e.change {
            let Some(base) = world.current(e.kind, e.key.clone()).await? else {
                problems.push(fail(
                    "does not exist; a change needs a record to change".into(),
                ));
                continue;
            };
            starts.entry(id.clone()).or_insert_with(|| base.clone());
            let after = match merged(e.kind, &base, &e.values) {
                Ok(v) => v,
                Err(why) => {
                    problems.push(fail(why));
                    continue;
                }
            };
            let expected = match merged(e.kind, &base, &e.was) {
                Ok(v) => v,
                Err(why) => {
                    problems.push(fail(format!("`was`: {why}")));
                    continue;
                }
            };
            let own = created_here.contains_key(&id);
            if !own {
                let mut fields = planned
                    .get(&id)
                    .and_then(|p| p.changed_fields.clone())
                    .unwrap_or_default();
                for k in e.values.keys() {
                    fields.insert(
                        k.clone(),
                        after.get(k.as_str()).cloned().unwrap_or(Value::Null),
                    );
                }
                changed_fields = Some(fields);
            }
            // a change this overlay (or one it overrides) made before: a field may hold the
            // value that overlay last set rather than `was`, as long as nobody has changed it
            // since
            let mut revised = false;
            if !own && expected != base {
                let mut excused: Vec<(String, Value)> = Vec::new();
                for name in std::iter::once(&overlay.name).chain(overlay.overrides.iter()) {
                    let last =
                        overlay_records::last_change(db, e.kind.name(), &e.key.to_string(), name)
                            .await
                            .map_err(db_err)?;
                    if let Some(Ok(Value::Object(last))) = last.map(|f| serde_json::from_str(&f)) {
                        excused.extend(last);
                    }
                }
                revised = !excused.is_empty()
                    && e.was.keys().all(|k| {
                        let now = base.get(k.as_str());
                        expected.get(k.as_str()) == now
                            || excused.iter().any(|(f, v)| f == k && Some(v) == now)
                    });
            }
            if after == base {
                (after, Action::Unchanged, own)
            } else if expected != base && !revised {
                let diffs: Vec<String> = e
                    .was
                    .keys()
                    .filter(|k| expected.get(k.as_str()) != base.get(k.as_str()))
                    .map(|k| {
                        format!(
                            "`{k}` is {}, the overlay expects {}",
                            base.get(k.as_str()).unwrap_or(&Value::Null),
                            expected.get(k.as_str()).unwrap_or(&Value::Null)
                        )
                    })
                    .collect();
                problems.push(fail(format!("not as expected: {}", diffs.join("; "))));
                continue;
            } else {
                // a change to a record this overlay creates is part of creating it
                match planned.get(&id) {
                    Some(p) if p.created => {
                        let action = match p.action {
                            Action::Unchanged => Action::Rewrite,
                            other => other,
                        };
                        (after, action, true)
                    }
                    _ => (after, Action::Change, false),
                }
            }
        } else {
            if let Some(first) = created_here.get(&id) {
                problems.push(fail(format!("already created by {first}")));
                continue;
            }
            let mut action = Action::Create;
            if let Some(old) = &stored {
                let owner = overlay_records::owner(db, e.kind.name(), &e.key.to_string())
                    .await
                    .map_err(db_err)?;
                let placeholder = e.kind == Kind::Room
                    && old["name"]
                        .as_str()
                        .is_some_and(|n| n.starts_with(PLACEHOLDER_ROOM));
                action = match owner {
                    Owner::Overlay(name) if name == overlay.name => Action::Rewrite,
                    _ if placeholder || e.replace => Action::TakeOver,
                    Owner::Overlay(name) if overlay.overrides.contains(&name) => Action::TakeOver,
                    Owner::Overlay(name) => {
                        problems.push(fail(format!("exists, created by the overlay `{name}`")));
                        continue;
                    }
                    Owner::Realm => {
                        problems.push(fail(
                            "exists in the realm; pick a free number, or set `replace = true` \
                             to take the record over"
                                .into(),
                        ));
                        continue;
                    }
                };
            }
            let built = async {
                let mut v = new_base(&mut world, e).await?;
                let mut start = v.clone();
                set_key(e.kind, e.key.clone(), &mut start);
                if e.kind == Kind::Text {
                    v = text_value(&e.values)?;
                } else {
                    build::merge(e.kind, &mut v, &e.values)?;
                }
                set_key(e.kind, e.key.clone(), &mut v);
                Ok::<_, String>((start, v))
            }
            .await;
            let v = match built {
                Ok((start, v)) => {
                    starts.entry(id.clone()).or_insert(start);
                    v
                }
                Err(why) => {
                    problems.push(fail(why));
                    continue;
                }
            };
            if action == Action::Rewrite
                && stored.as_ref().map(|old| design(e.kind, old)) == Some(design(e.kind, &v))
            {
                action = Action::Unchanged;
            }
            created_here.insert(id.clone(), e.origin.clone());
            (v, action, true)
        };
        if changed_fields.is_none() {
            changed_fields = planned.get(&id).and_then(|p| p.changed_fields.clone());
        }
        let record = match typed(e.kind, e.key.clone(), value.clone()) {
            Ok(r) => r,
            Err(why) => {
                problems.push(fail(why));
                continue;
            }
        };
        if let Record::Text(parts) = &record {
            let text: String = parts.iter().map(|p| p.text.as_str()).collect();
            for (i, line) in text.lines().enumerate() {
                // script lines are read by the game, not shown
                if build::visible_width(line) > TEXT_WIDTH && !is_script_line(line) {
                    warnings.push(format!(
                        "{}: text {} line {} is {} columns, wider than {TEXT_WIDTH}",
                        e.origin,
                        e.key,
                        i + 1,
                        build::visible_width(line)
                    ));
                }
            }
        }
        world.pending.insert(id.clone(), value);
        if !planned.contains_key(&id) {
            order.push(id.clone());
        }
        planned.insert(
            id,
            Planned {
                kind: e.kind,
                key: e.key.clone(),
                origin: e.origin.clone(),
                action,
                created,
                changed_fields,
                record,
            },
        );
    }

    // every number a record points at must exist, in the world or the overlay
    for id in &order {
        let p = &planned[id];
        if p.action == Action::Unchanged && !p.created {
            continue;
        }
        let (mut hard, mut soft) = references(&p.record);
        if let Some(start) = starts
            .get(id)
            .and_then(|v| typed(p.kind, p.key.clone(), v.clone()).ok())
        {
            let (had_hard, had_soft) = references(&start);
            hard.retain(|r| !had_hard.contains(r));
            soft.retain(|r| !had_soft.contains(r));
        }
        for (field, kind, key) in hard {
            if !world.exists(kind, key.clone()).await? {
                let list = if overlay.references_warn {
                    &mut warnings
                } else {
                    &mut problems
                };
                list.push(format!(
                    "{}: {} {}: `{field}` names {kind} {key}, which does not exist",
                    p.origin, p.kind, p.key
                ));
            }
        }
        for (field, kind, key) in soft {
            if !world.exists(kind, key.clone()).await? {
                warnings.push(format!(
                    "{}: {} {}: {field} names {kind} {key}, which does not exist",
                    p.origin, p.kind, p.key
                ));
            }
        }
    }
    if !problems.is_empty() {
        return Err(OverlayError::Invalid(problems));
    }
    Ok(Plan {
        overlay: overlay.name.clone(),
        records: order
            .into_iter()
            .filter_map(|id| planned.remove(&id))
            .collect(),
        warnings,
    })
}

type Ref = (String, Kind, Key);

fn num(n: impl Into<i64>) -> Key {
    Key::Number(n.into())
}

fn ability_refs(out: &mut Vec<Ref>, abilities: &[AbilitySlot]) {
    for (i, &(a, v)) in abilities.iter().enumerate() {
        if v <= 0 {
            continue;
        }
        let kind = match a {
            Ability::TextBlock | Ability::DeathText => Kind::Text,
            Ability::LinkSpell
            | Ability::OneTimeCast
            | Ability::GiveSpell
            | Ability::RemoveSpell
            | Ability::EndCast
            | Ability::TermSpell
            | Ability::Negate => Kind::Spell,
            Ability::Summon => Kind::Monster,
            Ability::DescMsg | Ability::StartMsg | Ability::ShockMsg | Ability::ConfuseMsg => {
                Kind::Message
            }
            _ => continue,
        };
        out.push((format!("abilities[{i}]"), kind, num(v)));
    }
}

fn push_if(out: &mut Vec<Ref>, field: impl Into<String>, kind: Kind, n: i64) {
    if n > 0 {
        out.push((field.into(), kind, num(n)));
    }
}

/// The numbers a record points at: those that must exist, and (for scripts, read loosely)
/// those that should.
fn references(r: &Record) -> (Vec<Ref>, Vec<Ref>) {
    let mut hard = Vec::new();
    let mut soft = Vec::new();
    match r {
        Record::Room(room) => {
            for (i, x) in room.exits.iter().enumerate() {
                if x.target.0 != 0 {
                    // a map-change exit leads to the map in its first parameter
                    let map = if x.kind == ExitType::MapChange && x.par1 != 0 {
                        x.par1
                    } else {
                        room.key.map.0
                    };
                    hard.push((
                        format!("exits[{i}]"),
                        Kind::Room,
                        Key::Room(map, x.target.0),
                    ));
                }
            }
            push_if(&mut hard, "specmon", Kind::Monster, room.specmon.0.into());
            push_if(
                &mut hard,
                "genmonster",
                Kind::Monster,
                room.genmonster.0.into(),
            );
            push_if(&mut hard, "cmdtext", Kind::Text, room.cmdtext.0.into());
            push_if(&mut hard, "protmsg", Kind::Text, room.protmsg.0.into());
            push_if(&mut hard, "roomspell", Kind::Spell, room.roomspell.into());
            if room.room_type == RoomType::Shop {
                push_if(&mut hard, "shopnum", Kind::Shop, room.shopnum.0.into());
            }
            for (i, it) in room.placed.iter().enumerate() {
                push_if(&mut hard, format!("placed[{i}]"), Kind::Item, it.0.into());
            }
        }
        Record::Monster(m) => {
            push_if(&mut hard, "weapon", Kind::Item, m.weapon.0.into());
            push_if(&mut hard, "wornitem", Kind::Item, m.wornitem.0.into());
            push_if(&mut hard, "deathspell", Kind::Spell, m.deathspell.0.into());
            push_if(
                &mut hard,
                "createspell",
                Kind::Spell,
                m.createspell.0.into(),
            );
            push_if(&mut hard, "talktext", Kind::Text, m.talktext.0.into());
            push_if(&mut hard, "randtext", Kind::Text, m.randtext.0.into());
            push_if(&mut hard, "arrivemsg", Kind::Message, m.arrivemsg.0.into());
            push_if(&mut hard, "deathmsg", Kind::Message, m.deathmsg.0.into());
            for (i, it) in m.items.iter().enumerate() {
                push_if(
                    &mut hard,
                    format!("items[{i}]"),
                    Kind::Item,
                    it.item.0.into(),
                );
            }
            for (i, s) in m.spells.iter().enumerate() {
                push_if(
                    &mut hard,
                    format!("spells[{i}]"),
                    Kind::Spell,
                    s.spell.0.into(),
                );
            }
            for (i, a) in m.attacks.iter().enumerate() {
                if a.kind == MonsterAttackType::Spell {
                    push_if(
                        &mut hard,
                        format!("attacks[{i}].accuracy"),
                        Kind::Spell,
                        a.accuracy.into(),
                    );
                }
                push_if(
                    &mut hard,
                    format!("attacks[{i}].hitspell"),
                    Kind::Spell,
                    a.hitspell.0.into(),
                );
                push_if(
                    &mut hard,
                    format!("attacks[{i}].hitmsg"),
                    Kind::Message,
                    a.hitmsg.0.into(),
                );
                push_if(
                    &mut hard,
                    format!("attacks[{i}].missmsg"),
                    Kind::Message,
                    a.missmsg.0.into(),
                );
            }
            ability_refs(&mut hard, &m.abilities);
        }
        Record::Item(it) => {
            push_if(&mut hard, "textnum", Kind::Text, it.textnum.0.into());
            push_if(&mut hard, "hitmsg", Kind::Message, it.hitmsg.0.into());
            push_if(&mut hard, "missmsg", Kind::Message, it.missmsg.0.into());
            push_if(
                &mut hard,
                "destroymsg",
                Kind::Message,
                it.destroymsg.0.into(),
            );
            for (i, s) in it.negates.iter().enumerate() {
                push_if(&mut hard, format!("negates[{i}]"), Kind::Spell, s.0.into());
            }
            ability_refs(&mut hard, &it.abilities);
        }
        Record::Class(c) => {
            push_if(&mut hard, "titles", Kind::Text, c.titles.0.into());
            ability_refs(&mut hard, &c.abilities);
        }
        Record::Race(r) => ability_refs(&mut hard, &r.abilities),
        Record::Action(_) => {}
        Record::Spell(s) => {
            push_if(&mut hard, "msgnum", Kind::Message, s.msgnum.0.into());
            ability_refs(&mut hard, &s.abilities);
        }
        Record::Shop(s) => {
            for (i, slot) in s.stock.iter().enumerate() {
                push_if(
                    &mut hard,
                    format!("stock[{i}]"),
                    Kind::Item,
                    slot.item.0.into(),
                );
            }
        }
        Record::Message(_) => {}
        Record::Text(parts) => {
            let text: String = parts.iter().map(|p| p.text.as_str()).collect();
            script_refs(&text, parts.first().map_or(0, |p| p.link.0), &mut soft);
        }
    }
    (hard, soft)
}

/// The numbers a script names, read loosely: topics (`word:N`), `text`, the item, monster,
/// spell, room and message commands, and the message a failed condition shows.
fn script_refs(text: &str, link: i32, out: &mut Vec<Ref>) {
    push_if(out, "link".to_string(), Kind::Text, link.into());
    for (ln, line) in text.lines().enumerate() {
        let at = |what: &str| format!("line {} `{what}`", ln + 1);
        let line = line.trim();
        if let Some((topic, n)) = line.rsplit_once(':')
            && let Ok(n) = n.trim().parse::<i64>()
            && !topic.contains(':')
            && topic
                .split_whitespace()
                .next()
                .is_some_and(|w| !is_command(w))
        {
            push_if(out, at(line), Kind::Text, n);
            continue;
        }
        for clause in line.split(':') {
            let words: Vec<&str> = clause.split_whitespace().collect();
            let Some(op) = words.first().map(|w| w.to_ascii_lowercase()) else {
                continue;
            };
            let nums: Vec<i64> = words[1..].iter().filter_map(|w| w.parse().ok()).collect();
            let first = nums.first().copied().unwrap_or(0);
            match op.as_str() {
                "text" | "random" => push_if(out, at(clause), Kind::Text, first),
                "checkitem" | "takeitem" | "giveitem" | "failitem" | "clearitem" | "roomitem"
                | "failroomitem" => {
                    push_if(out, at(clause), Kind::Item, first);
                    if nums.len() > 1 {
                        push_if(out, at(clause), Kind::Message, nums[1]);
                    }
                }
                "summon" | "needmonster" => push_if(out, at(clause), Kind::Monster, first),
                "cast" | "learnspell" => push_if(out, at(clause), Kind::Spell, first),
                "message" => push_if(out, at(clause), Kind::Message, first),
                "teleport" if nums.len() > 1 => {
                    if let (Ok(m), Ok(r)) = (u32::try_from(nums[1]), u32::try_from(first)) {
                        out.push((at(clause), Kind::Room, Key::Room(m, r)));
                    }
                }
                "teleport" => {}
                "goodaligned" | "evilaligned" | "minlevel" | "maxlevel" | "class" | "race"
                    if nums.len() > 1 =>
                {
                    push_if(out, at(clause), Kind::Message, nums[nums.len() - 1]);
                }
                "testability" | "checkability" if nums.len() > 2 => {
                    push_if(out, at(clause), Kind::Message, nums[2]);
                }
                "failability" if nums.len() > 1 => {
                    push_if(out, at(clause), Kind::Message, nums[1]);
                }
                _ => {}
            }
        }
    }
}

/// Whether `line` reads as a script line: a topic (`word:N`) or clauses that start with a
/// command.
fn is_script_line(line: &str) -> bool {
    let line = line.trim();
    if let Some((_, n)) = line.rsplit_once(':')
        && n.trim().parse::<i64>().is_ok()
    {
        return true;
    }
    line.split(':')
        .any(|clause| clause.split_whitespace().next().is_some_and(is_command))
}

fn is_command(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "text"
            | "random"
            | "checkitem"
            | "takeitem"
            | "giveitem"
            | "failitem"
            | "clearitem"
            | "roomitem"
            | "failroomitem"
            | "summon"
            | "needmonster"
            | "cast"
            | "learnspell"
            | "message"
            | "teleport"
            | "goodaligned"
            | "evilaligned"
            | "minlevel"
            | "maxlevel"
            | "class"
            | "race"
            | "testability"
            | "checkability"
            | "failability"
            | "giveability"
            | "addability"
            | "removeability"
            | "addexp"
            | "givecoins"
            | "price"
            | "addevil"
            | "nomonsters"
    )
}

/// Reads the overlays in `dirs`, puts them in load order (each after the overlays its `after`
/// names), and plans each against `db`; with `write`, writes each before planning the next, so
/// a later overlay sees what an earlier one wrote. An `after` naming an overlay not among them
/// must have been applied to `db` before. Stops at the first overlay with a problem; what the
/// overlays before it wrote stays.
pub async fn apply_dirs(
    db: &Db,
    dirs: &[std::path::PathBuf],
    write_them: bool,
) -> Result<Vec<Plan>, OverlayError> {
    let overlays = dirs
        .iter()
        .map(|d| super::load(d))
        .collect::<Result<Vec<_>, _>>()?;
    let (ordered, outside) = super::order(overlays)?;
    let mut missing = Vec::new();
    for (who, needs) in outside {
        if !overlay_records::applied(db, &needs).await.map_err(db_err)? {
            missing.push(format!(
                "`{who}` loads after `{needs}`, which is neither given nor applied here"
            ));
        }
    }
    if !missing.is_empty() {
        return Err(OverlayError::Invalid(missing));
    }
    let mut plans = Vec::new();
    for o in &ordered {
        let p = plan(db, o).await?;
        if write_them {
            write(db, &p).await?;
        }
        plans.push(p);
    }
    Ok(plans)
}

/// Writes `plan`: every record not already as planned, kind by kind, state created for new
/// rows and kept for the rest; then records the overlay as the owner of what it created.
pub async fn write(db: &Db, plan: &Plan) -> Result<(), OverlayError> {
    let todo = || {
        plan.records
            .iter()
            .filter(|p| p.action != Action::Unchanged)
    };
    let rooms: Vec<Room> = todo()
        .filter_map(|p| match &p.record {
            Record::Room(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let monsters: Vec<KnownMonster> = todo()
        .filter_map(|p| match &p.record {
            Record::Monster(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let items: Vec<Item> = todo()
        .filter_map(|p| match &p.record {
            Record::Item(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let spells: Vec<Spell> = todo()
        .filter_map(|p| match &p.record {
            Record::Spell(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let shops: Vec<Shop> = todo()
        .filter_map(|p| match &p.record {
            Record::Shop(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let messages: Vec<Message> = todo()
        .filter_map(|p| match &p.record {
            Record::Message(r) => Some(r.clone()),
            _ => None,
        })
        .collect();
    let classes: Vec<Class> = todo()
        .filter_map(|p| match &p.record {
            Record::Class(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let races: Vec<Race> = todo()
        .filter_map(|p| match &p.record {
            Record::Race(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let actions: Vec<ActionRecord> = todo()
        .filter_map(|p| match &p.record {
            Record::Action(r) => Some((**r).clone()),
            _ => None,
        })
        .collect();
    let texts: Vec<&Vec<TextBlock>> = todo()
        .filter_map(|p| match &p.record {
            Record::Text(r) => Some(r),
            _ => None,
        })
        .collect();

    if !rooms.is_empty() {
        db.rooms()
            .put_many(&rooms, StateWrite::CreateMissing)
            .await?;
    }
    if !monsters.is_empty() {
        db.monsters()
            .put_many(&monsters, StateWrite::CreateMissing)
            .await?;
    }
    if !items.is_empty() {
        db.items()
            .put_many(&items, StateWrite::CreateMissing)
            .await?;
    }
    if !spells.is_empty() {
        db.spells().put_many(&spells).await?;
    }
    if !shops.is_empty() {
        db.shops()
            .put_many(&shops, StateWrite::CreateMissing)
            .await?;
    }
    if !messages.is_empty() {
        db.messages().put_many(&messages).await?;
    }
    if !classes.is_empty() {
        db.classes().put_many(&classes).await?;
    }
    if !races.is_empty() {
        db.races().put_many(&races).await?;
    }
    if !actions.is_empty() {
        db.actions().put_many(&actions).await?;
    }
    for parts in texts {
        let Some(first) = parts.first() else { continue };
        let count = i16::try_from(parts.len()).unwrap_or(i16::MAX);
        db.text_blocks().delete_from(first.number, count).await?;
        db.text_blocks().put_many(parts).await?;
    }
    for p in plan.records.iter().filter(|p| p.created) {
        overlay_records::record(db, p.kind.name(), &p.key.to_string(), &plan.overlay).await?;
    }
    for p in &plan.records {
        if let Some(fields) = &p.changed_fields {
            let fields = Value::Object(fields.clone()).to_string();
            overlay_records::record_change(
                db,
                p.kind.name(),
                &p.key.to_string(),
                &plan.overlay,
                &fields,
            )
            .await?;
        }
    }
    Ok(())
}

/// What adopting an overlay found ([`adopt_dir`]).
#[derive(Debug, Clone, Default)]
pub struct Adoption {
    pub overlay: String,
    /// Records it creates, now recorded as its own.
    pub owned: usize,
    /// Records it changes, now recorded as changed by it.
    pub changed: usize,
    /// Records another overlay has taken over since: left that overlay's.
    pub taken_over: usize,
    /// Records another overlay has changed since, so no longer as this one made them: adopted
    /// as they are.
    pub changed_since: usize,
}

/// Adopts the overlay in `dir`: records that the world already holds it, for a world built
/// before it was an overlay (a realm imported from the data files the overlay was exported
/// from). No content is written; with `write_it`, the overlay's ownership and changes are
/// recorded, in one transaction, as an `apply` would have recorded them.
///
/// Every record the overlay creates must exist and be as the overlay builds it, and every change
/// must already be made. A record another overlay has changed since is excused, and one another
/// overlay has taken over stays that overlay's. Anything else is a problem, and nothing is
/// adopted. Records are read one at a time and only their keys kept, so a whole world's overlay
/// is adopted in little memory.
pub async fn adopt_dir(
    db: &Db,
    dir: &std::path::Path,
    write_it: bool,
) -> Result<Adoption, OverlayError> {
    let overlay = super::load(dir)?;
    let mut problems = Vec::new();
    for needs in &overlay.after {
        if !overlay_records::applied(db, needs).await.map_err(db_err)? {
            problems.push(format!(
                "`{}` loads after `{needs}`, which is not applied here",
                overlay.name
            ));
        }
    }
    if !problems.is_empty() {
        return Err(OverlayError::Invalid(problems));
    }
    let mut report = Adoption {
        overlay: overlay.name.clone(),
        ..Adoption::default()
    };
    let mut owned: Vec<(String, String)> = Vec::new();
    let mut created: std::collections::HashSet<(Kind, String)> = Default::default();
    let mut changed: BTreeMap<(Kind, String), Map<String, Value>> = BTreeMap::new();
    // a record this overlay both creates and changes is compared once its changes are in; only
    // those are held whole
    let changes_of: std::collections::HashSet<(Kind, String)> = overlay
        .entries
        .iter()
        .filter(|e| e.change)
        .map(|e| (e.kind, e.key.to_string()))
        .collect();
    let mut held: HashMap<(Kind, String), (Value, Value, Origin)> = HashMap::new();
    for e in &overlay.entries {
        let key = e.key.to_string();
        let fail = |why: &str| format!("{}: {} {}: {why}", e.origin, e.kind, e.key);
        let touched = || overlay_records::touched_by_others(db, e.kind.name(), &key, &overlay.name);
        let Some(stored) = stored_record(db, e.kind, e.key.clone()).await? else {
            problems.push(fail("does not exist in this world"));
            continue;
        };
        if e.change {
            // a change to a record this overlay creates is part of making it
            if let Some((built, _, _)) = held.get_mut(&(e.kind, key.clone())) {
                match merged(e.kind, built, &e.values) {
                    Ok(v) => *built = v,
                    Err(why) => problems.push(fail(&why)),
                }
                continue;
            }
            if created.contains(&(e.kind, key.clone())) {
                continue;
            }
            let after = match merged(e.kind, &stored, &e.values) {
                Ok(v) => v,
                Err(why) => {
                    problems.push(fail(&why));
                    continue;
                }
            };
            if design(e.kind, &after) != design(e.kind, &stored) {
                if touched().await.map_err(db_err)? {
                    report.changed_since += 1;
                } else {
                    problems.push(fail("this change is not made in this world"));
                    continue;
                }
            }
            let fields = changed.entry((e.kind, key)).or_default();
            for k in e.values.keys() {
                fields.insert(
                    k.clone(),
                    after.get(k.as_str()).cloned().unwrap_or(Value::Null),
                );
            }
            continue;
        }
        if let Owner::Overlay(name) = overlay_records::owner(db, e.kind.name(), &key)
            .await
            .map_err(db_err)?
            && name != overlay.name
        {
            report.taken_over += 1;
            continue;
        }
        let mut world = World {
            db,
            stored: HashMap::new(),
            pending: HashMap::new(),
        };
        let built = async {
            let mut v = new_base(&mut world, e).await?;
            if e.kind == Kind::Text {
                v = text_value(&e.values)?;
            } else {
                build::merge(e.kind, &mut v, &e.values)?;
            }
            set_key(e.kind, e.key.clone(), &mut v);
            Ok::<_, String>(v)
        }
        .await;
        let v = match built {
            Ok(v) => v,
            Err(why) => {
                problems.push(fail(&why));
                continue;
            }
        };
        created.insert((e.kind, key.clone()));
        owned.push((e.kind.name().to_owned(), key.clone()));
        if changes_of.contains(&(e.kind, key.clone())) {
            held.insert((e.kind, key), (v, stored, e.origin.clone()));
            continue;
        }
        if design(e.kind, &v) != design(e.kind, &stored) {
            if touched().await.map_err(db_err)? {
                report.changed_since += 1;
            } else {
                owned.pop();
                problems.push(fail("differs from what the overlay makes"));
            }
        }
    }
    for ((kind, key), (built, stored, origin)) in held {
        if design(kind, &built) != design(kind, &stored) {
            if overlay_records::touched_by_others(db, kind.name(), &key, &overlay.name)
                .await
                .map_err(db_err)?
            {
                report.changed_since += 1;
            } else {
                problems.push(format!(
                    "{origin}: {kind} {key}: differs from what the overlay makes"
                ));
            }
        }
    }
    if !problems.is_empty() {
        return Err(OverlayError::Invalid(problems));
    }
    report.owned = owned.len();
    report.changed = changed.len();
    if write_it {
        let changed: Vec<(String, String, String)> = changed
            .into_iter()
            .map(|((kind, key), fields)| {
                (
                    kind.name().to_owned(),
                    key,
                    Value::Object(fields).to_string(),
                )
            })
            .collect();
        overlay_records::record_many(db, &overlay.name, &owned, &changed)
            .await
            .map_err(db_err)?;
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_name_their_numbers() {
        let mut out = Vec::new();
        script_refs(
            "go vent:message 10:teleport 5 19:message 11\n\
             cult:9001\n\
             goodaligned -51 801:testability 126 6:checkability 126 6:minlevel 20 3243:giveitem 2752:text 9002\n",
            0,
            &mut out,
        );
        let got: Vec<(Kind, Key)> = out.iter().map(|(_, k, n)| (*k, n.clone())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Message, Key::Number(10)),
                (Kind::Room, Key::Room(19, 5)),
                (Kind::Message, Key::Number(11)),
                (Kind::Text, Key::Number(9001)),
                (Kind::Message, Key::Number(801)),
                (Kind::Message, Key::Number(3243)),
                (Kind::Item, Key::Number(2752)),
                (Kind::Text, Key::Number(9002)),
            ]
        );
    }
}
