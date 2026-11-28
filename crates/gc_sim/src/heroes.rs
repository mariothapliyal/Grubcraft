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
//!   (healing them) and to its damage, and ranks up its abilities (see
//!   [`crate::abilities`]).
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

use gc_fixed::{Fx, FxVec2};

use crate::abilities::{HeroKit, MAX_ZONES, Pending, Zone};
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

/// Morale a regiment loses for each soldier a hero kills in melee
/// (`morale::WHY_HERO`): a hero is a shock weapon, breaking lines it cuts
/// into, though it can be pulled down.
pub const HERO_SHOCK: i16 = 4;
/// Melee blows on a hero land this many percent as hard: only a few men of
/// a line reach a one-man block, so without it a hero shrugs off whole
/// regiments.
pub const HERO_EXPOSED_PERCENT: u64 = 130;
/// How far a hero cutting its way out must get from every enemy regiment's
/// block before it marches on as usual, in tiles.
pub const CLEAR_TILES: i32 = 4;

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
    /// Where it stood at the end of the last tick, or where it fell.
    pub at: FxVec2,
    /// Which hero it is, for its abilities (see [`crate::abilities`]).
    pub kit: Option<HeroKit>,
    /// The tick each of its two abilities can be cast again from.
    pub ready_at: [u32; 2],
    /// A cast waiting for it to walk into reach.
    pub pending: Option<Pending>,
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
        gained
    }
}

/// Hero slot `i`'s soldier's hit points at full health, with its levels'
/// growth.
pub(crate) fn full_hp(regs: &Regiments, stats: &UnitStats, i: usize) -> u16 {
    stats.hp_per_model.saturating_add(regs.hp_bonus[i])
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
    /// Abilities pulsing on the field (see [`crate::abilities`]).
    pub(crate) zones: Vec<Zone>,
    /// Scratch for sharing experience: the heroes in reach of one death.
    near: Vec<usize>,
}

impl Heroes {
    pub(crate) fn new() -> Heroes {
        Heroes {
            list: Vec::with_capacity(MAX_PARTIES * MAX_HEROES),
            zones: Vec::with_capacity(MAX_ZONES),
            near: Vec::with_capacity(MAX_PARTIES * MAX_HEROES),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Hero> {
        self.list.iter()
    }

    /// Whether nobody has a hero.
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// Why `player` can't put hero `unit` on the field now
    /// ([`crate::Command::SpawnHero`]): they have it already, standing or
    /// fallen, or as many heroes as anyone may.
    pub(crate) fn check_place(&self, player: PlayerId, unit: UnitId) -> Result<(), Refusal> {
        if self.find(player, unit).is_some() {
            return Err(Refusal::HeroTaken);
        }
        if self.list.iter().filter(|h| h.owner == player).count() >= MAX_HEROES {
            return Err(Refusal::HeroLimit);
        }
        Ok(())
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

    /// Where in the list the hero whose regiment is `r` is.
    pub(crate) fn index_of(&self, r: RegimentHandle) -> Option<usize> {
        self.list.iter().position(|h| h.regiment == Some(r))
    }

    pub(crate) fn get(&self, k: usize) -> &Hero {
        &self.list[k]
    }

    pub(crate) fn get_mut(&mut self, k: usize) -> &mut Hero {
        &mut self.list[k]
    }

    /// The abilities pulsing on the field.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    /// Start zone `z`: false with [`MAX_ZONES`] already pulsing.
    pub(crate) fn add_zone(&mut self, z: Zone) -> bool {
        let room = self.zones.len() < MAX_ZONES;
        if room {
            self.zones.push(z);
        }
        room
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
    /// `r`, or was put on the field at `level` (a Field Battle's,
    /// [`crate::Command::SpawnHero`]): a new hero at `level`, with the
    /// experience that level took, or their fallen one back, at its own
    /// level; at full hit points.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn raised(
        &mut self,
        regs: &mut Regiments,
        stats: &UnitStats,
        (growth, kit): (HeroGrowth, Option<HeroKit>),
        owner: PlayerId,
        unit: UnitId,
        r: RegimentHandle,
        level: u8,
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
                let level = level.clamp(1, MAX_LEVEL);
                let xp = usize::from(level).checked_sub(2).map_or(0, |k| LEVELS[k]);
                self.list.push(Hero {
                    owner,
                    unit,
                    regiment: None,
                    level,
                    xp,
                    at,
                    kit,
                    ready_at: [0; 2],
                    pending: None,
                });
                self.list.len() - 1
            }
        };
        let h = &mut self.list[k];
        h.regiment = Some(r);
        h.pending = None;
        h.at = at;
        set_level(regs, i, stats, growth, h.level);
        let full = full_hp(regs, stats, i);
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
            // Summoned and raised regiments are free: they feed no hero.
            if regs.summoned_left[j] > 0 {
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
                    // Not one that fell this tick (seen in `update`, after).
                    let standing = h.regiment.and_then(|r| regs.resolve(r)).is_some();
                    let near = standing && stances.hostile(h.owner, victim);
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
    /// the regiments each standing hero leads ([`Regiments::led`]). A hero
    /// who is his side's general (`generals`, in a Field Battle) fell as
    /// one, which his side felt already.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        combat: &Combat,
        stats: &[UnitStats],
        units: &[ConquestUnit],
        stances: &Stances,
        generals: Option<&generals::Generals>,
    ) {
        let mut led = core::mem::take(&mut regs.led);
        for h in &mut self.list {
            let Some(r) = h.regiment else {
                continue;
            };
            let Some(i) = regs.resolve(r) else {
                h.regiment = None;
                // A cast it was walking to dies with it.
                h.pending = None;
                if !generals.is_some_and(|g| g.hero_general(r)) {
                    fall(regs, stats, units, h);
                }
                continue;
            };
            h.at = regs.position[i];
            // Clear of the fight it cut its way out of, or out of time to
            // (its fall back would end the march where it stands): on to
            // where it was sent, as a march.
            if regs.breaking_out[i]
                && (regs.falling_back[i] <= 1 || clear_of_foes(regs, stats, stances, i))
            {
                regs.end_break_out(i);
            }
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
            h.i32(hero.at.x.raw());
            h.i32(hero.at.y.raw());
            HeroKit::hash_into(hero.kit, h);
            h.u32(hero.ready_at[0]);
            h.u32(hero.ready_at[1]);
            match hero.pending {
                Some(p) => {
                    h.u8(1 + p.slot);
                    p.target.hash_into(h);
                    match p.toward {
                        Some(t) => {
                            h.u8(1);
                            h.i32(t.x.raw());
                            h.i32(t.y.raw());
                        }
                        None => h.u8(0),
                    }
                }
                None => h.u8(0),
            }
        }
        h.u32(self.zones.len() as u32);
        for z in &self.zones {
            z.hash_into(h);
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

/// Whether every enemy regiment's block is more than [`CLEAR_TILES`] from
/// hero regiment `i`'s middle.
fn clear_of_foes(regs: &Regiments, stats: &[UnitStats], stances: &Stances, i: usize) -> bool {
    let near = Fx::from_int(CLEAR_TILES);
    let (at, owner) = (regs.position[i], regs.owner[i]);
    !(0..regs.alive.len()).any(|j| {
        if !regs.alive[j] || !stances.hostile(owner, regs.owner[j]) {
            return false;
        }
        let half = stats.get(usize::from(regs.unit[j])).map_or(Fx::ZERO, |u| {
            let (w, d) = u.block_size_in(regs.formation[j], regs.files[j], regs.models[j]);
            w.max(d) / 2
        });
        (regs.position[j] - at).length() <= near + half
    })
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
