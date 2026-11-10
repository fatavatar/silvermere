//! A monster's average damage per round: NMR's combat simulation.
//!
//! NMR fights each monster against a target with no armour class, dodge or damage resistance and
//! 50 magic resistance until the average settles (it compares the running average every 1,000
//! rounds and stops when it moved less than 1%), then rounds it to a tenth. Spell attacks are
//! cast at their slot's level, duration spells tick every round and again every third, and a
//! failed or resisted cast costs half its energy.
//!
//! NMR draws from the clock-seeded generator, so its averages differ a little from one export to
//! the next; the port seeds the same generator with the monster's number, which gives the same
//! averages every time (see [`super::vb::Rnd`]).
//!
//! One simulator serves every monster, in number order, and like NMR's it keeps a few values
//! from the monster before: the duration of an attack slot the current monster does not set,
//! and the physical and spell averages of a monster with nothing to simulate (no energy, or no
//! damage anywhere). MME shows those carried-over averages, so the port keeps them.

use silvermere_types::records::KnownMonster;

use super::vb::{self, Rnd};
use super::world::{World, signed};

/// What the simulation found for one monster.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(super) struct Damage {
    /// Average damage per round.
    pub total: f64,
    /// Of it, physical.
    pub phys: f64,
    /// Of it, from spells.
    pub spell: f64,
    /// Each attack slot's share of the attacks attempted, in percent (`AttTrue%`).
    pub share: [f64; 5],
}

/// An attack slot as the simulator runs it.
#[derive(Debug, Clone, Copy, Default)]
struct Attack {
    kind: i32,
    energy: i32,
    min: i32,
    max: i32,
    /// Cumulative percent chance.
    chance: i32,
    /// Hit chance of a spell attack (physical attacks always hit 99%).
    success: i32,
    hit_min: i32,
    hit_max: i32,
    resist: i32,
}

/// A spell cast between rounds.
#[derive(Debug, Clone, Copy, Default)]
struct Between {
    min: i32,
    max: i32,
    chance: i32,
    resist: i32,
}

/// A duration spell in effect.
#[derive(Debug, Clone, Copy, Default)]
struct Active {
    ticks: i32,
    left: i32,
    value: i32,
}

impl Active {
    /// Its damage this tick; counts the tick down and ends it when it runs out.
    fn tick(&mut self, count_ticks: bool) -> i32 {
        let value = self.value;
        self.left -= 1;
        if count_ticks {
            self.ticks = if self.ticks >= 3 { 1 } else { self.ticks + 1 };
        }
        if self.left < 1 {
            *self = Self::default();
        }
        value
    }
}

/// The target's magic resistance.
const TARGET_MR: i32 = 50;
/// Rounds the simulation stops at when the average never settles.
const MAX_ROUNDS: i32 = 100_000;

/// The simulator, carried from one monster to the next.
#[derive(Default)]
pub(super) struct Simulator {
    attacks: [Attack; 5],
    /// Not cleared between monsters.
    durations: [i32; 5],
    between: [Between; 5],
    /// Not cleared between monsters.
    between_durations: [i32; 5],
    energy: i32,
    /// Not cleared between monsters.
    phys: f64,
    /// Not cleared between monsters.
    spell: f64,
}

impl Simulator {
    /// Simulates monster `m`.
    pub(super) fn run(&mut self, world: &World, m: &KnownMonster) -> Damage {
        self.attacks = Default::default();
        self.between = Default::default();
        self.energy = i32::from(signed(m.energy));
        self.set_up(world, m);
        let mut rnd = Rnd::new(m.number.0 as u32);
        let mut fight = Fight::default();
        let total = self.fight(&mut fight, &mut rnd);
        let attempts: f64 = fight.attempted.iter().sum();
        let share = std::array::from_fn(|x| {
            if fight.attempted[x] > 0.0 && attempts > 0.0 {
                vb::round(fight.attempted[x] / attempts, 3) * 100.0
            } else {
                0.0
            }
        });
        Damage {
            total,
            phys: self.phys,
            spell: self.spell,
            share,
        }
    }

