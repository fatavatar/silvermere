//! Game screens on the page, as a telnet session shows them.
//!
//! A screen is not drawn to look like the game: it *is* the game's output. The lines go through
//! `silvermere-text`'s renderer (the templates, colours and 79-column wrapping the server sends
//! a player in the default palette), and the ANSI that comes out is turned into HTML in the
//! sixteen colours of a PC terminal, on black, in 80 columns.

use std::fmt::Write as _;

use silvermere_text::colour::{ColRow, Colour, Palette};
use silvermere_text::prompt::PromptInput;
use silvermere_text::statline::StatlineCtx;
use silvermere_text::{Msg, cp437, render, render_prompt};
use silvermere_types::enums::{Magery, StatlineMode};

use crate::html::esc;

/// What a room screen shows.
#[derive(Debug, Clone, Default)]
pub struct RoomScreen<'a> {
    pub name: &'a str,
    /// The room's description lines, as stored.
    pub description: &'a [String],
    /// After `You notice `.
    pub items: Option<&'a str>,
    /// Players after `Also here: `.
    pub players: Option<&'a str>,
    /// After `Obvious exits: `.
    pub exits: Option<&'a str>,
    /// The prompt's hit points and mana.
    pub prompt: (i32, i32),
}

/// The room as the game prints it on arrival, and the prompt, as HTML.
pub fn room(screen: &RoomScreen) -> String {
    let palette = Palette::default();
    let mut out = Vec::new();
    out.extend_from_slice(
        &render(
            &Msg::RoomTitle {
                name: screen.name.to_owned(),
            },
            palette,
        )
        .bytes,
    );
    let text = paragraph(screen.description);
    if !text.is_empty() {
        out.extend_from_slice(&render(&Msg::RoomLongDesc { text }, palette).bytes);
    }
    if let Some(items) = screen.items {
        out.extend_from_slice(
            &render(
                &Msg::RoomItems {
                    items: items.to_owned(),
                },
                palette,
            )
            .bytes,
        );
    }
    if let Some(players) = screen.players {
        // the room display colours the names itself (silvermere-engine `display::room`)
        let names = format!(
            "{}{players}",
            std::str::from_utf8(Colour::new(ColRow::HereNames, palette).prefix()).unwrap_or("")
        );
        out.extend_from_slice(&render(&Msg::RoomAlsoHere { who: names }, palette).bytes);
    }
    if let Some(exits) = screen.exits {
        out.extend_from_slice(
            &render(
                &Msg::RoomExits {
                    exits: exits.to_owned(),
                },
                palette,
            )
            .bytes,
        );
    }
    out.extend_from_slice(&prompt(screen.prompt.0, screen.prompt.1, palette));
    terminal(&out)
}

/// The description as the room display joins it: each non-empty line without its trailing
/// spaces, and a space after each.
fn paragraph(lines: &[String]) -> String {
    let mut text = String::new();
    for l in lines.iter().filter(|l| !l.is_empty()) {
        text.push_str(l.trim_end_matches(' '));
        text.push(' ');
    }
    text
}

/// A caster's prompt at full health: `[HP=24/MA=10]:`.
fn prompt(hp: i32, mana: i32, palette: Palette) -> Vec<u8> {
    let input = PromptInput {
        hp,
        maxhp: hp,
        mana,
        maxmana: mana,
        magery: Some(Magery::from_code(1)),
        resting: false,
        meditating: false,
        mode: StatlineMode::from_code(1),
        template: None,
        palette,
        sysop_debug: None,
    };
    // only a custom statline reads the context
    let ctx = StatlineCtx {
        hp,
        maxhp: hp,
        mana,
        maxmana: mana,
        exp: 0,
        exp_needed: None,
        wealth_copper: 0,
        first_name: "",
        evil_warn: false,
        resting: false,
        meditating: false,
        pronoun_possessive: "",
        pronoun_object: "",
        title: None,
        palette,
    };
    render_prompt(&input, &ctx)
}

