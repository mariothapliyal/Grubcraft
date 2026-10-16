//! The Conquest economy (docs/04-economy.md): Grub seeps and Marrow veins,
//! workers carrying what they gather to a drop-off, each player's store,
//! the supply cap and upkeep.
//!
//! - **Nodes.** A seep starts with [`SEEP_GRUB`] and a vein with
//!   [`VEIN_MARROW`] (the match setup may give others). Each takes so many
//!   workers at once ([`GRUB_SLOTS`], [`MARROW_SLOTS`]); a worker who finds
//!   it full waits beside it for a place. A node run dry turns to **dregs**:
//!   it still yields, at [`DREGS_PERCENT`] of the rate, forever.
//! - **Workers** are regiments of a worker type (the setup's unit roles).
//!   Ordered to a node ([`crate::Command::Gather`]) a worker walks there,
//!   fills its load over [`GRUB_TICKS`] (or [`MARROW_TICKS`]), carries it to
//!   the nearest of its owner's buildings that takes that resource, puts it
//!   in the store and goes back for more, until given another order. A
//!   worker with a load given another order keeps it, and drops it off the
//!   next time it gathers.
//! - **Supply.** A player's cap is what their standing buildings add, up to
//!   [`SUPPLY_LIMIT`]; what their regiments need counts against it.
//! - **Upkeep.** Grub brought in is cut by how much supply is in use: all
//!   of it up to 60, 75% up to 100, 55% beyond. Marrow is never cut.
//! - **Tempo** scales gathering: a worker fills its load in
//!   `100 / gather_percent` of the time.
//!
//! Five Hands on a seep beside their hall bring in about 200 Grub a
//! minute, and three on a vein about 50 Marrow, as docs/04 sets.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::regiments::Regiments;
use crate::structures::{DROP_OFF_GRUB, DROP_OFF_MARROW, Structures};
use crate::{MAX_PARTIES, PlayerId, StateHasher, TICKS_PER_SECOND};

/// What a node yields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum Resource {
    #[default]
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
pub const GRUB_TICKS: u32 = 8 * TICKS_PER_SECOND;
pub const MARROW_TICKS: u32 = 14 * TICKS_PER_SECOND;
/// A dry node still yields at this percent of its rate.
pub const DREGS_PERCENT: u32 = 25;
/// The most supply a player can have.
pub const SUPPLY_LIMIT: u32 = 150;
/// Upkeep: (supply in use up to, percent of Grub kept).
pub const UPKEEP: [(u32, u32); 3] = [(60, 100), (100, 75), (SUPPLY_LIMIT, 55)];
/// How near a worker's centre must be to a node to gather from it, tiles.
pub const NODE_REACH: Fx = Fx::from_int(2);

/// A seep or vein as the match setup places it.
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

/// A player's store, as the game's top bar shows it.
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
        UPKEEP
            .iter()
            .find(|&&(up_to, _)| self.supply_used <= up_to)
            .map_or(UPKEEP[UPKEEP.len() - 1].1, |&(_, keep)| keep)
    }
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
