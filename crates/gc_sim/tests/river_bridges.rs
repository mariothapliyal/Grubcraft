//! The water engine, stage 4 (docs/39-water-engine.md, section 4): debris
//! reaching a bridge's piers jams against them or strikes them, and a
//! bridge tile that falls floats on down as a wreck.

use gc_fixed::{Fx, FxVec2};
use gc_sim::StructureHandle;
use gc_sim::clearing::ClearStyle;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::rivers::Band;
use gc_sim::rivers::debris::Kind;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::{Spell, Weather, WeatherKind};
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
/// A bridge across the river, from bank to bank.
const BRIDGE_X: i32 = 24;
const BRIDGE_HP: u16 = 300;

fn river(y: i32) -> bool {
    RIVER.contains(&y)
}

fn world(style: ClearStyle) -> World {
    world_with(style, 5, &[], true)
}

fn world_with(style: ClearStyle, seed: u64, spells: &'static [Spell], wooded: bool) -> World {
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
    for y in WOOD_Y.filter(|_| wooded) {
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
        seed,
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
        sky: gc_sim::Sky {
            spells,
            ..gc_sim::Sky::STILL
        },
        generals: true,
    });
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (START_GRUB, 0);
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
    let bridge = BuildingStats {
        role: Role::Bridge,
        cost_grub: 12,
        build_ticks: 20,
        hp: BRIDGE_HP,
        width: 1,
        depth: 1,
        flags: gc_sim::structures::LINE,
        supply: 0,
        ..hall
    };
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, bridge],
        units: &[ConquestUnit {
            worker: true,
            supply: 1,
            tier: 1,
            clearing: style,
            ..ConquestUnit::default()
        }],
        structures: 16,
        start,
        ..ConquestSetup::default()
    }));
    w
}

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

fn run(w: &mut World, seconds: u32) {
    for _ in 0..seconds * SECOND {
        w.tick(&[]);
    }
}

/// Put up a finished bridge across the river at [`BRIDGE_X`], bank to bank.
fn bridge(w: &mut World) {
    for y in RIVER.start - 1..=RIVER.end {
        assert!(order(
            w,
            Command::SpawnStructure {
                building: 1,
                tile: (BRIDGE_X as i16, y as i16),
                turn: 0,
            }
        ));
    }
    w.tick(&[]);
}

/// The bridge tile in row `y`, and its hit points.
fn deck(w: &World, y: i32) -> Option<(StructureHandle, u32)> {
    let s = w.structures()?;
    s.iter()
        .find(|v| v.tile == (BRIDGE_X, y))
        .map(|v| (v.handle, v.hp))
}

/// The bridge's hit points, all its tiles over the water together.
fn bridge_hp(w: &World) -> u32 {
    RIVER.filter_map(|y| deck(w, y)).map(|(_, hp)| hp).sum()
}

#[test]
fn rafts_reaching_a_bridge_strike_or_jam_on_its_piers() {
    let mut w = world(ClearStyle::STANDARD);
    bridge(&mut w);
    let whole = RIVER.len() as u32 * u32::from(BRIDGE_HP);
    assert_eq!(bridge_hp(&w), whole);
    // Rafts tipped in upstream on both banks, a few at a time.
    let hands = [worker(&mut w, 8, 28), worker(&mut w, 8, 35)];
    let mut jammed = false;
    for round in 0..4 {
        for (k, &h) in hands.iter().enumerate() {
            let row = if k == 0 { RIVER.start } else { RIVER.end - 1 };
            assert!(order(
                &mut w,
                Command::Dump {
                    regiment: h,
                    tile: (8 + round as i16, row as i16)
                }
            ));
        }
        for _ in 0..20 * SECOND {
            w.tick(&[]);
            jammed |= w
                .flood()
                .rivers()
                .debris()
                .pieces(MAP)
                .any(|p| p.grounded && p.at.x.floor_int() == BRIDGE_X - 1);
        }
    }
    let hp = bridge_hp(&w);
    assert!(hp < whole, "the piers were struck: {hp} of {whole}");
    assert!(
        hp > whole / 2,
        "but a steady river's rafts don't bring it down"
    );
    assert!(
        jammed,
        "some jammed on the piers, and washed free in low water"
    );
    // Nothing is left jammed in low water: small jams wash free.
    run(&mut w, 30);
    assert!(
        !(w.flood().rivers().debris().pieces(MAP))
            .any(|p| p.at.x.floor_int() == BRIDGE_X - 1 && p.grounded),
        "small jams wash free"
    );
}