    /// Fills the attack and between-round slots from the monster, its weapon and drops and the
    /// spells they cast.
    fn set_up(&mut self, world: &World, m: &KnownMonster) {
        let damage_bonus = item_bonus(world, m, &[4]);
        let accuracy_bonus = item_bonus(world, m, &[22, 105, 106]);
        for (x, a) in m.attacks.iter().enumerate() {
            let kind = i32::from(a.kind.code());
            if !(1..=3).contains(&kind) {
                continue;
            }
            let slot = &mut self.attacks[x];
            slot.kind = kind;
            slot.energy = a.energy.into();
            slot.chance = a.pct.into();
            if kind == 2 {
                let n = i32::from(a.accuracy);
                let level = i32::from(a.max);
                if let Some(spell) = world.spell(n) {
                    // an area spell doing plain damage from an attack slot is never cast
                    if spell.target.code() == 12 && world.spell_ability(n, 1) > -1 {
                        self.durations[x] = 0;
                        slot.energy = 0;
                        continue;
                    }
                    slot.resist = spell.resmode.code().into();
                    self.durations[x] = world.spell_duration(n, level);
                    (slot.min, slot.max) = spell_damage(world, n, level).unwrap_or((0, 0));
                }
                slot.success = a.min.into();
            } else {
                slot.min = i32::from(a.min) + damage_bonus;
                slot.max = i32::from(a.max) + damage_bonus;
                slot.success = i32::from(a.accuracy) + accuracy_bonus;
                let n = i32::from(a.hitspell.0);
                if n > 0
                    && let Some(spell) = world.spell(n)
                {
                    slot.resist = spell.resmode.code().into();
                    self.durations[x] = world.spell_duration(n, 0);
                    if world.spell_ability(n, 1) >= 0 || world.spell_ability(n, 17) >= 0 {
                        slot.hit_min = world.spell_min(n, 0);
                        slot.hit_max = world.spell_max(n, 0);
                    }
                }
            }
        }
        for (x, s) in m.spells.iter().enumerate() {
            let n = i32::from(s.spell.0);
            let Some(spell) = (n > 0).then(|| world.spell(n)).flatten() else {
                continue;
            };
            let level = i32::from(s.level);
            let slot = &mut self.between[x];
            slot.resist = spell.resmode.code().into();
            slot.chance = s.pct.into();
            self.between_durations[x] = world.spell_duration(n, level);
            if let Some((min, max)) = spell_damage(world, n, level) {
                (slot.min, slot.max) = (min, max);
            }
        }
    }

    /// Runs rounds until the average settles; the average damage per round.
    fn fight(&mut self, f: &mut Fight, rnd: &mut Rnd) -> f64 {
        if self.energy < 1 {
            return 0.0;
        }
        let anything = self
            .attacks
            .iter()
            .any(|a| a.min > 0 || a.max > 0 || a.hit_min > 0 || a.hit_max > 0)
            || self.between.iter().any(|b| b.min > 0 || b.max > 0);
        if !anything {
            return 0.0;
        }
        let mut energy = 0;
        let mut checked_average = 0.0;
        let mut since_check = 0;
        let mut round = 1;
        while round <= MAX_ROUNDS {
            // a duration spell's second tick in a round, every third round
            for x in 0..5 {
                let mut damage = 0;
                for active in [&mut f.attack_spells[x], &mut f.between_spells[x]] {
                    if active.left > 0 && active.ticks == 3 {
                        damage += active.tick(false);
                    }
                }
                f.add_spell(damage);
            }
            energy += self.energy;
            self.attack_round(f, rnd, &mut energy);
            self.between_round(f, rnd);
            for x in 0..5 {
                let mut damage = 0;
                for active in [&mut f.attack_spells[x], &mut f.between_spells[x]] {
                    if active.left > 0 {
                        damage += active.tick(true);
                    }
                }
                f.add_spell(damage);
            }
            if since_check > 1000 {
                let average = vb::round(f.total / f64::from(round), 3);
                if checked_average == 0.0 {
                    checked_average = average;
                    if checked_average < 1.0 {
                        break;
                    }
                } else if vb::money((1.0 - average / checked_average).abs()) < 0.01 {
                    break;
                } else {
                    checked_average = average;
                }
                since_check = 0;
            }
            since_check += 1;
            round += 1;
        }
        let rounds = f64::from(round);
        self.phys = vb::round(f.phys / rounds, 1);
        self.spell = vb::round(f.spell / rounds, 1);
        vb::round(f.total / rounds, 1)
    }

    /// Up to six attacks, each picked by one roll against the slots' cumulative chances.
    fn attack_round(&self, f: &mut Fight, rnd: &mut Rnd, energy: &mut i32) {
        for _ in 0..6 {
            let roll = rnd.between(1, 100);
            let mut last: Option<&Attack> = None;
            for x in 0..5 {
                let a = &self.attacks[x];
                if !(roll <= a.chance && a.kind > 0) {
                    continue;
                }
                last = Some(a);
                if *energy >= a.energy && !self.attack(f, rnd, energy, x) {
                    // the spell is already in effect at this strength: the next slot that
                    // the same roll reaches attacks instead
                    continue;
                }
                break;
            }
            // too little energy left for a physical attack ends the round
            if let Some(a) = last
                && a.kind == 1
                && *energy < a.energy
            {
                break;
            }
        }
    }

