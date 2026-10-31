//! A flat test map to paint rivers, woods and thicket on, made by the
//! game's own map maker, played with the real game data (every faction's
//! units and buildings), and the orders the scenarios give on it.

use core::ops::Range;
use std::cell::OnceCell;

use gc_fixed::{Angle, Fx, FxVec2};
use gc_map::Map;
use gc_map::def::{CoverKind, CoverOp, MapDef, Mode, Shape, SizeClass, Symmetry, TerrainOp};
use gc_sim::terrain::Cover;
use gc_sim::{
    Capacities, Command, ConquestSetup, MatchSetup, PlayerCommand, PlayerId, RegimentHandle,
    RegimentView, StructureHandle, World,
};

pub(crate) use crate::headless::Real;

/// Dry ground, the water line and a river's bed, in sixteenths of a tile
/// (as a map file writes them): the river is a tile and a half deep, far
/// past wading.
const LAND: i32 = 40;
const WATER_LINE: i32 = 24;
const RIVER_BED: i32 = 0;

/// A square map `tiles` across, open and flat until painted, made by the
/// game's own map maker (`gc_map`) from a description, as a map file is:
/// the same heights, river banks, woods and thicket rules as a real match.
pub(crate) struct Arena {
    def: MapDef,
    /// The map, made the first time it is asked for; painting after that
    /// is a mistake.
    map: OnceCell<Map>,
}

impl Arena {
    pub fn new(tiles: u32) -> Arena {
        Arena {
            def: MapDef {
                name: "Stress arena".into(),
                size: SizeClass::Field(tiles as i32),
                symmetry: Symmetry::None,
                players: 2,
                base: LAND,
                water: WATER_LINE,
                noise: None,
                grown: None,
                terrain: Vec::new(),
                cover: Vec::new(),
                land: Vec::new(),
                objects: Vec::new(),
                set_pieces: Vec::new(),
            },
            map: OnceCell::new(),
        }
    }

    /// The tiles with x in `xs` and y in `ys` as a map shape.
    fn rect(xs: Range<i32>, ys: Range<i32>) -> Shape {
        assert!(!xs.is_empty() && !ys.is_empty(), "nothing to paint");
        Shape::Rect {
            min: (xs.start, ys.start),
            max: (xs.end, ys.end),
        }
    }

    fn paint(&mut self) -> &mut MapDef {
        assert!(self.map.get().is_none(), "painted after the map was made");
        &mut self.def
    }

    /// Deep water no one wades (a bridge crosses it). Its banks are sheer,
    /// so, as on a real map, the half of each bank tile next to the water
    /// is blocked to the march too.
    pub fn river(&mut self, xs: Range<i32>, ys: Range<i32>) {
        let shape = Arena::rect(xs, ys);
        self.paint().terrain.push(TerrainOp {
            shape,
            mode: Mode::Set,
            height: RIVER_BED,
            edge: 0,
        });
    }

    /// Woods: they hide regiments in them and slow the march.
    pub fn woods(&mut self, xs: Range<i32>, ys: Range<i32>) {
        self.cover(xs, ys, CoverKind::Wood);
    }

    /// Thicket: woods no one marches through.
    pub fn thicket(&mut self, xs: Range<i32>, ys: Range<i32>) {
        self.cover(xs, ys, CoverKind::Thicket);
    }

    fn cover(&mut self, xs: Range<i32>, ys: Range<i32>, kind: CoverKind) {
        let shape = Arena::rect(xs, ys);
        self.paint().cover.push(CoverOp { shape, kind });
    }

    /// The map as painted.
    fn map(&self) -> &Map {
        self.map.get_or_init(|| Map::from_def(&self.def))
    }

    /// The map and tile `(x, y)`'s place in its rows, if it is on it.
    fn tile(&self, x: i32, y: i32) -> Option<(&Map, usize)> {
        let map = self.map();
        let n = map.tiles as i32;
        ((0..n).contains(&x) && (0..n).contains(&y)).then(|| (map, (y * n + x) as usize))
    }

    /// Whether tile `(x, y)` is under water.
    pub fn is_river(&self, x: i32, y: i32) -> bool {
        self.tile(x, y).is_some_and(|(m, t)| m.heights[t] < m.water)
    }

    /// Whether tile `(x, y)` is woods or thicket.
    pub fn is_woods(&self, x: i32, y: i32) -> bool {
        self.tile(x, y)
            .is_some_and(|(m, t)| m.cover[t] == Cover::Wood as u8)
    }

