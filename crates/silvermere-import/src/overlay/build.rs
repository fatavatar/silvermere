//! Building records from overlay entries: the blank records new rooms, messages and text start
//! from, the merge of an entry's values onto a record, and the conveniences an author gets (a
//! description as one string, exits by direction, colour markup in text).
//!
//! Records are merged as their serde form, so an entry names fields exactly as the record does
//! and a value of the wrong type fails the same way a record read from a file would. A field
//! the record does not have is an error rather than ignored, so a misspelt field never
//! silently does nothing.

use serde_json::{Map, Value, json};
use silvermere_types::enums::{
    Ability, Alignment, ArmourType, Currency, Element, ExitType, GangHouse, Gender, GroupRole,
    ItemType, Magery, MonsterAttackType, MonsterType, ResistMode, ResistStat, RoomType, ShopType,
    SpellTarget, SpellType, WeaponClass, WeaponType, WornLocation,
};
use silvermere_types::flags::{ActionTargets, RoomFlags, SpellFlags};
use silvermere_types::ids::{
    ActiveMonsterId, ClassNo, ItemNo, MapId, MessageNo, MonsterNo, RaceNo, RoomId, ShopNo, SpellNo,
    TextBlockNo,
};
use silvermere_types::records::{
    Action, Class, Coins, Exit, Item, KnownMonster, Message, MonsterAttack, MonsterItem,
    MonsterSpell, Race, Room, RoomItem, Shop, ShopSlot, Spell, TextBlock,
};
use silvermere_types::sizes::{
    NDIRS, NHIDITEMS, NITEMABILS, NMONATTACKS, NMONITEMS, NMONSPELLS, NPLACED, NRECABILS,
    NROOMITEMS, NROOMMONS, NSHOPITEMS,
};

use super::Kind;

/// The most text one stored part holds: the record's 2022 bytes less its 23-byte header, one
/// byte a character (the game's character set), however many bytes a character takes here.
pub const TEXT_PART_CHARS: usize = 1999;

/// The widest line the game's text screens show whole.
pub const TEXT_WIDTH: usize = 79;

const EMPTY_EXIT: Exit = Exit {
    target: RoomId(0),
    kind: ExitType::Normal,
    par1: 0,
    par2: 0,
    par3: 0,
    par4: 0,
};

const EMPTY_ROOM_ITEM: RoomItem = RoomItem {
    item: ItemNo(0),
    uses: 0,
    count: 0,
};

/// A room with nothing in it: no exits, no name, no monsters.
pub fn blank_room(map: u32, room: u32) -> Room {
    Room {
        key: silvermere_types::ids::RoomKey {
            map: MapId(map),
            room: RoomId(room),
        },
        name: String::new(),
        desc: Default::default(),
        mapfile: String::new(),
        exits: [EMPTY_EXIT; NDIRS],
        monsters: [ActiveMonsterId(0); NROOMMONS],
        room_type: RoomType::Normal,
        shopnum: ShopNo(0),
        owner: String::new(),
        minlevel: 0,
        maxlevel: 0,
        genmonster: MonsterNo(0),
        light: 0,
        ganghouse: GangHouse::None,
        items: [EMPTY_ROOM_ITEM; NROOMITEMS],
        hiditems: [EMPTY_ROOM_ITEM; NHIDITEMS],
        coins: Coins::default(),
        hidcoins: Coins::default(),
        maxmonsters: 0,
        mongroup: 0,
        lastkill: 0,
        flags: RoomFlags::empty(),
        foundmask: 0,
        deathroom: RoomId(0),
        huproom: RoomId(0),
        cmdtext: TextBlockNo(0),
        regendelay: 0,
        areamax: 0,
        areacount: 0,
        ctlroom: RoomId(0),
        specmon: MonsterNo(0),
        placed: [ItemNo(0); NPLACED],
        protmsg: TextBlockNo(0),
        picktrap: 0,
        roomspell: 0,
        searchers: 0,
        nummonsters: 0,
    }
}

