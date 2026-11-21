//! Heroes (docs/43-heroes.md, faction depth stage 9): single levelling
//! characters, hired at their faction's hero building.
//!
//! - **A regiment of one.** A hero is a one-model regiment of a unit type
//!   whose [`ConquestUnit::hero`] growth is set. It is hired like any
//!   regiment is trained ([`crate::Command::Train`]) at the building its
//!   unit names, but a player may have only as many heroes as their tier
//!   (counting the living, the dead and those in training) and each hero
//!   once (see [`Heroes::check_hire`]).
//! - **Levels.** Every soldier hostile to a hero's player who dies within
//!   [`XP_TILES`] of the hero, whatever killed him, is worth his full hit
//!   points of experience, shared evenly between that player's heroes in
//!   reach (allies' heroes in reach share too). [`LEVELS`] says what each
//!   level needs. A level adds the unit's growth to the hero's hit points
//!   (healing them) and to its damage, and a skill point.
//! - **Leadership.** A standing hero leads its player's regiments within
//!   [`LEAD_TILES`] as a Field Battle general does (see
//!   [`crate::generals`]): [`Regiments::led`] is set for them, and morale
//!   and combat do the rest. Undead are led by the unit's undead flag, not
//!   by having no morale, so workers and heroes aren't driven harder.
//! - **The fall.** When a hero's regiment is gone its player's regiments
//!   within [`FALL_TILES`] lose [`FALL`] morale and its undead there fall
//!   into disorder. A fallen hero keeps its level and can be revived at a
//!   hero building of its owner's for a price and time that grow with its
//!   level (see [`revive_price`]); it takes no supply while dead.
//!
//! The hero list is in the state hash; the regiments' level bonuses are
//! regiment fields, hashed with them.

use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::combat::Combat;
use crate::conquest::ConquestUnit;
use crate::regiments::{Regiments, hash_handle};
use crate::stances::Stances;
use crate::structures::Refusal;
use crate::{MAX_PARTIES, PlayerId, RegimentHandle, StateHasher, UnitId, UnitStats};
use crate::{generals, morale};

/// The most heroes a player has: one for each tier.
pub const MAX_HEROES: usize = 3;
/// The highest level.
pub const MAX_LEVEL: u8 = 10;
/// Experience each level from the second needs, in hit points of enemy
/// soldiers dead near the hero.
pub const LEVELS: [u32; 9] = [200, 450, 750, 1100, 1500, 1950, 2450, 3000, 3600];
/// Enemy soldiers who die this close to a hero (centre to centre, in
/// tiles) give it experience.
pub const XP_TILES: i32 = 24;
/// Its player's regiments this close to a hero are led by it.
pub const LEAD_TILES: i32 = 16;
/// Its player's regiments this close to where a hero fell lose [`FALL`]
/// morale.
pub const FALL_TILES: i32 = 16;
pub const FALL: i16 = 10;
/// A revive costs the hire's price and takes its time, times this percent
/// plus [`REVIVE_LEVEL_PERCENT`] a level.
pub const REVIVE_PERCENT: u32 = 50;
pub const REVIVE_LEVEL_PERCENT: u32 = 15;

/// How a hero grows each level past the first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HeroGrowth {
    /// Hit points.
    pub hp: u16,
    /// Melee damage a blow.
    pub melee: u16,
    /// Missile damage a shot.
    pub missile: u16,
}

/// One hired hero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hero {
    pub owner: PlayerId,
    pub unit: UnitId,
    /// Its regiment, or `None` while it lies dead.
    pub regiment: Option<RegimentHandle>,
    /// 1 to [`MAX_LEVEL`].
    pub level: u8,
    pub xp: u32,
    /// Skill points not yet spent.
    pub points: u8,
    /// Where it stood at the end of the last tick, or where it fell.
    pub at: FxVec2,
}

impl Hero {
    /// Whether it lies dead.
    pub fn dead(&self) -> bool {
        self.regiment.is_none()
    }

    /// Experience its next level needs, if it isn't at the top.
    pub fn next_level_xp(&self) -> Option<u32> {
        LEVELS.get(usize::from(self.level.max(1)) - 1).copied()
    }

    /// Experience its level took (0 at level 1).
    pub fn level_start_xp(&self) -> u32 {
        usize::from(self.level.max(1))
            .checked_sub(2)
            .map_or(0, |k| LEVELS[k])
    }

    /// Add `xp` experience; the levels it gains.
    fn gain(&mut self, xp: u32) -> u8 {
        self.xp = self.xp.saturating_add(xp);
        let gained = level_of(self.xp).saturating_sub(self.level);
        self.level += gained;
        self.points = self.points.saturating_add(gained);
        gained
    }
}

/// The level `xp` earns.
pub fn level_of(xp: u32) -> u8 {
    1 + LEVELS.iter().filter(|&&t| xp >= t).count() as u8
}