    /// One attack from slot `x`; false when it is not made after all.
    fn attack(&self, f: &mut Fight, rnd: &mut Rnd, energy: &mut i32, x: usize) -> bool {
        let a = &self.attacks[x];
        let duration = self.durations[x];
        f.attempted[x] += 1.0;
        let chance = if a.kind == 2 { a.success } else { 99 };
        let mut hit = rnd.between(1, 100) <= chance;
        let mut resisted = false;
        let mut damage = 0;
        let mut spell_damage = 0;
        if hit {
            damage = rnd.between(a.min, a.max);
            if a.kind == 2 {
                if duration > 0 {
                    let active = &mut f.attack_spells[x];
                    if active.left >= 1 && active.value == damage {
                        f.attempted[x] -= 1.0;
                        return false;
                    }
                    if resists(rnd, a.resist) {
                        resisted = true;
                        hit = false;
                    } else {
                        *active = Active {
                            ticks: 0,
                            left: duration,
                            value: damage,
                        };
                    }
                    damage = 0;
                } else if resists(rnd, a.resist) {
                    hit = false;
                    damage = 0;
                }
            } else if a.hit_min > 0 || a.hit_max > 0 || duration > 0 {
                spell_damage = rnd.between(a.hit_min, a.hit_max);
                if duration > 0 {
                    let active = &mut f.attack_spells[x];
                    // a hit spell already in effect at this strength is not cast again
                    if (active.left < 1 || active.value != spell_damage) && !resists(rnd, a.resist)
                    {
                        *active = Active {
                            ticks: 0,
                            left: duration,
                            value: spell_damage,
                        };
                    }
                    // its damage comes as it ticks
                    spell_damage = 0;
                } else if resists(rnd, a.resist) {
                    spell_damage = 0;
                }
            }
            damage = damage.max(0);
        }
        if hit {
            f.total += f64::from(damage + spell_damage);
            f.spell += f64::from(spell_damage);
            if a.kind == 2 {
                f.spell += f64::from(damage);
            } else {
                f.phys += f64::from(damage);
            }
            *energy -= a.energy;
        } else if a.kind != 2 {
            *energy -= a.energy;
        } else if duration == 0 || resisted {
            *energy -= vb::whole(f64::from(a.energy) / 2.0);
        }
        true
    }

    /// The spell cast after the round, if the roll reaches one.
    fn between_round(&self, f: &mut Fight, rnd: &mut Rnd) {
        let roll = rnd.between(1, 100);
        for x in 0..5 {
            let b = &self.between[x];
            if b.chance <= 0 || roll > b.chance {
                continue;
            }
            let damage = rnd.between(b.min, b.max);
            let duration = self.between_durations[x];
            if duration > 0 {
                let active = &mut f.between_spells[x];
                if active.left >= 1 && active.value == damage {
                    continue;
                }
                if !resists(rnd, b.resist) {
                    *active = Active {
                        ticks: 0,
                        left: duration,
                        value: damage,
                    };
                }
            } else if !resists(rnd, b.resist) {
                f.total += f64::from(damage);
                f.spell += f64::from(damage);
            }
            break;
        }
    }
}

/// The running totals of one simulation.
#[derive(Default)]
struct Fight {
    attempted: [f64; 5],
    total: f64,
    phys: f64,
    spell: f64,
    attack_spells: [Active; 5],
    between_spells: [Active; 5],
}

impl Fight {
    fn add_spell(&mut self, damage: i32) {
        self.total += f64::from(damage);
        self.spell += f64::from(damage);
    }
}

/// Whether a spell is resisted: only a spell resisted by magic resistance can be, half the
/// target's MR in percent. The target has 50, which also takes nothing off damage.
fn resists(rnd: &mut Rnd, resist: i32) -> bool {
    resist == 2 && f64::from(rnd.between(1, 100)) <= f64::from(TARGET_MR) / 2.0
}

/// A spell's damage range at a cast level, from the last of its damage, damage-less-MR and drain
/// abilities: the ability's value when it has one, else the spell's scaled range.
fn spell_damage(world: &World, n: i32, level: i32) -> Option<(i32, i32)> {
    let mut range = None;
    for ability in [1, 17, 8] {
        let value = world.spell_ability(n, ability);
        if value > 0 {
            range = Some((value, value));
        } else if value == 0 {
            range = Some((world.spell_min(n, level), world.spell_max(n, level)));
        }
    }
    range
}

/// The bonus to the monster's attacks from abilities on its unlimited weapon and drops; a drop
/// counts in proportion to its chance (capped at 100%), rounded as it is added.
fn item_bonus(world: &World, m: &KnownMonster, abilities: &[u16]) -> i32 {
    let mut bonus = 0;
    let weapon = m.weapon.0;
    if weapon > 0 && world.item_limit(weapon) == 0 {
        for &ability in abilities {
            let value = world.item_ability(weapon, ability);
            if value > 0 {
                bonus += value;
            }
        }
    }
    for drop in &m.items {
        let n = drop.item.0;
        if n <= 0 || world.item_limit(n) != 0 {
            continue;
        }
        for &ability in abilities {
            let value = world.item_ability(n, ability);
            if value > 0 {
                let pct = f64::from(drop.pct.min(100)) / 100.0;
                bonus = vb::whole(f64::from(bonus) + f64::from(value) * pct);
            }
        }
    }
    bonus
}