/// A known monster with every field zero or empty: nothing alive, no attacks, no drops.
pub fn blank_monster(number: i32) -> KnownMonster {
    KnownMonster {
        number: MonsterNo(number),
        name: String::new(),
        montype: MonsterType::from_code(0),
        level: 0,
        genindex: 0,
        wornitem: ItemNo(0),
        weapon: ItemNo(0),
        dr: 0,
        ac: 0,
        group: 0,
        followpct: 0,
        magicres: 0,
        bsdefense: 0,
        exp: 0,
        maxhp: 0,
        energy: 0,
        hpregen: 0,
        abilities: [(Ability::None, 0); NRECABILS],
        maxalive: 0,
        curalive: 0,
        grouprole: GroupRole::from_code(0),
        maxfollow: 0,
        undead: 0,
        alignment: Alignment::from_code(0),
        regenhrs: 0,
        killed_at: 0,
        arrivemsg: MessageNo(0),
        deathmsg: MessageNo(0),
        items: [MonsterItem {
            item: ItemNo(0),
            uses: 0,
            pct: 0,
        }; NMONITEMS],
        cash: Coins::default(),
        talktext: TextBlockNo(0),
        charmlvl: 0,
        namelist: 0,
        attacks: [MonsterAttack {
            kind: MonsterAttackType::from_code(0),
            accuracy: 0,
            pct: 0,
            min: 0,
            max: 0,
            hitmsg: MessageNo(0),
            defmsg: MessageNo(0),
            missmsg: MessageNo(0),
            energy: 0,
            hitspell: SpellNo(0),
        }; NMONATTACKS],
        randtext: TextBlockNo(0),
        charmres: 0,
        deathspell: SpellNo(0),
        createspell: SpellNo(0),
        spells: [MonsterSpell {
            spell: SpellNo(0),
            pct: 0,
            level: 0,
        }; NMONSPELLS],
        desc: Default::default(),
        gender: Gender::from_code(0),
    }
}

/// An item with every field zero or empty.
pub fn blank_item(number: i32) -> Item {
    Item {
        number: ItemNo(number),
        level: 0,
        limit: 0,
        name: String::new(),
        desc: Default::default(),
        hiddesc: Default::default(),
        active: 0,
        weight: 0,
        item_type: ItemType::from_code(0),
        abilities: [(Ability::None, 0); NITEMABILS],
        charges: 0,
        energy: 0,
        cost: 0,
        classes: [ClassNo(0); 10],
        mindmg: 0,
        maxdmg: 0,
        ac: 0,
        races: [RaceNo(0); 10],
        negates: [SpellNo(0); 10],
        wpntype: WeaponType::from_code(0),
        armtype: ArmourType::from_code(0),
        worn: WornLocation::from_code(0),
        accuracy: 0,
        dr: 0,
        gettable: 0,
        strreq: 0,
        cash: Coins::default(),
        speed: 0,
        hitmsg: MessageNo(0),
        missmsg: MessageNo(0),
        textnum: TextBlockNo(0),
        destroymsg: MessageNo(0),
        nodrop: 0,
        currency: Currency::from_code(0),
        retain: 0,
        robable: 0,
        destroy: 0,
    }
}

/// A spell with every field zero or empty.
pub fn blank_spell(number: i16) -> Spell {
    Spell {
        number: SpellNo(number),
        name: String::new(),
        descs: Default::default(),
        resist: 0,
        levelcap: 0,
        flags: SpellFlags::empty(),
        abilities: [(Ability::None, 0); NRECABILS],
        energy: 0,
        level: 0,
        minval: 0,
        maxval: 0,
        spell_type: SpellType::from_code(0),
        resmode: ResistMode::from_code(0),
        diff: 0,
        durlvl: 0,
        target: SpellTarget::from_code(0),
        duration: 0,
        element: Element::from_code(0),
        resstat: ResistStat::from_code(0),
        magery: Magery::from_code(0),
        msgnum: MessageNo(0),
        mana: 0,
        maxinc: 0,
        maxlvls: 0,
        magelvl: 0,
        mininc: 0,
        minlvls: 0,
        durinc: 0,
        durlvls: 0,
        shortnm: String::new(),
    }
}

/// A shop with nothing in stock.
pub fn blank_shop(number: i32) -> Shop {
    Shop {
        number: ShopNo(number),
        name: String::new(),
        desc: Default::default(),
        shop_type: ShopType::from_code(0),
        minlevel: 0,
        maxlevel: 0,
        markup: 0,
        ownable: 0,
        expandable: 0,
        trainclass: ClassNo(0),
        stock: [ShopSlot {
            item: ItemNo(0),
            maxqty: 0,
            qty: 0,
            regentime: 0,
            regennum: 0,
            regenpct: 0,
        }; NSHOPITEMS],
    }
}