/// What reviving a hero of type `u` at `level` costs (Grub, Marrow) and
/// how many ticks it takes at Standard tempo.
pub fn revive_price(u: &ConquestUnit, level: u8) -> (u32, u32, u32) {
    let percent = REVIVE_PERCENT + REVIVE_LEVEL_PERCENT * u32::from(level.max(1));
    let of = |n: u16| u32::from(n) * percent / 100;
    (of(u.cost_grub), of(u.cost_marrow), of(u.train_ticks).max(1))
}

/// What a hero's level adds to regiment slot `i`, set afresh: its hit
/// points per model past the unit's, and its damage in percent past the
/// unit's.
fn set_level(regs: &mut Regiments, i: usize, stats: &UnitStats, g: HeroGrowth, level: u8) {
    let past = u32::from(level.max(1) - 1);
    regs.hero_level[i] = level;
    regs.hp_bonus[i] = (u32::from(g.hp) * past).min(u32::from(u16::MAX)) as u16;
    let percent = |growth: u16, base: u16| -> u16 {
        if base == 0 {
            0
        } else {
            (u32::from(growth) * past * 100 / u32::from(base)).min(u32::from(u16::MAX)) as u16
        }
    };
    regs.melee_bonus[i] = percent(g.melee, stats.melee_damage);
    regs.missile_bonus[i] = percent(g.missile, stats.missile_damage);
}

/// Every player's heroes, in the order they were hired.
#[derive(Clone, Debug, Default)]
pub struct Heroes {
    list: Vec<Hero>,
    /// Scratch for sharing experience: the heroes in reach of one death.
    near: Vec<usize>,
}

impl Heroes {
    pub(crate) fn new() -> Heroes {
        Heroes {
            list: Vec::with_capacity(MAX_PARTIES * MAX_HEROES),
            near: Vec::with_capacity(MAX_PARTIES * MAX_HEROES),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Hero> {
        self.list.iter()
    }

    /// `player`'s hero of type `unit`, if they have hired it.
    pub fn find(&self, player: PlayerId, unit: UnitId) -> Option<&Hero> {
        self.list
            .iter()
            .find(|h| h.owner == player && h.unit == unit)
    }

    /// The hero whose regiment is `r`, if it is one.
    pub fn of_regiment(&self, r: RegimentHandle) -> Option<&Hero> {
        self.list.iter().find(|h| h.regiment == Some(r))
    }

    /// Whether `player` may queue a hero of type `unit` now (to hire it,
    /// or to revive their fallen one), with `queued` heroes of theirs in
    /// training and `this_queued` of them of that type, at tier `tier`.
    pub(crate) fn check_hire(
        &self,
        player: PlayerId,
        unit: UnitId,
        tier: u8,
        queued: usize,
        this_queued: bool,
    ) -> Result<(), Refusal> {
        if this_queued {
            return Err(Refusal::Busy);
        }
        match self.find(player, unit) {
            Some(h) if h.dead() => Ok(()),
            Some(_) => Err(Refusal::HeroTaken),
            None => {
                let have = self.list.iter().filter(|h| h.owner == player).count() + queued;
                if have >= usize::from(tier).min(MAX_HEROES) {
                    Err(Refusal::HeroLimit)
                } else {
                    Ok(())
                }
            }
        }
    }

    /// A hero of type `unit` came out of training for `owner` as regiment
    /// `r`: a new hero at level 1, or their fallen one back, at its level
    /// and full hit points.
    pub(crate) fn raised(
        &mut self,
        regs: &mut Regiments,
        stats: &UnitStats,
        growth: HeroGrowth,
        owner: PlayerId,
        unit: UnitId,
        r: RegimentHandle,
    ) {
        let Some(i) = regs.resolve(r) else {
            return;
        };
        let at = regs.position[i];
        let k = match self
            .list
            .iter()
            .position(|h| h.owner == owner && h.unit == unit)
        {
            Some(k) => k,
            None => {
                self.list.push(Hero {
                    owner,
                    unit,
                    regiment: None,
                    level: 1,
                    xp: 0,
                    points: 1,
                    at,
                });
                self.list.len() - 1
            }
        };
        let h = &mut self.list[k];
        h.regiment = Some(r);
        h.at = at;
        set_level(regs, i, stats, growth, h.level);
        let full = stats.hp_per_model.saturating_add(regs.hp_bonus[i]);
        for k in 0..regs.models[i] {
            *regs.hp_mut(i, k as u8) = full;
        }
    }

    /// The tick's dead pay their experience to the heroes near them.
    /// Before the dead are counted off ([`Regiments::wear_powers`]).
    pub(crate) fn earn(
        &mut self,
        regs: &mut Regiments,
        stats: &[UnitStats],
        units: &[ConquestUnit],
        stances: &Stances,
    ) {
        if self.list.iter().all(Hero::dead) {
            return;
        }
        for j in 0..regs.deaths.len() {
            let dead = regs.deaths[j];
            if dead == 0 {
                continue;
            }
            let hp = stats
                .get(usize::from(regs.unit[j]))
                .map_or(1, |s| s.hp_per_model);
            let worth = u32::from(dead) * u32::from(hp);
            let (at, victim) = (regs.position[j], regs.owner[j]);
            self.near.clear();
            self.near
                .extend(self.list.iter().enumerate().filter_map(|(k, h)| {
                    let near = !h.dead() && stances.hostile(h.owner, victim);
                    (near && generals::near(h.at, at, XP_TILES)).then_some(k)
                }));
            let Some(share) = worth.checked_div(self.near.len() as u32) else {
                continue;
            };
            for &k in &self.near {
                let h = &mut self.list[k];
                let gained = h.gain(share);
                if gained > 0 {
                    level_up(regs, stats, units, h, gained);
                }
            }
        }
    }

    /// Once a tick, before the morale checks: see who has fallen, and mark
    /// the regiments each standing hero leads ([`Regiments::led`]).
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        stats: &[UnitStats],
        units: &[ConquestUnit],
        stances: &Stances,
    ) {
        let mut led = core::mem::take(&mut regs.led);
        for h in &mut self.list {
            let Some(r) = h.regiment else {
                continue;
            };
            let Some(i) = regs.resolve(r) else {
                h.regiment = None;
                fall(regs, stats, units, h);
                continue;
            };
            h.at = regs.position[i];
            if regs.morale_state[i].broken() || regs.cocoon_left[i] > 0 {
                continue;
            }
            generals::each_led(regs, combat, stances, i, LEAD_TILES, |j| {
                if leadable(regs, stats, units, j) {
                    led[j] = true;
                }
            });
        }
        regs.led = led;
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.list.len() as u32);
        for hero in &self.list {
            h.u8(hero.owner);
            h.u16(hero.unit);
            hash_handle(h, hero.regiment);
            h.u8(hero.level);
            h.u32(hero.xp);
            h.u8(hero.points);
            h.i32(hero.at.x.raw());
            h.i32(hero.at.y.raw());
        }
    }
}

