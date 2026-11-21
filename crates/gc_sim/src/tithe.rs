//! Tithe (docs/03-factions.md, the Carrion Concord's meter): what a player
//! whose meter is [`Meter::Tithe`] collects from the dead, from 0 to
//! [`MAX_TITHE`], and spends on contracts (see [`crate::powers`]) and on
//! refilling their undead.
//!
//! - Every soldier who dies, on any side, within [`TITHE_TILES`] of one of
//!   their finished Reliquaries (a building that collects Tithe) pays them
//!   1 Tithe, however many of their Reliquaries are that near.
//! - The dead lie where they fell as corpses, counted in graves (at most
//!   [`MAX_GRAVES`] across the field; a soldier who falls within
//!   [`GRAVE_TILES`] of a grave joins it, and with every grave taken the
//!   oldest is dug over). A grave rots away [`ROT_TICKS`] after its last
//!   corpse fell. Only a match with a Tithe player keeps them.
//! - Raising (Raise the Bound, Indenture): a regiment of the power's unit
//!   rises at the chosen point from the corpses in the graves within the
//!   power's reach of it, as many soldiers as there are corpses up to the
//!   unit's full strength, nearest graves first; refused with fewer than
//!   [`MIN_RAISED`]. Raise the Bound's lasts its power's duration; an
//!   Indentured one stays, takes no supply, and crumbles like any undead.
//! - Hiring (Hire Militia): a regiment of the power's unit comes out of the
//!   map's hamlet nearest the chosen point, if one is within the power's
//!   reach of it and no enemy regiment stands within [`HAMLET_CLEAR`] of
//!   it, for the power's duration.
//! - Refilling: an undead regiment short of soldiers, standing quiet within
//!   [`TITHE_TILES`] of one of its owner's finished Reliquaries, takes a
//!   soldier every [`crate::reinforce::REINFORCE_SECONDS`] for
//!   [`tithe_per_soldier`] Tithe, as a living regiment does for Grub (see
//!   [`crate::reinforce`]).
//! - Crumbling: an undead regiment that isn't summoned, more than
//!   [`KEEP_TILES`] from every one of its owner's finished Reliquaries and
//!   main buildings, loses its last soldier every [`CRUMBLE_SECONDS`].
//!
//! Every player's Tithe and the graves are in the state hash.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::conquest::Conquest;
use crate::hash::StateHasher;
use crate::nav::Nav;
use crate::powers::Meter;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::structures::{Refusal, Role, Structures, TITHE};
use crate::veins::within;
use crate::{
    Formation, MAX_PARTIES, PlayerId, RegimentHandle, TICKS_PER_SECOND, UnitId, UnitStats,
};

/// The most Tithe a player holds.
pub const MAX_TITHE: u32 = 250;
/// How far from a Reliquary's edge a death pays it, and an undead
/// regiment standing quiet is refilled, in tiles.
pub const TITHE_TILES: i32 = 12;
/// How far from a Reliquary's or main building's edge its owner's undead
/// don't crumble, in tiles.
pub const KEEP_TILES: i32 = 30;
/// Seconds out of reach for each soldier an undead regiment crumbles.
pub const CRUMBLE_SECONDS: u8 = 60;
/// The most graves on the field.
pub const MAX_GRAVES: usize = 48;
/// How near a grave a soldier falls to lie in it, in tiles.
pub const GRAVE_TILES: i32 = 3;
/// How long after its last corpse a grave rots away (3 minutes).
pub const ROT_TICKS: u32 = 3 * 60 * TICKS_PER_SECOND;
/// The fewest corpses a raising raises.
pub const MIN_RAISED: u16 = 4;
/// How near a hamlet an enemy regiment keeps it from hiring out, in tiles.
pub const HAMLET_CLEAR: i32 = 10;

/// What a new soldier of an undead regiment costs in Tithe: one, and one
/// more for every 8 hit points past the first 8 a soldier of it has.
pub fn tithe_per_soldier(stats: &UnitStats) -> u32 {
    (u32::from(stats.hp_per_model) / 8).max(1)
}

/// Corpses lying together.
#[derive(Clone, Copy, Debug)]
struct Grave {
    at: FxVec2,
    dead: u16,
    /// The tick the last of them fell.
    since: u32,
}

