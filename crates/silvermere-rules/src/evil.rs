//! Evil: evil points and the evil law.
//!
//! The pure half of the evil law: the eight legal levels an evil point total falls in
//! ([`legal_level`]), what one evil act costs or why it is refused ([`cost`], for the
//! engine's `add_evil_points`), and how many points a dead character leaves to the
//! account's next one ([`keep_on_death`], for `save_evil_points`). The timers, the messages
//! and the callers are the engine's (`evil`).
//!
//! The game's points are 16-bit; the sums and products below wrap as `i16` where the
//! game's do, so the (sic) overflow guard of the victim scaling behaves as in the game.

/// The legal level of an evil point total, the index into [`LEGAL_NAMES`] and the legal
/// colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Legal {
    /// 0: -50 to 29.
    Neutral = 0,
    /// 1: 30 to 39.
    Seedy = 1,
    /// 2: 40 to 79.
    Outlaw = 2,
    /// 3: 80 to 119.
    Criminal = 3,
    /// 4: 120 to 209.
    Villain = 4,
    /// 5: 210 and more.
    Fiend = 5,
    /// 6: -200 to -51.
    Good = 6,
    /// 7: below -200.
    Saint = 7,
}

impl Legal {
    /// The level's number, the index into [`LEGAL_NAMES`] and the legal colours.
    pub fn index(self) -> usize {
        self as usize
    }

    /// The level's name as the game shows it.
    pub fn name(self) -> &'static str {
        LEGAL_NAMES[self.index()]
    }
}

/// The legal levels' names, by [`Legal::index`].
pub const LEGAL_NAMES: [&str; 8] = [
    "Neutral", "Seedy", "Outlaw", "Criminal", "Villain", "FIEND", "Good", "Saint",
];

/// A victim with this many points (Seedy or worse) costs nothing.
pub const EVIL_VICTIM_LIMIT: i16 = 30;
/// An attacker above this may not do evil.
pub const EVIL_ATTACKER_MAX: i16 = 300;
/// The points are not raised from this many.
pub const EVIL_POINTS_MAX: i16 = 30000;
/// A good character who does evil lands on at least this many points.
pub const EVIL_MIN_AFTER: i16 = 10;
/// The overflow guard of the scaled points.
pub const SCALE_LIMIT: i16 = 32000;
/// The `lawful` value of a character who chose Lawful.
pub const LAWFUL_FORCED: i8 = -10;
/// What a free act (0 points) costs against a lawful victim.
pub const LAWFUL_POINTS: i16 = 10;
/// From this many points a character is an Outlaw, guards attack, the evil reincarnation
/// room is used.
pub const OUTLAW_POINTS: i16 = 40;

/// The points the callers charge: 10 for an attack, 1 for a robbery (`rob_user`).
pub const ATTACK_POINTS: i16 = 10;
/// See [`ATTACK_POINTS`].
pub const ROBBERY_POINTS: i16 = 1;
/// An evil spell with no player victim (T-050).
pub const SPELL_POINTS: i16 = 10;
/// The rounds (slow ticks) every caller gives the timer.
pub const TIMER_ROUNDS: i8 = 11;

/// The kind of evil act `add_evil_points` charges.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 0: an attack (or an offensive spell).
    Attack,
    /// 1: a robbery the victim noticed.
    Robbery,
    /// 2: a robbery the victim is not shown the evil star for.
    NoStar,
}

impl Kind {
    /// The base cost its callers pass ([`ATTACK_POINTS`], [`ROBBERY_POINTS`]).
    pub fn base_points(self) -> i16 {
        match self {
            Kind::Attack => ATTACK_POINTS,
            Kind::Robbery | Kind::NoStar => ROBBERY_POINTS,
        }
    }
}

