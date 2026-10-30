//! Power and tuning (docs/03-factions.md, the Choir's economy, and
//! docs/21-buildings.md): the fields that let the Choir's buildings work,
//! and the buildings that work nodes with no workers.
//!
//! - A finished building with the [`POWERS_FIELD`] flag powers a field its
//!   `field` of tiles from its edge (the Spire 8 at every tier, a Resonator
//!   or Siphon 6, in the shipped rosters).
//! - A building with the [`NEEDS_POWER`] flag can only be laid where one of
//!   its owner's fields reaches, and works only while one does: unpowered,
//!   it trains nothing until power returns (its queue waits).
//! - A finished building that tunes ([`crate::structures::BuildingStats::tunes`],
//!   a Resonator for seeps and a Siphon for veins) works one node of its
//!   kind within its reach with no workers (see [`crate::economy`]): tuners
//!   and nodes pair up nearest first, so a tuner works the nearest node no
//!   nearer tuner has taken, and a node takes at most one tuner.
//!
//! Which buildings power and tune is worked out afresh each tick from the
//! buildings standing, so it is not stored or hashed; the economy hands
//! nodes to tuners again only when the tuners change.

use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::PlayerId;
use crate::economy::Resource;
use crate::structures::{NEEDS_POWER, POWERS_FIELD, StructureHandle, Structures};
use crate::veins::{foot_reaches, within};

/// A finished building powering a field, as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldView {
    pub owner: PlayerId,
    pub building: StructureHandle,
    /// Its footprint, `[x, y, width, depth]` in tiles.
    pub foot: [i32; 4],
    /// How far its field reaches from its edge, in tiles.
    pub reach: i32,
}

impl Structures {
    /// Work out this tick's powering and tuning buildings.
    pub(crate) fn link_power(&mut self) {
        self.powerers.clear();
        core::mem::swap(&mut self.tuners, &mut self.tuners_was);
        self.tuners.clear();
        core::mem::swap(&mut self.linkers, &mut self.linkers_was);
        self.linkers.clear();
        for i in 0..self.slots() {
            if !self.stands(i) || !self.finished(i) {
                continue;
            }
            let b = *self.stats_of(i);
            if b.flags & POWERS_FIELD != 0 && b.field > 0 {
                self.powerers.push(self.handle(i));
            }
            if b.tunes.is_some() {
                self.tuners.push(self.handle(i));
            }
            if b.links > 0 {
                self.linkers.push(self.handle(i));
            }
        }
        // By owner, so each owner's lie together for the chords.
        let mut linkers = core::mem::take(&mut self.linkers);
        linkers.sort_unstable_by_key(|h| (self.owner_of(h.index as usize), h.index));
        self.linkers = linkers;
        let relink = core::mem::take(&mut self.restats) || self.linkers != self.linkers_was;
        if relink {
            self.find_chords();
        }
        if relink || self.tuners != self.tuners_was {
            self.power_stamp = self.power_stamp.wrapping_add(1);
        }
    }

