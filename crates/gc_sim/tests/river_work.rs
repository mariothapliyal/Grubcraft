//! The water engine, stage 3b (docs/39-water-engine.md, section 3):
//! workers fell bank woods into a river (the Clans' Fell order) and tip
//! rafts in (the Dump order), and what they put in rides the current.

use gc_fixed::FxVec2;
use gc_sim::clearing::ClearStyle;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::rivers::debris::{Kind, RAFT_GRUB};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, DROP_OFF_GRUB, Role};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Cover, Flow, Ground, Growth,
    MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World,
};

const MAP: i32 = 64;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
const START_GRUB: u32 = 500;
/// A river running east off the map, with a ford near its top.
const RIVER: core::ops::Range<i32> = 30..34;
const FORD: core::ops::Range<i32> = 4..7;
/// A wood on the north bank, its last row beside the river.
const WOOD_X: core::ops::Range<i32> = 10..14;
const WOOD_Y: core::ops::Range<i32> = 27..30;

fn river(y: i32) -> bool {
    RIVER.contains(&y)
}

fn world(style: ClearStyle) -> World {
    world_with(style, START_GRUB)
}

fn world_with(style: ClearStyle, start_grub: u32) -> World {
    let n = MAP as usize;
    let heights: Vec<u16> = (0..MAP * MAP)
        .map(|i| match (i % MAP, i / MAP) {
            (x, y) if river(y) && FORD.contains(&x) => WATER - 40,
            (_, y) if river(y) => 20,
            _ => WATER + 300,
        })
        .collect();
    let side = MAP * CELLS_PER_TILE;
    let nav: Vec<u8> = (0..side * side)
        .map(|c| {
            let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
            match () {
                _ if river(y) && FORD.contains(&x) => 3,
                _ if river(y) => BLOCKED,
                _ => 1,
            }
        })
        .collect();
    let flow: Vec<u8> = (0..MAP * MAP)
        .map(|i| {
            if river(i / MAP) {
                Flow::byte(gc_fixed::Angle(0), 2)
            } else {
                0
            }
        })
        .collect();
    let mut cover = vec![Cover::Open as u8; n * n];
    let mut growth = vec![Growth::Open as u8; n * n];
    for y in WOOD_Y {
        for x in WOOD_X {
            cover[(y * MAP + x) as usize] = Cover::Wood as u8;
            growth[(y * MAP + x) as usize] = Growth::Woods as u8;
        }
    }
    let worker = UnitStats {
        models: 1,
        files: 1,
        turn_rate: 180 * 65_536 / 360,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 16 },
        units: &[worker],
        map_tiles: MAP as u32,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: WATER,
            cover: &cover,
            growth: &growth,
            footing: &[],
            flow: &flow,
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (start_grub, 0);
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 60,
        cost_marrow: 0,
        build_ticks: 100,
        hp: 600,
        armour: Armour::Structure,
        width: 3,
        depth: 3,
        supply: 10,
        flags: DROP_OFF_GRUB,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall],
        units: &[ConquestUnit {
            worker: true,
            supply: 1,
            tier: 1,
            clearing: style,
            ..ConquestUnit::default()
        }],
        structures: 4,
        start,
        ..ConquestSetup::default()
    }));
    w
}

const FELLS: ClearStyle = ClearStyle {
    fells: true,
    ..ClearStyle::STANDARD
};

fn order(w: &mut World, command: Command) -> bool {
    w.tick(&[PlayerCommand { player: 0, command }])
        .commands_applied
        == 1
}

fn worker(w: &mut World, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

fn grub(w: &World) -> u32 {
    w.bank(0).unwrap().grub
}

fn wood() -> (i16, i16) {
    (WOOD_X.start as i16, WOOD_Y.start as i16)
}

fn size() -> (u8, u8) {
    (WOOD_X.len() as u8, WOOD_Y.len() as u8)
}

fn run(w: &mut World, seconds: u32) {
    for _ in 0..seconds * SECOND {
        w.tick(&[]);
    }
}

#[test]
fn felling_drops_the_bank_row_into_the_river_and_pays_only_for_the_rest() {
    let mut w = world(FELLS);
    let h = worker(&mut w, 12, 24);
    let before = grub(&w);
    assert!(order(
        &mut w,
        Command::Fell {
            regiment: h,
            tile: wood(),
            size: size(),
        }
    ));
    let (mut logs, mut most_x) = (0, 0);
    for _ in 0..240 * SECOND {
        w.tick(&[]);
        let debris = w.flood().rivers().debris();
        logs = logs.max(debris.owned(0));
        for p in debris.pieces(MAP) {
            assert_eq!(p.kind, Kind::Log);
            most_x = most_x.max(p.at.x.floor_int());
        }
    }
    let area = [
        WOOD_X.start,
        WOOD_Y.start,
        WOOD_X.len() as i32,
        WOOD_Y.len() as i32,
    ];
    assert!(!w.overgrown(area), "all cleared");
    // Only the row beside the river goes in: a log a woods tile.
    assert_eq!(logs, WOOD_X.len(), "a log for each bank tile");
    // The other rows pay their timber, the bank row none.
    let rows = (WOOD_Y.len() - 1) * WOOD_X.len();
    assert_eq!(
        grub(&w) - before,
        rows as u32 * u32::from(ClearStyle::STANDARD.pays[1])
    );
    assert!(most_x > WOOD_X.end, "the logs ride the current down");
}

#[test]
fn plain_clearing_drops_nothing_and_a_faction_that_does_not_fell_is_refused() {
    let mut w = world(ClearStyle::STANDARD);
    let h = worker(&mut w, 12, 24);
    assert!(!order(
        &mut w,
        Command::Fell {
            regiment: h,
            tile: wood(),
            size: size(),
        }
    ));
    assert!(order(
        &mut w,
        Command::Clear {
            regiment: h,
            tile: wood(),
            size: size(),
        }
    ));
    for _ in 0..200 * SECOND {
        w.tick(&[]);
        assert!(w.flood().rivers().debris().is_empty());
    }
}

#[test]
fn a_dumped_raft_costs_its_grub_and_rides_the_current() {
    let mut w = world(ClearStyle::STANDARD);
    let h = worker(&mut w, 20, 27);
    // Dry land, and a still or missing tile, are refused.
    assert!(!order(
        &mut w,
        Command::Dump {
            regiment: h,
            tile: (20, 26)
        }
    ));
    assert!(!order(
        &mut w,
        Command::Dump {
            regiment: h,
            tile: (-1, 31)
        }
    ));
    let before = grub(&w);
    assert!(order(
        &mut w,
        Command::Dump {
            regiment: h,
            tile: (20, 30)
        }
    ));
    run(&mut w, 15);
    let debris = w.flood().rivers().debris();
    let rafts: Vec<_> = debris
        .pieces(MAP)
        .filter(|p| p.kind == Kind::Raft)
        .collect();
    assert_eq!(rafts.len(), 1, "one raft went in");
    assert_eq!(debris.owned(0), 1);
    assert_eq!(before - grub(&w), RAFT_GRUB);
    assert!(rafts[0].at.x.floor_int() > 20, "and rides the current down");
    assert!(w.clearing_of(h).is_none(), "the order is done");
}

#[test]
fn no_raft_without_the_grub_for_it() {
    let mut w = world_with(ClearStyle::STANDARD, RAFT_GRUB - 1);
    let h = worker(&mut w, 20, 27);
    assert!(!order(
        &mut w,
        Command::Dump {
            regiment: h,
            tile: (20, 30)
        }
    ));
}
