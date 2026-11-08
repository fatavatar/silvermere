//! The module's two records in the host's generic database (genbb, Worldgroup 3.x's
//! `WGSGEN2.DAT`): the permanent-info record `permrec` ("WCC PERMANENT USER", the settings
//! and kept experience that outlive a character) and `evilrec` ("WCC MAJOR MUD EVIL", the
//! evil points kept over a death). Each is keyed by the user-id and the module name
//! (`modnam`, the genbb key's second part). Not carried: `modnam`, the `unk_*` members.
//! The statline template keeps its spaces: the engine saves and shows it as typed.

use silvermere_types::enums::{MessageLevel, StatlineMode};
use silvermere_types::flags::{PlayerFlags, PlayerSettings};
use silvermere_types::records::{EvilRecord, PermanentRecord, layout};

use super::{DecodeError, LegacyClock, Raw, Text, record};

/// The genbb module name of [`PermrecRecord`].
pub const PERMREC_MODULE: &str = "WCC PERMANENT USER";
/// The genbb module name of [`EvilrecRecord`].
pub const EVILREC_MODULE: &str = "WCC MAJOR MUD EVIL";

record! {
    /// The `permrec` record, genbb "WCC PERMANENT USER": every member, in record order.
    pub struct PermrecRecord = layout::PERMREC {
        userid: Text<30>,
        modnam: Text<25>,
        deathdate: u16,
        palette: u8,
        msglevel: u8,
        settings: u16,
        pagemax: u8,
        hairlen: u8,
        haircol: u8,
        unk_0x40: u8,
        eyecol: u8,
        unk_0x42: u8,
        statline: u8,
        fasttalk: u8,
        verbose: u8,
        unk_0x46: u8,
        exp: u32,
        firstname: Text<11>,
        lastname: Text<19>,
        flags: u16,
        statfmt: Text<61>,
        gossmax: u8,
        auctmax: u8,
        daybyte: u8,
        unk_0xab: u16,
        unk_0xad: u16,
        expbil: u32,
        exprem: u32,
        unk_0xb7: Raw<22>,
    }
}

record! {
    /// The `evilrec` record, genbb "WCC MAJOR MUD EVIL": every member, in record order.
    pub struct EvilrecRecord = layout::EVILREC {
        userid: Text<30>,
        modnam: Text<25>,
        evil: i16,
    }
}

/// The schema's profile; `deathdate` (a DOS date) in `clock`'s zone.
pub fn to_profile(r: &PermrecRecord, clock: &LegacyClock) -> Result<PermanentRecord, DecodeError> {
    let userid = r.userid.trimmed();
    let deathdate = clock.at(
        "permrec",
        format!("userid {userid}"),
        "deathdate",
        r.deathdate,
        0,
    )?;
    Ok(PermanentRecord {
        userid,
        deathdate,
        palette: r.palette,
        msglevel: MessageLevel::from_code(u16::from(r.msglevel)),
        settings: PlayerSettings::from_bits_retain(r.settings),
        pagemax: r.pagemax,
        hairlen: r.hairlen,
        haircol: r.haircol,
        eyecol: r.eyecol,
        statline: StatlineMode::from_code(u16::from(r.statline)),
        fasttalk: r.fasttalk,
        verbose: r.verbose,
        exp: r.exp,
        firstname: r.firstname.trimmed(),
        lastname: r.lastname.trimmed(),
        flags: PlayerFlags::from_bits_retain(r.flags),
        // the template as typed: a trailing space is part of the prompt
        statfmt: r.statfmt.0.clone(),
        gossmax: r.gossmax,
        auctmax: r.auctmax,
        daybyte: r.daybyte,
        expbil: r.expbil,
        exprem: r.exprem,
    })
}

/// The schema's evil-points record.
pub fn to_evil_points(r: &EvilrecRecord) -> EvilRecord {
    EvilRecord {
        userid: r.userid.trimmed(),
        evil: r.evil,
    }
}
