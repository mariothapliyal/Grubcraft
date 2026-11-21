//! Friends standing at ease in the way of a march step aside and back.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World,
};

fn world() -> World {
    World::new(&MatchSetup {
        seed: 5,
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

fn spawn(w: &mut World, at: (i32, i32), facing: Angle) -> RegimentHandle {
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit: 0,
            position: FxVec2::from_ints(at.0, at.1),
            facing,
        },
    }]);
    w.regiments.iter().last().unwrap().handle
}

struct Crossing {
    /// Ticks until every regiment stands still again.
    ticks: u32,
    /// Regiment-ticks in disorder, marchers and standers.
    disordered: u32,
    /// Of those, the standers'.
    standers_disordered: u32,
    /// How far the standers end up from where they stood, at most.
    displaced: Fx,
}

/// A line of six regiments standing at ease a tile apart, and a column of
/// three more marching through it from behind to 20 tiles beyond it,
/// `offset` tiles right of the line's middle.
fn crossing(offset: i32) -> Crossing {
    let mut w = world();
    let line: Vec<_> = (0..6)
        .map(|k| spawn(&mut w, (40 + 7 * k, 60), NORTH))
        .collect();
    let homes: Vec<_> = line
        .iter()
        .map(|&h| w.regiments.get(h).unwrap().position)
        .collect();
    let marchers: Vec<_> = (0..3)
        .map(|k| spawn(&mut w, (57 + offset, 80 + 8 * k), NORTH))
        .collect();
    let orders: Vec<_> = marchers
        .iter()
        .enumerate()
        .map(|(k, &regiment)| PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment,
                target: FxVec2::from_ints(57 + offset, 24 + 8 * k as i32),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
                brigade: 0,
            },
        })
        .collect();
    w.tick(&orders);
    let (mut disordered, mut standers_disordered) = (0, 0);
    let mut ticks = 0;
    for t in 1..4_000 {
        w.tick(&[]);
        let all: Vec<_> = w.regiments.iter().collect();
        for r in &all {
            if r.disorder > 0 {
                disordered += 1;
                if line.contains(&r.handle) {
                    standers_disordered += 1;
                }
            }
        }
        ticks = t;
        if t > 40 && all.iter().all(|r| !r.moving) {
            break;
        }
    }
    let displaced = line
        .iter()
        .zip(&homes)
        .map(|(&h, &home)| (w.regiments.get(h).unwrap().position - home).length())
        .fold(Fx::ZERO, Fx::max);
    Crossing {
        ticks,
        disordered,
        standers_disordered,
        displaced,
    }
}

#[test]
fn a_column_marches_through_a_line_at_ease_without_pushing_through() {
    for offset in 0..4 {
        let c = crossing(offset);
        // Before standing friends made way: about 700 regiment-ticks in
        // disorder (400 the line's), and 20 s before all stood again.
        assert!(
            c.disordered <= 60,
            "offset {offset}: {} in disorder",
            c.disordered
        );
        assert!(
            c.ticks <= 19 * TICKS_PER_SECOND,
            "offset {offset}: {} ticks",
            c.ticks
        );
        // Every block of the line is back where it stood.
        assert!(
            c.displaced <= Fx::from_ratio(1, 8),
            "offset {offset}: off by {:?}",
            c.displaced
        );
    }
}

#[test]
#[ignore = "a report: cargo test -p gc_sim --test make_way -- --ignored --nocapture"]
fn report_crossings() {
    for offset in 0..4 {
        let c = crossing(offset);
        println!(
            "offset {offset}: {} ticks, disorder {} regiment-ticks ({} standers'), standers moved up to {:.2} tiles",
            c.ticks,
            c.disordered,
            c.standers_disordered,
            c.displaced.raw() as f64 / 65536.0
        );
    }
}
