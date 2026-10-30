//! A flat test map to paint rivers, woods and thicket on, played with the
//! real game data (every faction's units and buildings), and the orders
//! the scenarios give on it.

use gc_data::GameData;
use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::terrain::{Cover, Growth};
use gc_sim::{
    Capacities, Command, ConquestSetup, ConquestUnit, Ground, MatchSetup, PlayerCommand, PlayerId,
    RegimentHandle, RegimentView, StructureHandle, UnitStats, World,
};

use crate::buildings::Table;
use crate::conquest;

/// The real game data, as a Conquest world reads it.
pub(crate) struct Real {
    pub data: GameData<'static>,
    pub table: Table,
    pub units: Vec<UnitStats>,
    pub conquest_units: Vec<ConquestUnit>,
}

impl Real {
    pub fn load() -> Real {
        let data = crate::data::load();
        Real {
            table: Table::new(&data),
            units: conquest::unit_stats(&data),
            conquest_units: conquest::units_of(&data),
            data,
        }
    }

    /// The Conquest unit called `name`.
    pub fn unit(&self, name: &str) -> u16 {
        (self.data.conquest_units.iter())
            .position(|u| u.name() == name)
            .unwrap_or_else(|| panic!("no unit called {name}")) as u16
    }

    /// The Ember Clans' spearmen, swordsmen and archers.
    pub fn ember_line(&self) -> [u16; 3] {
        ["Yari Ashigaru", "Katana Samurai", "Yumi Ashigaru"].map(|n| self.unit(n))
    }

    /// The building called `name`.
    pub fn building(&self, name: &str) -> u16 {
        (0..self.table.stats.len() as u16)
            .find(|&b| self.table.name(b) == name)
            .unwrap_or_else(|| panic!("no building called {name}"))
    }
}

/// Height of dry ground and the water line, in the sim's height units.
const LAND: u16 = 300;
const WATER_LINE: u16 = 256;

/// A square map `tiles` across, open and flat until painted.
pub(crate) struct Arena {
    pub tiles: u32,
    nav: Vec<u8>,
    heights: Vec<u16>,
    cover: Vec<u8>,
    growth: Vec<u8>,
}

impl Arena {
    pub fn new(tiles: u32) -> Arena {
        let n = tiles as usize;
        let side = n * CELLS_PER_TILE as usize;
        Arena {
            tiles,
            nav: vec![1; side * side],
            heights: vec![LAND; n * n],
            cover: vec![Cover::Open as u8; n * n],
            growth: vec![Growth::Open as u8; n * n],
        }
    }

    /// Every tile with x in `xs` and y in `ys`.
    fn each(
        &mut self,
        xs: core::ops::Range<i32>,
        ys: core::ops::Range<i32>,
        mut paint: impl FnMut(&mut Arena, usize),
    ) {
        for y in ys {
            for x in xs.clone() {
                if let Some(t) = self.index(x, y) {
                    paint(self, t);
                }
            }
        }
    }

    /// Tile `(x, y)`'s place in the rows, if it is on the map.
    fn index(&self, x: i32, y: i32) -> Option<usize> {
        let n = self.tiles as i32;
        ((0..n).contains(&x) && (0..n).contains(&y)).then(|| (y * n + x) as usize)
    }

    /// Deep water no one wades (a bridge crosses it).
    pub fn river(&mut self, xs: core::ops::Range<i32>, ys: core::ops::Range<i32>) {
        self.each(xs, ys, |a, t| {
            a.heights[t] = 0;
            a.block(t);
        });
    }

    /// Woods: they hide regiments in them and slow the march.
    pub fn woods(&mut self, xs: core::ops::Range<i32>, ys: core::ops::Range<i32>) {
        self.each(xs, ys, |a, t| {
            a.cover[t] = Cover::Wood as u8;
            a.growth[t] = Growth::Woods as u8;
        });
    }

    /// Thicket: woods no one marches through.
    pub fn thicket(&mut self, xs: core::ops::Range<i32>, ys: core::ops::Range<i32>) {
        self.each(xs, ys, |a, t| {
            a.cover[t] = Cover::Wood as u8;
            a.growth[t] = Growth::Thicket as u8;
        });
    }

    /// Mark tile `t`'s cells blocked.
    fn block(&mut self, t: usize) {
        let n = self.tiles as usize;
        let c = CELLS_PER_TILE as usize;
        let side = n * c;
        let (x, y) = (t % n, t / n);
        for cy in y * c..(y + 1) * c {
            for cx in x * c..(x + 1) * c {
                self.nav[cy * side + cx] = BLOCKED;
            }
        }
    }

    /// Whether tile `(x, y)` is river.
    pub fn is_river(&self, x: i32, y: i32) -> bool {
        self.index(x, y)
            .is_some_and(|t| self.heights[t] < WATER_LINE)
    }

    /// Whether tile `(x, y)` is woods or thicket.
    pub fn is_woods(&self, x: i32, y: i32) -> bool {
        self.index(x, y)
            .is_some_and(|t| self.cover[t] == Cover::Wood as u8)
    }

