//! The realm's web site, as static files.
//!
//! [`render`] turns a [`Site`] (what the realm knows about itself: its classes and races, the
//! overlays its world is built from and their news, the address to connect to) into pages.
//! Nothing here reads the database or the disk: the `silvermere` binary gathers a [`Site`],
//! writes the pages under its data directory and serves them (docs/site.md).
//!
//! The look borrows MajorMUD's own screen: page titles are room names, the navigation is the
//! room's obvious exits, and news of new content is written the way a room tells you who else
//! is there.

#![forbid(unsafe_code)]

pub mod help;
mod html;
mod pages;
pub mod screen;

use silvermere_types::records::{Class, Race};

/// Everything the site shows.
#[derive(Debug, Clone, Default)]
pub struct Site {
    /// The realm's name.
    pub realm: String,
    /// One line under the name on the home page; empty for none.
    pub tagline: String,
    /// Where players connect, as they would type it (`realm.example.org:2323`, or a
    /// `telnet://` URL); `None` hides the connect line.
    pub connect: Option<String>,
    /// Players may make their own accounts (`new` at the login prompt).
    pub open_signup: bool,
    /// Hit points at or below which a character dies (`game.death_hp`); above it and below 1,
    /// a character is only unconscious.
    pub death_hp: i32,
    /// The name of the realm's top coin (`game.usr_cur_name`).
    pub top_coin: String,
    /// When the pages were made (`YYYY-MM-DD`).
    pub built: String,
    /// silvermere's version, for the footer.
    pub version: String,
    /// The room new characters start in, shown on the home page as the game shows it.
    pub start_room: Option<RoomView>,
    /// The classes, in number order.
    pub classes: Vec<Class>,
    /// The races, in number order.
    pub races: Vec<Race>,
    /// The game's help topics: (the words a topic answers to, its text). The class and race
    /// pages read their help screens from these ([`help`]).
    pub help_topics: Vec<(String, String)>,
    /// The overlays the world is built from, in load order.
    pub overlays: Vec<OverlayView>,
    /// The MajorMUD Explorer database, when one was built.
    pub mme: Option<MmeFile>,
}

/// A room as the game prints it.
#[derive(Debug, Clone, Default)]
pub struct RoomView {
    pub name: String,
    /// Description lines, without colour codes.
    pub description: Vec<String>,
    /// Exit directions, as the game words them (`north`, `southeast`, ...).
    pub exits: Vec<String>,
}

/// An overlay the world is built from.
#[derive(Debug, Clone, Default)]
pub struct OverlayView {
    pub name: String,
    /// The name players see; empty for the overlay's name.
    pub title: String,
    pub description: String,
    /// Who made it, or the content it comes from.
    pub credits: String,
    /// Newest first.
    pub news: Vec<NewsView>,
    /// Records it created, by kind (`room`, `monster`, ...).
    pub created: Vec<(String, i64)>,
    /// Records of other overlays (or the stock world) it changed, by kind.
    pub changed: Vec<(String, i64)>,
}

impl OverlayView {
    /// The name players see.
    pub fn title(&self) -> &str {
        if self.title.is_empty() {
            &self.name
        } else {
            &self.title
        }
    }
}

/// A news entry of an overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewsView {
    /// `YYYY-MM-DD`.
    pub date: String,
    pub title: String,
    pub kind: NewsKind,
    /// Paragraphs separated by blank lines.
    pub text: String,
}

/// What a news entry is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewsKind {
    /// New content, told as a hint.
    New,
    /// A change to what was there.
    Change,
    /// A correction.
    Fix,
}

/// The MajorMUD Explorer database offered for download.
#[derive(Debug, Clone, Default)]
pub struct MmeFile {
    /// The file's name in the site (`realm.mdb`).
    pub file: String,
    pub bytes: u64,
    /// Rows per table, for the download page.
    pub tables: Vec<(String, usize)>,
}

/// One file of the site.
#[derive(Debug, Clone)]
pub struct Page {
    /// Relative to the site's root (`index.html`, `assets/site.css`).
    pub path: String,
    pub bytes: Vec<u8>,
}

/// Every file of the site except the MME database itself (which the binary writes).
pub fn render(site: &Site) -> Vec<Page> {
    let mut out = vec![
        page("index.html", pages::home::render(site)),
        page("play.html", pages::play::render(site)),
        page("classes.html", pages::classes::render(site)),
        page("races.html", pages::races::render(site)),
        page("world.html", pages::world::render(site)),
        page("news.html", pages::news::render(site)),
        page("tools.html", pages::tools::render(site)),
        Page {
            path: "assets/site.css".into(),
            bytes: include_bytes!("assets/site.css").to_vec(),
        },
        Page {
            path: "assets/site.js".into(),
            bytes: include_bytes!("assets/site.js").to_vec(),
        },
    ];
    out.push(page("assets/exp.js", pages::classes::exp_data(site)));
    out
}

fn page(path: &str, body: String) -> Page {
    Page {
        path: path.into(),
        bytes: body.into_bytes(),
    }
}
