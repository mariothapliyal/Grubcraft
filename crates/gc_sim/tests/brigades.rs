//! Brigades (docs/42-armies-that-march.md): regiments ordered together
//! with a brigade number set off together, keep their shape on the way,
//! file through a narrows and arrive together.

use gc_fixed::{Fx, FxVec2};
use gc_sim::formation::{GroupMember, GroupOrder, group_move};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND,
    UnitStats, World,
};

const MAP: i32 = 256;

/// The drill units with morale off, so nobody runs.
fn fearless() -> [UnitStats; 4] {
    DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u })
}

fn world(nav: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 64 },
        units: &fearless(),
        map_tiles: MAP as u32,
        nav,
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    })
}

/// A wall across the map from y = 110 to y = 130 with one gap in it nine
/// tiles wide: room for one block, not two side by side.
fn walled() -> Vec<u8> {
    let side = MAP * CELLS_PER_TILE;
    (0..side * side)
        .map(|c| {
            let (x, y) = ((c % side) / CELLS_PER_TILE, (c / side) / CELLS_PER_TILE);
            if (110..130).contains(&y) && !(124..133).contains(&x) {
                BLOCKED
            } else {
                1
            }
        })
        .collect()
}

/// Six regiments of player 0 facing north: four about a line and two
/// that have fallen well behind it.
const SCATTERED: [(u16, i32, i32); 6] = [
    (0, 100, 200),
    (1, 112, 203),
    (0, 124, 199),
    (3, 136, 202),
    (0, 106, 222),
    (1, 130, 226),
];

fn spawn(w: &mut World, player: u8, army: &[(u16, i32, i32)]) -> Vec<RegimentHandle> {
    let spawns: Vec<PlayerCommand> = army
        .iter()
        .map(|&(unit, x, y)| PlayerCommand {
            player,
            command: Command::SpawnRegiment {
                unit,
                position: FxVec2::from_ints(x, y),
                facing: NORTH,
            },
        })
        .collect();
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&spawns);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .filter(|h| !before.contains(h))
        .collect()
}

/// The moves a group order to (`x`, `y`) gives `who`.
fn group(w: &World, who: &[RegimentHandle], x: i32, y: i32, brigade: u16) -> Vec<PlayerCommand> {
    let units = fearless();
    let mut members: Vec<GroupMember> = who
        .iter()
        .map(|&h| {
            let r = w.regiments.get(h).unwrap();
            GroupMember::of(&r, &units[r.unit as usize])
        })
        .collect();
    let mut moves = Vec::new();
    group_move(
        &mut members,
        GroupOrder {
            target: FxVec2::from_ints(x, y),
            facing: Some(NORTH),
            width: None,
            brigade,
        },
        &mut moves,
    );
    moves
        .into_iter()
        .map(|command| PlayerCommand { player: 0, command })
        .collect()
}

fn goal_of(c: &PlayerCommand) -> (RegimentHandle, FxVec2) {
    match c.command {
        Command::MoveTo {
            regiment, target, ..
        }
        | Command::AttackMove {
            regiment, target, ..
        } => (regiment, target),
        _ => unreachable!(),
    }
}

/// Tick `w` until every regiment the `orders` moved stands at its place,
/// and give the second each got there. Panics after `limit` seconds.
fn arrivals(w: &mut World, orders: &[PlayerCommand], limit: u32) -> Vec<u32> {
    let start = w.tick_count();
    let mut at = vec![None; orders.len()];
    while at.iter().any(Option::is_none) {
        let run = w.tick_count() - start;
        assert!(
            run < limit * TICKS_PER_SECOND,
            "not all there after {limit} s: {at:?}"
        );
        w.tick(&[]);
        for (k, c) in orders.iter().enumerate() {
            let (h, goal) = goal_of(c);
            let r = w.regiments.get(h).unwrap();
            let there = !r.moving && (r.position - goal).length() <= Fx::from_int(2);
            if at[k].is_none() && there {
                at[k] = Some(run / TICKS_PER_SECOND);
            }
        }
    }
    at.into_iter().flatten().collect()
}