    /// A two-player Conquest world on this map with every faction's units
    /// and buildings, each player with `stores` Grub and Marrow, nobody
    /// winning however it goes.
    pub fn world(&self, real: &Real, seed: u64, stores: u32) -> World {
        let ground = Ground {
            heights: &self.heights,
            water_level: WATER_LINE,
            cover: &self.cover,
            growth: &self.growth,
            ..Ground::FLAT
        };
        let mut world = World::new(&MatchSetup {
            seed,
            capacities: Capacities { regiments: 128 },
            units: &real.units,
            map_tiles: self.tiles,
            nav: &self.nav,
            teams: &[0, 1],
            ground,
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: false,
        });
        let mut start = [(0, 0); gc_sim::MAX_PARTIES];
        start[0] = (stores, stores);
        start[1] = (stores, stores);
        assert!(world.conquest(&ConquestSetup {
            buildings: &real.table.stats,
            units: &real.conquest_units,
            structures: 1024,
            start,
            ..ConquestSetup::default()
        }));
        world
    }
}

/// The middle of tile `(x, y)`.
pub(crate) fn centre(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_ratio(2 * x + 1, 2), Fx::from_ratio(2 * y + 1, 2))
}

/// Facing east and west.
pub(crate) const EAST: Angle = Angle(0);
pub(crate) const WEST: Angle = Angle(0x8000);

/// Put down `regiments` (unit, where) of `player`'s in one tick, and their
/// handles in the same order. Setting up, so the tick is not timed.
pub(crate) fn spawn(
    world: &mut World,
    player: PlayerId,
    regiments: &[(u16, FxVec2)],
    facing: Angle,
) -> Vec<RegimentHandle> {
    let before: Vec<RegimentHandle> = world.regiments.iter().map(|r| r.handle).collect();
    let orders: Vec<PlayerCommand> = regiments
        .iter()
        .map(|&(unit, position)| PlayerCommand {
            player,
            command: Command::SpawnRegiment {
                unit,
                position,
                facing,
            },
        })
        .collect();
    let stats = world.tick(&orders);
    assert_eq!(
        stats.commands_applied as usize,
        orders.len(),
        "spawn refused"
    );
    // Handles are handed out in order, so the new ones come in the order
    // they were asked for.
    let mut new: Vec<RegimentHandle> = world
        .regiments
        .iter()
        .map(|r| r.handle)
        .filter(|h| !before.contains(h))
        .collect();
    new.sort_unstable_by_key(|h| h.index);
    new
}

/// Put up a finished `building` of `player`'s at `tile`, turned `turn`.
/// Setting up, so the tick is not timed.
pub(crate) fn put_up(
    world: &mut World,
    player: PlayerId,
    building: u16,
    tile: (i32, i32),
    turn: u8,
) -> StructureHandle {
    let stats = world.tick(&[PlayerCommand {
        player,
        command: Command::SpawnStructure {
            building,
            tile: (tile.0 as i16, tile.1 as i16),
            turn,
        },
    }]);
    assert_eq!(
        stats.commands_applied, 1,
        "building {building} at {tile:?} refused"
    );
    (world.structures().unwrap().iter())
        .find(|s| s.tile == tile && s.owner == player)
        .unwrap()
        .handle
}

/// March `regiment` of `player`'s to `to`.
pub(crate) fn move_to(player: PlayerId, regiment: RegimentHandle, to: FxVec2) -> PlayerCommand {
    PlayerCommand {
        player,
        command: Command::MoveTo {
            regiment,
            target: to,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }
}

/// The regiments of `regiments` still on the field.
pub(crate) fn of<'a>(
    world: &'a World,
    regiments: &'a [RegimentHandle],
) -> impl Iterator<Item = RegimentView> + 'a {
    (world.regiments.iter()).filter(|r| regiments.contains(&r.handle))
}

/// Soldiers alive in `regiments`.
pub(crate) fn soldiers(world: &World, regiments: &[RegimentHandle]) -> u32 {
    of(world, regiments).map(|r| u32::from(r.models)).sum()
}

/// The middle of the `i`th tile of a grid from `origin`, `per_row` across
/// and `step` tiles apart.
pub(crate) fn grid(origin: (i32, i32), per_row: usize, step: (i32, i32), i: usize) -> FxVec2 {
    let (col, row) = ((i % per_row) as i32, (i / per_row) as i32);
    centre(origin.0 + col * step.0, origin.1 + row * step.1)
}

/// Orders for each of `attackers` (of `player`'s) to attack the nearest
/// regiment of `foe`'s.
pub(crate) fn attack_nearest(
    world: &World,
    player: PlayerId,
    attackers: &[RegimentHandle],
    foe: PlayerId,
) -> Vec<PlayerCommand> {
    let foes: Vec<_> = world.regiments.iter().filter(|r| r.owner == foe).collect();
    (world.regiments.iter())
        .filter(|r| attackers.contains(&r.handle))
        .filter_map(|r| {
            let target = foes
                .iter()
                .min_by_key(|f| (f.position - r.position).length_sq_raw())?;
            Some(PlayerCommand {
                player,
                command: Command::Attack {
                    regiment: r.handle,
                    target: target.handle,
                },
            })
        })
        .collect()
}
