//! Water that runs downhill (docs/27-living-land.md, stage 5): a grown
//! map's highland lakes and rivers stand at their own level, not the map's
//! line, and every water rule reads that level: wading, the ground rain
//! soaks, and the routes round deep water.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::terrain::HEIGHT_UNITS;
use gc_sim::{Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, World};

const MAP: u32 = 64;
const TILE: u16 = HEIGHT_UNITS as u16;
const SPEARS: u16 = 0;
/// The map's line, one tile up.
const LINE: u16 = TILE;
/// A plateau five tiles up.
const PLATEAU: u16 = 5 * TILE;
/// A highland lake on it over rows 20 to 40: its water four tiles up,
/// its bed half a tile under that (wading water).
const LAKE: core::ops::Range<u32> = 20..40;
const LAKE_LEVEL: u16 = 4 * TILE;
const LAKE_BED: u16 = LAKE_LEVEL - TILE / 2;
/// Its shore rows take its level but stand over it, dry: a low bank (a
/// quarter tile over the water, ground rain soaks) and the plateau.
const SHORE: core::ops::Range<u32> = 16..44;
const BANK: core::ops::Range<u32> = 40..42;
const BANK_HEIGHT: u16 = LAKE_LEVEL + TILE / 4;

fn heights() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| match i / MAP {
            y if LAKE.contains(&y) => LAKE_BED,
            y if BANK.contains(&y) => BANK_HEIGHT,
            _ => PLATEAU,
        })
        .collect()
}

fn levels() -> Vec<u16> {
    (0..MAP * MAP)
        .map(|i| {
            if SHORE.contains(&(i / MAP)) {
                LAKE_LEVEL
            } else {
                0
            }
        })
        .collect()
}

fn world(heights: &[u16], levels: &[u16]) -> World {
    World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights,
            water_level: LINE,
            cover: &[],
            growth: &[],
            footing: &[],
            flow: &[],
            water_levels: levels,
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    })
}

fn tile(n: i32) -> Fx {
    Fx::from_int(n) + Fx::HALF
}

fn at(y: u32) -> FxVec2 {
    FxVec2::new(tile(32), tile(y as i32))
}

#[test]
fn highland_water_is_wet_at_its_own_level() {
    let w = world(&heights(), &levels());
    let t = w.terrain();
    assert!(t.has_own_levels());
    assert!(t.wet(at(30)), "the lake");
    assert!(!t.wet(at(41)), "its bank, over its level");
    assert!(!t.wet(at(18)), "the plateau inside its shore");
    assert!(!t.wet(at(50)), "the plateau beyond");
    assert_eq!(t.water_line_at(at(30)), i32::from(LAKE_LEVEL));
    assert_eq!(t.water_line_at(at(50)), i32::from(LINE));
    // Without its own level the lake's bed stands far over the map's line:
    // dry ground.
    let w = world(&heights(), &[]);
    assert!(!w.terrain().has_own_levels());
    assert!(!w.terrain().wet(at(30)));
    // Levels that don't fit the map are none.
    let w = world(&heights(), &[LAKE_LEVEL; 10]);
    assert!(!w.terrain().has_own_levels());
}

fn spawn(w: &mut World, y: i32) -> RegimentHandle {
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: SPEARS,
            position: FxVec2::new(tile(32), tile(y)),
            facing: NORTH,
        },
    }]);
    w.regiments.iter().last().unwrap().handle
}

/// How far spearmen march north in two seconds from row `y`.
fn walked(levels: &[u16], y: i32) -> Fx {
    let mut w = world(&heights(), levels);
    let r = spawn(&mut w, y);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: r,
            target: FxVec2::new(tile(32), tile(y - 20)),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            brigade: 0,
        },
    }]);
    for _ in 0..40 {
        w.tick(&[]);
    }
    tile(y) - w.regiments.get(r).unwrap().position.y
}

#[test]
fn wading_a_highland_lake_is_slow() {
    let dry = walked(&levels(), 58);
    let wet = walked(&levels(), 37);
    let no_lake = walked(&[], 37);
    // As in a river at the line: 60% of the march on land.
    assert!(
        wet * 10 <= dry * 7 && wet * 10 >= dry * 5,
        "wet {wet:?} vs dry {dry:?}"
    );
    assert!(
        no_lake * 10 > dry * 9,
        "{no_lake:?} vs {dry:?} with no lake"
    );
}

#[test]
fn rain_soaks_the_bank_of_a_highland_lake() {
    let soaked = |levels: &[u16]| {
        let w = world(&heights(), levels);
        w.terrain()
            .soak_order()
            .filter(|&(_, y)| BANK.contains(&(y as u32)))
            .count()
    };
    // The bank stands a quarter tile over the lake: soft ground in rain.
    // Without the lake's level it is three tiles over the line, and dry.
    assert!(soaked(&levels()) > 0, "no bank soaks by the lake");
    assert_eq!(soaked(&[]), 0);
}

#[test]
fn highland_water_is_the_same_every_run() {
    let run = || {
        let mut w = world(&heights(), &levels());
        let r = spawn(&mut w, 37);
        w.tick(&[PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment: r,
                target: FxVec2::new(tile(32), tile(10)),
                facing: Some(Angle(0)),
                files: 0,
                pace: Fx::ZERO,
                brigade: 0,
            },
        }]);
        for _ in 0..100 {
            w.tick(&[]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
}
