//! What the screen is told about each regiment: what it is doing and why
//! its nerve is going (docs/29, "Clear orders and feedback").

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::morale::{WHY_ALLY_BROKE, WHY_FLANK, WHY_REAR};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Doing, MatchSetup, MoraleState, PlayerCommand, RegimentHandle, World,
};

const SOUTH: Angle = Angle(0x4000);
/// Drill unit 0: 24 spearmen, morale 60.
const SPEARS: u16 = 0;
/// Drill unit 1: 24 skitterlings, fragile, morale 45.
const SKITTERS: u16 = 1;

fn world() -> World {
    World::new(&MatchSetup {
        seed: 23,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn run(w: &mut World, seconds: u32) {
    for _ in 0..seconds * 20 {
        w.tick(&[]);
    }
}

#[test]
fn a_regiment_shows_what_it_is_doing() {
    let mut w = world();
    let h = spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    assert_eq!(w.regiments.feedback(h).unwrap().doing, Doing::Holding);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: h,
            target: FxVec2::from_ints(100, 140),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    let r = w.regiments.feedback(h).unwrap();
    assert_eq!(r.doing, Doing::Marching);
    assert_eq!(r.waiting, None);
}

#[test]
fn a_regiment_hit_from_behind_says_so() {
    let mut w = world();
    let skitters = spawn(&mut w, 1, SKITTERS, 100, 100, NORTH);
    spawn(&mut w, 0, SPEARS, 100, 103, NORTH);
    run(&mut w, 4);
    let r = w.regiments.feedback(skitters).unwrap();
    assert_eq!(r.doing, Doing::Fighting);
    // By its first check it is already turning, so the blows land on its
    // flank; either way the screen says it was struck from the side.
    assert_ne!(r.why & (WHY_REAR | WHY_FLANK), 0, "{r:?}");
}

#[test]
fn routers_keep_their_reasons_and_friends_hear_of_it() {
    let mut w = world();
    spawn(&mut w, 0, SPEARS, 100, 100, NORTH);
    let skitters = spawn(&mut w, 1, SKITTERS, 100, 97, SOUTH);
    let friend = spawn(&mut w, 1, SKITTERS, 110, 90, SOUTH);
    for _ in 0..20 * 120 {
        if w.regiments.get(skitters).unwrap().morale == MoraleState::Routing {
            break;
        }
        w.tick(&[]);
    }
    assert_eq!(
        w.regiments.get(skitters).unwrap().morale,
        MoraleState::Routing
    );
    let r = w.regiments.feedback(skitters).unwrap();
    assert_eq!(r.doing, Doing::Running);
    assert_ne!(r.why, 0, "a router shows why it broke");
    let f = w.regiments.feedback(friend).unwrap();
    assert_ne!(f.why & WHY_ALLY_BROKE, 0, "{f:?}");
}
