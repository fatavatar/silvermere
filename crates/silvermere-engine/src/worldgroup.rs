//! The 32-bit Worldgroup module (`1.11p-WG3NT`), behind `game.worldgroup_module` (off: the
//! DOS module).
//!
//! The two modules are built from the same source, but DOS has a 16-bit `int` and Worldgroup a
//! 32-bit one. The records keep their 16-bit members on both, so a result stored back into a
//! record field comes out the same; the modules part where a value is divided, compared or
//! printed before it is stored, and where an unsigned 16-bit field meets an `int` (unsigned
//! on DOS, signed on Worldgroup). The places that differ, each checked against the module's
//! code (RVA in `WCCMMUD.DLL` 1.11p-WG3NT):
//!
//! - inventory: the load and the maximum are printed whole (`%lu`, `%d`; `0x39E3D`);
//! - shop lists: the quantity unsigned and the price with its markup whole (`0x3A0A9`);
//! - the hospital's healing fee, `(maxhp-hp)*2`, whole (`0x1B8FF`);
//! - a toll exit's refusal prints the whole toll (`0x17E93`);
//! - rob: the coins to take are rolled out of the victim's whole purse (`0x1F869`);
//! - numbers typed to GIVE, BANKBOOK, MARKUP and SET MAX..., and a random event's line
//!   numbers, are whole `atol` results (`0x6779C`, `0x6897B`, `0x53823`, `0x59344`,
//!   `0x21507`);
//! - secondary stats: `(health-50)*level/16` of the maximum hit points is whole, the maximum
//!   mana is never floored at 0 (its test reads the unsigned field) and the mana is compared
//!   with it signed, as at login (`0x1AB9F`, `0x1AACC`, `0x1ABFE`, `0x15C16`);
//! - an item's minimum and maximum level and its strength requirement compare signed
//!   (`0x1FF2D`, `0x6DEBC`);
//! - evil points: the victim scaling is whole before it is added (`0x4E62B`);
//! - spells: a player's magic resistance under 50 gives no cut but extra damage, as a
//!   monster's does, and an area spell clears the cut for every target (`0x46148`,
//!   `0x41C16`).

use crate::world::World;

/// The world runs as the Worldgroup module.
pub fn on(world: &World) -> bool {
    world.config.worldgroup_module
}
