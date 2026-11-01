//! Bridges (docs/26-fortifications.md, "Bridges"): a worker draws a bridge
//! straight across a river and builds it out from the bank; once up anyone
//! crosses it dry, and when a tile falls those on it drop into the river.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::structures::{Armour, LINE, Refusal, Role, WallTwist};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Ground, MatchSetup,
    PlayerCommand, RegimentHandle, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const FOOT: u16 = 1;
const GNAWER: u16 = 2;
const HALL: u16 = 0;
const BRIDGE: u16 = 1;
const MAP: u32 = 64;
/// The river runs north to south over these columns, deep (no ford).
const RIVER: core::ops::Range<i32> = 30..34;
/// Height of the ground, and the water line under it.
const LAND: u16 = 300;
const WATER: u16 = 256;
/// The row the bridge is drawn along, from the west bank to the east.
const ROW: i32 = 20;
/// Rock no one walks on, but dry: tiles across row `CRAG_ROW`.
const CRAG: core::ops::Range<i32> = 10..13;
const CRAG_ROW: i32 = 40;

fn world() -> World {
    world_over(false)
}

/// The test world, its river a ford (shallow, waded) if `ford`.
fn world_over(ford: bool) -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav: Vec<u8> = (0..side * side)
        .map(|c| {
            let x = (c % side) as i32 / CELLS_PER_TILE;
            let y = (c / side) as i32 / CELLS_PER_TILE;
            u8::from((ford || !RIVER.contains(&x)) && !(CRAG.contains(&x) && y == CRAG_ROW))
        })
        .collect();
    let n = MAP as usize;
    let heights: Vec<u16> = (0..n * n)
        .map(|t| {
            if RIVER.contains(&((t % n) as i32)) {
                if ford { WATER - 20 } else { 0 }
            } else {
                LAND
            }
        })
        .collect();
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 16 },
        units: &[worker, DRILL_UNITS[0], worker],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: WATER,
            ..Ground::FLAT
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 1000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 0,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let bridge = BuildingStats {
        role: Role::Bridge,
        cost_grub: 12,
        build_ticks: 20,
        hp: 300,
        armour: Armour::Structure,
        width: 1,
        depth: 1,
        flags: LINE,
        ..hall
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (2000, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, bridge],
        units: &[
            ConquestUnit {
                worker: true,
                ..ConquestUnit::default()
            },
            ConquestUnit::default(),
            ConquestUnit {
                worker: true,
                consumed: true,
                ..ConquestUnit::default()
            },
        ],
        structures: 512,
        start,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (58, 58))] {
        order(
            &mut w,
            player,
            Command::SpawnStructure {
                building: HALL,
                tile,
                turn: 0,
            },
        );
    }
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        0,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: Angle(0x4000),
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn draw(w: &mut World, worker: Option<RegimentHandle>, from: i32, to: (i32, i32)) -> bool {
    order(
        w,
        0,
        Command::BuildWall {
            regiment: worker,
            building: BRIDGE,
            from: (from as i16, ROW as i16),
            to: (to.0 as i16, to.1 as i16),
        },
    )
}

/// Put up a finished bridge, both lanes of it, along [`ROW`] and the row
/// below it from x = 27 to 36.
fn span(w: &mut World) {
    for (x, y) in (27..=36).flat_map(|x| [(x, ROW), (x, ROW + 1)]) {
        let tile = (x, y as i16);
        assert!(order(
            w,
            0,
            Command::SpawnStructure {
                building: BRIDGE,
                tile,
                turn: 0
            }
        ));
    }
    w.tick(&[]);
}

fn tile(w: &World, x: i32) -> StructureHandle {
    let s = w.structures().unwrap();
    s.iter().find(|v| v.tile == (x, ROW)).unwrap().handle
}

fn centre(x: i32) -> FxVec2 {
    FxVec2::new(Fx::from_ratio(2 * x + 1, 2), Fx::from_ratio(2 * ROW + 1, 2))
}