/// Each player's Tithe and the field's graves.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tithe {
    points: [u8; MAX_PARTIES],
    graves: Vec<Grave>,
    /// Scratch: the finished Reliquaries standing, as (slot, owner),
    /// listed at the tick's first death.
    keepers: Vec<(u32, PlayerId)>,
}

impl Tithe {
    pub(crate) fn new(structures: usize) -> Tithe {
        Tithe {
            points: [0; MAX_PARTIES],
            graves: Vec::with_capacity(MAX_GRAVES),
            keepers: Vec::with_capacity(structures),
        }
    }

    /// `player`'s Tithe.
    pub(crate) fn points(&self, player: PlayerId) -> u32 {
        (self.points.get(usize::from(player))).map_or(0, |&p| u32::from(p))
    }

    /// `player` spends `points` of Tithe (at most what they hold).
    pub(crate) fn spend(&mut self, player: PlayerId, points: u32) {
        if let Some(p) = self.points.get_mut(usize::from(player)) {
            *p = u32::from(*p).saturating_sub(points) as u8;
        }
    }

    pub(crate) fn gain(&mut self, player: PlayerId, points: u32) {
        if let Some(p) = self.points.get_mut(usize::from(player)) {
            *p = (u32::from(*p) + points).min(MAX_TITHE) as u8;
        }
    }

    /// The corpses lying within `tiles` of `at`.
    pub(crate) fn corpses_near(&self, at: FxVec2, tiles: u8) -> u32 {
        let reach = reach_sq(i32::from(tiles));
        (self.graves.iter())
            .filter(|g| (g.at - at).length_sq_raw() <= reach)
            .map(|g| u32::from(g.dead))
            .sum()
    }

