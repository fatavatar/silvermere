//! A Worldgroup 3.x realm's own host files: the players' BBS accounts and the module's
//! records in the host's generic database, read with `--live` from a Worldgroup set.
//!
//! The module kept part of a player outside its data files, in the host's:
//!
//! - `WGSUSR2.DAT`, the host's accounts (`usracc`): the user-id, the password (stored as
//!   typed) and the class. The import creates an account per record with the password
//!   hashed, so players log in as before.
//! - `WGSKEY2.DAT`, the key lists: one record per user-id, and one per class named with a
//!   leading `&`. The module's sysop, game-operator, gossip, auction and loan tests were
//!   `haskey` on keys named in `WCCMMUD.MSG`; the import evaluates them once per account and
//!   stores the result as the account's roles and flags (`docs/auth.md`).
//! - `WGSGEN2.DAT`, the generic database: the module's `permrec` ("WCC PERMANENT USER",
//!   settings, statline template and kept experience) and `evilrec` ("WCC MAJOR MUD EVIL",
//!   evil points kept over a death), keyed by user-id and module name. Records of other
//!   modules are not read.
//!
//! `usracc` and the key record are the host's, not the module's: only the members read here
//! are known, placed from a realm's files (user-id at +0, the 10-byte password at +30, the
//! 16-byte current class at +240 of a 304-byte record; a key record's name at +0 and its
//! keys, space separated, from +31).
//!
//! How `haskey` decides (WGSERVER.EXE 3.x, `haskey` at 0x424AC0, the test of one key at
//! 0x425010): an empty lock is held; a lock is keys joined by `&` and `|`, taken left to
//! right; a user with the master flag holds every key; otherwise a key is held when it is
//! in the user's own list or the class's. The master flag is taken to be the `MASTER` key or
//! the `SYSOP` class [I]: a realm's sysop account, in class `SYSOP` with no keys of its own
//! or its class's, issued the module's sysop commands.

use std::collections::HashMap;
use std::path::Path;

use silvermere_auth::AccountFlags;
use silvermere_auth::roles::{Perm, Role};

use crate::btrieve::OpenOptions;
use crate::decode::{self, EvilrecRecord, PermrecRecord, Record};
use crate::importer::{Decoded, ImportError, Note, find_file, preflight_siblings};
use silvermere_types::records::LayoutSet;

/// The host's accounts.
pub const ACCOUNTS_FILE: &str = "WGSUSR2.DAT";
/// The host's key lists.
pub const KEYS_FILE: &str = "WGSKEY2.DAT";
/// The host's generic database.
pub const GENBB_FILE: &str = "WGSGEN2.DAT";
/// The module's options, with the key names.
pub const OPTIONS_FILE: &str = "WCCMMUD.MSG";
/// The module's undocumented settings (`CODEC_OF_TWINKS`, the `SYS_*` keys).
pub const INI_FILE: &str = "WCCMMUD.INI";

/// `usracc` members read: user-id, password, current class.
const USERID: std::ops::Range<usize> = 0..30;
const PASSWORD: std::ops::Range<usize> = 30..40;
const CLASS: std::ops::Range<usize> = 240..256;
/// A key record's name; its keys follow from +31.
const KEY_NAME: std::ops::Range<usize> = 0..30;
const KEY_LIST: usize = 31;
/// The genbb key's module name, after the 30-byte user-id.
const MODULE: std::ops::Range<usize> = 30..55;

/// One host account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAccount {
    /// The user-id, the account's name.
    pub userid: String,
    /// The password as the host stored it (plain text).
    pub password: String,
    /// The current class (`USER`, `SYSOP`, ...).
    pub class: String,
}

/// The module's key options: the keys the roles and flags come from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyOptions {
    /// `SYSOPKEY`: the sysop role.
    pub sysop: Option<String>,
    /// `GAMOPKEY`: the game-operator role.
    pub gameop: Option<String>,
    /// `GOSSKEY`: takes gossip away.
    pub gossip: Option<String>,
    /// `AUCTKEY`: takes auctions away.
    pub auction: Option<String>,
    /// `LOANKEY`: needed for loans.
    pub loan: Option<String>,
    /// `WCCMMUD.INI`'s `CODEC_OF_TWINKS`: 78 opened the developer commands to sysops.
    pub codec_of_twinks: Option<String>,
    /// `WCCMMUD.INI`'s `SYS_*` keys, as set.
    pub sys_keys: Vec<(String, String)>,
}

/// What an account gets from the keys it held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grants {
    /// The `account_roles` rows: role names and permission (INI) names.
    pub roles: Vec<String>,
    /// The account flags (no gossip, no auction, no loan).
    pub flags: u32,
}