    /// A two-player Conquest world on this map with every faction's units
    /// and buildings, each player with `stores` Grub and Marrow, nobody
    /// winning however it goes.
    pub fn world(&self, real: &Real, seed: u64, stores: u32) -> World {
        let map = self.map();
        let nav = map.sim_nav_costs();
        let mut world = World::new(&MatchSetup {
            seed,
            capacities: Capacities { regiments: 128 },
            units: &real.units,
            map_tiles: map.tiles,
            nav: &nav,
            teams: &[0, 1],
            ground: map.ground(),
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

/// Put up finished buildings of `player`'s, each (building, north-west
/// tile, turn), all in one tick, and their handles in the same order.
/// Setting up, so the tick is not timed.
pub(crate) fn put_up(
    world: &mut World,
    player: PlayerId,
    pieces: &[(u16, (i32, i32), u8)],
) -> Vec<StructureHandle> {
    let orders: Vec<PlayerCommand> = (pieces.iter())
        .map(|&(building, (x, y), turn)| PlayerCommand {
            player,
            command: Command::SpawnStructure {
                building,
                tile: (x as i16, y as i16),
                turn,
            },
        })
        .collect();
    let stats = world.tick(&orders);
    assert_eq!(
        stats.commands_applied as usize,
        pieces.len(),
        "buildings refused"
    );
    let s = world.structures().unwrap();
    (pieces.iter())
        .map(|&(_, tile, _)| {
            (s.iter())
                .find(|v| v.tile == tile && v.owner == player)
                .unwrap()
                .handle
        })
        .collect()
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

/// What a march found: when every regiment still on the field stood at
/// its goal, how close that came to the best pace, and the losses.
pub(crate) struct March {
    /// The tick every regiment still on the field was standing within
    /// [`NEAR`] of its goal, or `None` if the limit came first.
    pub done: Option<u32>,
    /// The slowest regiment's straight march over open ground, in ticks:
    /// the best the army could do.
    pub ideal: u32,
    /// Soldiers at the start and at the end.
    pub soldiers: (u32, u32),
    /// The limit, in seconds.
    limit: u32,
}

impl March {
    /// The ideal time as a percent of the time taken (100 is a straight
    /// march on open ground with nothing in the way), or 0 if never done.
    pub fn pace_percent(&self) -> u32 {
        self.done.map_or(0, |t| self.ideal * 100 / t.max(1))
    }

    /// Report the march and check it: every regiment there and standing
    /// within the limit, at least `min_pace` percent of the straight
    /// march's pace, and no soldier lost.
    pub fn report(&self, report: &mut super::Report, min_pace: u32) {
        let pace = self.pace_percent();
        report.metric("soldiers", self.soldiers.0);
        report.metric("all arrived and standing", super::seconds(self.done));
        report.metric(
            "straight march on open ground",
            super::seconds(Some(self.ideal)),
        );
        report.metric("pace, percent of the straight march", pace);
        report.check(
            format!("every regiment arrives and stands within {} s", self.limit),
            self.done.is_some(),
        );
        report.check(
            format!("at least {min_pace}% of the straight march's pace"),
            pace >= min_pace,
        );
        report.check("no soldier lost", self.soldiers.1 == self.soldiers.0);
    }
}

/// How near its goal a regiment must stand to have arrived.
const NEAR: Fx = Fx::from_int(3);

/// March each of `army` to the goal of the same place in `goals`, ticking
/// (timed) for at most `limit` seconds, calling `each` with the tick after
/// every tick.
pub(crate) fn march(
    real: &Real,
    world: &mut World,
    report: &mut super::Report,
    army: &[RegimentHandle],
    goals: &[FxVec2],
    limit: u32,
    mut each: impl FnMut(u32, &World),
) -> March {
    assert_eq!(army.len(), goals.len());
    let tps = f64::from(gc_sim::TICKS_PER_SECOND);
    let mut ideal = 0;
    let mut orders = Vec::new();
    for (r, &g) in army.iter().zip(goals) {
        let v = of(world, core::slice::from_ref(r))
            .next()
            .expect("on the field");
        // Tiles over tiles a second: the fixed point's scale cancels.
        let (tiles, speed) = (
            (g - v.position).length(),
            real.units[usize::from(v.unit)].speed,
        );
        ideal = ideal.max((f64::from(tiles.raw()) / f64::from(speed.raw().max(1)) * tps) as u32);
        orders.push(move_to(v.owner, *r, g));
    }
    let start = soldiers(world, army);
    report.tick(world, &orders);
    each(0, world);
    let mut done = None;
    for t in 1..=limit * gc_sim::TICKS_PER_SECOND {
        report.tick(world, &[]);
        each(t, world);
        // Every regiment still on the field there and standing (losses
        // are checked on their own).
        let (mut alive, mut there) = (0, 0);
        for v in of(world, army) {
            let g = goals[army.iter().position(|&r| r == v.handle).unwrap()];
            alive += 1;
            there += usize::from(!v.moving && (g - v.position).length() <= NEAR);
        }
        if alive > 0 && there == alive {
            done = Some(t);
            break;
        }
    }
    March {
        done,
        ideal,
        soldiers: (start, soldiers(world, army)),
        limit,
    }
}

/// The map maker paints what the scenarios ask for, corners and map
/// edges included: a river exactly as wide as asked and blocked to the
/// march, woods where asked, and thicket blocked too.
#[test]
fn the_arena_paints_what_it_is_asked() {
    let mut arena = Arena::new(64);
    arena.river(30..34, 0..64);
    arena.woods(4..20, 0..64);
    arena.thicket(8..12, 20..25);
    for y in [0, 31, 63] {
        let wet: Vec<i32> = (0..64).filter(|&x| arena.is_river(x, y)).collect();
        assert_eq!(wet, (30..34).collect::<Vec<_>>(), "row {y}");
    }
    for y in [0, 40, 63] {
        let woods: Vec<i32> = (0..64).filter(|&x| arena.is_woods(x, y)).collect();
        assert_eq!(woods, (4..20).collect::<Vec<_>>(), "row {y}");
    }
    let nav = arena.map().nav_costs();
    let cells = gc_sim::nav::CELLS_PER_TILE as usize;
    let side = 64 * cells;
    let cell = |x: usize, y: usize| nav[y * cells * side + x * cells];
    assert_eq!(cell(32, 31), gc_sim::nav::BLOCKED, "the river");
    for (x, y) in [(8, 20), (11, 20), (8, 24), (11, 24)] {
        assert_eq!(
            cell(x, y),
            gc_sim::nav::BLOCKED,
            "the thicket's corner ({x}, {y})"
        );
    }
    assert_ne!(cell(12, 24), gc_sim::nav::BLOCKED, "beside the thicket");
    assert_ne!(cell(15, 40), gc_sim::nav::BLOCKED, "the woods");
    assert_ne!(cell(45, 31), gc_sim::nav::BLOCKED, "open ground");
}