    /// The graves, as (where, corpses), oldest first.
    pub(crate) fn graves(&self) -> impl Iterator<Item = (FxVec2, u16)> + '_ {
        self.graves.iter().map(|g| (g.at, g.dead))
    }

    /// Up to `want` corpses within `tiles` of `at` are taken from their
    /// graves, nearest first: how many.
    pub(crate) fn dig(&mut self, at: FxVec2, tiles: u8, want: u16) -> u16 {
        let reach = reach_sq(i32::from(tiles));
        let mut taken = 0;
        while taken < want {
            let nearest = (self.graves.iter().enumerate())
                .filter(|(_, g)| g.dead > 0 && (g.at - at).length_sq_raw() <= reach)
                .min_by_key(|(k, g)| ((g.at - at).length_sq_raw(), *k))
                .map(|(k, _)| k);
            let Some(k) = nearest else { break };
            let g = &mut self.graves[k];
            let n = g.dead.min(want - taken);
            g.dead -= n;
            taken += n;
        }
        self.graves.retain(|g| g.dead > 0);
        taken
    }

    /// `dead` soldiers fell at `at` at `tick`: into the grave there, or a
    /// new one.
    fn bury(&mut self, at: FxVec2, dead: u16, tick: u32) {
        let near = reach_sq(GRAVE_TILES);
        if let Some(g) = (self.graves.iter_mut()).find(|g| (g.at - at).length_sq_raw() <= near) {
            g.dead = g.dead.saturating_add(dead);
            g.since = tick;
            return;
        }
        let grave = Grave {
            at,
            dead,
            since: tick,
        };
        if self.graves.len() < MAX_GRAVES {
            self.graves.push(grave);
        } else if let Some(oldest) = (self.graves.iter_mut()).min_by_key(|g| g.since) {
            *oldest = grave;
        }
    }

    /// The dead of tick `tick` lie down, and pay the Reliquaries near
    /// them; once a second the old graves rot. Nothing in a match with no
    /// Tithe player.
    pub(crate) fn collect(
        &mut self,
        meters: &[Meter; MAX_PARTIES],
        structures: &Structures,
        regs: &Regiments,
        tick: u32,
    ) {
        if !meters.contains(&Meter::Tithe) {
            return;
        }
        if tick.is_multiple_of(TICKS_PER_SECOND) {
            self.graves
                .retain(|g| tick.wrapping_sub(g.since) < ROT_TICKS);
        }
        let mut listed = false;
        for i in 0..regs.deaths.len() {
            let dead = regs.deaths[i];
            if dead == 0 {
                continue;
            }
            let at = regs.position[i];
            self.bury(at, dead, tick);
            if !listed {
                let tithed = |p: PlayerId| meters.get(usize::from(p)) == Some(&Meter::Tithe);
                let keepers = &mut self.keepers;
                keepers.clear();
                keepers.extend((0..structures.slots()).filter_map(|s| {
                    let owner = structures.owner_of(s);
                    let keeps = structures.stands(s)
                        && structures.finished(s)
                        && structures.stats_of(s).flags & TITHE != 0
                        && tithed(owner);
                    keeps.then_some((s as u32, owner))
                }));
                listed = true;
            }
            let mut paid = [false; MAX_PARTIES];
            for &(s, owner) in &self.keepers {
                let o = usize::from(owner);
                if !paid[o] && within(structures.footprint(s as usize), at, TITHE_TILES) {
                    paid[o] = true;
                }
            }
            for (p, _) in paid.iter().enumerate().filter(|(_, paid)| **paid) {
                self.gain(p as PlayerId, u32::from(dead));
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        for &p in &self.points {
            h.u8(p);
        }
        h.u32(self.graves.len() as u32);
        for g in &self.graves {
            h.i32(g.at.x.raw());
            h.i32(g.at.y.raw());
            h.u16(g.dead);
            h.u32(g.since);
        }
    }
}

/// `tiles` tiles, squared, as a raw squared length.
fn reach_sq(tiles: i32) -> u64 {
    let reach = u64::from(Fx::from_int(tiles).raw().unsigned_abs());
    reach * reach
}

impl Conquest {
    /// Whether `point` is within [`TITHE_TILES`] of one of `owner`'s
    /// finished Reliquaries: where their undead are refilled.
    pub(crate) fn tithes_at(&self, owner: PlayerId, point: FxVec2) -> bool {
        let st = &self.structures;
        st.finished_with(owner, TITHE)
            .any(|s| within(st.footprint(s), point, TITHE_TILES))
    }

    /// Whether `point` is within [`KEEP_TILES`] of one of `owner`'s
    /// finished Reliquaries or main buildings: where their undead don't
    /// crumble.
    pub(crate) fn keeps_at(&self, owner: PlayerId, point: FxVec2) -> bool {
        let st = &self.structures;
        st.finished_of(owner).any(|s| {
            let b = st.stats_of(s);
            (b.flags & TITHE != 0 || b.role == Role::Main)
                && within(st.footprint(s), point, KEEP_TILES)
        })
    }

    /// Once a second: every undead regiment out of its keepers' reach
    /// counts a second, and crumbles a soldier when its time is up (gone,
    /// with the last).
    pub(crate) fn crumble(&mut self, regs: &mut Regiments) {
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            let unit = self.unit(regs.unit[i]);
            if !unit.undead
                || regs.summoned_left[i] > 0
                || self.keeps_at(regs.owner[i], regs.position[i])
            {
                regs.crumble[i] = 0;
                continue;
            }
            regs.crumble[i] = regs.crumble[i].saturating_add(1);
            if regs.crumble[i] < CRUMBLE_SECONDS {
                continue;
            }
            regs.crumble[i] = 0;
            regs.crumble_one(i);
            if regs.models[i] == 0 {
                self.stop_work(i);
                regs.vanish_from_fight(i);
                regs.despawn(regs.handle(i));
            }
        }
    }

    /// `player` raises a regiment of `unit` from the corpses within
    /// `tiles` of `at`, for `lasts` ticks (0: for good). Refused (`Err`)
    /// with too few corpses or no room there.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn raise(
        &mut self,
        regs: &mut Regiments,
        nav: &Nav,
        units: &[UnitStats],
        stances: &Stances,
        player: PlayerId,
        (unit, tiles): (UnitId, u8),
        at: FxVec2,
        lasts: u16,
    ) -> Result<RegimentHandle, Refusal> {
        let mut stats = *units.get(usize::from(unit)).ok_or(Refusal::Unknown)?;
        let corpses = self.tithe.corpses_near(at, tiles);
        if corpses < u32::from(MIN_RAISED) {
            return Err(Refusal::NoCorpses);
        }
        stats.models = stats.models.min(u16::try_from(corpses).unwrap_or(u16::MAX));
        stats.files = stats.files.min(stats.models);
        let (spot, facing) =
            place(regs, nav, stances, &stats, player, [at].into_iter()).ok_or(Refusal::Crowded)?;
        let h = regs
            .spawn(player, unit, spot, facing, &stats)
            .ok_or(Refusal::NoRoom)?;
        regs.summoned_left[h.index as usize] = lasts;
        self.tithe.dig(at, tiles, stats.models);
        Ok(h)
    }

    /// The map's hamlet nearest `at`, within `tiles` of it, that no enemy
    /// of `player`'s holds: no hostile regiment within [`HAMLET_CLEAR`].
    pub(crate) fn hamlet_near(
        &self,
        regs: &Regiments,
        stances: &Stances,
        player: PlayerId,
        at: FxVec2,
        tiles: u8,
    ) -> Option<FxVec2> {
        let reach = reach_sq(i32::from(tiles));
        let clear = reach_sq(HAMLET_CLEAR);
        let held = |post: FxVec2| {
            (0..regs.alive.len()).any(|r| {
                regs.alive[r]
                    && stances.hostile(player, regs.owner[r])
                    && (regs.position[r] - post).length_sq_raw() <= clear
            })
        };
        (self.economy.posts().iter().copied())
            .filter(|&post| (post - at).length_sq_raw() <= reach)
            .min_by_key(|&post| (post - at).length_sq_raw())
            .filter(|&post| !held(post))
    }

    /// `player` hires a regiment of `unit` from the hamlet within `tiles`
    /// of `at` (see [`Conquest::hamlet_near`]), for `lasts` ticks. Refused
    /// (`Err`) with no such hamlet or no room by it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hire(
        &mut self,
        regs: &mut Regiments,
        nav: &Nav,
        units: &[UnitStats],
        stances: &Stances,
        player: PlayerId,
        (unit, tiles): (UnitId, u8),
        at: FxVec2,
        lasts: u16,
    ) -> Result<RegimentHandle, Refusal> {
        let stats = *units.get(usize::from(unit)).ok_or(Refusal::Unknown)?;
        let post = (self.hamlet_near(regs, stances, player, at, tiles)).ok_or(Refusal::NoHamlet)?;
        let (spot, facing) =
            place(regs, nav, stances, &stats, player, rings(post)).ok_or(Refusal::Crowded)?;
        let h = regs
            .spawn(player, unit, spot, facing, &stats)
            .ok_or(Refusal::NoRoom)?;
        regs.summoned_left[h.index as usize] = lasts;
        Ok(h)
    }
}