/// Why `add_evil_points` refused the act, in the order the game checks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Evil warnings are on: "To do this action, you must turn off your evil warnings."
    Warnings,
    /// More than [`EVIL_ATTACKER_MAX`] points: "You have progressed too far to the evil
    /// side to do this action."
    TooEvil,
    /// A lawful character while ENABLAWF: "You have chosen a way of life which does not
    /// allow this action."
    Lawful,
}

/// The attacker as `add_evil_points` reads them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Attacker {
    /// The attacker's evil points.
    pub evil: i16,
    /// The attacker chose Lawful ([`LAWFUL_FORCED`]) and ENABLAWF is on.
    pub lawful: bool,
    /// The attacker's evil warnings setting (`PlayerSettings::EVILWARN`) is on.
    pub warnings: bool,
}

/// The player victim as `add_evil_points` reads them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Victim {
    /// The victim's evil points.
    pub evil: i16,
    /// The victim chose Lawful ([`LAWFUL_FORCED`]) and ENABLAWF is on.
    pub lawful: bool,
}

/// What an act that goes ahead does to the attacker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Charge {
    /// The attacker's evil points afterwards.
    pub evil: i16,
    /// The points the timer records (0 for a free victim). With a
    /// victim this is the scaled cost even when the cap kept the points from rising.
    pub timer_points: i16,
    /// `true` when the act was charged: "A dark cloud passes over you" and the
    /// evil-added setting. `false` against an evil victim (timer only).
    pub charged: bool,
}

/// The legal level of `evil` points.
pub fn legal_level(evil: i16) -> Legal {
    match evil {
        e if e < -200 => Legal::Saint,
        e if e < -50 => Legal::Good,
        e if e < 30 => Legal::Neutral,
        e if e < 40 => Legal::Seedy,
        e if e < 80 => Legal::Outlaw,
        e if e < 120 => Legal::Criminal,
        e if e < 210 => Legal::Villain,
        _ => Legal::Fiend,
    }
}

/// `points` times `by`, or times [`SCALE_LIMIT`] when that product falls outside
/// `±SCALE_LIMIT`, 16-bit. (sic) The guard multiplies by 32000 where it meant to cap.
fn scale(points: i16, by: i16) -> i16 {
    let scaled = points.wrapping_mul(by);
    let factor = if !(-SCALE_LIMIT..=SCALE_LIMIT).contains(&scaled) {
        SCALE_LIMIT
    } else {
        by
    };
    points.wrapping_mul(factor)
}

/// [`scale`] with the Worldgroup module's 32-bit `int`.
fn scale32(points: i32, by: i32) -> i32 {
    let limit = i32::from(SCALE_LIMIT);
    let factor = if (-limit..=limit).contains(&points.wrapping_mul(by)) {
        by
    } else {
        limit
    };
    points.wrapping_mul(factor)
}

/// The cost of one evil act, the decision half of `add_evil_points` after the caller has
/// settled the timer question (`should_give_evil`): `points` is the base cost, `victim` the
/// player hurt or `None` (a monster, no scaling, no timer).
///
/// A victim with [`EVIL_VICTIM_LIMIT`] or more points is free: nothing is checked or
/// charged. Otherwise the refusals in their order (warnings, over 300, lawful), then the
/// victim scaling (lawful x3 or 10 for a free act, Saint x3, Good x2), the fall to
/// [`EVIL_MIN_AFTER`] and the add below [`EVIL_POINTS_MAX`] that never lowers the points.
///
/// `wide`: the Worldgroup module's 32-bit `int` (`game.worldgroup_module`). The scaling, the
/// fall and the never-lower test are worked in 32 bits there, and only the sum is cut back to
/// the 16-bit field.
pub fn cost(
    points: i16,
    attacker: Attacker,
    victim: Option<Victim>,
    wide: bool,
) -> Result<Charge, Refusal> {
    if let Some(v) = victim
        && v.evil >= EVIL_VICTIM_LIMIT
    {
        return Ok(Charge {
            evil: attacker.evil,
            timer_points: 0,
            charged: false,
        });
    }
    if attacker.warnings {
        return Err(Refusal::Warnings);
    }
    if attacker.evil > EVIL_ATTACKER_MAX {
        return Err(Refusal::TooEvil);
    }
    if attacker.lawful {
        return Err(Refusal::Lawful);
    }
    if wide {
        return Ok(cost32(points, attacker.evil, victim));
    }
    let mut points = points;
    if let Some(v) = victim {
        if v.lawful {
            points = if points == 0 {
                LAWFUL_POINTS
            } else {
                scale(points, 3)
            };
        } else if v.evil < -200 {
            points = scale(points, 3);
        } else if v.evil < -50 {
            points = scale(points, 2);
        }
    }
    let evil = attacker.evil;
    if evil < 0 && evil.wrapping_add(points) < EVIL_MIN_AFTER {
        points = EVIL_MIN_AFTER.wrapping_sub(evil);
    }
    let sum = evil.wrapping_add(points);
    let after = if evil < EVIL_POINTS_MAX && sum > evil {
        sum
    } else {
        evil
    };
    Ok(Charge {
        evil: after,
        timer_points: points,
        charged: true,
    })
}

