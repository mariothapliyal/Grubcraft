//! How quickly regiments answer an order: the ticks from a move order to
//! the block's first step (docs/29, "How battles play", item 6).
//!
//! An order reaches the sim in the tick after the click (50 ms; online play
//! adds the lockstep's input delay). From then on the regiment must not
//! stand about: it marches off at once toward anything up to 90 degrees off
//! its facing, wheeling as it goes, and only turns on the spot first for
//! an order further round than that.

use gc_fixed::{Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH, ribs_map};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World,
};

fn world(nav: &[u8]) -> World {
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    })
}

/// A regiment of drill unit `unit` at `from` facing north, ordered to `to`.
fn ordered(nav: &[u8], unit: u16, from: (i32, i32), to: (i32, i32)) -> (World, RegimentHandle) {
    let mut w = world(nav);
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(from.0, from.1),
            facing: NORTH,
        },
    }]);
    let h = w.regiments.iter().next().unwrap().handle;
    let stats = w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: h,
            target: FxVec2::from_ints(to.0, to.1),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        },
    }]);
    assert_eq!(stats.commands_applied, 1);
    (w, h)
}

fn pos(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.get(h).unwrap().position
}

/// Ticks after the order's own tick until the block has moved.
fn ticks_to_first_step(nav: &[u8], unit: u16, from: (i32, i32), to: (i32, i32)) -> u32 {
    let (mut w, h) = ordered(nav, unit, from, to);
    let start = FxVec2::from_ints(from.0, from.1);
    for t in 0..400 {
        if pos(&w, h) != start {
            return t;
        }
        w.tick(&[]);
    }
    panic!("unit {unit} never moved toward {to:?}");
}

/// Ticks a regiment of `unit` needs to turn `degrees` on the spot.
fn turn_ticks(unit: u16, degrees: u32) -> u32 {
    let per_second = DRILL_UNITS[unit as usize].turn_rate;
    (degrees * 0x10000 / 360).div_ceil(per_second / TICKS_PER_SECOND)
}

#[test]
fn an_order_ahead_moves_in_the_same_tick() {
    for unit in 0..DRILL_UNITS.len() as u16 {
        assert_eq!(
            ticks_to_first_step(&[], unit, (100, 100), (100, 60)),
            0,
            "unit {unit}"
        );
    }
}

#[test]
fn an_order_to_the_side_moves_off_at_once() {
    for unit in 0..DRILL_UNITS.len() as u16 {
        for to in [(140, 100), (60, 100), (140, 90)] {
            assert_eq!(
                ticks_to_first_step(&[], unit, (100, 100), to),
                0,
                "unit {unit} to {to:?}"
            );
        }
    }
}

#[test]
fn an_order_behind_turns_only_until_it_can_wheel() {
    for unit in 0..DRILL_UNITS.len() as u16 {
        let got = ticks_to_first_step(&[], unit, (100, 100), (100, 140));
        // Only the turn past 90 degrees is made standing.
        assert!(got <= turn_ticks(unit, 90), "unit {unit}: {got} ticks");
    }
}

#[test]
fn a_routed_march_moves_off_without_waiting_for_its_search() {
    // Across the ridged test map, round ridges: the route search must not
    // hold the regiment up beyond its turn. The way lies about 135 degrees
    // off its facing, so only about 45 of them are turned standing.
    let nav = ribs_map(256);
    for unit in 0..DRILL_UNITS.len() as u16 {
        let got = ticks_to_first_step(&nav, unit, (30, 30), (220, 220));
        assert!(got <= turn_ticks(unit, 45) + 2, "unit {unit}: {got} ticks");
    }
}

#[test]
fn a_block_wheeling_sideways_marches_at_half_pace() {
    // Drill unit 3 turns slowest (45 degrees a second): ordered due east
    // from facing north, its first step is at about half pace, and it is
    // at full pace once it faces within 45 degrees of its way.
    let (mut w, h) = ordered(&[], 3, (100, 100), (160, 100));
    let speed = DRILL_UNITS[3].speed.raw() / TICKS_PER_SECOND as i32;
    // The order's own tick made the first step.
    let first = (w.regiments.get(h).unwrap().position - FxVec2::from_ints(100, 100))
        .length()
        .raw();
    assert!(
        first > 0 && first < speed * 6 / 10,
        "first step {first} of {speed}"
    );
    for _ in 0..TICKS_PER_SECOND * 2 {
        w.tick(&[]);
    }
    let before = w.regiments.get(h).unwrap().position;
    w.tick(&[]);
    let step = (w.regiments.get(h).unwrap().position - before)
        .length()
        .raw();
    assert!(step >= speed * 95 / 100, "step {step} of {speed}");
}