fn spread(at: &[u32]) -> u32 {
    at.iter().max().unwrap() - at.iter().min().unwrap()
}

#[test]
fn a_brigade_arrives_together_where_separate_moves_straggle() {
    let run = |brigade: u16| {
        let mut w = world(&[]);
        let who = spawn(&mut w, 0, &SCATTERED);
        let orders = group(&w, &who, 120, 90, brigade);
        w.tick(&orders);
        if brigade != 0 {
            w.tick(&[]);
            assert!(who.iter().all(|&h| w.brigade(h).is_some()));
        }
        let at = arrivals(&mut w, &orders, 150);
        // Arrived, the brigade is no more.
        assert!(who.iter().all(|&h| w.brigade(h).is_none()));
        at
    };
    let (alone, together) = (run(0), run(1));
    assert!(
        spread(&alone) >= 8,
        "the stragglers are late on their own: {alone:?}"
    );
    assert!(
        spread(&together) <= 3,
        "a brigade arrives as one: {together:?}"
    );
    // ... and no later than its last regiment would have alone, but for
    // the forming up.
    assert!(
        together.iter().max().unwrap() <= &(alone.iter().max().unwrap() + 6),
        "{together:?} against {alone:?}"
    );
}

#[test]
fn a_brigade_keeps_its_shape_on_the_way() {
    let mut w = world(&[]);
    let who = spawn(&mut w, 0, &SCATTERED);
    let orders = group(&w, &who, 120, 60, 1);
    w.tick(&orders);
    // It sets off as it stands and forms on the way (the ones behind
    // gain on the rest; nobody waits to form up first). Well before it
    // arrives each is at its place in the body.
    for _ in 0..42 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    for &h in &who {
        let view = w.brigade(h).expect("still on the march");
        let place = view.place.expect("the brigade has found its way");
        let off = (w.regiments.get(h).unwrap().position - place).length();
        assert!(view.in_step, "{h:?} is {off:?} tiles from its place");
    }
    // The body is as wide as it will stand, not strung out: front to
    // back it is two lines deep at most.
    let ys: Vec<Fx> = who
        .iter()
        .map(|&h| w.regiments.get(h).unwrap().position.y)
        .collect();
    let deep = *ys.iter().max().unwrap() - *ys.iter().min().unwrap();
    assert!(deep < Fx::from_int(12), "strung out {deep:?} tiles");
}

#[test]
fn the_same_order_sent_again_changes_nothing() {
    let mut once = world(&[]);
    let mut again = world(&[]);
    let who = spawn(&mut once, 0, &SCATTERED);
    assert_eq!(spawn(&mut again, 0, &SCATTERED), who);
    let orders = group(&once, &who, 120, 90, 1);
    once.tick(&orders);
    again.tick(&orders);
    for tick in 0..40 * TICKS_PER_SECOND {
        once.tick(&[]);
        // A computer player sends its orders over and over.
        if tick % 10 == 0 {
            again.tick(&orders);
        } else {
            again.tick(&[]);
        }
        for &h in &who {
            assert_eq!(
                once.regiments.get(h).unwrap(),
                again.regiments.get(h).unwrap(),
                "tick {tick}"
            );
            assert_eq!(once.brigade(h), again.brigade(h), "tick {tick}");
        }
    }
}

#[test]
fn a_member_given_another_order_leaves_and_the_rest_march_on() {
    let mut w = world(&[]);
    let who = spawn(&mut w, 0, &SCATTERED);
    let orders = group(&w, &who, 120, 90, 1);
    w.tick(&orders);
    for _ in 0..10 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let away = FxVec2::from_ints(200, 200);
    let speed = fearless()[0].speed;
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: who[0],
            target: away,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            brigade: 0,
        },
    }]);
    w.tick(&[]);
    assert!(w.brigade(who[0]).is_none());
    assert!(who[1..].iter().all(|&h| w.brigade(h).is_some()));
    let rest: Vec<PlayerCommand> = orders
        .iter()
        .copied()
        .filter(|c| goal_of(c).0 != who[0])
        .collect();
    let at = arrivals(&mut w, &rest, 150);
    assert!(spread(&at) <= 3, "{at:?}");
    // The one sent away got there at its own pace, not the brigade's.
    let mut ticks = 0;
    while (w.regiments.get(who[0]).unwrap().position - away).length() > Fx::from_int(2) {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < 200 * TICKS_PER_SECOND);
    }
    assert!(speed > Fx::ZERO);
}