#[test]
fn a_bridge_tile_that_falls_floats_on_as_a_wreck() {
    let mut w = world(ClearStyle::STANDARD);
    bridge(&mut w);
    let (target, _) = deck(&w, RIVER.start + 1).unwrap();
    // An enemy on the deck hacks the next tile down.
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let spawned = w.tick(&[PlayerCommand {
        player: 1,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::new(
                Fx::from_int(BRIDGE_X) + Fx::HALF,
                Fx::from_int(RIVER.start) + Fx::HALF,
            ),
            facing: NORTH,
        },
    }]);
    assert_eq!(spawned.commands_applied, 1);
    let enemy = (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap();
    let hacked = w.tick(&[PlayerCommand {
        player: 1,
        command: Command::AttackStructure {
            regiment: enemy,
            structure: target,
        },
    }]);
    assert_eq!(hacked.commands_applied, 1);
    let wreck =
        |w: &World| (w.flood().rivers().debris().pieces(MAP)).find(|p| p.kind == Kind::Wreck);
    for _ in 0..600 * SECOND {
        if deck(&w, RIVER.start + 1).is_none() {
            break;
        }
        w.tick(&[]);
    }
    assert!(deck(&w, RIVER.start + 1).is_none(), "the tile fell");
    let at = wreck(&w).expect("its wreck went in").at;
    assert_eq!(at.x.floor_int(), BRIDGE_X);
    run(&mut w, 5);
    let on = wreck(&w).expect("it floats on").at;
    assert!(on.x > at.x, "down the river: {on:?} from {at:?}");
}

#[test]
fn a_tile_taken_away_leaves_no_wreck() {
    let mut w = world(ClearStyle::STANDARD);
    bridge(&mut w);
    let (gone, _) = deck(&w, RIVER.start + 1).unwrap();
    assert!(order(&mut w, Command::RemoveStructure { structure: gone }));
    run(&mut w, 1);
    assert!(w.flood().rivers().debris().is_empty());
}

#[test]
fn a_bank_tile_that_falls_leaves_no_wreck() {
    let mut w = world(ClearStyle::STANDARD);
    bridge(&mut w);
    let (gone, _) = deck(&w, RIVER.start - 1).unwrap();
    assert!(order(&mut w, Command::RemoveStructure { structure: gone }));
    run(&mut w, 1);
    assert!(w.flood().rivers().debris().is_empty());
}

/// A storm ten seconds in.
const SQUALL: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: 10 * SECOND,
        weather: Weather {
            kind: WeatherKind::Rain,
            strength: 255,
            cloud: 255,
            wind: (0, 0),
        },
    },
    Spell {
        at: 20 * SECOND,
        weather: Weather::FAIR,
    },
];

/// The first seed (from 0) whose squall rolls a front of `band`.
fn seed_of(band: Band) -> u64 {
    (0..4000)
        .find(|&seed| {
            let mut w = world_with(ClearStyle::STANDARD, seed, SQUALL, true);
            while w.tick_count() < 11 * SECOND {
                w.tick(&[]);
            }
            w.flood().rivers().fronts().any(|f| f.band() == band)
        })
        .expect("some seed rolls it")
}

#[test]
fn a_sound_bridge_stands_through_a_catastrophic_flood_with_nothing_in_the_water() {
    let seed = seed_of(Band::Catastrophic);
    let mut w = world_with(ClearStyle::STANDARD, seed, SQUALL, false);
    bridge(&mut w);
    let whole = bridge_hp(&w);
    run(&mut w, 120);
    assert_eq!(bridge_hp(&w), whole, "no debris, no harm to sound piers");
}

#[test]
fn a_catastrophic_flood_tears_logs_off_the_banks_that_batter_the_bridge_below() {
    let seed = seed_of(Band::Catastrophic);
    let mut w = world_with(ClearStyle::STANDARD, seed, SQUALL, true);
    bridge(&mut w);
    let whole = bridge_hp(&w);
    run(&mut w, 120);
    assert!(bridge_hp(&w) < whole, "the flood's logs struck and jammed");
}

/// The bridge's hit points lost to rafts tipped in at the start of a
/// match on `seed` with `spells`.
fn rafts_harm(seed: u64, spells: &'static [Spell]) -> u32 {
    let mut w = world_with(ClearStyle::STANDARD, seed, spells, false);
    bridge(&mut w);
    let whole = bridge_hp(&w);
    let hands = [worker(&mut w, 8, 28), worker(&mut w, 8, 35)];
    // Tipped in as the crest comes, a few at a time.
    run(&mut w, 8);
    for round in 0..3 {
        for (k, &h) in hands.iter().enumerate() {
            let row = if k == 0 { RIVER.start } else { RIVER.end - 1 };
            assert!(order(
                &mut w,
                Command::Dump {
                    regiment: h,
                    tile: (8 + round as i16, row as i16)
                }
            ));
        }
        run(&mut w, 10);
    }
    run(&mut w, 60);
    whole.saturating_sub(bridge_hp(&w))
}

#[test]
fn rafts_in_a_flood_do_far_more_harm_than_in_a_steady_river() {
    let seed = seed_of(Band::Major);
    let (steady, flood) = (rafts_harm(seed, &[]), rafts_harm(seed, SQUALL));
    assert!(steady > 0);
    assert!(flood > steady * 2, "flood {flood}, steady {steady}");
}
