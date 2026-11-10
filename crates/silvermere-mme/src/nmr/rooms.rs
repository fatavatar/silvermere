//! The Rooms table: each room's exits as text, its lair, placed items and regeneration delay.

use std::collections::HashMap;

use silvermere_types::records::Room;

use super::world::{World, ansi};
use super::{Options, RoomRange};
use crate::table::{Row, Value};

/// The rooms stock MajorMUD keeps out of play (sysop, module-test and placeholder rooms), which
/// NMR leaves out by default.
pub const DEFAULT_EXCLUDED_ROOMS: [RoomRange; 20] = [
    RoomRange::new(1, 288, 288),    // fake Silver River room
    RoomRange::new(1, 289, 289),    // Halls of Training, entrance
    RoomRange::new(1, 2306, 2306),  // small house, attic
    RoomRange::new(4, 1, 1),        // Silvermere property office
    RoomRange::new(1, 164, 164),    // sysop support chamber
    RoomRange::new(1, 238, 238),    // sysop cheat room
    RoomRange::new(1, 3347, 3347),  // sysop support chamber
    RoomRange::new(1, 2779, 2782),  // sysop support module test rooms
    RoomRange::new(1, 3343, 3346),  // sysop support empty rooms
    RoomRange::new(1, 3437, 3437),  // sysop support rubbish room
    RoomRange::new(17, 2867, 2867), // sysop training room
    RoomRange::new(2, 2963, 2963),  // module test rooms, one per module
    RoomRange::new(3, 788, 788),
    RoomRange::new(8, 1407, 1407),
    RoomRange::new(6, 3275, 3275),
    RoomRange::new(9, 1432, 1432),
    RoomRange::new(12, 2258, 2258),
    RoomRange::new(16, 2673, 2673),
    RoomRange::new(15, 2055, 2055),
    RoomRange::new(17, 2839, 2839),
];

/// Which rooms the export leaves out.
pub(super) struct Exclusions {
    defaults: bool,
    extra: Vec<RoomRange>,
    hide_only: bool,
}

impl Exclusions {
    pub(super) fn new(options: &Options) -> Self {
        let legit = options.kind == super::ExportKind::Legit;
        Self {
            defaults: legit || options.exclude_default_rooms,
            extra: if legit {
                Vec::new()
            } else {
                options.excluded_rooms.clone()
            },
            hide_only: options.hide_excluded_rooms,
        }
    }

    fn listed(&self, room: &Room, defaults_only: bool) -> bool {
        let (map, number) = (room.key.map.0, room.key.room.0);
        let defaults = if self.defaults {
            &DEFAULT_EXCLUDED_ROOMS[..]
        } else {
            &[]
        };
        let extra = if defaults_only { &[][..] } else { &self.extra };
        defaults
            .iter()
            .chain(extra)
            .any(|r| r.map == map && (r.from..=r.to).contains(&number))
    }

    /// Whether the room is out of the world entirely: not written and nothing in it counts as
    /// in the game. "Hide only" keeps the listed rooms in the game (just not mapped), except the
    /// default ones.
    pub(super) fn left_out(&self, room: &Room) -> bool {
        self.listed(room, self.hide_only)
    }

    /// Whether the room is written by name only ("hide only").
    fn hidden(&self, room: &Room) -> bool {
        self.listed(room, false)
    }
}

/// The monsters of each generation group and index, at most 15 in number order: the ones a lair
/// of that group and index can generate.
pub(super) struct MonsterIndex(HashMap<(i32, i32), Vec<i32>>);

impl MonsterIndex {
    pub(super) fn new(world: &World) -> Self {
        let mut index: HashMap<(i32, i32), Vec<i32>> = HashMap::new();
        for m in &world.monsters {
            // NMR treats a name starting "sdf" as a deleted monster
            if m.name.starts_with("sdf") || m.genindex < 0 {
                continue;
            }
            let slot = index
                .entry((i32::from(m.montype.code()), i32::from(m.genindex)))
                .or_default();
            if slot.len() < 15 {
                slot.push(m.number.0);
            }
        }
        Self(index)
    }

    pub(super) fn get(&self, group: i32, index: i32) -> &[i32] {
        self.0.get(&(group, index)).map_or(&[], Vec::as_slice)
    }
}

/// The ten exit columns, in direction order.
pub(super) const EXIT_COLUMNS: [&str; 10] = ["N", "S", "E", "W", "NE", "NW", "SE", "SW", "U", "D"];

/// The rows of the Rooms table.
pub(super) fn rows(
    world: &World,
    mobs: &MonsterIndex,
    out: &Exclusions,
    options: &Options,
) -> Vec<Row> {
    if options.kind != super::ExportKind::Legit && options.no_rooms {
        return Vec::new();
    }
    world
        .rooms
        .iter()
        // the realm's unused room numbers are "Buffer Room" placeholders
        .filter(|r| !r.name.to_uppercase().starts_with("BUFFER ROOM"))
        .filter(|r| !out.left_out(r))
        .map(|r| row(world, mobs, out, r))
        .collect()
}

