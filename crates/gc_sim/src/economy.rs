//! The Conquest economy (docs/04-economy.md): Grub seeps and Marrow veins,
//! workers carrying what they gather to a drop-off, each player's store,
//! the supply cap and upkeep.
//!
//! - **Nodes.** A seep starts with [`SEEP_GRUB`] and a vein with
//!   [`VEIN_MARROW`] (the map may give others). Each takes so many workers
//!   at once ([`GRUB_SLOTS`], [`MARROW_SLOTS`]); a worker who finds it full
//!   waits beside it for a place. A node run dry turns to **dregs**: it
//!   still yields, at [`DREGS_PERCENT`] of the rate, forever.
//! - **Workers** are regiments of a worker type (`ConquestUnit::worker`).
//!   Ordered to a node ([`crate::Command::Gather`]) a worker walks there,
//!   fills its load over [`GRUB_TICKS`] (or [`MARROW_TICKS`]), carries it
//!   to the nearest of its owner's buildings that takes that resource,
//!   puts it in the store and goes back for more, until given another
//!   order. A worker given another order keeps what it carries, and drops
//!   it off first the next time it is sent to gather.
//! - **Supply.** A player's cap is what their standing buildings add, up to
//!   [`SUPPLY_LIMIT`]; what their regiments take counts against it.
//! - **Upkeep.** Grub brought in is cut by how much supply is in use: all
//!   of it up to 60, 75% up to 100, 55% beyond. Marrow is never cut.
//! - **Tempo** scales gathering: a worker fills its load in
//!   `100 / gather_percent` of the time.
//!
//! Five workers on a seep seven tiles from their hall bring in about 200
//! Grub a minute, and three on a vein about 50 Marrow, as docs/04 sets.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::conquest::{ConquestSetup, ConquestUnit};
use crate::regiments::{Order, Regiments};
use crate::structures::{Approach, DROP_OFF_GRUB, DROP_OFF_MARROW, Structures};
use crate::{MAX_PARTIES, PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats};

/// What a node yields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Resource {
    Grub = 0,
    Marrow = 1,
}

/// A fresh seep's Grub and a fresh vein's Marrow.
pub const SEEP_GRUB: u32 = 7_500;
pub const VEIN_MARROW: u32 = 3_000;
/// Workers who can gather at a seep, and at a vein, at once.
pub const GRUB_SLOTS: u8 = 5;
pub const MARROW_SLOTS: u8 = 3;
/// A worker's load.
pub const GRUB_LOAD: u32 = 10;
pub const MARROW_LOAD: u32 = 5;
/// Ticks to fill a load at Standard tempo.
pub const GRUB_TICKS: u32 = 9 * TICKS_PER_SECOND;
pub const MARROW_TICKS: u32 = 12 * TICKS_PER_SECOND;
/// A dry node still yields at this percent of its rate.
pub const DREGS_PERCENT: u32 = 25;
/// The most supply a player can have.
pub const SUPPLY_LIMIT: u32 = 150;
/// Upkeep: (supply in use up to, percent of Grub kept).
pub const UPKEEP: [(u32, u32); 3] = [(60, 100), (100, 75), (SUPPLY_LIMIT, 55)];
/// How near a worker's centre must come to a node's to gather from it.
pub const NODE_REACH: Fx = Fx::from_int(2);
/// Ticks a worker that can't get on waits before trying again.
const RETRY_TICKS: u16 = TICKS_PER_SECOND as u16;
/// Grub is kept in hundredths, so upkeep's cut of a small load isn't lost.
const GRUB_SCALE: u32 = 100;

/// A seep or vein as the map places it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeSetup {
    pub resource: Resource,
    /// Its centre.
    pub at: FxVec2,
    /// What it holds before it runs to dregs.
    pub amount: u32,
}

/// A node as the game sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeView {
    pub index: u16,
    pub resource: Resource,
    pub at: FxVec2,
    /// Left before dregs.
    pub left: u32,
    pub full: u32,
    /// Workers gathering there now.
    pub workers: u8,
}

/// A player's store and supply, as the game's top bar shows them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bank {
    pub grub: u32,
    pub marrow: u32,
    pub supply_used: u32,
    pub supply_cap: u32,
}

