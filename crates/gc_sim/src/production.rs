//! Conquest training (docs/21-buildings.md): each building's queue of
//! regiments to raise, paid for when queued, and where they go once out.
//!
//! - A building trains the unit types whose `ConquestUnit::trained_at` is
//!   it or a building it is a tier upgrade of, and only those its owner's
//!   tier allows (see [`crate::structures::Structures::tier_of`]).
//! - A queue holds [`QUEUE`] regiments. The one at the front starts once
//!   its owner has the supply for it, counting those already in training,
//!   and takes its `train_ticks` at the match's tempo.
//! - A finished regiment comes out at the building's south side and marches
//!   to its rally point, if it has one.
//! - Calling off the last in a queue gives back all it cost. A building
//!   that falls takes its queue with it, and gives back nothing.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::conquest::ConquestUnit;
use crate::structures::{StructureHandle, Structures};
use crate::{MAX_PARTIES, PlayerId, StateHasher, UnitId};

/// Regiments a building can have queued.
pub const QUEUE: usize = 5;

/// A regiment that finished training, for the world to put on the field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Trained {
    pub(crate) owner: PlayerId,
    pub(crate) unit: UnitId,
    pub(crate) at: FxVec2,
    pub(crate) rally: Option<FxVec2>,
}

/// What a regiment of type `unit` takes of its owner's supply.
fn supply(units: &[ConquestUnit], unit: UnitId) -> u32 {
    units
        .get(usize::from(unit))
        .map_or(0, |u| u32::from(u.supply))
}

/// Every building's training queue and rally point, by building slot.
pub struct Production {
    build_percent: u32,
    /// The generation of the building each slot's queue belongs to: a slot
    /// reused by a new building starts empty.
    queue_gen: Vec<u16>,
    queue: Vec<[UnitId; QUEUE]>,
    queued: Vec<u8>,
    /// Ticks left for the regiment at the front; 0 until it starts.
    clock: Vec<u32>,
    rally: Vec<Option<FxVec2>>,
    /// Regiments that finished training in the last tick.
    pub(crate) trained: Vec<Trained>,
}

impl Production {
    pub(crate) fn new(structures: usize, build_percent: u32) -> Production {
        Production {
            build_percent: build_percent.clamp(10, 1000),
            queue_gen: vec![0; structures],
            queue: vec![[0; QUEUE]; structures],
            queued: vec![0; structures],
            clock: vec![0; structures],
            rally: vec![None; structures],
            trained: Vec::with_capacity(structures),
        }
    }

    /// Building `h`'s record, emptied first if its slot has a new building.
    fn slot(&mut self, h: StructureHandle) -> usize {
        let i = h.index as usize;
        if self.queue_gen[i] != h.generation {
            self.queue_gen[i] = h.generation;
            self.queued[i] = 0;
            self.clock[i] = 0;
            self.rally[i] = None;
        }
        i
    }

    /// What building `h` has queued, front first.
    pub fn queue_of(&self, h: StructureHandle) -> &[UnitId] {
        let i = h.index as usize;
        if self.queue_gen.get(i) != Some(&h.generation) {
            return &[];
        }
        &self.queue[i][..usize::from(self.queued[i])]
    }

    /// Ticks left for the regiment at the front of building `h`'s queue,
    /// once it has started.
    pub fn front_ticks_left(&self, h: StructureHandle) -> Option<u32> {
        let i = h.index as usize;
        (self.queue_gen.get(i) == Some(&h.generation) && self.clock[i] > 0).then(|| self.clock[i])
    }

    /// Building `h`'s rally point.
    pub fn rally_of(&self, h: StructureHandle) -> Option<FxVec2> {
        let i = h.index as usize;
        self.rally[i].filter(|_| self.queue_gen[i] == h.generation)
    }

    /// Whether building `h` has room in its queue.
    pub(crate) fn has_room(&self, h: StructureHandle) -> bool {
        self.queue_of(h).len() < QUEUE
    }