fn row(world: &World, mobs: &MonsterIndex, out: &Exclusions, r: &Room) -> Row {
    let mut row = Row::new();
    let (map, number) = (r.key.map.0, r.key.room.0);
    row.insert("Map Number".into(), Value::Long(map as i32));
    row.insert("Room Number".into(), Value::Long(number as i32));
    row.insert("Name".into(), ansi(&r.name).into());
    if out.hidden(r) {
        for col in ["Shop", "Light", "Spell", "NPC", "CMD", "Delay"] {
            row.insert(col.into(), Value::Long(0));
        }
        row.insert("Placed".into(), "\0".into());
        for col in EXIT_COLUMNS.iter().chain(&["Lair"]) {
            row.insert((*col).into(), "\0".into());
        }
        return row;
    }
    let shop = if r.room_type.code() == 1 {
        r.shopnum.0.max(0)
    } else {
        0
    };
    row.insert("Shop".into(), Value::Long(shop));
    row.insert("Light".into(), Value::Long(r.light.into()));
    row.insert("Spell".into(), Value::Long(r.roomspell));
    row.insert("NPC".into(), Value::Long(r.specmon.0));
    row.insert("CMD".into(), Value::Long(r.cmdtext.0));
    let delay = if r.regendelay == 0 { 5 } else { r.regendelay };
    row.insert("Delay".into(), Value::Long(delay.into()));
    let placed: String = r
        .placed
        .iter()
        .filter(|p| p.0 > 0)
        .map(|p| format!("{},", p.0))
        .collect();
    row.insert("Placed".into(), format!("{placed}\0").into());
    for (col, text) in EXIT_COLUMNS.iter().zip(exits(world, r)) {
        row.insert((*col).into(), text.map_or(Value::Null, Value::from));
    }
    row.insert("Lair".into(), lair(mobs, r).into());
    row
}

/// The Lair column: the monsters a lair can generate, "(Max 3): 1,2,[6-0-5-3]" (group, lowest
/// and highest index, most at once), or "\0" for a room that is not a lair.
fn lair(mobs: &MonsterIndex, r: &Room) -> String {
    let (group, low, high) = (
        i32::from(r.mongroup),
        i32::from(r.minlevel),
        i32::from(r.maxlevel),
    );
    let max = r.maxmonsters;
    if !(max > 0 && r.room_type.code() == 3 && low <= high && high > 0) {
        return "\0".into();
    }
    let list: String = (low..=high)
        .flat_map(|i| mobs.get(group, i))
        .map(|n| format!("{n},"))
        .collect();
    if list.is_empty() {
        "\0".into()
    } else {
        format!("(Max {max}): {list}[{group}-{low}-{high}-{max}]")
    }
}

/// The exit columns as NMR describes each exit; `None` for an exit type it does not describe.
///
/// Remote actions that open a multi-action hidden exit elsewhere are numbered by the order
/// they must be done in, and their texts are then moved onto this room's remote-action exits
/// in that order.
fn exits(world: &World, r: &Room) -> [Option<String>; 10] {
    let (map, number) = (r.key.map.0, r.key.room.0);
    let mut out: [Option<String>; 10] = Default::default();
    // per exit: (action number, target room, text) of the remote actions that are numbered
    let mut actions: Vec<(usize, usize, u32, String)> = Vec::new();
    let mut open = [false; 10];
    for (x, e) in r.exits.iter().enumerate() {
        if e.target.0 == 0 {
            out[x] = Some("0".into());
            continue;
        }
        let to = format!("{map}/{}", e.target.0);
        let (p1, p2, p3, p4) = (e.par1 as i32, i32::from(e.par2), e.par3, e.par4);
        let text = match e.kind.code() {
            0 | 1 | 5 | 19 | 21 => to,
            2 => {
                let picks = if p3 == 0 {
                    ")".to_string()
                } else {
                    format!(" [or {} picklocks])", picks(p3))
                };
                format!("{to} (Key: {p1}{picks}")
            }
            3 => format!("{to} (Item: {p1})"),
            4 => format!("{to} (Toll: {p1})"),
            6 => {
                let how = match p1 {
                    0 => {
                        out[x] = Some("0".into());
                        continue;
                    }
                    1 | 3 => " (Hidden/Passable)".to_string(),
                    2 | 4 => " (Hidden/Searchable)".to_string(),
                    _ if p2 < 0 => format!(" (Hidden/Needs {} Actions, any order)", -p2),
                    _ if p2 > 0 => format!(" (Hidden/Needs {p2} Actions, specific order)"),
                    _ => " (Hidden/Unknown)".to_string(),
                };
                format!("{to}{how}")
            }
            7 | 11 => {
                let force = if p2 == 0 {
                    ")".to_string()
                } else if p4 <= 0 {
                    format!(" [{} picklocks/strength])", picks(p2))
                } else {
                    format!(" [or {} picklocks/strength])", picks(p2))
                };
                if p4 <= 0 {
                    format!("{to} (Door{force}")
                } else {
                    format!("{to} (Key: {p4}{force}")
                }
            }
            8 => format!("{}/{}", e.par1 as i32, e.target.0),
            9 => format!("{to} (Trap, {p1} damage)"),
            10 => format!("{to} (Text: {})", world.message(p1, 0)),
            12 => {
                let dir = direction(p2 % 10);
                let mut text = if e.target.0 == number {
                    format!("[on the {dir} exit of this room]: ")
                } else {
                    format!("[on the {dir} exit of room {to}]: ")
                };
                text.push_str(&world.message(p1, 0));
                if p4 != 0 {
                    text.push_str(&format!(" (Item: {p4})"));
                }
                let actions_needed = if (10..100).contains(&p2) {
                    hidden_actions(world, r, e.target.0, (p2 % 10) as usize)
                } else {
                    -1
                };
                if actions_needed > 0 && actions_needed < 10 {
                    // a step past the exit's action count stops NMR's whole export; the port
                    // files it as step 0, which no exit takes
                    let order = (actions_needed - p2 / 10 + 1).max(0) as usize;
                    actions.push((x, order, e.target.0, format!("Action#{order} {text}")));
                    open[x] = true;
                    continue;
                }
                format!("Action {text}")
            }
            13 => format!("{to} (Class: {p1} OK, {p2} NO)"),
            14 => format!("{to} (Race: {p1} OK, {p2} NO)"),
            15 => format!("{to} (Level: {p1} to {p2})"),
            16 => format!("{to} (Timed: {p2}*5 minutes)"),
            17 => format!("{to} (Ticket/Item: {p1})"),
            18 => format!("{to} (Max Users: {p1})"),
            20 => format!("{to} (Alignment: {} to {})", alignment(p1), alignment(p2)),
            22 => format!("{to} (Cast: pre-{p1}, post-{p2})"),
            23 => format!("{to} (Ability: {p1} w/value {p2} to {p3})"),
            24 => format!("{to} (Spell Trap: {p1})"),
            _ => continue,
        };
        out[x] = Some(text);
    }
    for (x, text) in order_actions(&actions, open) {
        out[x] = Some(text);
    }
    out
}

