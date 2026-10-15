//! The Field Battle deployment phase (docs/06-game-modes.md): zones, placing,
//! time standing still, hidden enemies, and Ready.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::deploy::DEPLOY_SECONDS;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World,
};

const NORTH: Angle = Angle(0xC000);
const SOUTH: Angle = Angle(0x4000);

fn world() -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[0, 1],
        ground: gc_sim::Ground::FLAT,
        champions: false,
    })
}

fn pc(player: u8, command: Command) -> PlayerCommand {
    PlayerCommand { player, command }
}

fn zone(x: i32, y: i32) -> Command {
    Command::DeployZone {
        center: FxVec2::from_ints(x, y),
        radius: Fx::from_int(20),
    }
}

fn spawn(x: i32, y: i32, facing: Angle) -> Command {
    Command::SpawnRegiment {
        unit: 0,
        position: FxVec2::from_ints(x, y),
        facing,
    }
}

fn place(regiment: RegimentHandle, x: i32, y: i32) -> Command {
    Command::Place {
        regiment,
        position: FxVec2::from_ints(x, y),
        facing: NORTH,
        files: 6,
    }
}

/// Two armies facing each other 40 tiles apart, each in its own zone, the
/// way a lobby battle starts. Returns the world and each side's regiment.
fn deployed() -> (World, RegimentHandle, RegimentHandle) {
    let mut w = world();
    let stats = w.tick(&[
        pc(0, zone(100, 140)),
        pc(0, spawn(100, 140, NORTH)),
        pc(1, zone(100, 100)),
        pc(1, spawn(100, 100, SOUTH)),
    ]);
    assert_eq!(stats.commands_rejected, 0);
    let mine = w.regiments.iter().find(|r| r.owner == 0).unwrap().handle;
    let theirs = w.regiments.iter().find(|r| r.owner == 1).unwrap().handle;
    (w, mine, theirs)
}

#[test]
fn time_stands_still_and_only_placing_is_allowed() {
    let (mut w, mine, theirs) = deployed();
    assert!(w.deploying());
    let stats = w.tick(&[
        pc(
            0,
            Command::MoveTo {
                regiment: mine,
                target: FxVec2::from_ints(100, 110),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            },
        ),
        pc(
            0,
            Command::Attack {
                regiment: mine,
                target: theirs,
            },
        ),
        // A zone only on the first tick.
        pc(0, zone(10, 10)),
    ]);
    assert_eq!(stats.commands_rejected, 3);
    // Placing inside your own zone works at once; outside it, or someone
    // else's regiment, doesn't.
    let stats = w.tick(&[
        pc(0, place(mine, 110, 150)),
        pc(0, place(mine, 100, 100)),
        pc(1, place(mine, 100, 100)),
    ]);
    assert_eq!(stats.commands_applied, 1);
    let r = w.regiments.get(mine).unwrap();
    assert_eq!(r.position, FxVec2::from_ints(110, 150));
    assert_eq!((r.facing, r.files), (NORTH, 6));
    for _ in 0..100 {
        w.tick(&[]);
    }
    assert_eq!(
        w.regiments.get(mine).unwrap().position,
        FxVec2::from_ints(110, 150)
    );
}

#[test]
fn enemies_are_hidden_until_the_battle() {
    let (mut w, mine, theirs) = deployed();
    assert!(w.sees(0, mine) && !w.sees(0, theirs) && !w.sees(1, mine));
    w.tick(&[pc(0, Command::Ready), pc(1, Command::Ready)]);
    assert!(!w.deploying());
    assert!(w.sees(0, theirs) && w.sees(1, mine));
}

#[test]
fn the_battle_starts_when_everyone_is_ready() {
    let (mut w, mine, _) = deployed();
    let stats = w.tick(&[pc(0, Command::Ready), pc(0, Command::Ready)]);
    assert_eq!((stats.commands_applied, stats.commands_rejected), (1, 1));
    assert!(w.deploying() && w.deploy_ready(0) && !w.deploy_ready(1));
    // Still placing after you are ready.
    assert_eq!(w.tick(&[pc(0, place(mine, 100, 130))]).commands_applied, 1);
    w.tick(&[pc(1, Command::Ready)]);
    assert!(!w.deploying());
    // Now orders march.
    let at = w.regiments.get(mine).unwrap().position;
    w.tick(&[pc(
        0,
        Command::MoveTo {
            regiment: mine,
            target: FxVec2::from_ints(100, 110),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    )]);
    for _ in 0..20 {
        w.tick(&[]);
    }
    assert!(w.regiments.get(mine).unwrap().position.y < at.y);
    assert_eq!(w.tick(&[pc(0, Command::Ready)]).commands_rejected, 1);
}

#[test]
fn the_battle_starts_when_time_runs_out() {
    let (mut w, _, _) = deployed();
    let limit = DEPLOY_SECONDS * TICKS_PER_SECOND;
    assert_eq!(w.deploy_ticks_left(), limit - 1);
    while w.tick_count() < limit {
        assert!(w.deploying());
        w.tick(&[]);
    }
    assert!(!w.deploying());
}

#[test]
fn deploying_is_deterministic_and_leaves_other_worlds_alone() {
    let run = || {
        let (mut w, mine, _) = deployed();
        w.tick(&[pc(0, place(mine, 95, 145)), pc(0, Command::Ready)]);
        w.tick(&[pc(1, Command::Ready)]);
        for _ in 0..200 {
            w.tick(&[]);
        }
        w.state_hash()
    };
    assert_eq!(run(), run());
    // A world with no deployment hashes as it did before deployment existed:
    // the same as one where the zone was refused.
    let plain = || {
        let mut w = world();
        w.tick(&[pc(0, spawn(100, 140, NORTH))]);
        w.state_hash()
    };
    let refused = || {
        let mut w = world();
        w.tick(&[pc(9, zone(100, 140)), pc(0, spawn(100, 140, NORTH))]);
        w.state_hash()
    };
    assert_eq!(plain(), refused());
}