/// The host files read.
#[derive(Debug, Clone, Default)]
pub struct Host {
    /// `WGSUSR2.DAT`.
    pub accounts: Vec<Decoded<HostAccount>>,
    /// `WGSKEY2.DAT`: the keys of each user-id and `&class`, upper case, by lower-case name.
    pub keys: HashMap<String, Vec<String>>,
    /// `WCCMMUD.MSG` and `WCCMMUD.INI`.
    pub options: KeyOptions,
    /// `WGSGEN2.DAT` "WCC PERMANENT USER".
    pub profiles: Vec<Decoded<PermrecRecord>>,
    /// `WGSGEN2.DAT` "WCC MAJOR MUD EVIL".
    pub evil_points: Vec<Decoded<EvilrecRecord>>,
    /// Records read and not carried.
    pub notes: Vec<Note>,
}

impl Host {
    /// Whether `account` held `lock`, as `haskey` decided (see the module docs).
    pub fn holds(&self, account: &HostAccount, lock: &str) -> bool {
        self.holds_with(account, lock, true)
    }

    /// [`Host::holds`], the master flag counting only when `master_counts`.
    fn holds_with(&self, account: &HostAccount, lock: &str, master_counts: bool) -> bool {
        let lock = lock.trim();
        if lock.is_empty() {
            return true;
        }
        let own = self.keys.get(&account.userid.to_lowercase());
        let class = self.keys.get(&format!("&{}", account.class.to_lowercase()));
        let held: Vec<&String> = own.into_iter().chain(class).flatten().collect();
        let master = master_counts
            && (account.class.eq_ignore_ascii_case("SYSOP") || held.iter().any(|k| *k == "MASTER"));
        let one = |key: &str| {
            let key = key.trim();
            key.is_empty() || master || held.iter().any(|k| k.eq_ignore_ascii_case(key))
        };
        // keys joined by & and |, taken left to right as the host does
        let mut result = None;
        let mut op = '|';
        let mut start = 0;
        for (i, c) in lock.char_indices().chain([(lock.len(), '|')]) {
            if c == '&' || c == '|' {
                let this = one(&lock[start..i]);
                result = Some(match (result, op) {
                    (None, _) => this,
                    (Some(r), '&') => r && this,
                    (Some(r), _) => r || this,
                });
                op = c;
                start = i + 1;
            }
        }
        result.unwrap_or(true)
    }

    /// The roles, permissions and flags `account` gets: `sysop` and `gameop` for the holders
    /// of `SYSOPKEY` and `GAMOPKEY`; `DEBUG` for a sysop when `CODEC_OF_TWINKS` is 78;
    /// `SYS_DISBAND_KEY` for a game operator holding that INI key; the gossip and auction
    /// flags for those given `GOSSKEY` and `AUCTKEY` by name (not through the master flag),
    /// the loan flag for those without `LOANKEY`. Without `WCCMMUD.MSG` no key option is known and the account is a player.
    pub fn grants(&self, account: &HostAccount) -> Grants {
        let o = &self.options;
        let mut roles = vec![Role::Player.name().to_string()];
        let sysop = o.sysop.as_deref().is_some_and(|k| self.holds(account, k));
        let gameop = o.gameop.as_deref().is_some_and(|k| self.holds(account, k));
        if sysop {
            roles.push(Role::Sysop.name().to_string());
        }
        if gameop {
            roles.push(Role::GameOp.name().to_string());
        }
        if sysop && o.codec_of_twinks.as_deref().map(str::trim) == Some("78") {
            roles.push(Perm::Debug.ini_name().to_string());
        }
        let disband = o
            .sys_keys
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(Perm::SysDisband.ini_name()));
        if gameop
            && let Some((_, key)) = disband
            && !key.trim().is_empty()
            && self.holds(account, key)
        {
            roles.push(Perm::SysDisband.ini_name().to_string());
        }
        let mut flags = AccountFlags::NONE;
        // the master flag held these too, and the module let staff keep what they take away:
        // only a key given by name takes it
        if takes_away(&o.gossip).is_some_and(|k| self.holds_with(account, k, false)) {
            flags.insert(AccountFlags::NO_GOSSIP);
        }
        if takes_away(&o.auction).is_some_and(|k| self.holds_with(account, k, false)) {
            flags.insert(AccountFlags::NO_AUCTION);
        }
        if o.loan.as_deref().is_some_and(|k| !self.holds(account, k)) {
            flags.insert(AccountFlags::NO_LOAN);
        }
        Grants {
            roles,
            flags: flags.bits(),
        }
    }
}

/// A key that takes something away: an empty one takes nothing (every lock it would be is
/// held, so it would take it from everyone).
fn takes_away(key: &Option<String>) -> Option<&str> {
    key.as_deref().filter(|k| !k.trim().is_empty())
}

