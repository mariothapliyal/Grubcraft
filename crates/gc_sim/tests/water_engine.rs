//! The water engine, stage 1 (docs/39-water-engine.md): every river
//! carries its own water. Rain fills a river's stretches and runs on down
//! it, so the lower river rises most and drains last; flash floods run
//! down as fronts of every size, each ford shutting as the water over it
//! comes up, the upper ford first; and a big one is told map-wide well
//! before it comes.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::rivers::{Band, Gauge, Stage};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::sky::{Spell, Weather, WeatherKind};
use gc_sim::{Capacities, Flow, Ground, Mass, MatchSetup, Sky, World};

const MAP: i32 = 128;
const SECOND: u32 = gc_sim::TICKS_PER_SECOND;
const WATER: u16 = 256;
/// A long river running east off the map, deep but for two fords: one
/// up the river, one near its mouth.
const RIVER: core::ops::Range<i32> = 60..68;
const UPPER: core::ops::Range<i32> = 20..24;
const LOWER: core::ops::Range<i32> = 100..104;

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

const RAIN: Weather = Weather {
    kind: WeatherKind::Rain,
    strength: 160,
    cloud: 255,
    wind: (0, 0),
};

const STORM: Weather = Weather {
    kind: WeatherKind::Rain,
    strength: 255,
    ..RAIN
};

/// Rain for three minutes, then fair.
const SHOWER: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: SECOND,
        weather: RAIN,
    },
    Spell {
        at: 3 * 60 * SECOND,
        weather: Weather::FAIR,
    },
];

/// Heavy rain for eight minutes: the ground soaks through.
const SOAKING: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: SECOND,
        weather: Weather {
            strength: 210,
            ..RAIN
        },
    },
    Spell {
        at: 8 * 60 * SECOND,
        weather: Weather::FAIR,
    },
];

/// A storm ten seconds in, short enough that the ground never floods the
/// whole map.
const SQUALL: &[Spell] = &[
    Spell {
        at: 0,
        weather: Weather::FAIR,
    },
    Spell {
        at: 10 * SECOND,
        weather: STORM,
    },
    Spell {
        at: 20 * SECOND,
        weather: Weather::FAIR,
    },
];

fn world(seed: u64, spells: &'static [Spell]) -> World {
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
            spells,
            ..Sky::STILL
        },
        generals: false,
    })
}

#[test]
fn a_long_river_is_cut_into_stretches_with_a_ford_each() {
    let w = world(1, SHOWER);
    let rivers = w.flood().rivers();
    assert_eq!(rivers.stretches(), MAP as usize / 16);
    assert_eq!(rivers.fords(), 2);
}

#[test]
fn rain_raises_the_lower_river_most_and_it_drains_last() {
    let mut w = world(1, SHOWER);
    let (upper, lower) = (UPPER.start, LOWER.start);
    let mut most = (0, 0);
    while w.tick_count() < 3 * 60 * SECOND {
        w.tick(&[]);
        most = (
            most.0.max(w.flood().rivers().swell(upper, 62)),
            most.1.max(w.flood().rivers().swell(lower, 62)),
        );
    }
    // Upriver, little land drains in; near the mouth, the whole river.
    assert!(most.1 > most.0 * 3, "{most:?}");
    assert!(most.1 > 0);
    // After the rain, the upper river goes down first.
    while w.tick_count() < 4 * 60 * SECOND {
        w.tick(&[]);
    }
    let after = (
        w.flood().rivers().swell(upper, 62),
        w.flood().rivers().swell(lower, 62),
    );
    assert!(after.1 >= after.0, "{after:?}");
    while w.tick_count() < 12 * 60 * SECOND {
        w.tick(&[]);
    }
    assert_eq!(w.flood().rivers().swell(lower, 62), 0);
    assert!(!w.terrain().river_up());
}

/// The first seed (from 0) whose squall rolls a front of `band`.
fn seed_of(band: Band) -> u64 {
    (0..4000)
        .find(|&seed| {
            let mut w = world(seed, SQUALL);
            while w.tick_count() < 11 * SECOND {
                w.tick(&[]);
            }
            w.flood().rivers().fronts().any(|f| f.band() == band)
        })
        .expect("some seed rolls it")
}

/// The ticks each ford shuts and opens, and the stages and great floods
/// told, up to `ticks`.
#[derive(Debug, Default)]
struct Log {
    upper_shut: Option<u32>,
    lower_shut: Option<u32>,
    stages: Vec<(u32, Stage)>,
    great: Vec<(u32, Band)>,
}

fn log(w: &mut World, ticks: u32) -> Log {
    let mut log = Log::default();
    while w.tick_count() < ticks {
        w.tick(&[]);
        let t = w.tick_count();
        for e in w.events() {
            match *e {
                Event::FlashFlood { stage } => log.stages.push((t, stage)),
                Event::GreatFlood { band } => log.great.push((t, band)),
                _ => {}
            }
        }
        let rivers = w.flood().rivers();
        if log.upper_shut.is_none() && rivers.shuts(UPPER.start, 62) {
            log.upper_shut = Some(t);
        }
        if log.lower_shut.is_none() && rivers.shuts(LOWER.start, 62) {
            log.lower_shut = Some(t);
        }
    }
    log
}

