//! Shift-queued orders and attack-moves (see `gc_sim::orders`).

use gc_fixed::{Fx, FxVec2};
use gc_sim::orders::{QUEUE_LEN, Step};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, RegimentView, TICKS_PER_SECOND,
    World,
};

fn world() -> World {
    World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    })
}

/// Spawn a regiment of drill unit 0 for `player` at `at`, facing north.
fn spawn(w: &mut World, player: u8, at: (i32, i32)) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(at.0, at.1),
            facing: NORTH,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn send(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn to(x: i32, y: i32) -> Step {
    Step::Move {
        target: FxVec2::from_ints(x, y),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
        hunt: false,
    }
}

fn move_to(regiment: RegimentHandle, x: i32, y: i32) -> Command {
    Command::MoveTo {
        regiment,
        target: FxVec2::from_ints(x, y),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
    }
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.get(h).unwrap()
}

fn run(w: &mut World, seconds: u32) {
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

#[test]
fn a_queued_order_on_an_idle_regiment_starts_at_once() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    assert!(send(
        &mut w,
        0,
        Command::Then {
            regiment: r,
            step: to(50, 40)
        }
    ));
    assert!(view(&w, r).moving);
    assert_eq!(view(&w, r).queued, 0);
}

#[test]
fn queued_waypoints_are_walked_in_order() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    assert!(send(&mut w, 0, move_to(r, 50, 40)));
    for (x, y) in [(60, 40), (60, 50)] {
        assert!(send(
            &mut w,
            0,
            Command::Then {
                regiment: r,
                step: to(x, y)
            }
        ));
    }
    assert_eq!(view(&w, r).queued, 2);
    // It reaches the first before turning for the second.
    let mut passed = [false; 2];
    for _ in 0..40 * TICKS_PER_SECOND {
        w.tick(&[]);
        let p = view(&w, r).position;
        passed[0] |= p == FxVec2::from_ints(50, 40);
        passed[1] |= p == FxVec2::from_ints(60, 40);
    }
    assert_eq!(passed, [true, true]);
    let r = view(&w, r);
    assert_eq!(
        (r.position, r.moving, r.queued),
        (FxVec2::from_ints(60, 50), false, 0)
    );
}

#[test]
fn an_order_without_shift_drops_the_queue() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    assert!(send(&mut w, 0, move_to(r, 50, 40)));
    assert!(send(
        &mut w,
        0,
        Command::Then {
            regiment: r,
            step: to(60, 40)
        }
    ));
    assert!(send(&mut w, 0, move_to(r, 40, 50)));
    assert_eq!(view(&w, r).queued, 0);
    run(&mut w, 20);
    assert_eq!(view(&w, r).position, FxVec2::from_ints(40, 50));
}

#[test]
fn a_full_queue_refuses_more() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    assert!(send(&mut w, 0, move_to(r, 50, 40)));
    for k in 0..QUEUE_LEN as i32 {
        assert!(send(
            &mut w,
            0,
            Command::Then {
                regiment: r,
                step: to(60 + k, 40)
            }
        ));
    }
    assert!(!send(
        &mut w,
        0,
        Command::Then {
            regiment: r,
            step: to(99, 99)
        }
    ));
    // Nobody may queue orders for another's regiment.
    let theirs = spawn(&mut w, 1, (150, 150));
    assert!(!send(
        &mut w,
        0,
        Command::Then {
            regiment: theirs,
            step: to(1, 1)
        }
    ));
}

#[test]
fn a_queued_attack_follows_the_march() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    let foe = spawn(&mut w, 1, (80, 40));
    assert!(send(&mut w, 0, move_to(r, 50, 40)));
    assert!(send(
        &mut w,
        0,
        Command::Then {
            regiment: r,
            step: Step::Attack { target: foe },
        }
    ));
    run(&mut w, 3);
    assert!(!view(&w, r).engaged);
    run(&mut w, 15);
    assert!(view(&w, r).engaged, "{:?}", view(&w, r));
}

#[test]
fn an_attack_move_fights_what_it_meets_then_marches_on() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    // Standing just off the line of march.
    let foe = spawn(&mut w, 1, (58, 80));
    assert!(send(
        &mut w,
        0,
        Command::AttackMove {
            regiment: r,
            target: FxVec2::from_ints(50, 120),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    assert!(view(&w, r).hunting);
    let mut fought = false;
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
        fought |= view(&w, r).engaged;
    }
    assert!(fought, "it turned on the enemy it met");
    // The enemy is gone: the march goes on to its end.
    if w.regiments.get(foe).is_some() {
        assert!(send(&mut w, 1, Command::RemoveRegiment { regiment: foe }));
    }
    run(&mut w, 40);
    let r = view(&w, r);
    assert!(
        (r.position - FxVec2::from_ints(50, 120)).length() < Fx::from_int(1),
        "{r:?}"
    );
}

#[test]
fn a_plain_move_marches_past_the_enemy() {
    let mut w = world();
    let r = spawn(&mut w, 0, (50, 50));
    spawn(&mut w, 1, (62, 80));
    assert!(send(&mut w, 0, move_to(r, 50, 120)));
    assert!(!view(&w, r).hunting);
    let mut fought = false;
    for _ in 0..30 * TICKS_PER_SECOND {
        w.tick(&[]);
        fought |= view(&w, r).engaged;
    }
    assert!(!fought);
    assert_eq!(view(&w, r).position, FxVec2::from_ints(50, 120));
}
