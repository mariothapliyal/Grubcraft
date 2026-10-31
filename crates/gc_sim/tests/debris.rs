//! The water engine, stage 3a (docs/39-water-engine.md, section 3):
//! debris on the rivers. Logs from burnt bank woods and from big floods
//! ride the current down, ground on the fords in quiet water (a snag
//! that slows the wading), float off again in a flood, and strike the
//! troops wading where they pass.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::rivers::Band;
use gc_sim::rivers::debris::{Kind, NATURAL_MOST, SNAG_MASS};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{
    Capacities, Command, Cover, Flow, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, World,
};

const MAP: i32 = 128;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
/// A river running east off the map, deep but for a ford near its top
/// and one near its mouth.
const RIVER: core::ops::Range<i32> = 60..68;
const UPPER: core::ops::Range<i32> = 20..24;
const LOWER: core::ops::Range<i32> = 100..104;
/// A wood on the north bank between the fords.
const WOOD_X: core::ops::Range<i32> = 30..60;
const WOOD_Y: core::ops::Range<i32> = 56..60;

fn ford(x: i32) -> bool {
    UPPER.contains(&x) || LOWER.contains(&x)
}

fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            match () {
                _ if RIVER.contains(&y) && ford(x) => WATER - 40,
                _ if RIVER.contains(&y) => 20,
                _ => WATER + 300,
            }
        })
        .collect()
}

fn nav() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
            match () {
                _ if RIVER.contains(&y) && ford(x) => 3,
                _ if RIVER.contains(&y) => BLOCKED,
                _ => 1,
            }
        })
        .collect()
}

fn flow() -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            if RIVER.contains(&(i / MAP)) {
                Flow::byte(Angle(0), 2)
            } else {
                0
            }
        })
        .collect()
}

fn cover() -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if WOOD_X.contains(&x) && WOOD_Y.contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
}

fn world(seed: u64, spells: &'static [Spell]) -> World {
    let (heights, nav, flow, cover) = (heights(), nav(), flow(), cover());
    World::new(&MatchSetup {
        seed,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP as u32,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: WATER,
            cover: &cover,
            growth: &[],
            footing: &[],
            flow: &flow,
        },
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

fn middle(x: i32, y: i32) -> FxVec2 {
    FxVec2::from_ints(x, y) + FxVec2::new(Fx::HALF, Fx::HALF)
}

fn spawn(w: &mut World, player: u8, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: 0,
            position: middle(x, y),
            facing,
        },
    }]);
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .expect("spawned")
}

/// Whether wading the lower ford is slow anywhere.
fn slow(w: &World) -> bool {
    RIVER
        .clone()
        .any(|y| LOWER.clone().any(|x| w.terrain().deep(middle(x, y))))
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    (w.regiments.model_hp(h)).map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

/// The first seed (from 0) whose storm at `at` seconds rolls a front of
/// at least `band`.
fn seed_of(band: Band, spells: &'static [Spell], at: u32) -> u64 {
    (0..4000)
        .find(|&seed| {
            let mut w = world(seed, spells);
            // Nothing but the storm: no wood burns.
            while w.tick_count() < (at + 1) * SECOND {
                w.tick(&[]);
            }
            w.flood().rivers().fronts().any(|f| f.band() >= band)
        })
        .expect("some seed rolls it")
}

/// The bank wood set alight, and the troops wading the lower ford: the
/// logs it topples ride down and strike them, then ground on the ford.
fn burn_the_bank(w: &mut World) -> RegimentHandle {
    let burner = spawn(w, 0, 45, WOOD_Y.start - 2, NORTH);
    let wading = spawn(w, 1, LOWER.start + 2, RIVER.start + 1, NORTH);
    for _ in 0..SECOND {
        w.tick(&[]);
    }
    let lit = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::Burn { regiment: burner },
    }]);
    assert_eq!(lit.commands_applied, 1);
    wading
}

