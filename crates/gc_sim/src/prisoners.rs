//! Prisoners after battles (docs/45-prisoners.md).
//!
//! - Once a second, after the morale checks and in slot order, a routing or
//!   shattered regiment that has run at least [`RUN_SECONDS`] gives itself
//!   up when an enemy regiment (not a
//!   worker or caravan) touches its block at melee reach and no fighting
//!   regiment of its own side is within [`ALONE_TILES`]. Its soldiers are
//!   prisoners of the touching enemy with the lowest slot's owner, and it
//!   leaves the field at once: not killed, so it plants no standard, gives
//!   no experience or Tithe and counts as no kills
//!   ([`crate::events::Event::Surrendered`]).
//! - Never: regiments that can't rout, non-fighters (Caravans), heroes,
//!   regiments a general rides with, summoned ones, and in Conquest any
//!   unit nothing trains.
//! - In every mode the soldiers each player took are counted. In Conquest
//!   they are held at the captor's finished main building or outpost hall
//!   nearest where they gave up (its *stockade*, [`MAIN_HOLDS`] and
//!   [`OUTPOST_HOLDS`] soldiers), or the next nearest with room; those that
//!   fit nowhere are let go.
//! - Held soldiers work: each earns its captor [`WORK_GRUB`] Grub a minute.
//! - Their owner ransoms them back ([`crate::Command::Ransom`]), one lot (at
//!   most a full regiment) at a time, for the reinforcement share of what
//!   they cost, paid to the captor; they come out at the owner's main
//!   building with the rank they had.
//! - A stockade that falls lets its prisoners go: they stand up there as
//!   their owner's regiments while the owner has the supply. An
//!   owner put out of the match loses theirs.
//!
//! The records and the counts are in the state hash, once anyone has taken
//! a prisoner.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{REACH, placement};
use crate::events::Event;
use crate::hash::StateHasher;
use crate::morale::{MoraleState, NON_FIGHTER_MORALE};
use crate::regiments::{RegimentHandle, Regiments};
use crate::structures::{Role, StructureHandle};
use crate::{MAX_MODELS, MAX_PARTIES, PlayerId, TICKS_PER_SECOND, UnitId, UnitStats, World};

/// A broken regiment gives itself up only once it has run this long: it
/// had its head start and was run down, not taken where it broke.
pub const RUN_SECONDS: u32 = 5;
/// No fighting friend within this many tiles: the regiment is alone.
pub const ALONE_TILES: i32 = 8;
/// Soldiers a main building holds.
pub const MAIN_HOLDS: u32 = 60;
/// Soldiers an outpost hall holds.
pub const OUTPOST_HOLDS: u32 = 30;
/// Grub a held soldier earns its captor each minute.
pub const WORK_GRUB: u32 = 1;

/// Soldiers of one unit and rank, taken from `owner` by `captor`, held at
/// `stockade`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Held {
    pub captor: PlayerId,
    pub owner: PlayerId,
    pub unit: UnitId,
    pub rank: u8,
    pub stockade: StructureHandle,
    /// Where they stand up if the stockade falls: its door when taken.
    pub at: FxVec2,
    pub soldiers: u16,
}

/// Every player's prisoners.
#[derive(Clone, Debug, Default)]
pub(crate) struct Prisoners {
    /// Held soldiers, in the order taken (Conquest only).
    held: Vec<Held>,
    /// Soldiers each player has taken in the match.
    taken: [u32; MAX_PARTIES],
    /// The regiments that gave up this tick, so they aren't told as killed.
    pub(crate) gone: Vec<RegimentHandle>,
}

/// Records held before the list grows (ticking doesn't allocate until then).
const HELD_ROOM: usize = 64;

impl Prisoners {
    /// Room for `regiments` to give up in one tick.
    pub(crate) fn with_capacity(regiments: usize) -> Prisoners {
        Prisoners {
            held: Vec::with_capacity(HELD_ROOM),
            taken: [0; MAX_PARTIES],
            gone: Vec::with_capacity(regiments),
        }
    }

    /// Soldiers `player` has taken in the match.
    pub(crate) fn taken(&self, player: PlayerId) -> u32 {
        self.taken.get(usize::from(player)).copied().unwrap_or(0)
    }

    pub(crate) fn held(&self) -> &[Held] {
        &self.held
    }