#[test]
fn a_bridge_is_drawn_straight_from_bank_to_bank() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 25, ROW);
    let plan = |w: &World, from: (i32, i32), to: (i32, i32)| {
        w.wall_plan(0, BRIDGE, from, to).map(|p| p.len())
    };
    // Two lanes wide: the line drawn and the row beside it.
    assert_eq!(plan(&w, (27, ROW), (36, ROW)), Ok(20));
    // Not on the slant, not ending in the water, and not on dry land.
    assert_eq!(plan(&w, (27, ROW), (36, ROW + 3)), Err(Refusal::Crooked));
    assert_eq!(plan(&w, (27, ROW), (32, ROW)), Err(Refusal::Rough));
    assert_eq!(plan(&w, (10, ROW), (20, ROW)), Err(Refusal::Rough));
    // Rock is not water, and nothing bridges it.
    assert_eq!(plan(&w, (8, CRAG_ROW), (15, CRAG_ROW)), Err(Refusal::Rough));
    assert!(draw(&mut w, Some(worker), 27, (36, ROW)));
    // Laid but not built: the river is still deep, the banks are built
    // on, and nothing else goes up over it.
    assert!(!w.nav().walkable(0, centre(31)));
    assert!(!w.nav().walkable(0, centre(27)));
    assert_eq!(w.can_build(BRIDGE, (27, ROW), 0), Err(Refusal::Built));
    // Drawn again over itself, nothing is new.
    assert_eq!(plan(&w, (27, ROW), (36, ROW)), Err(Refusal::Rough));
}

#[test]
fn a_gap_is_drawn_again_from_deck_to_deck() {
    let mut w = world();
    span(&mut w);
    let worker = spawn(&mut w, WORKER, 25, ROW);
    let gone = tile(&w, 31);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    w.tick(&[]);
    assert!(!w.nav().walkable(0, centre(31)));
    let plan = w.wall_plan(0, BRIDGE, (30, ROW), (32, ROW)).unwrap();
    assert_eq!(plan.len(), 1, "only the gap is new");
    assert_eq!(plan[0].tile, (31, ROW));
    assert!(draw(&mut w, Some(worker), 30, (32, ROW)));
    for _ in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert!(w.nav().walkable(0, centre(31)), "mended");
}

#[test]
fn a_worker_builds_it_out_from_the_bank_and_troops_cross_it() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 25, ROW);
    assert!(draw(&mut w, Some(worker), 27, (36, ROW)));
    for _ in 0..120 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    assert!(
        s.iter()
            .filter(|v| v.building == BRIDGE)
            .all(|v| v.finished()),
        "built across"
    );
    assert!(w.nav().walkable(0, centre(31)));
    // Foot march across, dry-shod.
    let foot = spawn(&mut w, FOOT, 22, ROW);
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: foot,
            target: FxVec2::from_ints(42, ROW),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    let mut dry = true;
    for _ in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = w
            .regiments
            .iter()
            .find(|r| r.handle == foot)
            .unwrap()
            .position;
        dry &= !w.terrain().wet(p);
    }
    let p = w
        .regiments
        .iter()
        .find(|r| r.handle == foot)
        .unwrap()
        .position;
    assert!(p.x > Fx::from_int(38), "{p:?}");
    assert!(dry, "nobody wades on a bridge");
}

#[test]
fn nothing_else_is_built_on_a_bridge() {
    let mut w = world();
    span(&mut w);
    assert_eq!(w.can_build(BRIDGE, (31, ROW), 0), Err(Refusal::Built));
    // Nothing new to draw over its own bridge.
    assert_eq!(
        w.wall_plan(0, BRIDGE, (27, ROW), (36, ROW))
            .map(|p| p.len()),
        Err(Refusal::Rough)
    );
}

#[test]
fn those_on_a_tile_that_falls_drop_into_the_river() {
    let mut w = world();
    span(&mut w);
    let foot = spawn(&mut w, FOOT, 31, ROW);
    // Stand it on the bridge's middle.
    let v = |w: &World| w.regiments.iter().find(|r| r.handle == foot).unwrap();
    let models = v(&w).models;
    let hp = |w: &World| -> u32 {
        w.regiments
            .model_hp(foot)
            .unwrap()
            .iter()
            .map(|&h| u32::from(h))
            .sum()
    };
    let before = hp(&w);
    assert!(w.nav().walkable(0, v(&w).position));
    let gone = tile(&w, 31);
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    let r = v(&w);
    assert_eq!(r.models, models, "hurt, not drowned");
    assert!(hp(&w) < before);
    assert!(r.disorder > 0);
    assert!(
        w.nav().walkable(0, r.position),
        "out on the bank: {:?}",
        r.position
    );
}

#[test]
fn a_gnawer_builds_a_bridge_by_hand_and_is_not_used_up() {
    let mut w = world();
    let gnawer = spawn(&mut w, GNAWER, 25, ROW);
    assert!(draw(&mut w, Some(gnawer), 27, (36, ROW)));
    for _ in 0..120 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    assert!(
        s.iter()
            .filter(|v| v.building == BRIDGE)
            .all(|v| v.finished()),
        "built across"
    );
    assert!(
        w.regiments.iter().any(|r| r.handle == gnawer),
        "still alive"
    );
}

