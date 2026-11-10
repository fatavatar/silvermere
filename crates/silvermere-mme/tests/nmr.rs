//! The MME export over a small world stored through silvermere-db: the columns NMR works out by
//! cross-referencing, in the forms the MudRev export (NMR 1.8.3, mudrev.mdb) holds them.

use silvermere_db::{Db, TestDb};
use silvermere_import::overlay::build::{
    blank_item, blank_message, blank_monster, blank_room, blank_shop, blank_spell,
};
use silvermere_mme::nmr::{Content, Options, export};
use silvermere_mme::{Row, Tables, Value};
use silvermere_types::enums::{ExitType, ItemType, MonsterAttackType, MonsterType, RoomType};
use silvermere_types::ids::{ItemNo, MessageNo, RoomId, ShopNo, TextBlockNo};
use silvermere_types::records::TextBlock;

/// A lair room 1/1 generating monster 1 (group 6, index 1), with a key exit east and a normal
/// exit north to a "Buffer Room"; a shop room 1/3 for shop 7; monster 1 drops item 6 and has
/// a topic "hello" whose block gives item 7 and teaches spell 3.
async fn store(db: &Db) {
    let mut lair = blank_room(1, 1);
    lair.name = "Rat Nest".into();
    lair.room_type = RoomType::from_code(3);
    lair.mongroup = 6;
    (lair.minlevel, lair.maxlevel) = (1, 1);
    lair.maxmonsters = 2;
    lair.exits[0].target = RoomId(2);
    lair.exits[2].target = RoomId(4);
    lair.exits[2].kind = ExitType::from_code(2);
    lair.exits[2].par1 = 5;
    db.rooms().insert(&lair).await.unwrap();
    let mut buffer = blank_room(1, 2);
    buffer.name = "Buffer Room".into();
    db.rooms().insert(&buffer).await.unwrap();
    let mut shop_room = blank_room(1, 3);
    shop_room.name = "General Store".into();
    shop_room.room_type = RoomType::from_code(1);
    shop_room.shopnum = ShopNo(7);
    db.rooms().insert(&shop_room).await.unwrap();

    let mut shop = blank_shop(7);
    shop.name = "General Store".into();
    shop.stock[0].item = ItemNo(5);
    (
        shop.stock[0].maxqty,
        shop.stock[0].regennum,
        shop.stock[0].regenpct,
    ) = (10, 1, 100);
    shop.stock[1].item = ItemNo(6);
    db.shops().insert(&shop).await.unwrap();

    for n in 5..=7 {
        let mut item = blank_item(n);
        item.name = format!("item {n}");
        item.item_type = ItemType::from_code(0);
        db.items().insert(&item).await.unwrap();
    }
    let mut spell = blank_spell(3);
    spell.name = "lesson".into();
    db.spells().insert(&spell).await.unwrap();

    let mut message = blank_message(20);
    message.msgs[0] = "%s bites you for %d damage!".into();
    db.messages().insert(&message).await.unwrap();

    let mut rat = blank_monster(1);
    rat.name = "rat".into();
    rat.montype = MonsterType::from_code(6);
    rat.genindex = 1;
    rat.energy = 1000;
    rat.exp = 10;
    rat.level = 1;
    rat.maxhp = 12;
    rat.talktext = TextBlockNo(10);
    rat.items[0].item = ItemNo(6);
    rat.items[0].pct = 50;
    rat.attacks[0].kind = MonsterAttackType::from_code(1);
    (rat.attacks[0].pct, rat.attacks[0].min, rat.attacks[0].max) = (100, 2, 10);
    rat.attacks[0].energy = 1000;
    rat.attacks[0].hitmsg = MessageNo(20);
    db.monsters().insert(&rat).await.unwrap();
    // no energy: nothing to simulate
    let mut statue = blank_monster(2);
    statue.name = "statue".into();
    statue.montype = MonsterType::from_code(39);
    db.monsters().insert(&statue).await.unwrap();

    for (number, link, text) in [
        (10, 0, "hello:11\n"),
        (11, 12, "Hello yourself.\n"),
        (12, 0, "giveitem 7\nlearnspell 3\n"),
    ] {
        let block = TextBlock {
            part: 0,
            number: TextBlockNo(number),
            link: TextBlockNo(link),
            text: text.into(),
        };
        db.text_blocks().insert(&block).await.unwrap();
    }
}

/// The content as a caller loads it: every record the export reads, from the repositories.
async fn load(db: &Db) -> Content {
    Content {
        rooms: db.rooms().all().await.unwrap(),
        monsters: db.monsters().all().await.unwrap(),
        items: db.items().all().await.unwrap(),
        spells: db.spells().all().await.unwrap(),
        shops: db.shops().all().await.unwrap(),
        classes: db.classes().all().await.unwrap(),
        races: db.races().all().await.unwrap(),
        messages: db.messages().all().await.unwrap(),
        text_blocks: db.text_blocks().all().await.unwrap(),
    }
}

fn row<'a>(rows: &'a [Row], column: &str, key: i32) -> &'a Row {
    rows.iter()
        .find(|r| r.get(column) == Some(&Value::Long(key)))
        .unwrap_or_else(|| panic!("no row with {column} {key}"))
}

