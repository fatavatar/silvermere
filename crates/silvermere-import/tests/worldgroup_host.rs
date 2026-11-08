//! A Worldgroup 3.x realm's own host files: the BBS accounts (`WGSUSR2.DAT`) with their
//! passwords and the keys they held (`WGSKEY2.DAT`, the key names of `WCCMMUD.MSG` and
//! `WCCMMUD.INI`), and the module's records in the generic database (`WGSGEN2.DAT`).
//!
//! The realm is the Worldgroup twin of `live_dat.rs`'s (Mira's), with the three host files
//! written beside it as variable-length Btrieve files and the two option files as text.

use std::path::Path;

use silvermere_auth::AccountFlags;
use silvermere_auth::password::verify;
use silvermere_db::{Db, TestDb};
use silvermere_import::decode::{EvilrecRecord, PermrecRecord, Record, Text};
use silvermere_import::importer::{ImportOptions, Importer};
use silvermere_types::records::LayoutSet;

use crate::live_dat::stock;
use crate::support::btrieve_v5::{template_for, write_v5_variable};
use crate::worldgroup::{dos_realm, worldgroup_realm};

const WG: LayoutSet = LayoutSet::Worldgroup;

/// A variable-length file of `records` whose fixed part is `fixed` bytes.
fn write_host_file(dir: &Path, name: &str, fixed: usize, records: &[Vec<u8>]) {
    let tpl = std::fs::read(stock().join("WCCTEXT.VIR")).unwrap();
    std::fs::write(
        dir.join(name),
        write_v5_variable(&template_for(&tpl, fixed), records),
    )
    .unwrap();
}

/// A host account: user-id, password, class, in a 304-byte record.
fn account(userid: &str, password: &str, class: &str) -> Vec<u8> {
    let mut r = vec![0u8; 304];
    r[..userid.len()].copy_from_slice(userid.as_bytes());
    r[30..30 + password.len()].copy_from_slice(password.as_bytes());
    r[240..240 + class.len()].copy_from_slice(class.as_bytes());
    r[300] = 1; // the variable part is never all zero
    r
}

/// A key record: the name, then the keys from +31.
fn keys(name: &str, keys: &str) -> Vec<u8> {
    let mut r = vec![0u8; 31];
    r[..name.len()].copy_from_slice(name.as_bytes());
    r.extend_from_slice(keys.as_bytes());
    r.push(0);
    r
}

fn profile(userid: &str) -> Vec<u8> {
    let mut p = PermrecRecord::decode_as(&[0; 208], WG).unwrap();
    p.userid = Text(userid.into());
    p.modnam = Text("WCC PERMANENT USER".into());
    p.statfmt = Text("[HP=%h]: ".into());
    p.firstname = Text("Mira".into());
    p.lastname = Text("Vale".into());
    p.expbil = 1;
    p.exprem = 306_753_831;
    // 2026-09-15
    p.deathdate = ((2026 - 1980) << 9) | (9 << 5) | 15;
    p.encode_as(WG)
}

fn evil(userid: &str, points: i16) -> Vec<u8> {
    let mut e = EvilrecRecord::decode_as(&[0; 58], WG).unwrap();
    e.userid = Text(userid.into());
    e.modnam = Text("WCC MAJOR MUD EVIL".into());
    e.evil = points;
    e.encode_as(WG)
}

/// A record of another module in the generic database: not read.
fn paging(userid: &str) -> Vec<u8> {
    let mut r = vec![0u8; 80];
    r[..userid.len()].copy_from_slice(userid.as_bytes());
    r[30..36].copy_from_slice(b"Paging");
    r[70] = 1;
    r
}