/// Hero `h`, in regiment slot `i` if it stands, gained `gained` levels:
/// their hit points come whole, and its blows land harder.
fn level_up(
    regs: &mut Regiments,
    stats: &[UnitStats],
    units: &[ConquestUnit],
    h: &Hero,
    gained: u8,
) {
    let grow = grow_by(units, h.unit);
    let (Some(i), Some(s)) = (
        h.regiment.and_then(|r| regs.resolve(r)),
        stats.get(usize::from(h.unit)),
    ) else {
        return;
    };
    set_level(regs, i, s, grow, h.level);
    let more = grow.hp.saturating_mul(u16::from(gained));
    for m in 0..regs.models[i] {
        let hp = regs.hp_mut(i, m as u8);
        *hp = hp.saturating_add(more);
    }
}

/// Unit type `unit`'s growth (none for a type that isn't a hero).
fn grow_by(units: &[ConquestUnit], unit: UnitId) -> HeroGrowth {
    units
        .get(usize::from(unit))
        .and_then(|u| u.hero)
        .unwrap_or_default()
}

/// Whether regiment `j` can be led: it has morale to steady, or it is
/// undead (driven harder instead). Workers and heroes, with no morale and
/// not undead, aren't.
fn leadable(regs: &Regiments, stats: &[UnitStats], units: &[ConquestUnit], j: usize) -> bool {
    let unit = usize::from(regs.unit[j]);
    units.get(unit).is_some_and(|u| u.undead) || stats.get(unit).is_some_and(|s| s.morale > 0)
}

/// Hero `h` has fallen where it last stood: its player's regiments near
/// there lose heart, and its undead there fall into disorder.
/// Once a hero's life, so a pass over the pool.
fn fall(regs: &mut Regiments, stats: &[UnitStats], units: &[ConquestUnit], h: &Hero) {
    for j in 0..regs.alive.len() {
        let near = regs.alive[j] && regs.owner[j] == h.owner;
        if !near || !generals::near(regs.position[j], h.at, FALL_TILES) {
            continue;
        }
        if units
            .get(usize::from(regs.unit[j]))
            .is_some_and(|u| u.undead)
        {
            regs.disorder(j);
        } else if leadable(regs, stats, units, j) {
            morale::shake(regs, j, FALL, morale::WHY_GENERAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_the_table() {
        assert_eq!(level_of(0), 1);
        assert_eq!(level_of(199), 1);
        assert_eq!(level_of(200), 2);
        assert_eq!(level_of(3599), 9);
        assert_eq!(level_of(3600), 10);
        assert_eq!(level_of(u32::MAX), 10);
    }

    #[test]
    fn a_revive_costs_more_each_level() {
        let u = ConquestUnit {
            cost_grub: 200,
            cost_marrow: 50,
            train_ticks: 800,
            ..ConquestUnit::default()
        };
        assert_eq!(revive_price(&u, 1), (130, 32, 520));
        assert_eq!(revive_price(&u, 10), (400, 100, 1600));
    }
}
