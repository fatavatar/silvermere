//! The `[MAJORMUD]:` menu a player sees outside the realm, and the two `.MSG` screens
//! around it, the realm entry banner (`USRFOLED`) and the logon greeting (`USRLOGIN`).
//!
//! `MENUTEXT`, `MENUPMPT`, `USRFOLED` and `USRLOGIN` come from the installed package text
//! ([`crate::package::installed`]), which holds the realm's own (the database's
//! `game_texts`, which the import and overlays write). Until it is installed they are the
//! stock entries of `messages/prompt.toml`, which match the stock files byte for byte. The
//! menu's other lines (the race change, `[SYSOP]`, the tournament notice) are the DLL's and
//! always come from `prompt.toml`.
//!
//! Every renderer returns the bytes the player receives (`render::finish`: `\r\n` line
//! ends, no wrap); the menu ends with the `[MAJORMUD]: ` prompt and no newline.

use crate::catalogue::{Catalogue, TemplatePiece};
use crate::cp437;
use crate::package::{self, PackageText};
use crate::prompt::prompt_text;
use crate::render::finish;

/// The version with the original's build stamp, shown in the menu title. silvermere
/// takes it from `compat.version_string`; this is the default.
pub const MUDVER: &str = "1.11p-WG (Dec 30 2005 14:20:05)";

/// The version without the build stamp, for the logon greeting.
pub const SHORTVER: &str = "1.11p-WG";

/// The one-time race (or race and class) change offer, while `can_user_change_race`
/// (T-047) says it is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaceChange {
    /// `ENABLE_CLASS_CHANGE`: the line says "Race/Class Change".
    pub class_too: bool,
    /// Game minutes left to undo the change; 0 or less shows
    /// `[Last chance]`.
    pub minutes_left: i32,
}

/// What the menu shows depends on the player and the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuInput<'a> {
    /// The full version, the text after the `v` of the title.
    pub version: &'a str,
    /// `Some` while the race change is open.
    pub race_change: Option<RaceChange>,
    /// A sysop: the `[SYSOP]` line.
    pub sysop: bool,
    /// Tournament play on: the tournament notice.
    pub tournament: bool,
}

/// The menu as the player receives it. Lines that add-ons could add to the menu are not
/// carried.
pub fn render_menu(input: MenuInput) -> Vec<u8> {
    render_menu_from(package::installed(), input)
}

/// [`render_menu`] with `MENUTEXT` and `MENUPMPT` from `text` (the stock `prompt.toml`
/// entries without it).
pub fn render_menu_from(text: Option<&PackageText>, input: MenuInput) -> Vec<u8> {
    let mut prf = String::new();
    // MENUTEXT with the full version.
    prf.push_str(&msg_text(
        text,
        PackageText::wcctext,
        "menutext",
        "menu_text",
        input.version,
    ));
    // the race change offer
    if let Some(change) = input.race_change {
        prf.push_str(prompt_text(if change.class_too {
            "change_race_class"
        } else {
            "change_race"
        }));
        if change.minutes_left > 0 {
            prf.push_str(
                &prompt_text("change_race_minutes")
                    .replace("{minutes}", &change.minutes_left.to_string()),
            );
        } else {
            prf.push_str(prompt_text("change_race_last_chance"));
        }
        prf.push_str(prompt_text("change_race_end"));
    }
    if input.sysop {
        prf.push_str(prompt_text("sysop_item"));
    }
    if input.tournament {
        prf.push_str(prompt_text("tournament"));
    }
    prf.push_str(&msg_text(
        text,
        PackageText::wcctext,
        "menupmpt",
        "menu_prompt",
        "",
    ));
    finish(&cp437::encode(&prf), None)
}

/// The realm entry banner, `USRFOLED` (stock: clear screen, `MajorMUD - ROL` and the
/// `=-=` rule).
pub fn render_entry_banner() -> Vec<u8> {
    render_entry_banner_from(package::installed())
}

/// [`render_entry_banner`] from `text` (the stock `prompt.toml` entry without it).
pub fn render_entry_banner_from(text: Option<&PackageText>) -> Vec<u8> {
    let banner = msg_text(text, PackageText::wccmmud, "usrfoled", "entry_banner", "");
    finish(&cp437::encode(&banner), None)
}

/// The logon greeting, `USRLOGIN` with the short version (stock:
/// `Fantasy awaits you in MajorMUD v1.11p-WG!`).
pub fn render_login_greeting(version: &str) -> Vec<u8> {
    render_login_greeting_from(package::installed(), version)
}

/// [`render_login_greeting`] from `text` (the stock `prompt.toml` entry without it).
pub fn render_login_greeting_from(text: Option<&PackageText>, version: &str) -> Vec<u8> {
    let greeting = msg_text(
        text,
        PackageText::wccmmud,
        "usrlogin",
        "login_greeting",
        version,
    );
    finish(&cp437::encode(&greeting), None)
}

/// The text block `key` of one of `text`'s message files with its one argument (the
/// version) filled in; the `prompt.toml` entry `stock` when there is no package text.
fn msg_text(
    text: Option<&PackageText>,
    file: fn(&PackageText) -> &Catalogue,
    key: &str,
    stock: &str,
    version: &str,
) -> String {
    let Some(entry) = text.and_then(|t| file(t).messages.get(key)) else {
        return prompt_text(stock).replace("{version}", version);
    };
    let Ok(pieces) = entry.pieces(key) else {
        return prompt_text(stock).replace("{version}", version);
    };
    let mut out = String::new();
    for piece in pieces {
        match piece {
            TemplatePiece::Text(t) => out.push_str(&t),
            TemplatePiece::Arg(_) => out.push_str(version),
            // the message files' text blocks hold no macros
            TemplatePiece::Ansi(_) | TemplatePiece::Colour(_) => {}
        }
    }
    out
}