#[test]
fn burnt_bank_woods_topple_logs_that_ride_down_and_strike_the_wading() {
    static FAIR: [Spell; 1] = [Spell {
        at: 0,
        weather: Weather::FAIR,
    }];
    let mut w = world(7, &FAIR);
    let wading = burn_the_bank(&mut w);
    let (before, mut struck, mut most_x, mut grounded) = (hp(&w, wading), false, 0, false);
    while w.tick_count() < 5 * 60 * SECOND {
        w.tick(&[]);
        struck |= (w.events().iter())
            .any(|e| matches!(e, Event::DebrisStruck { owners } if owners & 2 != 0));
        for p in w.flood().rivers().debris().pieces(MAP) {
            assert_eq!(p.kind, Kind::Log);
            assert!(RIVER.contains(&p.at.y.floor_int()), "{:?}", p.at);
            most_x = most_x.max(p.at.x.floor_int());
            grounded |= p.grounded;
        }
    }
    assert!(
        most_x >= LOWER.start,
        "the logs came down to the lower ford"
    );
    assert!(struck, "and struck the troops wading it");
    assert!(hp(&w, wading) < before);
    assert!(grounded, "they ground on the ford in quiet water");
}

#[test]
fn logs_grounded_on_a_ford_snag_it_until_a_flood_lifts_them_off() {
    // The burn first, then a storm once the logs have come down.
    static SPELLS: [Spell; 3] = storm_spells();
    const fn storm_spells() -> [Spell; 3] {
        [
            Spell {
                at: 0,
                weather: Weather::FAIR,
            },
            Spell {
                at: 240 * SECOND,
                weather: Weather {
                    kind: WeatherKind::Rain,
                    strength: 255,
                    cloud: 255,
                    wind: (0, 0),
                },
            },
            Spell {
                at: 250 * SECOND,
                weather: Weather::FAIR,
            },
        ]
    }
    let seed = seed_of(Band::Flood, &SPELLS, 240);
    let mut w = world(seed, &SPELLS);
    burn_the_bank(&mut w);
    let ford_tile = |w: &World| {
        let rivers = w.flood().rivers();
        let mass: u32 = (rivers.debris().pieces(MAP))
            .filter(|p| p.grounded && LOWER.contains(&p.at.x.floor_int()))
            .map(|p| u32::from(p.kind.mass()))
            .sum();
        mass
    };
    while w.tick_count() < 235 * SECOND {
        w.tick(&[]);
    }
    let snag = ford_tile(&w);
    assert!(
        snag >= u32::from(SNAG_MASS),
        "a snag on the lower ford: {snag}"
    );
    assert!(slow(&w), "the snag makes wading the ford slow");
    // The flood floats them off and carries them away.
    while w.tick_count() < 330 * SECOND {
        w.tick(&[]);
    }
    assert!(ford_tile(&w) < snag, "the flood lifted the snag");
    assert!(!slow(&w) || w.flood().rivers().fronts().next().is_some());
}

#[test]
fn a_catastrophic_flood_tears_logs_off_the_wooded_banks() {
    static SPELLS: [Spell; 3] = [
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
    let seed = seed_of(Band::Catastrophic, &SPELLS, 10);
    let mut w = world(seed, &SPELLS);
    let (mut most, mut ever) = (0, 0);
    while w.tick_count() < 4 * 60 * SECOND {
        w.tick(&[]);
        let debris = w.flood().rivers().debris();
        let floating = debris.pieces(MAP).filter(|p| !p.grounded).count();
        most = most.max(floating);
        ever = ever.max(debris.len());
    }
    assert!(ever > 0, "the flood took logs off the banks");
    assert!(most <= NATURAL_MOST);
}

#[test]
fn the_same_seed_sends_the_same_logs_down() {
    static FAIR: [Spell; 1] = [Spell {
        at: 0,
        weather: Weather::FAIR,
    }];
    let (mut a, mut b) = (world(3, &FAIR), world(3, &FAIR));
    burn_the_bank(&mut a);
    burn_the_bank(&mut b);
    for _ in 0..3 * 60 * SECOND {
        a.tick(&[]);
        b.tick(&[]);
        assert_eq!(a.state_hash(), b.state_hash());
    }
    assert!(!a.flood().rivers().debris().is_empty());
}
