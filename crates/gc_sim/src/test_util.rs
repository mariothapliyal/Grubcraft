//! Helpers the sim's unit tests share: a small open battlefield, orders
//! sent through a tick, and regiments raised on it.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::nav::CELLS_PER_TILE;
use crate::scenario::NORTH;
use crate::{
    Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World,
};

/// The side of [`world`]'s map, in tiles.
pub(crate) const MAP: u32 = 128;

/// A flat, open [`MAP`]-tile battlefield with room for 16 regiments of
/// `units`.
pub(crate) fn world(units: &[UnitStats]) -> World {
    let side = (MAP as usize) * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 16 },
        units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: crate::Sky::STILL,
        generals: false,
    })
}

/// `player` gives `command` in a tick of its own: whether it was applied.
pub(crate) fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

/// A regiment of `unit` raised for `player` at (`x`, `y`), facing north.
pub(crate) fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let command = Command::SpawnRegiment {
        unit,
        position: FxVec2::from_ints(x, y),
        facing: NORTH,
    };
    assert!(order(w, player, command));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}