    /// Soldiers held at stockade `s`.
    fn at_stockade(&self, s: StructureHandle) -> u32 {
        (self.held.iter())
            .filter(|h| h.stockade == s)
            .map(|h| u32::from(h.soldiers))
            .sum()
    }

    /// Add `soldiers` to the record of this captor, owner, unit, rank and
    /// stockade, or start one.
    fn hold(&mut self, h: Held) {
        match self.held.iter_mut().find(|r| {
            (r.captor, r.owner, r.unit, r.rank, r.stockade)
                == (h.captor, h.owner, h.unit, h.rank, h.stockade)
        }) {
            Some(r) => r.soldiers = r.soldiers.saturating_add(h.soldiers),
            None => self.held.push(h),
        }
    }

    /// The prisoners of the players in mask `out` are let go.
    pub(crate) fn forget_owners(&mut self, out: u8) {
        self.held.retain(|h| out & (1 << h.owner) == 0);
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if self.taken.iter().all(|&n| n == 0) {
            return;
        }
        for &n in &self.taken {
            h.u32(n);
        }
        h.u32(self.held.len() as u32);
        for r in &self.held {
            h.u8(r.captor);
            h.u8(r.owner);
            h.u16(r.unit);
            h.u8(r.rank);
            h.u32(r.stockade.index);
            h.u16(r.stockade.generation);
            h.i32(r.at.x.raw());
            h.i32(r.at.y.raw());
            h.u16(r.soldiers);
        }
    }
}

/// The soldiers of a regiment of `full` soldiers costing `grub` and
/// `marrow` are worth, `soldiers` of them: the reinforcement share (see
/// [`crate::reinforce`]).
pub(crate) fn share(grub: u16, marrow: u16, full: u16, soldiers: u16) -> (u32, u32) {
    // What reinforcing a regiment from `full - soldiers` back up to `full`
    // charges, step by step (`reinforce::share`), summed.
    let full = u32::from(full.max(1));
    let short = full - u32::from(soldiers).min(full);
    let cost = |c: u16| u32::from(c) - u32::from(c) * short / full;
    (cost(grub), cost(marrow))
}

impl Regiments {
    /// Slot `i`, raised just now at full strength, keeps only `soldiers` of
    /// them, with the experience of `rank`.
    fn thin_to(&mut self, i: usize, soldiers: u16, rank: u8, stats: &UnitStats) {
        let n = soldiers.clamp(1, stats.models.min(MAX_MODELS));
        let base = i * MAX_MODELS as usize;
        self.hp[base..][usize::from(n)..MAX_MODELS as usize].fill(0);
        self.models[i] = n;
        self.files[i] = stats.files.min(n).max(1);
        self.xp[i] = crate::veterancy::xp_for(rank);
    }
}

impl World {
    /// Soldiers `player` has taken prisoner in the match, in any mode.
    pub fn prisoners_taken(&self, player: PlayerId) -> u32 {
        self.prisoners.taken(player)
    }

    /// The soldiers held prisoner (Conquest), in the order taken.
    pub fn prisoners_held(&self) -> &[Held] {
        self.prisoners.held()
    }

    /// What ransoming the next lot of `owner`'s `unit` held by `from` costs
    /// (Grub, Marrow) and how many soldiers it is, if `from` holds some.
    pub fn ransom_price(
        &self,
        owner: PlayerId,
        from: PlayerId,
        unit: UnitId,
    ) -> Option<(u32, u32, u16)> {
        let c = self.conquest.as_ref()?;
        let held = (self.prisoners.held.iter())
            .find(|h| h.owner == owner && h.captor == from && h.unit == unit)?;
        let full = self.units.get(usize::from(unit))?.models;
        let lot = held.soldiers.min(full);
        let (_, grub, marrow) = c.reinforces_as(unit);
        let (g, m) = share(grub, marrow, full, lot);
        Some((g, m, lot))
    }

    /// Whether regiment `i` may give itself up now.
    fn may_surrender(&self, i: usize) -> bool {
        let r = &self.regiments;
        if !r.alive[i]
            || !matches!(
                r.morale_state[i],
                MoraleState::Routing | MoraleState::Shattered
            )
            || self.tick + 1 < r.last_rout[i] + RUN_SECONDS * TICKS_PER_SECOND
            || r.hero_level[i] > 0
            || r.general[i].is_some()
            || r.cant_rout(i)
            || self.combat.civil(r, i)
        {
            return false;
        }
        let unit = r.unit[i];
        if self
            .units
            .get(usize::from(unit))
            .is_none_or(|s| s.morale <= NON_FIGHTER_MORALE)
        {
            return false;
        }
        self.conquest
            .as_ref()
            .is_none_or(|c| c.reinforces_as(unit).0.trained_at.is_some())
    }