fn text<'a>(row: &'a Row, column: &str) -> &'a str {
    match row.get(column) {
        Some(Value::Text(t)) => t.as_str(),
        other => panic!("{column} is {other:?}"),
    }
}

async fn exported() -> Tables {
    let t = TestDb::from_env().await.expect("test database");
    store(t.db()).await;
    let content = load(t.db()).await;
    t.close().await;
    let options = Options {
        date: "9/15/2026".into(),
        time: "1:50:47 PM".into(),
        ..Options::default()
    };
    export(&content, &options)
}

// mudrev.mdb: lair rooms read "(Max 5): 149,150,[26-17-18-5]", an unset delay is 5, a key exit
// "1/4 (Key: 5)", no exit "0", nothing placed "", and "Buffer Room"s are not written
#[tokio::test]
async fn rooms_describe_lairs_and_exits() {
    let tables = exported().await;
    assert_eq!(tables.rooms.len(), 2, "the buffer room is left out");
    let lair = tables
        .rooms
        .iter()
        .find(|r| r.get("Room Number") == Some(&Value::Long(1)))
        .unwrap();
    assert_eq!(text(lair, "Lair"), "(Max 2): 1,[6-1-1-2]");
    assert_eq!(lair.get("Delay"), Some(&Value::Long(5)));
    assert_eq!(text(lair, "N"), "1/2");
    assert_eq!(text(lair, "E"), "1/4 (Key: 5)");
    assert_eq!(text(lair, "S"), "0");
    assert_eq!(text(lair, "Placed"), "\0");
}

// mudrev.mdb: Summoned By "[6-0-5][1]Group(lair): 1/552", Obtained From "Shop #68, Monster
// #863(10%)", sell-only "Shop(sell) #N" not counting as in the game, key items under
// References, and an NPC's topic gifts as "NPC #N"
#[tokio::test]
async fn cross_references_name_their_sources() {
    let tables = exported().await;
    let rat = row(&tables.monsters, "Number", 1);
    assert_eq!(text(rat, "Summoned By"), "[6-1-1][2]Group(lair): 1/1");
    assert_eq!(rat.get("In Game"), Some(&Value::Long(1)));
    assert_eq!(text(rat, "AttName-0"), "bites you");
    assert_eq!(rat.get("AvgLairExp"), Some(&Value::Double(20.0)));

    let key = row(&tables.items, "Number", 5);
    assert_eq!(text(key, "Obtained From"), "Shop #7");
    assert_eq!(text(key, "References"), "Room 1/1");
    assert_eq!(key.get("In Game"), Some(&Value::Long(1)));
    let sold = row(&tables.items, "Number", 6);
    assert_eq!(
        text(sold, "Obtained From"),
        "Shop(sell) #7, Monster #1(50%)"
    );
    assert_eq!(sold.get("In Game"), Some(&Value::Long(1)));
    let gift = row(&tables.items, "Number", 7);
    assert_eq!(text(gift, "Obtained From"), "NPC #1");

    let spell = row(&tables.spells, "Number", 3);
    assert_eq!(text(spell, "Learned From"), "NPC #1");
    assert_eq!(text(spell, "Classes"), "(*)");
    assert_eq!(spell.get("Learnable"), Some(&Value::Long(1)));

    let shop = row(&tables.shops, "Number", 7);
    assert_eq!(text(shop, "Assigned To"), "Room 1/3");

    let topics = row(&tables.tbinfo, "Number", 10);
    assert_eq!(text(topics, "Called From"), "Monster #1");
    assert_eq!(text(topics, "Action"), "hello:11\n");
    let gives = row(&tables.tbinfo, "Number", 12);
    assert_eq!(text(gives, "Called From"), "Textblock #11");
    assert_eq!(text(gives, "Action"), "giveitem 7\nlearnspell 3\n");
    assert_eq!(
        row(&tables.tbinfo, "Number", 11).get("LinkTo"),
        Some(&Value::Long(12))
    );
}

// mudrev.mdb: a monster with nothing to simulate shows the averages of the monster before it
// (monsters 13 and 14 both 8.8 physical after monster 12's 8.8)
#[tokio::test]
async fn the_simulation_is_repeatable_and_carries_over() {
    let (a, b) = (exported().await, exported().await);
    let (rat, again) = (row(&a.monsters, "Number", 1), row(&b.monsters, "Number", 1));
    assert_eq!(rat.get("AvgDmg"), again.get("AvgDmg"));
    let Some(&Value::Double(avg)) = rat.get("AvgDmg") else {
        panic!("no average")
    };
    // 2 to 10 every round, hitting 99%
    assert!((5.6..6.3).contains(&avg), "{avg}");
    assert_eq!(rat.get("AttTrue%-0"), Some(&Value::Double(100.0)));
    let statue = row(&a.monsters, "Number", 2);
    assert_eq!(statue.get("AvgDmg"), Some(&Value::Double(0.0)));
    assert_eq!(statue.get("AvgDmgPhys"), rat.get("AvgDmgPhys"));
    assert_eq!(text(&a.info[0], "Custom"), "My BBS Name");
}