#[test]
fn a_bridge_is_two_lanes_wide_and_both_are_built() {
    let mut w = world();
    let worker = spawn(&mut w, WORKER, 25, ROW);
    let plan = w.wall_plan(0, BRIDGE, (27, ROW), (36, ROW)).unwrap();
    let rows = |row: i32| plan.iter().filter(|t| t.tile.1 == row).count();
    assert_eq!((rows(ROW), rows(ROW + 1)), (10, 10));
    let grub = w.bank(0).unwrap().grub;
    assert!(draw(&mut w, Some(worker), 27, (36, ROW)));
    assert_eq!(grub - w.bank(0).unwrap().grub, 20 * 12, "12 Grub a tile");
    for _ in 0..240 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let s = w.structures().unwrap();
    assert!(
        s.iter()
            .filter(|v| v.building == BRIDGE)
            .all(|v| v.finished())
    );
    for y in [ROW, ROW + 1] {
        assert!(w.nav().walkable(
            0,
            FxVec2::from_ints(31, y) + FxVec2::new(Fx::HALF, Fx::HALF)
        ));
    }
    // A lane's tile knows its side: the lower lane runs towards higher
    // coordinates, the upper one back.
    let line = |y: i32| {
        s.iter()
            .find(|v| v.tile == (31, y))
            .unwrap()
            .line
            .unwrap()
            .map(i32::from)
    };
    assert_eq!(line(ROW), [27, ROW, 36, ROW]);
    assert_eq!(line(ROW + 1), [36, ROW + 1, 27, ROW + 1]);
}

#[test]
fn the_second_lane_goes_to_the_other_side_at_the_edge_of_the_map() {
    let w = world();
    let last = MAP as i32 - 1;
    let plan = w.wall_plan(0, BRIDGE, (27, last), (36, last)).unwrap();
    let rows = |row: i32| plan.iter().filter(|t| t.tile.1 == row).count();
    assert_eq!((rows(last - 1), rows(last)), (10, 10));
}

#[test]
fn a_gap_in_the_upper_lane_is_mended_without_a_third_lane() {
    let mut w = world();
    span(&mut w);
    let worker = spawn(&mut w, WORKER, 25, ROW);
    let s = w.structures().unwrap();
    let gone = s.iter().find(|v| v.tile == (31, ROW + 1)).unwrap().handle;
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    w.tick(&[]);
    // Redrawn along the upper lane, only its gap is new: no lane above it.
    let plan = w
        .wall_plan(0, BRIDGE, (30, ROW + 1), (32, ROW + 1))
        .unwrap();
    assert_eq!(
        plan.iter().map(|t| t.tile).collect::<Vec<_>>(),
        [(31, ROW + 1)]
    );
    assert!(order(
        &mut w,
        0,
        Command::BuildWall {
            regiment: Some(worker),
            building: BRIDGE,
            from: (30, (ROW + 1) as i16),
            to: (32, (ROW + 1) as i16),
        }
    ));
    let s = w.structures().unwrap();
    let line = s
        .iter()
        .find(|v| v.tile == (31, ROW + 1))
        .unwrap()
        .line
        .unwrap();
    assert_eq!(
        line.map(i32::from),
        [32, ROW + 1, 30, ROW + 1],
        "stored running back"
    );
    assert!(s.iter().all(|v| v.tile.1 != ROW + 2));
}

#[test]
fn a_block_crossing_a_bridge_over_a_ford_keeps_to_the_deck() {
    // A ford anyone may wade; a two-lane bridge over it. A block wider
    // than the bridge narrows onto the deck instead of wading beside it.
    let mut w = world_over(true);
    span(&mut w);
    let foot = spawn(&mut w, FOOT, 20, ROW + 1);
    let wide = w.regiments.iter().find(|r| r.handle == foot).unwrap().files;
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: foot,
            target: FxVec2::from_ints(44, ROW + 1),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    let stats = DRILL_UNITS[0];
    let mut widest_on_deck = Fx::ZERO;
    let mut on_deck = 0;
    for _ in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        let r = w.regiments.iter().find(|r| r.handle == foot).unwrap();
        // Over the middle of the river, well onto the deck.
        if (Fx::from_int(RIVER.start + 1)..Fx::from_int(RIVER.end - 1)).contains(&r.position.x) {
            on_deck += 1;
            let (width, _) = stats.block_size_in(r.formation, r.files, r.models);
            widest_on_deck = widest_on_deck.max(width);
        }
    }
    assert!(on_deck > 0, "it crossed by the bridge");
    let (full, _) = stats.block_size_in(gc_sim::Formation::Line, wide, stats.models);
    assert!(
        full > Fx::from_int(2),
        "wider than the bridge to begin with: {full:?}"
    );
    assert!(
        widest_on_deck <= Fx::from_int(2),
        "{widest_on_deck:?} across a 2-tile deck"
    );
}
