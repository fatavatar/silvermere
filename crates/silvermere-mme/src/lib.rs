//! Builds a MajorMUD Explorer database of a world.
//!
//! MajorMUD Explorer (MME, by syntax53) reads a world from an Access `.mdb` that Nightmare
//! Redux (NMR) exports from a BBS's data files. This crate does both halves without Windows
//! or Java: [`nmr`] ports NMR's export (the rows, with the columns NMR derives, such as where
//! an item is obtained from and each room's lair), and [`jet`] writes those rows into a copy
//! of an NMR-made database, replacing every table's rows and rebuilding its indexes.
//! [`triggers`] adds MegaMUD's message triggers, made from the realm's spells, as a table
//! of their own for Majordomo.

#![forbid(unsafe_code)]

pub mod jet;
pub mod nmr;
mod table;
pub mod triggers;

pub use table::{Row, Tables, Value};

/// The database the writer fills: MME's own `data-v1.11p.mdb` (an NMR v1.8.3 export of the
/// stock world), at a fixed commit of MME's repository. Only its catalog, table definitions and
/// indexes are used; every row of the tables MME reads is replaced.
pub mod template {
    /// Where it is published.
    pub const URL: &str = "https://raw.githubusercontent.com/syntax53/MMUD-Explorer/28d23d70caf4f971418aa7ea25471cdb0ab0888a/data-v1.11p.mdb";
    /// Its name in the cache.
    pub const FILE_NAME: &str = "mme-data-v1.11p.mdb";
    /// Its sha256, lowercase hex.
    pub const SHA256: &str = "eba20f06af8dc08df755eb6dcd9763b023794f12e809cbab6762236d64d7345f";
    /// Its size in bytes.
    pub const SIZE: u64 = 11_722_752;
}