/// The Worldgroup twin of Mira's realm with its host files: Mira and Moe play (class USER,
/// which holds NORMAL), Moe also holds the gossip key; Sysop is in class SYSOP; Test is a
/// DEMO account with no password; Ghost has kept records and no account.
fn host_realm(name: &str) -> std::path::PathBuf {
    let dos = dos_realm(&format!("{name}-dos"));
    let d = worldgroup_realm(&dos, name);
    write_host_file(
        &d,
        "WGSUSR2.DAT",
        30,
        &[
            account("Mira", "Dragonfly", "USER"),
            account("Sysop", "letmein1", "SYSOP"),
            account("Moe", "moe", "USER"),
            account("Test", "", "DEMO"),
        ],
    );
    write_host_file(
        &d,
        "WGSKEY2.DAT",
        31,
        &[
            keys("&USER", "DEMO NORMAL"),
            keys("&DEMO", "DEMO"),
            keys("Moe", "gidiot"),
        ],
    );
    write_host_file(
        &d,
        "WGSGEN2.DAT",
        55,
        &[
            profile("Mira"),
            evil("Mira", -42),
            paging("Mira"),
            profile("Ghost"),
        ],
    );
    std::fs::write(
        d.join("WCCMMUD.MSG"),
        "SYSOPKEY {NORMAL} S 15 Key Required to issue SYSOP commands\r\n\
         GAMOPKEY {NORMAL} S 15 Key Required to issue GAMEOP commands\r\n\
         LOANKEY {NORMAL} S 15 Key Required to get loans from the banks\r\n\
         GOSSKEY {gidiot} S 15 Key which will disable gossip ability\r\n\
         AUCTKEY {aidiot} S 15 Key which will disable auction ability\r\n",
    )
    .unwrap();
    std::fs::write(
        d.join("WCCMMUD.INI"),
        "CODEC_OF_TWINKS=78\r\nSYS_CHANGE_KEY=SYSOP",
    )
    .unwrap();
    d
}

fn live(d: &Path) -> ImportOptions {
    ImportOptions {
        no_patches: true,
        ..ImportOptions::live(d)
    }
}

async fn roles(db: &Db, name: &str) -> (Vec<String>, u32, String) {
    let a = db
        .accounts()
        .by_username(name)
        .await
        .unwrap()
        .unwrap()
        .account;
    let mut roles = a.roles;
    roles.sort();
    (roles, a.flags, a.password_hash)
}