/// Spots round `centre`, eight to a ring, nearest ring first: 3 tiles out,
/// then every 3 more, to 12.
fn rings(centre: FxVec2) -> impl Iterator<Item = FxVec2> {
    (1..=4).flat_map(move |ring| {
        let out = 3 * ring;
        [
            (0, 1),
            (1, 1),
            (1, 0),
            (1, -1),
            (0, -1),
            (-1, -1),
            (-1, 0),
            (-1, 1),
        ]
        .map(|(x, y)| centre + FxVec2::new(Fx::from_int(x * out), Fx::from_int(y * out)))
    })
}

/// The first of `spots` where open ground holds a regiment of `stats` of
/// `player`'s whole block (a spot near blocked ground settled to ground
/// that does), facing the nearest enemy regiment (or north with none).
/// `None` if none does or the regiment pool is full.
fn place(
    regs: &Regiments,
    nav: &Nav,
    stances: &Stances,
    stats: &UnitStats,
    player: PlayerId,
    spots: impl Iterator<Item = FxVec2>,
) -> Option<(FxVec2, Angle)> {
    if regs.free.is_empty() {
        return None;
    }
    let berth = Regiments::berth_of(stats, Formation::Line, stats.files, stats.models);
    let at = spots
        .filter(|&at| nav.walkable(player, at))
        .map(|at| nav.settle(player, at, berth))
        .find(|&at| nav.fits(player, at, berth))?;
    let foe = (0..regs.alive.len())
        .filter(|&r| regs.alive[r] && stances.hostile(player, regs.owner[r]))
        .min_by_key(|&r| ((regs.position[r] - at).length_sq_raw(), r));
    let facing = foe.map_or(Angle::ZERO, |r| Angle::of_vec(regs.position[r] - at));
    Some((at, facing))
}

impl Regiments {
    /// Slot `i`'s last soldier crumbles away.
    pub(crate) fn crumble_one(&mut self, i: usize) {
        let last = usize::from(self.models[i]);
        if last == 0 {
            return;
        }
        let hp = self.hp[i * crate::MAX_MODELS as usize + last - 1];
        self.take_damage(i, u32::from(hp.max(1)));
    }
}