/// The charge of [`cost`] past its refusals, worked as the Worldgroup module does.
fn cost32(points: i16, evil: i16, victim: Option<Victim>) -> Charge {
    let mut points = i32::from(points);
    if let Some(v) = victim {
        if v.lawful {
            points = if points == 0 {
                i32::from(LAWFUL_POINTS)
            } else {
                scale32(points, 3)
            };
        } else if v.evil < -200 {
            points = scale32(points, 3);
        } else if v.evil < -50 {
            points = scale32(points, 2);
        }
    }
    let have = i32::from(evil);
    if have < 0 && have.wrapping_add(points) < i32::from(EVIL_MIN_AFTER) {
        points = i32::from(EVIL_MIN_AFTER) - have;
    }
    let sum = have.wrapping_add(points);
    let after = if evil < EVIL_POINTS_MAX && sum > have {
        sum as i16
    } else {
        evil
    };
    Charge {
        evil: after,
        timer_points: points as i16,
        charged: true,
    }
}

/// The points `save_evil_points` keeps for the account's next character: none with
/// `evilkeep` 0, all of them when the character's last death was today, else
/// `evil * evilkeep / 100` (in 32 bits, then back to 16).
pub fn keep_on_death(evil: i16, keep_pct: i32, died_today: bool) -> i16 {
    if keep_pct == 0 {
        0
    } else if died_today {
        evil
    } else {
        (i64::from(evil) * i64::from(keep_pct) / 100) as i16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legal_level_boundaries() {
        // the boundaries on both sides
        let table = [
            (-201, Legal::Saint),
            (-200, Legal::Good),
            (-51, Legal::Good),
            (-50, Legal::Neutral),
            (29, Legal::Neutral),
            (30, Legal::Seedy),
            (39, Legal::Seedy),
            (40, Legal::Outlaw),
            (79, Legal::Outlaw),
            (80, Legal::Criminal),
            (119, Legal::Criminal),
            (120, Legal::Villain),
            (209, Legal::Villain),
            (210, Legal::Fiend),
            (i16::MIN, Legal::Saint),
            (i16::MAX, Legal::Fiend),
        ];
        for (evil, want) in table {
            assert_eq!(legal_level(evil), want, "{evil}");
        }
        assert_eq!(Legal::Fiend.name(), "FIEND");
        assert_eq!(Legal::Saint.index(), 7);
    }

    fn atk(evil: i16) -> Attacker {
        Attacker {
            evil,
            lawful: false,
            warnings: false,
        }
    }

    #[test]
    fn refusals_in_order() {
        let all = Attacker {
            evil: 301,
            lawful: true,
            warnings: true,
        };
        assert_eq!(cost(10, all, None, false), Err(Refusal::Warnings));
        let no_warn = Attacker {
            warnings: false,
            ..all
        };
        assert_eq!(cost(10, no_warn, None, false), Err(Refusal::TooEvil));
        let lawful = Attacker { evil: 0, ..no_warn };
        assert_eq!(cost(10, lawful, None, false), Err(Refusal::Lawful));
        // 300 is still allowed (`> EVIL_ATTACKER_MAX`)
        assert_eq!(cost(10, atk(300), None, false).map(|c| c.evil), Ok(310));
        // the same refusals with a victim who is not evil
        let v = Some(Victim {
            evil: 29,
            lawful: false,
        });
        assert_eq!(cost(10, all, v, false), Err(Refusal::Warnings));
    }

    #[test]
    fn an_evil_victim_is_free_whatever_the_attacker() {
        let all = Attacker {
            evil: 301,
            lawful: true,
            warnings: true,
        };
        let v = Some(Victim {
            evil: 30,
            lawful: false,
        });
        assert_eq!(
            cost(10, all, v, false),
            Ok(Charge {
                evil: 301,
                timer_points: 0,
                charged: false,
            })
        );
    }

    #[test]
    fn victim_scaling() {
        let v = |evil, lawful| Some(Victim { evil, lawful });
        // a plain victim: the base points
        assert_eq!(
            cost(10, atk(0), v(0, false), false).unwrap().timer_points,
            10
        );
        // Good x2, Saint x3, lawful x3 (lawful wins over Saint)
        assert_eq!(
            cost(10, atk(0), v(-51, false), false).unwrap().timer_points,
            20
        );
        assert_eq!(
            cost(10, atk(0), v(-201, false), false)
                .unwrap()
                .timer_points,
            30
        );
        assert_eq!(
            cost(10, atk(0), v(-201, true), false).unwrap().timer_points,
            30
        );
        assert_eq!(cost(1, atk(0), v(0, true), false).unwrap().timer_points, 3);
        // a free act against a lawful victim costs 10
        assert_eq!(cost(0, atk(0), v(0, true), false).unwrap().timer_points, 10);
        // (sic) the overflow guard multiplies by 32000
        assert_eq!(scale(11000, 3), 11000i16.wrapping_mul(32000));
        // no victim: no scaling
        assert_eq!(cost(10, atk(0), None, false).unwrap().evil, 10);
        // the Worldgroup module: the same for small points; 22000 x3 is 66000, which DOS
        // wraps to 464 inside the guard, while the 32-bit guard trips and the product of
        // 22000 x32000 keeps its low word, 12288
        assert_eq!(
            cost(10, atk(0), v(-201, false), true).unwrap().timer_points,
            30
        );
        assert_eq!(cost(0, atk(0), v(0, true), true).unwrap().timer_points, 10);
        let dos = cost(22000, atk(0), v(-201, false), false).unwrap();
        let wide = cost(22000, atk(0), v(-201, false), true).unwrap();
        assert_eq!((dos.evil, wide.evil), (464, 12288));
    }

    #[test]
    fn good_characters_fall_to_ten_and_the_cap_holds() {
        // a good character lands on 10
        let c = cost(10, atk(-100), None, false).unwrap();
        assert_eq!((c.evil, c.timer_points), (10, 110));
        // already at 0: plain add
        assert_eq!(cost(1, atk(0), None, false).unwrap().evil, 1);
        // not raised from 30000, never lowered by an overflow
        assert_eq!(cost(10, atk(30000), None, false), Err(Refusal::TooEvil));
        let c = cost(10, atk(299), None, false).unwrap();
        assert_eq!(c.evil, 309);
        assert!(c.charged);
    }

    #[test]
    fn kept_on_death() {
        assert_eq!(keep_on_death(100, 0, false), 0);
        assert_eq!(keep_on_death(100, 50, false), 50);
        assert_eq!(keep_on_death(100, 50, true), 100);
        assert_eq!(keep_on_death(-75, 100, false), -75);
        assert_eq!(keep_on_death(99, 33, false), 32);
    }
}