impl Bank {
    /// Percent of Grub brought in that the player keeps.
    pub fn upkeep_percent(&self) -> u32 {
        upkeep_percent(self.supply_used)
    }
}

fn upkeep_percent(supply_used: u32) -> u32 {
    UPKEEP
        .iter()
        .find(|&&(up_to, _)| supply_used <= up_to)
        .map_or(UPKEEP[UPKEEP.len() - 1].1, |&(_, keep)| keep)
}

/// Where a worker is in its round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
enum Step {
    /// Not gathering.
    #[default]
    Idle = 0,
    /// Walking to its node, or waiting beside it for a place.
    ToNode = 1,
    /// Filling its load; the clock counts down.
    Filling = 2,
    /// Carrying its load to a drop-off.
    Carrying = 3,
}

/// The match's nodes, each player's store and every worker's round.
pub struct Economy {
    units: Vec<ConquestUnit>,
    gather_percent: u32,
    node_resource: Vec<Resource>,
    node_at: Vec<FxVec2>,
    node_left: Vec<u32>,
    node_full: Vec<u32>,
    /// Workers filling a load at each node; counted afresh each tick.
    node_busy: Vec<u8>,
    /// Grub in hundredths, and Marrow, by player.
    grub: [u32; MAX_PARTIES],
    marrow: [u32; MAX_PARTIES],
    /// Counted afresh each tick from the regiments and buildings.
    supply_used: [u32; MAX_PARTIES],
    supply_cap: [u32; MAX_PARTIES],
    /// By regiment slot: its round, the node it works, the generation of
    /// the regiment sent (a slot reused by a new regiment doesn't inherit
    /// it), what it carries, and ticks until its load is full or its next
    /// try.
    step: Vec<Step>,
    node: Vec<u16>,
    work_gen: Vec<u16>,
    load: Vec<u8>,
    load_kind: Vec<Resource>,
    clock: Vec<u16>,
}

impl Economy {
    pub(crate) fn new(setup: &ConquestSetup, regiments: usize) -> Economy {
        let n = regiments;
        Economy {
            units: setup.units.to_vec(),
            gather_percent: setup.gather_percent.clamp(10, 1000),
            node_resource: setup.nodes.iter().map(|n| n.resource).collect(),
            node_at: setup.nodes.iter().map(|n| n.at).collect(),
            node_left: setup.nodes.iter().map(|n| n.amount).collect(),
            node_full: setup.nodes.iter().map(|n| n.amount).collect(),
            node_busy: vec![0; setup.nodes.len()],
            grub: setup.start.map(|(g, _)| g.saturating_mul(GRUB_SCALE)),
            marrow: setup.start.map(|(_, m)| m),
            supply_used: [0; MAX_PARTIES],
            supply_cap: [0; MAX_PARTIES],
            step: vec![Step::Idle; n],
            node: vec![0; n],
            work_gen: vec![0; n],
            load: vec![0; n],
            load_kind: vec![Resource::Grub; n],
            clock: vec![0; n],
        }
    }

    /// Player `p`'s store and supply.
    pub fn bank(&self, p: PlayerId) -> Bank {
        let p = usize::from(p);
        if p >= MAX_PARTIES {
            return Bank::default();
        }
        Bank {
            grub: self.grub[p] / GRUB_SCALE,
            marrow: self.marrow[p],
            supply_used: self.supply_used[p],
            supply_cap: self.supply_cap[p],
        }
    }

