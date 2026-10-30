//! The water engine, stage 2 (docs/39-water-engine.md): big floods go
//! over the banks. Low land beside a river is its floodplain: the river's
//! own water rises over it, so a flood makes it wading ground, and a
//! catastrophic one stands deep enough on it to shut it, sweeping or
//! sending the troops there to dry ground.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::rivers::{Band, PLAIN_TILES};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Command, Flow, Ground, MatchSetup, PlayerCommand, Sky, World};

const MAP: i32 = 128;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
/// A river running east off the map, deep but for one ford near its top
/// and one near its mouth.
const RIVER: core::ops::Range<i32> = 60..68;
const UPPER: core::ops::Range<i32> = 20..24;
const LOWER: core::ops::Range<i32> = 100..104;
/// South of it, a low meadow (an eighth of a tile above the line), then a
/// higher terrace (two fifths), then the hills.
const MEADOW: core::ops::Range<i32> = 68..71;
const TERRACE: core::ops::Range<i32> = 71..74;

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
                _ if MEADOW.contains(&y) => WATER + 32,
                _ if TERRACE.contains(&y) => WATER + 102,
                _ => 600,
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

fn world(seed: u64) -> World {
    let (heights, nav, flow) = (heights(), nav(), flow());
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
            cover: &[],
            growth: &[],
            footing: &[],
            flow: &flow,
        },
        champions: false,
        sky: Sky {
            spells: SQUALL,
            ..Sky::STILL
        },
        generals: false,
    })
}

/// The first seed (from 0) whose squall rolls a front of `band`.
fn seed_of(band: Band) -> u64 {
    (0..4000)
        .find(|&seed| {
            let mut w = world(seed);
            while w.tick_count() < 11 * SECOND {
                w.tick(&[]);
            }
            w.flood().rivers().fronts().any(|f| f.band() == band)
        })
        .expect("some seed rolls it")
}

fn middle(x: i32, y: i32) -> FxVec2 {
    FxVec2::from_ints(x, y) + FxVec2::new(Fx::HALF, Fx::HALF)
}

#[test]
fn the_floodplain_is_the_low_land_beside_the_river() {
    let w = world(1);
    let rivers = w.flood().rivers();
    let (groups, tiles) = rivers.floodplain();
    // The meadow and the terrace, the whole river's length: the hills are
    // over a tile above the line, and the north bank is all hills.
    assert_eq!(tiles, (MAP * (TERRACE.end - MEADOW.start)) as usize);
    assert!(TERRACE.end - RIVER.end <= i32::from(PLAIN_TILES));
    // Two height bands in each stretch.
    assert_eq!(groups, 2 * rivers.stretches());
    assert!(rivers.on_floodplain(40, MEADOW.start));
    assert!(rivers.on_floodplain(40, TERRACE.end - 1));
    assert!(!rivers.on_floodplain(40, TERRACE.end));
    assert!(!rivers.on_floodplain(40, RIVER.start - 1));
    assert!(!rivers.on_floodplain(40, 62), "the river itself");
    // Each tile reads the stretch of the river beside it.
    assert_eq!(
        rivers.stretch_at(40, MEADOW.start),
        rivers.stretch_at(40, 62)
    );
    assert!(!w.terrain().wet(middle(40, MEADOW.start)));
}

#[test]
fn a_flood_wets_the_meadow_but_shuts_none_of_it() {
    let mut w = world(seed_of(Band::Flood));
    let (meadow, terrace) = (
        middle(LOWER.start, MEADOW.start),
        middle(LOWER.start, TERRACE.start),
    );
    let mut wet = false;
    while w.tick_count() < 3 * 60 * SECOND {
        w.tick(&[]);
        wet |= w.terrain().wet(meadow);
        assert!(!w.terrain().wet(terrace), "the terrace stays dry");
        assert!(!w.flood().rivers().shuts(LOWER.start, MEADOW.start));
        assert!(w.nav().walkable(0, meadow));
    }
    assert!(wet, "the water came over the meadow");
    assert!(!w.terrain().wet(meadow), "and went down again");
}

/// A retreat through a flooding valley (docs/39, "Pass marks"): troops on
/// the meadow when a catastrophic flood comes over it all reach dry
/// ground within a minute of it shutting, and none is left on shut
/// ground.
#[test]
fn a_catastrophic_flood_drives_everyone_off_the_meadow() {
    let mut w = world(seed_of(Band::Catastrophic));
    let spots = [
        (30, MEADOW.start + 1),
        (60, MEADOW.start),
        (90, MEADOW.start + 2),
    ];
    let cmds: Vec<PlayerCommand> = spots
        .iter()
        .map(|&(x, y)| PlayerCommand {
            player: 0,
            command: Command::SpawnRegiment {
                unit: 0,
                position: middle(x, y),
                facing: Angle(0),
            },
        })
        .collect();
    w.tick(&cmds);
    let handles: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert_eq!(handles.len(), spots.len());
    let shut_at = |w: &World, p: FxVec2| w.flood().rivers().shuts(p.x.floor_int(), p.y.floor_int());
    let mut first_shut = None;
    let mut worst = 0;
    while w.tick_count() < 4 * 60 * SECOND {
        w.tick(&[]);
        let rivers = w.flood().rivers();
        if first_shut.is_none() && (spots.iter()).any(|&(x, y)| rivers.shuts(x, y)) {
            first_shut = Some(w.tick_count());
        }
        let Some(since) = first_shut else {
            continue;
        };
        let caught = (w.regiments.iter())
            .filter(|r| handles.contains(&r.handle) && shut_at(&w, r.position))
            .count();
        if caught > 0 {
            worst = w.tick_count() - since;
        }
    }
    assert!(first_shut.is_some(), "the meadow went under");
    assert!(
        worst <= 60 * SECOND,
        "a regiment stood on shut ground {worst} ticks"
    );
    for r in w.regiments.iter().filter(|r| handles.contains(&r.handle)) {
        assert!(!shut_at(&w, r.position), "{:?}", r.position);
        assert!(w.nav().walkable(0, r.position), "{:?}", r.position);
    }
    // And once it has gone down the meadow is open again.
    assert!(!w.flood().rivers().flooding());
    assert!(w.nav().walkable(0, middle(60, MEADOW.start)));
}

#[test]
fn the_same_seed_floods_the_same_floodplain() {
    let seed = seed_of(Band::Catastrophic);
    let (mut a, mut b) = (world(seed), world(seed));
    for _ in 0..2 * 60 * SECOND {
        a.tick(&[]);
        b.tick(&[]);
        assert_eq!(a.state_hash(), b.state_hash());
    }
}