/// Reads the host files beside a Worldgroup set in `dir`; `None` when `WGSUSR2.DAT` and
/// `WGSGEN2.DAT` are both missing (a copy of the module's files alone).
pub fn read_host(dir: &Path) -> Result<Option<Host>, ImportError> {
    let accounts_path = find_file(dir, ACCOUNTS_FILE).ok();
    let genbb_path = find_file(dir, GENBB_FILE).ok();
    if accounts_path.is_none() && genbb_path.is_none() {
        return Ok(None);
    }
    let mut host = Host::default();
    if let Some(path) = accounts_path {
        each_record(&path, ACCOUNTS_FILE, |ordinal, data| {
            if data.len() < CLASS.end {
                host.notes
                    .push(short(ACCOUNTS_FILE, ordinal, data.len(), CLASS.end));
                return;
            }
            host.accounts.push(Decoded {
                file: ACCOUNTS_FILE.to_string(),
                ordinal,
                record: HostAccount {
                    userid: text(&data[USERID]).trim().to_string(),
                    password: text(&data[PASSWORD]),
                    class: text(&data[CLASS]).trim().to_string(),
                },
            });
        })?;
        if let Ok(path) = find_file(dir, KEYS_FILE) {
            each_record(&path, KEYS_FILE, |_, data| {
                if data.len() <= KEY_LIST {
                    return;
                }
                let name = text(&data[KEY_NAME]).trim().to_lowercase();
                let keys = text(&data[KEY_LIST..])
                    .split_whitespace()
                    .map(str::to_uppercase)
                    .collect();
                host.keys.insert(name, keys);
            })?;
        }
        host.options = read_options(dir, &mut host.notes);
    }
    if let Some(path) = genbb_path {
        each_record(&path, GENBB_FILE, |ordinal, data| {
            let module = data.get(MODULE).map(text).unwrap_or_default();
            match module.trim() {
                decode::PERMREC_MODULE => {
                    genbb(&mut host.profiles, &mut host.notes, ordinal, data);
                }
                decode::EVILREC_MODULE => {
                    genbb(&mut host.evil_points, &mut host.notes, ordinal, data);
                }
                _ => {}
            }
        })?;
    }
    Ok(Some(host))
}

/// Decodes one genbb record of the module's into `out`; a short one is a note.
fn genbb<R: Record>(out: &mut Vec<Decoded<R>>, notes: &mut Vec<Note>, ordinal: u32, data: &[u8]) {
    let len = R::layout(LayoutSet::Worldgroup).len;
    match data
        .get(..len)
        .map(|b| R::decode_as(b, LayoutSet::Worldgroup))
    {
        Some(Ok(record)) => out.push(Decoded {
            file: GENBB_FILE.to_string(),
            ordinal,
            record,
        }),
        _ => notes.push(short(GENBB_FILE, ordinal, data.len(), len)),
    }
}

fn short(file: &str, ordinal: u32, found: usize, want: usize) -> Note {
    Note {
        file: file.to_string(),
        ordinal,
        key: String::new(),
        reason: format!("{found} bytes, at least {want} expected"),
    }
}

/// Every record of a host file (fixed or variable length), as its bytes.
fn each_record(path: &Path, name: &str, mut f: impl FnMut(u32, &[u8])) -> Result<(), ImportError> {
    preflight_siblings(path)?;
    let mut file = OpenOptions::new().variable_length(true).open(path)?;
    let mut records = file.records();
    while let Some(record) = records.next_record() {
        let record = record?;
        f(record.ordinal, record.data);
    }
    tracing::debug!(file = name, "read");
    Ok(())
}

/// CP437 bytes up to the first NUL.
fn text(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    bytes[..end]
        .iter()
        .map(|&b| silvermere_types::fixed::cp437_to_char(b))
        .collect()
}