#[test]
fn a_flash_flood_shuts_the_upper_ford_first_and_the_lower_later() {
    let mut w = world(seed_of(Band::Flood), SQUALL);
    let log = log(&mut w, 3 * 60 * SECOND);
    let (up, low) = (
        log.upper_shut.expect("upper"),
        log.lower_shut.expect("lower"),
    );
    // 80 tiles at six a second.
    let gap = low - up;
    assert!((12 * SECOND..15 * SECOND).contains(&gap), "{gap}");
    let order: Vec<Stage> = log.stages.iter().map(|&(_, s)| s).collect();
    assert_eq!(order, [Stage::Coming, Stage::Here, Stage::Gone], "{log:?}");
    assert!(log.great.is_empty());
    // All open again, the front gone.
    assert!(!w.flood().rivers().shut());
    assert!(!w.flood().rivers().running());
}

#[test]
fn a_minor_flash_flood_shuts_no_ford() {
    let mut w = world(seed_of(Band::Minor), SQUALL);
    let log = log(&mut w, 2 * 60 * SECOND);
    assert_eq!(log.upper_shut, None);
    assert_eq!(log.lower_shut, None);
    assert!(log.stages.is_empty(), "{log:?}");
}

#[test]
fn a_catastrophic_flood_is_told_to_everyone_long_before_it_comes() {
    let mut w = world(seed_of(Band::Catastrophic), SQUALL);
    let log = log(&mut w, 3 * 60 * SECOND);
    let &[(told, Band::Catastrophic)] = log.great.as_slice() else {
        panic!("{log:?}");
    };
    let shut = log.upper_shut.expect("upper");
    // (Told on the tick it was rolled, which counts as the first.)
    assert!(shut + 1 >= told + 30 * SECOND, "{told} {shut}");
    // And it shuts the lower ford too: it has barely faded on the way.
    assert!(log.lower_shut.is_some());
}

#[test]
fn the_same_seed_runs_the_same_rivers() {
    let seed = seed_of(Band::Flood);
    let (mut a, mut b) = (world(seed, SQUALL), world(seed, SQUALL));
    for _ in 0..90 * SECOND {
        a.tick(&[]);
        b.tick(&[]);
        assert_eq!(a.state_hash(), b.state_hash());
    }
    // And a different seed's rivers differ while they run.
    let mut c = world(seed_of(Band::Minor), SQUALL);
    let mut a = world(seed, SQUALL);
    let differ = (0..60 * SECOND).any(|_| {
        a.tick(&[]);
        c.tick(&[]);
        a.state_hash() != c.state_hash()
    });
    assert!(differ);
}

/// The stretch of the lower ford.
fn lower_stretch(w: &World) -> usize {
    (w.flood().rivers())
        .stretch_at(LOWER.start, 62)
        .expect("on the river")
}

#[test]
fn every_stretch_reads_its_river_and_the_gauge_rises_in_rain() {
    let mut w = world(1, SOAKING);
    let rivers = w.flood().rivers();
    assert_eq!(rivers.river_count(), 1);
    for s in 0..rivers.stretches() {
        let r = rivers.reading(s, 0);
        assert_eq!(r.river, 0);
        assert_eq!(rivers.stretch_at(r.at.0, r.at.1), Some(s), "{r:?}");
        assert_eq!(Gauge::of(r.rise, 0), Gauge::Normal);
    }
    assert_eq!(rivers.stretch_at(LOWER.start, 10), None);
    let lower = lower_stretch(&w);
    let mut most = Gauge::Normal;
    let mut top = 0;
    while w.tick_count() < 8 * 60 * SECOND {
        w.tick(&[]);
        let r = w.flood().rivers().reading(lower, 0);
        most = most.max(Gauge::of(r.rise, 0));
        top = top.max(r.rise);
    }
    // Long rain on soaked ground lifts the lower river into a minor
    // flood, short of its fords.
    assert_eq!(most, Gauge::Up, "{top}");
}

#[test]
fn a_flash_flood_is_forecast_at_each_ford_until_it_comes() {
    let mut w = world(seed_of(Band::Flood), SQUALL);
    let lower = lower_stretch(&w);
    let mut first = None;
    while w.tick_count() < 2 * 60 * SECOND && !w.flood().rivers().shuts(LOWER.start, 62) {
        w.tick(&[]);
        let coming = w.flood().rivers().reading(lower, 0).coming;
        if let (None, Some(c)) = (first, coming) {
            first = Some((w.tick_count(), c));
        }
    }
    let (told, c) = first.expect("forecast");
    let shut = w.tick_count();
    assert!(c.crest >= gc_sim::flood::FLOOD_RISE, "{c:?}");
    // The forecast is the crest reaching the stretch's top, a few tiles
    // above the ford: at most a few seconds early.
    let forecast = told + c.seconds * SECOND;
    assert!(
        forecast <= shut + SECOND && shut <= forecast + 4 * SECOND,
        "{told} {c:?} {shut}"
    );
}

#[test]
fn wading_is_slower_while_the_river_is_up() {
    let mut w = world(seed_of(Band::Minor), SQUALL);
    let p = FxVec2::from_ints(LOWER.start, 62) + FxVec2::new(Fx::HALF, Fx::HALF);
    let step = Fx::ONE;
    let pace = |w: &World| (w.terrain()).going(step, p, FxVec2::ZERO, Mass::Light);
    let before = pace(&w);
    assert!(w.terrain().wet(p));
    let mut slowest = before;
    while w.tick_count() < 2 * 60 * SECOND {
        w.tick(&[]);
        slowest = slowest.min(pace(&w));
        assert!(w.terrain().depth(p) > 0);
    }
    let (wading, deep) = (
        gc_sim::terrain::WADING_SPEED_PERCENT,
        gc_sim::terrain::DEEP_WADING_PERCENT,
    );
    assert_eq!(before.raw(), step.raw() * wading / 100);
    assert_eq!(slowest.raw(), step.raw() * (wading * deep / 100) / 100);
}
