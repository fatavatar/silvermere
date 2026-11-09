//! The published `WCCMMUD.DLL` edit set, behind `game.published_dll_edits` (off: stock 1.11p).
//!
//! Sysops patched the WG3NT 1.11p final to run worlds with more classes, races and shops than
//! stock. Offsets are file offsets in that DLL (SHA-256 `5A99E7C3...`). Only the edits that
//! change something here are in this module. The rest have nothing to port: the race and class
//! range checks (`0x1475B`, `0x14777`) already go by the loaded world; the monster limits
//! (`0x019F3`, `0x5F879`) have no counterpart in the unbounded quick reference; the add-on
//! cracks and the map check (`0x18290`, `0x4E9B4`, `0x4E9BA`, `0x16460`) have no activation to
//! pass; 256 users (`0x4E789`, `0x4E79C`) is `server.max_sessions`; and `0x5AEBD`, `0x5AED5`
//! bound the original's wall-clock carry loops, where restocks here count ticks.

use crate::world::World;

/// `0x2F57E`: the shops initiated at start-up, 200 raised to 32767.
const NRESTOCK_EDITED: i32 = 32767;
/// `0x5AE3D`: the queue entries one restock looks at, 1000 raised to 655340.
const MAXRESTOCK_EDITED: usize = 655_340;
/// `0x5B01D`: the queue entries the restocking display looks at, 2000 raised to 32767.
const MAXDISPLAY_EDITED: usize = 32767;

fn on(world: &World) -> bool {
    world.config.published_dll_edits
}

/// `0x14814`, `0x148BE`, `0x149B0`, `0x6D639`, `0x6D6E3`, `0x6D7D5`: Smash, Perfect Stealth and
/// Meditate stay on any class at any level, not only on the stock classes that keep them.
pub fn keeps_level_gated_abilities(world: &World) -> bool {
    on(world)
}

/// `0x15794`: See Hidden stays on any race, not only race 13.
pub fn keeps_see_hidden(world: &World) -> bool {
    on(world)
}

/// The shops initiated at start-up, `0..` this.
pub fn nrestock(world: &World) -> i32 {
    if on(world) {
        NRESTOCK_EDITED
    } else {
        crate::shops::NRESTOCK
    }
}

/// The queue entries one restock looks at.
pub fn maxrestock(world: &World) -> usize {
    if on(world) {
        MAXRESTOCK_EDITED
    } else {
        crate::shops::MAXRESTOCK
    }
}

/// The queue entries the sysop's restocking display looks at.
pub fn maxdisplay(world: &World) -> usize {
    if on(world) {
        MAXDISPLAY_EDITED
    } else {
        crate::display::sysop::MAXDISPLAY
    }
}

/// `0x392D8`: a two-handed weapon reads `(Weapon Hand)` too, which is what MegaMud parses.
pub fn two_handed_label(world: &World) -> &'static str {
    if on(world) {
        "Weapon Hand"
    } else {
        "Two handed"
    }
}

/// `0x8FD9F`: the mark of an immortal character.
pub fn edited_tag(world: &World) -> &'static str {
    if on(world) { "SYSOP" } else { "EDITED" }
}

/// `0x8BCC5`: the word of the hidden `SYSOP LANCE`.
pub fn lance_word(world: &World) -> &'static str {
    if on(world) { "warez" } else { "lance" }
}