/// The key options of `WCCMMUD.MSG` and the settings of `WCCMMUD.INI`; a file missing or
/// unreadable is a note, and its options stay unset.
fn read_options(dir: &Path, notes: &mut Vec<Note>) -> KeyOptions {
    let mut o = KeyOptions::default();
    let note = |notes: &mut Vec<Note>, file: &str, reason: String| {
        notes.push(Note {
            file: file.to_string(),
            ordinal: 0,
            key: String::new(),
            reason,
        });
    };
    match find_file(dir, OPTIONS_FILE).map(|p| std::fs::read(&p)) {
        Ok(Ok(bytes)) => match silvermere_text::package::parse_msg(OPTIONS_FILE, &bytes) {
            Ok(entries) => {
                let get = |key: &str| {
                    entries
                        .iter()
                        .find(|e| e.key == key)
                        .map(|e| e.value.trim().to_string())
                };
                o.sysop = get("SYSOPKEY");
                o.gameop = get("GAMOPKEY");
                o.gossip = get("GOSSKEY");
                o.auction = get("AUCTKEY");
                o.loan = get("LOANKEY");
            }
            Err(e) => note(notes, OPTIONS_FILE, format!("{e}; no account gets a role")),
        },
        _ => note(
            notes,
            OPTIONS_FILE,
            "not found; no account gets a role".to_string(),
        ),
    }
    if let Ok(Ok(bytes)) = find_file(dir, INI_FILE).map(|p| std::fs::read(&p)) {
        for line in String::from_utf8_lossy(&bytes).lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key.eq_ignore_ascii_case("CODEC_OF_TWINKS") {
                o.codec_of_twinks = Some(value.to_string());
            } else if Perm::from_ini_name(key).is_some_and(Perm::is_sysop_key) {
                o.sys_keys.push((key.to_uppercase(), value.to_string()));
            }
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(keys: &[(&str, &str)]) -> Host {
        Host {
            keys: keys
                .iter()
                .map(|(n, k)| {
                    (
                        n.to_lowercase(),
                        k.split_whitespace().map(str::to_uppercase).collect(),
                    )
                })
                .collect(),
            options: KeyOptions {
                sysop: Some("NORMAL".into()),
                gameop: Some("NORMAL".into()),
                gossip: Some("gidiot".into()),
                auction: Some("aidiot".into()),
                loan: Some("NORMAL".into()),
                codec_of_twinks: Some("78".into()),
                sys_keys: vec![("SYS_CHANGE_KEY".into(), "SYSOP".into())],
            },
            ..Host::default()
        }
    }

    fn account(userid: &str, class: &str) -> HostAccount {
        HostAccount {
            userid: userid.into(),
            password: String::new(),
            class: class.into(),
        }
    }

    // A realm's keys: classes give NORMAL, Healbot holds MASTER, Sysop is in class SYSOP.
    #[test]
    fn keys_come_from_the_user_the_class_or_the_master_flag() {
        let h = host(&[
            ("&USER", "DEMO NORMAL"),
            ("&DEMO", "DEMO"),
            ("Healbot", "DEMO NORMAL  SYSOP SUPER MASTER"),
            ("Moe", "gidiot"),
        ]);
        let fatty = account("Fatty", "USER");
        assert!(h.holds(&fatty, "NORMAL"));
        assert!(h.holds(&fatty, "normal"));
        assert!(!h.holds(&fatty, "SYSOP"));
        assert!(h.holds(&fatty, ""));
        assert!(h.holds(&account("Healbot", "SYSOP"), "anything"));
        assert!(h.holds(&account("Sysop", "SYSOP"), "NORMAL"));
        assert!(!h.holds(&account("Test", "DEMO"), "NORMAL"));
        assert!(h.holds(&account("Moe", "DEMO"), "GIDIOT"));
        // left to right
        assert!(h.holds(&fatty, "SYSOP|NORMAL"));
        assert!(!h.holds(&fatty, "SYSOP&NORMAL"));
        assert!(h.holds(&fatty, "SYSOP&NORMAL|DEMO"));
    }

    #[test]
    fn roles_and_flags_follow_the_module_keys() {
        let h = host(&[
            ("&USER", "DEMO NORMAL"),
            ("&DEMO", "DEMO"),
            ("Moe", "gidiot"),
        ]);
        let g = h.grants(&account("Fatty", "USER"));
        assert_eq!(g.roles, ["player", "sysop", "gameop", "DEBUG"]);
        assert_eq!(g.flags, 0);
        let g = h.grants(&account("Test", "DEMO"));
        assert_eq!(g.roles, ["player"]);
        assert_eq!(g.flags, AccountFlags::NO_LOAN.bits());
        // the master flag gives NORMAL, not the keys that take gossip and auctions away
        let mut h = h;
        h.keys.insert("healbot".into(), vec!["MASTER".into()]);
        let g = h.grants(&account("Healbot", "USER"));
        assert_eq!((g.roles.len(), g.flags), (4, 0));
        assert_eq!(h.grants(&account("Sysop", "SYSOP")).flags, 0);
        let g = h.grants(&account("Moe", "DEMO"));
        assert_eq!(
            g.flags,
            AccountFlags::NO_GOSSIP.bits() | AccountFlags::NO_LOAN.bits()
        );
        // without the codec at 78 a sysop has no DEBUG
        h.options.codec_of_twinks = None;
        assert_eq!(
            h.grants(&account("Fatty", "USER")).roles,
            ["player", "sysop", "gameop"]
        );
        // without WCCMMUD.MSG, a player
        h.options = KeyOptions::default();
        let g = h.grants(&account("Fatty", "USER"));
        assert_eq!(
            (g.roles.as_slice(), g.flags),
            (&["player".to_string()][..], 0)
        );
    }
}