    /// The slots of this tick's powering buildings still standing (a slot
    /// freed since, and reused, is not one of them).
    fn fields(&self) -> impl Iterator<Item = usize> + '_ {
        self.powerers.iter().filter_map(|&h| self.resolve(h))
    }

    /// Whether one of `owner`'s fields of power reaches footprint `foot`
    /// (`[x, y, w, d]`, tiles).
    pub(crate) fn power_reaches(&self, owner: PlayerId, foot: [i32; 4]) -> bool {
        self.fields()
            .filter(|&v| self.owner_of(v) == owner)
            .any(|v| foot_reaches(self.footprint(v), foot, self.stats_of(v).field))
    }

    /// Whether building slot `i` works: it needs no power, or has it.
    pub(crate) fn powered(&self, i: usize) -> bool {
        !self.stands(i)
            || !self.needs_power(i)
            || self.power_reaches(self.owner_of(i), self.footprint(i))
    }

    /// Whether building slot `i` needs power to work.
    pub(crate) fn needs_power(&self, i: usize) -> bool {
        self.stats_of(i).flags & NEEDS_POWER != 0
    }

    /// The finished buildings powering a field, as the game sees them.
    pub fn field_views(&self) -> impl Iterator<Item = FieldView> + '_ {
        self.fields().map(|v| FieldView {
            owner: self.owner_of(v),
            building: self.handle(v),
            foot: self.footprint(v),
            reach: i32::from(self.stats_of(v).field),
        })
    }

    /// For each node (its kind, centre and whether it has run dry), the
    /// slot of the tuning building that works it, if any, into
    /// `scratch.of`. Of this tick's tuners still standing, each works one
    /// node of its kind within its reach and each node has one tuner: as
    /// many nodes are worked as can be, each tuner taking the nearest it
    /// can (a node with Grub left before one run to dregs; then centre to
    /// centre; lower slot, then lower node, on a tie). `scratch` keeps its
    /// room between calls, so this seldom allocates.
    pub(crate) fn tunings(
        &self,
        kinds: &[Resource],
        at: &[FxVec2],
        dry: impl Fn(usize) -> bool,
        scratch: &mut TuneScratch,
    ) {
        let TuneScratch {
            pairs,
            busy,
            of,
            seen,
        } = scratch;
        pairs.clear();
        for v in self.tuners.iter().filter_map(|&h| self.resolve(h)) {
            let Some((r, tiles)) = self.stats_of(v).tunes else {
                continue;
            };
            let (foot, centre) = (self.footprint(v), self.centre(v));
            for (n, (&kind, &at)) in kinds.iter().zip(at).enumerate() {
                if kind == r && within(foot, at, i32::from(tiles)) {
                    pairs.push((dry(n), (centre - at).length_sq_raw(), v, n));
                }
            }
        }
        pairs.sort_unstable();
        of.clear();
        of.resize(kinds.len(), None);
        busy.clear();
        busy.resize(self.slots(), false);
        // Nearest first...
        for &(_, _, v, n) in pairs.iter() {
            if of[n].is_none() && !busy[v] {
                of[n] = Some(v);
                busy[v] = true;
            }
        }
        // ...then a tuner left idle takes a node from one that has another
        // free in reach, so no node goes untuned that a tuner could work.
        for i in 0..pairs.len() {
            let v = pairs[i].2;
            if busy[v] {
                continue;
            }
            seen.clear();
            seen.resize(kinds.len(), false);
            if hand_over(pairs, of, seen, v) {
                busy[v] = true;
            }
        }
    }
}

/// Finds tuner `v` a node, moving the tuner of a node it wants on to
/// another of its own if that one can (an augmenting path); true if it did.
fn hand_over(
    pairs: &[(bool, u64, usize, usize)],
    of: &mut [Option<usize>],
    seen: &mut [bool],
    v: usize,
) -> bool {
    for &(_, _, t, n) in pairs {
        if t != v || seen[n] {
            continue;
        }
        seen[n] = true;
        if of[n].is_none_or(|w| hand_over(pairs, of, seen, w)) {
            of[n] = Some(v);
            return true;
        }
    }
    false
}

/// Room [`Structures::tunings`] reuses from one call to the next.
#[derive(Debug, Default)]
pub(crate) struct TuneScratch {
    /// Each (node run dry, distance squared, tuner slot, node) in reach.
    pairs: Vec<(bool, u64, usize, usize)>,
    /// By slot: whether the tuner has its node.
    busy: Vec<bool>,
    /// By node: the slot of the tuner that works it.
    pub(crate) of: Vec<Option<usize>>,
    /// By node: looked at in this search for an idle tuner.
    seen: Vec<bool>,
}

impl TuneScratch {
    /// Room for `nodes` nodes and a few tuners in reach of each.
    pub(crate) fn with_room(nodes: usize) -> TuneScratch {
        TuneScratch {
            pairs: Vec::with_capacity(nodes * 4),
            busy: Vec::with_capacity(256),
            of: Vec::with_capacity(nodes),
            seen: Vec::with_capacity(nodes),
        }
    }
}