/// Moves numbered remote-action texts onto the exits that held them: for every numbered action
/// found, the texts of actions 1 to 9 on the same target room fill the open exits in order.
/// An exit left open keeps the word "open", as NMR writes it.
fn order_actions(
    actions: &[(usize, usize, u32, String)],
    open: [bool; 10],
) -> Vec<(usize, String)> {
    let mut slots: [Option<String>; 10] =
        std::array::from_fn(|x| open[x].then(|| "open".to_string()));
    let mut is_open = open;
    let at = |exit: usize, order: usize| {
        actions
            .iter()
            .find(|(x, o, _, _)| *x == exit && *o == order)
    };
    for x in 0..10 {
        for y in 0..10 {
            let Some((_, _, target, _)) = at(x, y) else {
                continue;
            };
            for z in 1..10 {
                'exits: for x2 in 0..10 {
                    if let Some((_, _, t2, text)) = at(x2, z)
                        && t2 == target
                    {
                        for y2 in 0..10 {
                            if is_open[y2] {
                                slots[y2] = Some(text.clone());
                                is_open[y2] = false;
                                break 'exits;
                            }
                        }
                    }
                }
            }
        }
    }
    slots
        .into_iter()
        .enumerate()
        .filter_map(|(x, s)| s.map(|s| (x, s)))
        .collect()
}

/// The number of actions a hidden exit needs, -1 when the exit is not a hidden one (or needs
/// ten or more) and 0 when the room does not exist.
fn hidden_actions(world: &World, r: &Room, target: u32, exit: usize) -> i32 {
    let room = if target == r.key.room.0 {
        Some(r)
    } else {
        world.room(i64::from(r.key.map.0), i64::from(target))
    };
    let Some(room) = room else {
        return 0;
    };
    let e = &room.exits[exit];
    let needed = i32::from(e.par2).abs();
    if e.kind.code() == 6 && needed <= 9 {
        needed
    } else {
        -1
    }
}

/// A lock's pick difficulty as NMR shows it: `|n - 1|` for a negative value, "any" otherwise.
fn picks(n: i32) -> String {
    if n < 0 {
        (n - 1).abs().to_string()
    } else {
        "any".into()
    }
}

fn direction(n: i32) -> &'static str {
    usize::try_from(n)
        .ok()
        .and_then(|i| EXIT_COLUMNS.get(i))
        .copied()
        .unwrap_or("")
}

/// The alignment band an evil-point value falls in.
fn alignment(evil: i32) -> &'static str {
    match evil {
        ..=-201 => "Saint",
        -200..=-51 => "Good",
        -50..=29 => "Neutral",
        30..=39 => "Seedy",
        40..=79 => "Outlaw",
        80..=119 => "Criminal",
        120..=209 => "Villain",
        210.. => "Fiend",
    }
}