#[test]
fn a_short_move_is_each_regiments_own() {
    let mut w = world(&[]);
    let who = spawn(&mut w, 0, &SCATTERED[..4]);
    let orders = group(&w, &who, 118, 192, 1);
    w.tick(&orders);
    w.tick(&[]);
    w.tick(&[]);
    assert!(who.iter().all(|&h| w.brigade(h).is_none()));
    arrivals(&mut w, &orders, 60);
}

#[test]
fn a_brigade_files_through_a_gap_and_forms_again_beyond_it() {
    let nav = walled();
    let run = |brigade: u16| {
        let mut w = world(&nav);
        let who = spawn(&mut w, 0, &SCATTERED);
        let orders = group(&w, &who, 128, 60, brigade);
        w.tick(&orders);
        arrivals(&mut w, &orders, 240)
    };
    let (alone, together) = (run(0), run(1));
    assert!(
        spread(&together) <= 6,
        "through the gap and there as one: {together:?} (alone: {alone:?})"
    );
    assert!(
        together.iter().max().unwrap() <= &(alone.iter().max().unwrap() + 15),
        "{together:?} against {alone:?}"
    );
}

#[test]
fn a_copy_of_a_brigade_on_the_march_marches_on_like_the_original() {
    let nav = walled();
    let mut w = world(&nav);
    let who = spawn(&mut w, 0, &SCATTERED);
    let orders = group(&w, &who, 128, 60, 1);
    w.tick(&orders);
    for _ in 0..20 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let mut copy = w.fork_region(FxVec2::from_ints(128, 128), Fx::from_int(1000));
    for tick in 0..60 * TICKS_PER_SECOND {
        w.tick(&[]);
        copy.tick(&[]);
        for &h in &who {
            assert_eq!(
                w.regiments.get(h).unwrap(),
                copy.regiments.get(h).unwrap(),
                "tick {tick}"
            );
            assert_eq!(w.brigade(h), copy.brigade(h), "tick {tick}");
        }
    }
}

#[test]
fn an_attack_move_brigade_halts_while_one_of_its_own_fights() {
    let mut w = world(&[]);
    let who = spawn(&mut w, 0, &SCATTERED[..4]);
    // A lone foe off to the east of the way north.
    let foe = spawn(&mut w, 1, &[(0, 175, 150)])[0];
    let mut orders = group(&w, &who, 118, 60, 1);
    for c in &mut orders {
        let Command::MoveTo {
            regiment,
            target,
            facing,
            files,
            pace,
            brigade,
        } = c.command
        else {
            unreachable!()
        };
        c.command = Command::AttackMove {
            regiment,
            target,
            facing,
            files,
            pace,
            brigade,
        };
    }
    w.tick(&orders);
    // Until one turns aside for the foe.
    let mut ticks = 0;
    while who.iter().all(|&h| w.brigade(h).is_some()) {
        w.tick(&[]);
        ticks += 1;
        assert!(ticks < 60 * TICKS_PER_SECOND, "nobody went for the foe");
    }
    for _ in 0..3 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    // Those still in the brigade stand and wait for it.
    let waiting: Vec<_> = who
        .iter()
        .copied()
        .filter(|&h| w.brigade(h).is_some())
        .collect();
    let before: Vec<_> = waiting
        .iter()
        .map(|&h| w.regiments.get(h).unwrap().position)
        .collect();
    for _ in 0..2 * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    for (&h, &was) in waiting.iter().zip(&before) {
        if w.regiments.get(foe).is_some() && w.brigade(h).is_some() {
            let moved = (w.regiments.get(h).unwrap().position - was).length();
            assert!(moved < Fx::ONE, "{h:?} marched on {moved:?} tiles");
        }
    }
    // The foe dealt with, they all go on and arrive.
    arrivals(&mut w, &orders, 400);
}