    /// Every node, in index order.
    pub fn nodes(&self) -> impl Iterator<Item = NodeView> + '_ {
        (0..self.node_at.len()).map(|i| NodeView {
            index: i as u16,
            resource: self.node_resource[i],
            at: self.node_at[i],
            left: self.node_left[i],
            full: self.node_full[i],
            workers: self.node_busy[i],
        })
    }

    /// Whether regiment type `unit` gathers.
    pub fn is_worker(&self, unit: crate::UnitId) -> bool {
        self.units.get(usize::from(unit)).is_some_and(|u| u.worker)
    }

    /// The node regiment slot `r` is working, if it is.
    pub(crate) fn work_of(&self, regs: &Regiments, r: usize) -> Option<u16> {
        (self.step[r] != Step::Idle && regs.alive[r] && regs.generation[r] == self.work_gen[r])
            .then_some(self.node[r])
    }

    /// What regiment slot `r` carries: how much of what.
    pub(crate) fn carried(&self, regs: &Regiments, r: usize) -> Option<(Resource, u32)> {
        (self.load[r] > 0 && regs.alive[r] && regs.generation[r] == self.work_gen[r])
            .then(|| (self.load_kind[r], u32::from(self.load[r])))
    }

    /// Send worker slot `r` to gather at `node`. The caller has checked the
    /// regiment is the issuer's; refused unless it is a worker and the
    /// node exists.
    pub(crate) fn gather(&mut self, regs: &mut Regiments, r: usize, node: u16) -> bool {
        if !self.is_worker(regs.unit[r]) || usize::from(node) >= self.node_at.len() {
            return false;
        }
        if regs.generation[r] != self.work_gen[r] {
            // A new regiment in the slot: nothing carried over.
            self.load[r] = 0;
        }
        regs.halt(r);
        self.step[r] = if self.load[r] > 0 {
            Step::Carrying
        } else {
            Step::ToNode
        };
        self.node[r] = node;
        self.work_gen[r] = regs.generation[r];
        self.clock[r] = 0;
        true
    }

    /// Regiment slot `r` was given another order: it stops gathering, but
    /// keeps what it carries.
    pub(crate) fn stop(&mut self, r: usize) {
        self.step[r] = Step::Idle;
    }

    /// Count each player's supply, used and cap.
    fn count_supply(&mut self, regs: &Regiments, structures: &Structures) {
        self.supply_used = [0; MAX_PARTIES];
        self.supply_cap = [0; MAX_PARTIES];
        for r in 0..regs.alive.len() {
            if !regs.alive[r] {
                continue;
            }
            let supply = self
                .units
                .get(usize::from(regs.unit[r]))
                .map_or(0, |u| u.supply);
            if let Some(used) = self.supply_used.get_mut(usize::from(regs.owner[r])) {
                *used += u32::from(supply);
            }
        }
        structures.add_supply(&mut self.supply_cap);
        for cap in &mut self.supply_cap {
            *cap = (*cap).min(SUPPLY_LIMIT);
        }
    }

    /// One tick of the economy: supply counted, and every worker on its
    /// round.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        structures: &Structures,
    ) {
        self.count_supply(regs, structures);
        self.node_busy.fill(0);
        for r in 0..self.step.len() {
            if self.work_of(regs, r).is_some() && self.step[r] == Step::Filling {
                self.node_busy[usize::from(self.node[r])] += 1;
            }
        }
        for r in 0..self.step.len() {
            let Some(n) = self.work_of(regs, r).map(usize::from) else {
                self.step[r] = Step::Idle;
                continue;
            };
            if regs.morale_state[r].broken() {
                self.step[r] = Step::Idle;
                continue;
            }
            if regs.engaged[r].is_some() {
                continue;
            }
            self.clock[r] = self.clock[r].saturating_sub(1);
            match self.step[r] {
                Step::Idle => {}
                Step::ToNode => self.walk_to_node(regs, r, n),
                Step::Filling => {
                    if self.clock[r] == 0 {
                        self.fill(r, n);
                    }
                }
                Step::Carrying => self.carry(regs, units, structures, r),
            }
        }
    }

    /// Walk to node `n`, and start filling once there and a place is free.
    fn walk_to_node(&mut self, regs: &mut Regiments, r: usize, n: usize) {
        let at = self.node_at[n];
        let here = regs.position[r];
        if (at - here).length() > NODE_REACH {
            if regs.target[r].is_none() && self.clock[r] == 0 {
                // A tile short on the side it comes from, so workers
                // arriving from different ways don't all crowd one point.
                let stand = at + (here - at).normalized();
                let h = regs.handle(r);
                regs.order(
                    h,
                    Order {
                        target: stand,
                        facing: None,
                        files: 0,
                        pace: Fx::ZERO,
                    },
                );
                self.clock[r] = RETRY_TICKS;
            }
            return;
        }
        regs.target[r] = None;
        let slots = match self.node_resource[n] {
            Resource::Grub => GRUB_SLOTS,
            Resource::Marrow => MARROW_SLOTS,
        };
        if self.node_busy[n] >= slots {
            return;
        }
        self.node_busy[n] += 1;
        self.step[r] = Step::Filling;
        let base = match self.node_resource[n] {
            Resource::Grub => GRUB_TICKS,
            Resource::Marrow => MARROW_TICKS,
        };
        let mut ticks = base * 100 / self.gather_percent;
        if self.node_left[n] == 0 {
            ticks = ticks * 100 / DREGS_PERCENT;
        }
        self.clock[r] = u16::try_from(ticks.max(1)).unwrap_or(u16::MAX);
    }

    /// The load from node `n` is full: take it and head for a drop-off.
    fn fill(&mut self, r: usize, n: usize) {
        let load = match self.node_resource[n] {
            Resource::Grub => GRUB_LOAD,
            Resource::Marrow => MARROW_LOAD,
        };
        // A dry node gives a whole load, only slower (see `walk_to_node`).
        let take = if self.node_left[n] == 0 {
            load
        } else {
            load.min(self.node_left[n])
        };
        self.node_left[n] -= take.min(self.node_left[n]);
        self.node_busy[n] -= 1;
        self.load[r] = take as u8;
        self.load_kind[r] = self.node_resource[n];
        self.step[r] = Step::Carrying;
        self.clock[r] = 0;
    }

    /// Carry the load to the nearest drop-off and put it in the store.
    fn carry(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        structures: &Structures,
        r: usize,
    ) {
        if regs.target[r].is_some() || self.clock[r] > 0 {
            // On the way, or waiting to try again.
            return;
        }
        let kind = self.load_kind[r];
        let flag = match kind {
            Resource::Grub => DROP_OFF_GRUB,
            Resource::Marrow => DROP_OFF_MARROW,
        };
        let owner = regs.owner[r];
        let Some(foot) = structures.nearest_drop_off(owner, flag, regs.position[r]) else {
            // Nowhere to take it: wait for one to go up.
            self.clock[r] = RETRY_TICKS;
            return;
        };
        let way = Approach::of(regs, &units[usize::from(regs.unit[r])], r, foot);
        if way.gap > way.reach() {
            way.walk(regs, r);
            self.clock[r] = RETRY_TICKS;
            return;
        }
        let p = usize::from(owner);
        let amount = u32::from(self.load[r]);
        match kind {
            Resource::Grub => {
                self.grub[p] = self.grub[p].saturating_add(
                    amount * upkeep_percent(self.supply_used[p]) * GRUB_SCALE / 100,
                );
            }
            Resource::Marrow => self.marrow[p] = self.marrow[p].saturating_add(amount),
        }
        self.load[r] = 0;
        self.step[r] = Step::ToNode;
    }

    pub(crate) fn hash_into(&self, regs: &Regiments, h: &mut StateHasher) {
        h.u32(self.units.len() as u32);
        for u in &self.units {
            u.hash_into(h);
        }
        h.u32(self.gather_percent);
        h.u32(self.node_at.len() as u32);
        for i in 0..self.node_at.len() {
            h.u8(self.node_resource[i] as u8);
            h.i32(self.node_at[i].x.raw());
            h.i32(self.node_at[i].y.raw());
            h.u32(self.node_left[i]);
            h.u32(self.node_full[i]);
        }
        for p in 0..MAX_PARTIES {
            h.u32(self.grub[p]);
            h.u32(self.marrow[p]);
        }
        for r in 0..self.step.len() {
            let live = regs.alive[r] && regs.generation[r] == self.work_gen[r];
            if !live || (self.step[r] == Step::Idle && self.load[r] == 0) {
                h.u8(0);
                continue;
            }
            h.u8(1 + self.step[r] as u8);
            h.u16(self.node[r]);
            h.u8(self.load[r]);
            h.u8(self.load_kind[r] as u8);
            h.u16(self.clock[r]);
        }
    }
}