/// A class with every field zero or empty.
pub fn blank_class(number: u16) -> Class {
    Class {
        number: ClassNo(number),
        name: String::new(),
        basehp: 0,
        hplevel: 0,
        exp: 0,
        attmult: 0,
        maxenergy: 0,
        restored: 0,
        abilities: [(Ability::None, 0); NRECABILS],
        magery: Magery::from_code(0),
        magelvl: 0,
        weapon: WeaponClass::from_code(0),
        armour: ArmourType::from_code(0),
        combat: 0,
        titles: TextBlockNo(0),
    }
}

/// A race with every field zero or empty.
pub fn blank_race(number: u16) -> Race {
    Race {
        number: RaceNo(number),
        name: String::new(),
        minint: 0,
        minwil: 0,
        minstr: 0,
        minhea: 0,
        minagi: 0,
        mincha: 0,
        hpbonus: 0,
        maxenergy: 0,
        restored: 0,
        abilities: [(Ability::None, 0); NRECABILS],
        basecp: 0,
        exp: 0,
        maxint: 0,
        maxwil: 0,
        maxstr: 0,
        maxhea: 0,
        maxagi: 0,
        maxcha: 0,
    }
}

/// An action (a social verb) with no texts.
pub fn blank_action(name: &str) -> Action {
    Action {
        name: name.to_owned(),
        texts: Default::default(),
        targets: ActionTargets::empty(),
    }
}

/// A message with three empty lines.
pub fn blank_message(number: i32) -> Message {
    Message {
        number: MessageNo(number),
        msgs: Default::default(),
    }
}

/// Clears what a copy of a template must not inherit: the live counts of a monster, the copies
/// in play of an item. A shop's stock quantities are its initial stock and stay.
pub fn fresh(kind: Kind, v: &mut Value) {
    let Value::Object(m) = v else { return };
    match kind {
        Kind::Monster => {
            m.insert("curalive".into(), json!(0));
            m.insert("killed_at".into(), json!(0));
        }
        Kind::Item => {
            m.insert("active".into(), json!(0));
        }
        _ => {}
    }
}

/// The empty value of one slot of a fixed-size array field, for the slots an entry's list does
/// not reach. `elem` is a slot of the record's own array, for the plain kinds.
pub(super) fn empty_slot(kind: Kind, field: &str, elem: &Value) -> Value {
    match (kind, field) {
        (_, "abilities") => json!(["None", 0]),
        (Kind::Room, "exits") => serde_json::to_value(EMPTY_EXIT).unwrap_or(Value::Null),
        (Kind::Room, "items" | "hiditems") => {
            serde_json::to_value(EMPTY_ROOM_ITEM).unwrap_or(Value::Null)
        }
        (Kind::Monster, "items") => json!({"item": 0, "uses": 0, "pct": 0}),
        (Kind::Monster, "attacks") => json!({
            "kind": "None", "accuracy": 0, "pct": 0, "min": 0, "max": 0, "hitmsg": 0,
            "defmsg": 0, "missmsg": 0, "energy": 0, "hitspell": 0
        }),
        (Kind::Monster, "spells") => json!({"spell": 0, "pct": 0, "level": 0}),
        (Kind::Shop, "stock") => json!({
            "item": 0, "maxqty": 0, "qty": 0, "regentime": 0, "regennum": 0, "regenpct": 0
        }),
        _ => match elem {
            Value::String(_) => json!(""),
            Value::Number(_) => json!(0),
            other => other.clone(),
        },
    }
}

/// The direction index of an exit name: `north`/`n` .. `down`/`d`.
pub fn direction(name: &str) -> Option<usize> {
    const NAMES: [(&str, &str); NDIRS] = [
        ("north", "n"),
        ("south", "s"),
        ("east", "e"),
        ("west", "w"),
        ("northeast", "ne"),
        ("northwest", "nw"),
        ("southeast", "se"),
        ("southwest", "sw"),
        ("up", "u"),
        ("down", "d"),
    ];
    let name = name.to_ascii_lowercase();
    NAMES
        .iter()
        .position(|(long, short)| name == *long || name == *short)
}

/// Fields that hold lines of text and may be written as one string.
pub(super) fn is_line_field(kind: Kind, field: &str) -> bool {
    matches!(
        (kind, field),
        (Kind::Room, "desc")
            | (Kind::Monster, "desc")
            | (Kind::Item, "desc" | "hiddesc")
            | (Kind::Spell, "descs")
            | (Kind::Shop, "desc")
            | (Kind::Message, "msgs")
    )
}