    /// Add `unit` to building `h`'s queue; the caller has checked it may
    /// and taken the price.
    pub(crate) fn enqueue(&mut self, h: StructureHandle, unit: UnitId) {
        let i = self.slot(h);
        let n = usize::from(self.queued[i]);
        if n < QUEUE {
            self.queue[i][n] = unit;
            self.queued[i] += 1;
        }
    }

    /// Take the last regiment off building `h`'s queue, for its price back.
    pub(crate) fn dequeue(&mut self, h: StructureHandle) -> Option<UnitId> {
        let i = self.slot(h);
        let n = usize::from(self.queued[i]);
        if n == 0 {
            return None;
        }
        self.queued[i] -= 1;
        if n == 1 {
            self.clock[i] = 0;
        }
        Some(self.queue[i][n - 1])
    }

    pub(crate) fn set_rally(&mut self, h: StructureHandle, at: Option<FxVec2>) {
        let i = self.slot(h);
        self.rally[i] = at;
    }

    /// Add the supply regiments in training take to `used`, by player.
    pub(crate) fn in_training(
        &self,
        structures: &Structures,
        units: &[ConquestUnit],
        used: &mut [u32; MAX_PARTIES],
    ) {
        for i in 0..self.queued.len() {
            if self.queued[i] > 0
                && self.clock[i] > 0
                && let Some((_, owner)) = structures.live_slot(i, self.queue_gen[i])
                && let Some(u) = used.get_mut(usize::from(owner))
            {
                *u += supply(units, self.queue[i][0]);
            }
        }
    }

    /// One tick of training. `used` is each player's supply in use by
    /// their regiments; `cap` their supply cap.
    pub(crate) fn update(
        &mut self,
        structures: &Structures,
        units: &[ConquestUnit],
        mut used: [u32; MAX_PARTIES],
        cap: impl Fn(PlayerId) -> u32,
    ) {
        self.trained.clear();
        self.in_training(structures, units, &mut used);
        for i in 0..self.queued.len() {
            if self.queued[i] == 0 {
                continue;
            }
            let Some((_, owner)) = structures.live_slot(i, self.queue_gen[i]) else {
                // It fell: the queue goes with it.
                self.queued[i] = 0;
                self.clock[i] = 0;
                continue;
            };
            let unit = self.queue[i][0];
            if self.clock[i] == 0 {
                let p = usize::from(owner);
                let need = supply(units, unit);
                if p >= MAX_PARTIES || used[p] + need > cap(owner) {
                    // Waits for room.
                    continue;
                }
                used[p] += need;
                let ticks = units
                    .get(usize::from(unit))
                    .map_or(1, |u| u32::from(u.train_ticks));
                self.clock[i] = (ticks * self.build_percent / 100).max(1);
            }
            self.clock[i] -= 1;
            if self.clock[i] > 0 {
                continue;
            }
            self.trained.push(Trained {
                owner,
                unit,
                at: structures.door(i),
                rally: self.rally[i],
            });
            self.queue[i].copy_within(1.., 0);
            self.queued[i] -= 1;
        }
    }

    pub(crate) fn hash_into(&self, structures: &Structures, h: &mut StateHasher) {
        h.u32(self.build_percent);
        for i in 0..self.queued.len() {
            let live = structures.live_slot(i, self.queue_gen[i]).is_some();
            if !live || (self.queued[i] == 0 && self.rally[i].is_none()) {
                h.u8(0);
                continue;
            }
            h.u8(self.queued[i] + 1);
            for &u in &self.queue[i][..usize::from(self.queued[i])] {
                h.u16(u);
            }
            h.u32(self.clock[i]);
            match self.rally[i] {
                Some(at) => {
                    h.u8(1);
                    h.i32(at.x.raw());
                    h.i32(at.y.raw());
                }
                None => h.u8(0),
            }
        }
    }
}
