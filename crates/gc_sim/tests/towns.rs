//! Walled towns (docs/26-fortifications.md, stage 8): walls closed round
//! a main building make a walled town, whose buildings stand up better to
//! ordinary regiments; a gate set Open breaches it.

use gc_fixed::FxVec2;
use gc_sim::events::Event;
use gc_sim::gates::GateMode;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, GATE, LINE, Role};
use gc_sim::towns::{SHELTERED_PERCENT, SURVEY_TICKS};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, PlayerCommand,
    RegimentHandle, StructureHandle, TICKS_PER_SECOND, World,
};

const WALL: u16 = 0;
const HALL: u16 = 1;
const HOUSE: u16 = 2;
const GATE_3: u16 = 3;
const MAP: u32 = 128;
/// The ring: the edge of the square from `LOW` to `HIGH`, both in.
const LOW: i16 = 54;
const HIGH: i16 = 73;
/// The gate, three tiles of the ring's west side from here south.
const GATE_AT: (i16, i16) = (LOW, 62);

fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 16 },
        units: &[DRILL_UNITS[0]],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let wall = BuildingStats {
        role: Role::Wall,
        tier: 1,
        upgrades_from: None,
        cost_grub: 10,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 250,
        armour: Armour::Fortified,
        width: 1,
        depth: 1,
        supply: 0,
        flags: LINE,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            wall,
            BuildingStats {
                role: Role::Main,
                width: 4,
                depth: 4,
                flags: 0,
                ..wall
            },
            BuildingStats {
                role: Role::Housing,
                width: 2,
                depth: 2,
                hp: 5000,
                armour: Armour::Structure,
                flags: 0,
                ..wall
            },
            BuildingStats {
                hp: 1000,
                depth: 3,
                flags: GATE,
                ..wall
            },
        ],
        units: &[ConquestUnit::default()],
        structures: 256,
        ..ConquestSetup::default()
    }));
    put_up(&mut w, 0, HALL, (62, 62));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, tile: (i16, i16)) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile,
            turn: 0,
        }
    ));
    let tile = (i32::from(tile.0), i32::from(tile.1));
    (w.structures().unwrap().iter())
        .find(|v| v.tile == tile)
        .unwrap()
        .handle
}

/// Player 0's ring round its hall, with a gate in its west side, all put
/// up in one tick.
fn ring(w: &mut World) -> StructureHandle {
    let mut orders = vec![PlayerCommand {
        player: 0,
        command: Command::SpawnStructure {
            building: GATE_3,
            tile: GATE_AT,
            turn: 0,
        },
    }];
    let mut tiles = Vec::new();
    for a in LOW..=HIGH {
        for (x, y) in [(a, LOW), (a, HIGH), (LOW, a), (HIGH, a)] {
            let in_gate = x == GATE_AT.0 && (GATE_AT.1..GATE_AT.1 + 3).contains(&y);
            if !in_gate && !tiles.contains(&(x, y)) {
                tiles.push((x, y));
                orders.push(PlayerCommand {
                    player: 0,
                    command: Command::SpawnStructure {
                        building: WALL,
                        tile: (x, y),
                        turn: 0,
                    },
                });
            }
        }
    }
    let n = orders.len();
    assert_eq!(w.tick(&orders).commands_applied as usize, n);
    let gate = (i32::from(GATE_AT.0), i32::from(GATE_AT.1));
    (w.structures().unwrap().iter())
        .find(|v| v.tile == gate)
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    (w.regiments.iter())
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

/// Play to just past the next survey, and the towns told of in it.
fn survey(w: &mut World) -> Vec<(u8, bool)> {
    let mut told = Vec::new();
    for _ in 0..=SURVEY_TICKS {
        w.tick(&[]);
        told.extend(w.events().iter().filter_map(|e| match *e {
            Event::Town { player, walled } => Some((player, walled)),
            _ => None,
        }));
    }
    told
}

fn walled(w: &World) -> bool {
    w.structures().unwrap().towns().walled(0)
}

#[test]
fn walls_closed_round_the_hall_make_a_walled_town_until_a_gate_is_opened() {
    let mut w = world();
    assert!(survey(&mut w).is_empty());
    assert!(!walled(&w));
    let gate = ring(&mut w);
    assert_eq!(survey(&mut w), vec![(0, true)]);
    let towns = w.structures().unwrap().towns();
    assert!(towns.inside(0, 58, 58) && towns.inside(0, i32::from(LOW) + 1, 62));
    assert!(!towns.inside(0, i32::from(LOW) - 1, 62) && !towns.inside(0, 10, 10));
    assert!(!towns.inside(1, 58, 58), "the town is its owner's only");
    // A gate set Open is a gap in the ring.
    let open = Command::SetGate {
        structure: gate,
        mode: GateMode::Open,
    };
    assert!(order(&mut w, 0, open));
    assert_eq!(survey(&mut w), vec![(0, false)]);
    assert!(!walled(&w));
    let shut = Command::SetGate {
        structure: gate,
        mode: GateMode::Locked,
    };
    assert!(order(&mut w, 0, shut));
    assert_eq!(survey(&mut w), vec![(0, true)]);
}

/// Hit points player 1's spears knock off a house inside player 0's ring
/// (or where it would be) in 20 seconds.
fn knocked_off(with_ring: bool) -> u32 {
    let mut w = world();
    if with_ring {
        ring(&mut w);
    }
    let house = put_up(&mut w, 0, HOUSE, (58, 58));
    survey(&mut w);
    assert_eq!(walled(&w), with_ring);
    let spears = spawn(&mut w, 1, 59, 62);
    assert!(order(
        &mut w,
        1,
        Command::AttackStructure {
            regiment: spears,
            structure: house,
        }
    ));
    let hp = |w: &World| {
        (w.structures().unwrap().iter())
            .find(|v| v.handle == house)
            .unwrap()
            .hp
    };
    let before = hp(&w);
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    before - hp(&w)
}

#[test]
fn a_walled_towns_buildings_take_a_quarter_less_from_regiments() {
    let open = knocked_off(false);
    let walled = knocked_off(true);
    assert!(open > 0, "the spears never struck the house");
    let expected = u64::from(open) * SHELTERED_PERCENT / 100;
    // A blow or so either way, as hit points round to whole ones.
    assert!(
        u64::from(walled).abs_diff(expected) <= u64::from(open) / 20,
        "open {open}, walled {walled}"
    );
}