/// ANSI output as a terminal panel.
pub fn terminal(bytes: &[u8]) -> String {
    format!(
        "<pre class=\"terminal\" role=\"img\" aria-label=\"{}\">{}</pre>",
        esc(&plain(bytes)),
        ansi_to_html(bytes)
    )
}

/// The text alone, for screen readers.
fn plain(bytes: &[u8]) -> String {
    let text = cp437::decode(&silvermere_text::strip_ansi(bytes));
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The colours a terminal shows: SGR 0 (reset), 1 (bright) and 30–37 (foreground) are
/// followed; backgrounds and cursor movement are not, since the screens drawn here use neither.
fn ansi_to_html(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (mut fg, mut bright) = (7u8, false);
    let mut open: Option<u8> = None;
    let mut text: Vec<u8> = Vec::new();
    let flush = |out: &mut String, text: &mut Vec<u8>, open: &mut Option<u8>, colour: u8| {
        if text.is_empty() {
            return;
        }
        if *open != Some(colour) {
            if open.is_some() {
                out.push_str("</span>");
            }
            let _ = write!(out, "<span class=\"t{colour}\">");
            *open = Some(colour);
        }
        out.push_str(&esc(&cp437::decode(text)));
        text.clear();
    };
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            let start = i + 2;
            let mut end = start;
            while end < bytes.len() && !bytes[end].is_ascii_alphabetic() {
                end += 1;
            }
            if bytes.get(end) == Some(&b'm') {
                let colour = fg + if bright { 8 } else { 0 };
                flush(&mut out, &mut text, &mut open, colour);
                let params = std::str::from_utf8(&bytes[start..end]).unwrap_or("");
                for p in params.split(';') {
                    match p.parse::<u8>().unwrap_or(0) {
                        0 => (fg, bright) = (7, false),
                        1 => bright = true,
                        22 => bright = false,
                        n @ 30..=37 => fg = n - 30,
                        39 => fg = 7,
                        _ => {}
                    }
                }
            }
            i = end + 1;
            continue;
        }
        match b {
            b'\r' => {}
            _ => text.push(b),
        }
        i += 1;
    }
    let colour = fg + if bright { 8 } else { 0 };
    flush(&mut out, &mut text, &mut open, colour);
    if open.is_some() {
        out.push_str("</span>");
    }
    // the last line's end would show as an empty line under the screen
    while let Some(stripped) = out.strip_suffix("\n</span>") {
        out = format!("{stripped}</span>");
    }
    out.trim_end_matches('\n').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_become_spans() {
        let html =
            ansi_to_html(b"\x1b[79D\x1b[K\x1b[1;36mNewhaven\r\n\x1b[0;32mObvious exits: north\r\n");
        assert_eq!(
            html,
            "<span class=\"t14\">Newhaven\n</span><span class=\"t2\">Obvious exits: north</span>"
        );
    }

    #[test]
    fn a_room_as_the_game_sends_it() {
        let lines = vec![
            "Welcome to Newhaven! You are standing at the crude wooden gates of the".to_owned(),
            "village entrance.".to_owned(),
        ];
        let html = room(&RoomScreen {
            name: "Newhaven, Village Entrance",
            description: &lines,
            items: Some("a small potion"),
            players: Some("Thornwyn"),
            exits: Some("north, south"),
            prompt: (24, 10),
        });
        assert!(html.contains("<span class=\"t14\">Newhaven, Village Entrance"));
        // the description is indented four spaces and wrapped at 79 columns
        assert!(html.contains("    Welcome to Newhaven!"));
        assert!(
            html.lines()
                .all(|l| crate::html::strip_tags(l).chars().count() <= 80)
        );
        assert!(html.contains("You notice a small potion here."));
        assert!(html.contains("Also here: "));
        assert!(html.contains("Obvious exits: north, south"));
        assert!(html.contains("[HP=") && html.contains("24") && html.contains("MA="));
    }
}