    /// Broken regiments caught alone give themselves up (once a second).
    pub(crate) fn take_prisoners(&mut self) {
        self.prisoners.gone.clear();
        if !self.tick.is_multiple_of(TICKS_PER_SECOND) {
            return;
        }
        let alone = Fx::from_int(ALONE_TILES);
        for i in 0..self.regiments.alive.len() {
            if !self.may_surrender(i) {
                continue;
            }
            let at = placement!(self.regiments, &self.stances);
            if self
                .combat
                .friend_fighting_near(&at, &self.regiments, i, alone)
            {
                continue;
            }
            let Some(j) = self.combat.catcher(&at, &self.regiments, i, REACH) else {
                continue;
            };
            self.surrender(i, j);
        }
    }

    /// Regiment `i` gives itself up to regiment `j`.
    fn surrender(&mut self, i: usize, j: usize) {
        let r = &self.regiments;
        let (owner, captor, unit, soldiers) = (r.owner[i], r.owner[j], r.unit[i], r.models[i]);
        let (at, rank, handle) = (r.position[i], crate::veterancy::rank_of(r, i), r.handle(i));
        if let Some(n) = self.prisoners.taken.get_mut(usize::from(captor)) {
            *n = n.saturating_add(u32::from(soldiers));
        }
        self.stockade(captor, owner, unit, rank, soldiers, at);
        match &mut self.conquest {
            Some(c) => c.remove(i, &mut self.regiments),
            None => {
                self.regiments.despawn(handle);
            }
        }
        self.prisoners.gone.push(handle);
        // A rehearsal has nobody to tell.
        if self.scratch {
            return;
        }
        self.events.surrendered(Event::Surrendered {
            regiment: handle,
            owner,
            captor,
            unit,
            soldiers,
            at,
        });
    }

    /// Hold `soldiers` taken at `at` in `captor`'s stockades, nearest
    /// first (by squared distance, then slot); what fits nowhere is let go.
    fn stockade(
        &mut self,
        captor: PlayerId,
        owner: PlayerId,
        unit: UnitId,
        rank: u8,
        soldiers: u16,
        at: FxVec2,
    ) {
        let Some(c) = &self.conquest else {
            return;
        };
        let s = &c.structures;
        let mut left = soldiers;
        // The nearest with room each time round (a player has few halls).
        while left > 0 {
            let mut best: Option<(u64, usize, u32)> = None;
            for k in s.finished_of(captor) {
                let room = match s.stats_of(k).role {
                    Role::Main => MAIN_HOLDS,
                    Role::Outpost => OUTPOST_HOLDS,
                    _ => continue,
                };
                let free = room.saturating_sub(self.prisoners.at_stockade(s.handle(k)));
                let Some(v) = s.get(s.handle(k)).filter(|_| free > 0) else {
                    continue;
                };
                let d = (v.centre() - at).length_sq_raw();
                if best.is_none_or(|(bd, bk, _)| (d, k) < (bd, bk)) {
                    best = Some((d, k, free));
                }
            }
            let Some((_, k, free)) = best else {
                // No room anywhere: the rest are let go.
                return;
            };
            let n = u32::from(left).min(free) as u16;
            self.prisoners.hold(Held {
                captor,
                owner,
                unit,
                rank,
                stockade: s.handle(k),
                at: s.door(k, &self.nav),
                soldiers: n,
            });
            left -= n;
        }
    }