/// One string as lines, without a final empty line from a closing newline.
fn lines(s: &str) -> Value {
    let s = s.strip_suffix('\n').unwrap_or(s);
    Value::Array(s.split('\n').map(|l| json!(l)).collect())
}

/// Merges `over` (an entry's values) onto `base` (a record's serde form). Errors name the
/// field: an unknown field, a list longer than the record's, a line too long.
pub fn merge(kind: Kind, base: &mut Value, over: &Map<String, Value>) -> Result<(), String> {
    let Value::Object(b) = base else {
        return Err("the record is not a table".into());
    };
    for (field, value) in over {
        let Some(slot) = b.get_mut(field) else {
            return Err(format!("`{field}`: no such field"));
        };
        let value = match value {
            Value::String(s) if is_line_field(kind, field) => lines(s),
            other => other.clone(),
        };
        if kind == Kind::Room
            && field == "exits"
            && let Value::Object(by_dir) = &value
        {
            merge_exits(slot, by_dir)?;
            continue;
        }
        merge_value(kind, field, slot, &value).map_err(|e| format!("`{field}`{e}"))?;
    }
    Ok(())
}

fn merge_value(kind: Kind, field: &str, slot: &mut Value, value: &Value) -> Result<(), String> {
    match (slot, value) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                let Some(inner) = b.get_mut(k) else {
                    return Err(format!(".{k}: no such field"));
                };
                merge_value(kind, k, inner, v).map_err(|e| format!(".{k}{e}"))?;
            }
            Ok(())
        }
        (Value::Array(b), Value::Array(o)) => {
            if o.len() > b.len() {
                return Err(format!(
                    ": {} entries, the record holds {}",
                    o.len(),
                    b.len()
                ));
            }
            let first = b.first().cloned().unwrap_or(Value::Null);
            let empty = empty_slot(kind, field, &first);
            for (i, cell) in b.iter_mut().enumerate() {
                let mut next = empty.clone();
                if let Some(v) = o.get(i) {
                    if let (Value::Object(_), Value::Object(_)) = (&next, v) {
                        merge_value(kind, field, &mut next, v).map_err(|e| format!("[{i}]{e}"))?;
                    } else {
                        next = v.clone();
                    }
                }
                *cell = next;
            }
            Ok(())
        }
        (slot, value) => {
            *slot = value.clone();
            Ok(())
        }
    }
}

/// `exits = { north = 12, up = { target = 5, kind = "Door", par1 = 2 } }`: each named
/// direction is set (a number is a plain exit to that room, 0 none); the rest stay.
fn merge_exits(slot: &mut Value, by_dir: &Map<String, Value>) -> Result<(), String> {
    let Value::Array(exits) = slot else {
        return Err("`exits`: the record's exits are not a list".into());
    };
    for (dir, v) in by_dir {
        let Some(i) = direction(dir) else {
            return Err(format!(
                "`exits.{dir}`: not a direction (north .. down, n .. d)"
            ));
        };
        let mut exit = serde_json::to_value(EMPTY_EXIT).unwrap_or(Value::Null);
        match v {
            Value::Number(n) => {
                exit["target"] = Value::Number(n.clone());
            }
            Value::Object(o) => {
                merge_value(Kind::Room, "exit", &mut exit, &Value::Object(o.clone()))
                    .map_err(|e| format!("`exits.{dir}`{e}"))?;
            }
            _ => return Err(format!("`exits.{dir}`: a room number or a table")),
        }
        exits[i] = exit;
    }
    Ok(())
}

/// `{ansi:1;32}` to the escape sequence `ESC[1;32m`; `{{` and `}}` are literal braces.
pub fn markup(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if let Some(t) = tail.strip_prefix("{{") {
            out.push('{');
            rest = t;
        } else if let Some(t) = tail.strip_prefix("}}") {
            out.push('}');
            rest = t;
        } else if let Some(t) = tail.strip_prefix("{ansi:") {
            let Some(end) = t.find('}') else {
                return Err("`{ansi:` without its `}`".into());
            };
            let codes = &t[..end];
            if codes.is_empty() || !codes.chars().all(|c| c.is_ascii_digit() || c == ';') {
                return Err(format!("`{{ansi:{codes}}}`: codes are digits and `;`"));
            }
            out.push_str("\u{1b}[");
            out.push_str(codes);
            out.push('m');
            rest = &t[end + 1..];
        } else {
            return Err(format!(
                "`{}` is not markup (write `{{{{` or `}}}}` for a brace)",
                &tail[..1]
            ));
        }
    }
    out.push_str(rest);
    Ok(out)
}