#[tokio::test]
async fn a_worldgroup_realm_brings_its_accounts_profiles_and_evil_points() {
    let d = host_realm("wg-host");
    let tdb = TestDb::sqlite_temp().await.unwrap();
    let db: &Db = &tdb;

    let report = Importer::new(live(&d), Some(db)).run().await.unwrap();
    let line = |table: &str| {
        report
            .counts
            .iter()
            .find(|c| c.table == table)
            .map(|c| c.line(false))
            .unwrap()
    };
    assert_eq!(
        line("accounts"),
        "accounts [WGSUSR2.DAT]: 4 records read, 4 written, 0 skipped"
    );
    assert_eq!(
        line("character_profiles"),
        "character_profiles [WGSGEN2.DAT]: 2 records read, 1 written, 1 skipped (no account \
         for the userid)"
    );
    assert_eq!(
        line("evil_points"),
        "evil_points [WGSGEN2.DAT]: 1 records read, 1 written, 0 skipped"
    );
    let notes: Vec<String> = report.uncarried.iter().map(|n| n.to_string()).collect();
    assert!(
        notes.contains(
            &"not carried [WGSGEN2.DAT] record 4 (userid Ghost): Ghost has no account and no \
              character"
                .to_string()
        ),
        "{notes:#?}"
    );
    assert!(
        notes.contains(
            &"not carried [WGSUSR2.DAT] record 4 (userid Test): no usable password: set one \
              with `silvermere account passwd`"
                .to_string()
        ),
        "{notes:#?}"
    );
    // only Test has no password
    assert_eq!(report.accounts_without_password, Some(1));

    // NORMAL, which class USER holds, is the sysop and game-operator key; the codec at 78
    // gives a sysop DEBUG; class SYSOP holds every key
    // the stored password is hashed in the form the host compared (any case, 9 characters)
    // and marked, so the login compares the first 9 characters typed in any case
    let bbs = AccountFlags::BBS_PASSWORD.bits();
    let (r, flags, hash) = roles(db, "Mira").await;
    assert_eq!(r, ["DEBUG", "gameop", "player", "sysop"]);
    assert_eq!(flags, bbs);
    assert!(verify("dragonfly", &hash));
    assert!(verify(
        &silvermere_auth::password::bbs_form("DRAGONFLYING"),
        &hash
    ));
    let (r, flags, hash) = roles(db, "Sysop").await;
    assert_eq!(r, ["DEBUG", "gameop", "player", "sysop"]);
    assert_eq!(flags, bbs, "the master flag takes nothing away");
    assert!(verify("letmein1", &hash));
    let (_, flags, _) = roles(db, "Moe").await;
    assert_eq!(flags, AccountFlags::NO_GOSSIP.bits() | bbs);
    let (r, flags, hash) = roles(db, "Test").await;
    assert_eq!(r, ["player"]);
    assert_eq!(flags, AccountFlags::NO_LOAN.bits(), "no password, no mark");
    assert!(hash.is_empty());

    // Mira's character is under the account she logged in with
    let mira = db.accounts().by_username("Mira").await.unwrap().unwrap().id;
    assert!(db.characters().by_account(mira).await.unwrap().is_some());
    let p = db.profiles().get(mira).await.unwrap().unwrap();
    assert_eq!(
        p.statfmt, "[HP=%h]: ",
        "the template keeps its trailing space"
    );
    assert_eq!((p.expbil, p.exprem), (1, 306_753_831));
    assert_eq!(
        (p.firstname.as_str(), p.lastname.as_str()),
        ("Mira", "Vale")
    );
    assert_eq!(
        p.deathdate,
        jiff::civil::date(2026, 9, 15)
            .to_zoned(jiff::tz::TimeZone::UTC)
            .unwrap()
            .timestamp()
            .as_second()
    );
    assert_eq!(db.evil_points().get(mira).await.unwrap().unwrap().evil, -42);

    // A second import keeps an account that has a password (a player may have changed it)
    db.accounts()
        .update(
            mira,
            &silvermere_db::repo::Account {
                password_hash: silvermere_auth::password::hash("changed"),
                ..db.accounts().get(mira).await.unwrap().unwrap().account
            },
        )
        .await
        .unwrap();
    let report = Importer::new(live(&d), Some(db)).run().await.unwrap();
    let notes: Vec<String> = report.uncarried.iter().map(|n| n.to_string()).collect();
    assert!(
        notes.contains(
            &"not carried [WGSUSR2.DAT] record 1 (userid Mira): the account already has a \
              password, kept (--reset-state rewrites it)"
                .to_string()
        ),
        "{notes:#?}"
    );
    let (_, _, hash) = roles(db, "Mira").await;
    assert!(verify("changed", &hash));
    // --reset-state writes it from the realm again
    let opts = ImportOptions {
        reset_state: true,
        ..live(&d)
    };
    Importer::new(opts, Some(db)).run().await.unwrap();
    let (_, _, hash) = roles(db, "Mira").await;
    assert!(verify("dragonfly", &hash));
}

// A dry run reads and counts the host files, and a DOS set or a copy of the module's files
// alone has none to read.
#[tokio::test]
async fn a_dry_run_counts_the_host_files_and_a_realm_without_them_has_no_lines() {
    let d = host_realm("wg-host-dry");
    let opts = ImportOptions {
        dry_run: true,
        ..live(&d)
    };
    let report = Importer::new(opts, None).run().await.unwrap();
    assert_eq!(report.count("WGSUSR2.DAT").map(|c| c.read), Some(4));
    // a dry run has no store: Ghost has no account, Mira has one from this import
    let profiles = report
        .counts
        .iter()
        .find(|c| c.table == "character_profiles")
        .unwrap();
    assert_eq!((profiles.read, profiles.written), (2, 1));

    let dos = dos_realm("wg-host-none-dos");
    let wg = worldgroup_realm(&dos, "wg-host-none");
    for dir in [&dos, &wg] {
        let opts = ImportOptions {
            dry_run: true,
            ..live(dir)
        };
        let report = Importer::new(opts, None).run().await.unwrap();
        assert!(
            report.counts.iter().all(|c| c.table != "accounts"),
            "{}",
            dir.display()
        );
    }
}