    /// Once a minute the held soldiers work for their captors; once a
    /// second those of fallen stockades go free.
    pub(crate) fn tend_prisoners(&mut self) {
        let Some(c) = &mut self.conquest else {
            return;
        };
        if self.prisoners.held.is_empty() {
            return;
        }
        if self.tick > 0 && self.tick.is_multiple_of(60 * TICKS_PER_SECOND) {
            let mut work = [0u32; MAX_PARTIES];
            for h in &self.prisoners.held {
                work[usize::from(h.captor)] += u32::from(h.soldiers) * WORK_GRUB;
            }
            for (p, &grub) in work.iter().enumerate() {
                c.economy
                    .earn_grub(p as PlayerId, grub, &self.regiments, &c.units);
            }
        }
        if !self.tick.is_multiple_of(TICKS_PER_SECOND) {
            return;
        }
        let mut k = 0;
        while k < self.prisoners.held.len() {
            let h = self.prisoners.held[k];
            let stands = (self.conquest.as_ref())
                .is_some_and(|c| c.structures.resolve(h.stockade).is_some());
            if stands {
                k += 1;
            } else {
                self.prisoners.held.remove(k);
                self.escape(h);
            }
        }
    }

    /// The prisoners of `h` stand up where they were held, as their
    /// owner's regiments, while the owner has the supply.
    fn escape(&mut self, h: Held) {
        let Some(stats) = self.units.get(usize::from(h.unit)).copied() else {
            return;
        };
        let mut left = h.soldiers;
        while left > 0 {
            let lot = left.min(stats.models);
            left -= lot;
            if !self.room_for(h.owner, h.unit) {
                return;
            }
            if self
                .raise_lot(h.owner, h.unit, h.rank, lot, h.at, Angle::ZERO)
                .is_none()
            {
                return;
            }
        }
    }

    /// Whether `owner` has the supply for another regiment of `unit`.
    /// Whether `owner` could take a ransomed lot of `unit` back now: it has
    /// a finished main building and the supply free. The price and the
    /// lot are [`World::ransom_price`].
    pub fn can_take_back(&self, owner: PlayerId, unit: UnitId) -> bool {
        self.conquest.as_ref().is_some_and(|c| {
            (c.structures.finished_of(owner)).any(|m| c.structures.stats_of(m).role == Role::Main)
        }) && self.room_for(owner, unit)
    }

    fn room_for(&self, owner: PlayerId, unit: UnitId) -> bool {
        let Some(c) = &self.conquest else {
            return false;
        };
        let need = u32::from(c.unit(unit).supply);
        c.supply_used(&self.regiments, owner) + need <= c.structures.supply_cap(owner)
    }

    /// Put `soldiers` of `owner`'s `unit` of `rank` on the field at `at`.
    fn raise_lot(
        &mut self,
        owner: PlayerId,
        unit: UnitId,
        rank: u8,
        soldiers: u16,
        at: FxVec2,
        facing: Angle,
    ) -> Option<RegimentHandle> {
        let stats = *self.units.get(usize::from(unit))?;
        let h = self.regiments.spawn(owner, unit, at, facing, &stats)?;
        let i = h.index as usize;
        self.regiments.thin_to(i, soldiers, rank, &stats);
        self.vision
            .start(&self.regiments, &self.units, &self.terrain, i);
        Some(h)
    }

    /// `player` ransoms the oldest lot of their `unit` that `from` holds:
    /// they pay its reinforcement share to `from`, and it comes out at
    /// their first finished main building.
    pub(crate) fn ransom(&mut self, player: PlayerId, from: PlayerId, unit: UnitId) -> bool {
        if !self.stances.hostile(player, from) {
            return false;
        }
        let Some(c) = &self.conquest else {
            return false;
        };
        let Some(k) = (self.prisoners.held.iter())
            .position(|h| h.owner == player && h.captor == from && h.unit == unit)
        else {
            return false;
        };
        let Some(main) = (c.structures.finished_of(player))
            .find(|&m| c.structures.stats_of(m).role == Role::Main)
        else {
            return false;
        };
        let held = self.prisoners.held[k];
        let Some((grub, marrow, lot)) = self.ransom_price(player, from, unit) else {
            return false;
        };
        if !self.room_for(player, unit) || !c.economy.can_afford(player, grub, marrow) {
            return false;
        }
        let (at, facing) = (
            c.structures.door(main, &self.nav),
            c.structures.facing(main),
        );
        if self
            .raise_lot(player, unit, held.rank, lot, at, facing)
            .is_none()
        {
            return false;
        }
        let Some(c) = &mut self.conquest else {
            return false;
        };
        c.economy.spend(player, grub, marrow);
        c.economy.refund(from, grub, marrow);
        let r = &mut self.prisoners.held[k];
        r.soldiers -= lot;
        if r.soldiers == 0 {
            self.prisoners.held.remove(k);
        }
        true
    }
}