/// The visible width of a line: escape sequences take no room.
pub fn visible_width(line: &str) -> usize {
    let mut n = 0;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            n += 1;
        }
    }
    n
}

/// The stored parts of a text block: `text` split at line ends into parts of at most
/// [`TEXT_PART_CHARS`], numbered from 0, each carrying `link` (the game reads the first
/// part's). A line longer than a whole part is an error.
pub fn text_parts(number: i32, link: i32, text: &str) -> Result<Vec<TextBlock>, String> {
    let mut parts: Vec<String> = vec![String::new()];
    for line in text.split_inclusive('\n') {
        if line.chars().count() > TEXT_PART_CHARS {
            return Err(format!(
                "a line of {} characters does not fit a part",
                line.chars().count()
            ));
        }
        let cur = parts.last_mut().expect("one part at least");
        if cur.chars().count() + line.chars().count() > TEXT_PART_CHARS {
            parts.push(line.to_string());
        } else {
            cur.push_str(line);
        }
    }
    parts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            Ok(TextBlock {
                part: i16::try_from(i).map_err(|_| "too many parts".to_string())?,
                number: TextBlockNo(number),
                link: TextBlockNo(link),
                text,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_makes_escapes_and_keeps_doubled_braces() {
        assert_eq!(
            markup("{ansi:1;32}word{ansi:0;32} {{x}}").unwrap(),
            "\u{1b}[1;32mword\u{1b}[0;32m {x}"
        );
        assert!(markup("{k}").is_err());
        assert!(markup("{ansi:1;x}").is_err());
    }

    #[test]
    fn width_skips_escapes() {
        assert_eq!(visible_width("\u{1b}[1;32mabc\u{1b}[0m"), 3);
    }

    #[test]
    fn long_text_splits_at_line_ends() {
        let line = format!("{}\n", "x".repeat(99));
        let text = line.repeat(30);
        let parts = text_parts(7, 0, &text).unwrap();
        assert_eq!(parts.len(), 2);
        assert!(
            parts
                .iter()
                .all(|p| p.text.chars().count() <= TEXT_PART_CHARS)
        );
        assert_eq!(
            parts.iter().map(|p| p.text.as_str()).collect::<String>(),
            text
        );
        assert_eq!((parts[1].part, parts[1].number), (1, TextBlockNo(7)));
    }

    #[test]
    fn exits_by_direction_and_lines_from_one_string() {
        let mut room = serde_json::to_value(blank_room(19, 1)).unwrap();
        let over: Map<String, Value> = serde_json::from_value(json!({
            "name": "Vent",
            "desc": "one\ntwo\n",
            "exits": { "north": 2, "d": { "target": 3, "kind": "Door", "par1": 2 } },
        }))
        .unwrap();
        merge(Kind::Room, &mut room, &over).unwrap();
        let room: Room = serde_json::from_value(room).unwrap();
        assert_eq!(room.desc[0], "one");
        assert_eq!(room.desc[1], "two");
        assert_eq!(room.desc[2], "");
        assert_eq!(room.exits[0].target, RoomId(2));
        assert_eq!(room.exits[9].target, RoomId(3));
        assert_eq!(room.exits[9].kind, ExitType::Door);
        assert_eq!(room.exits[9].par1, 2);
    }

    #[test]
    fn unknown_fields_and_long_lists_are_errors() {
        let mut room = serde_json::to_value(blank_room(19, 1)).unwrap();
        let typo: Map<String, Value> = serde_json::from_value(json!({"nmae": "x"})).unwrap();
        assert!(
            merge(Kind::Room, &mut room, &typo)
                .unwrap_err()
                .contains("nmae")
        );
        let long: Map<String, Value> =
            serde_json::from_value(json!({"desc": "1\n2\n3\n4\n5\n6\n7\n8"})).unwrap();
        assert!(
            merge(Kind::Room, &mut room, &long)
                .unwrap_err()
                .contains("8 entries")
        );
    }

    #[test]
    fn a_shorter_list_clears_the_slots_after_it() {
        let mut v = json!({"abilities": [["Damage", 5], ["Defence", 2], ["Magical", 1]]});
        let over: Map<String, Value> =
            serde_json::from_value(json!({"abilities": [["Energy", 9]]})).unwrap();
        merge(Kind::Monster, &mut v, &over).unwrap();
        assert_eq!(
            v["abilities"],
            json!([["Energy", 9], ["None", 0], ["None", 0]])
        );
    }
}
