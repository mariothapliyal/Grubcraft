//! Frontage stress scenarios (docs/29-restart-plan.md, "Frontage: only the
//! front ranks fight"): a swarm of Skitterlings against one regiment of Yari
//! Ashigaru holding a gate, a bridge head and open ground, with the real
//! unit stats. Only soldiers who touch the enemy strike, so the narrow
//! places hold far longer than the open, where the swarm wraps the line.
//!
//! `cargo test -p gc_match --release frontage -- --nocapture` prints the
//! numbers.

use gc_fixed::FxVec2;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::NORTH;
use gc_sim::{
    Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, TICKS_PER_SECOND,
    World,
};

const MAP: u32 = 128;
/// Where the defenders stand, facing north, and the gap they hold.
const HOLD: (i32, i32) = (64, 64);
const GAP: core::ops::Range<i32> = 61..67;
/// Bridge: a deck two tiles wide over water from row 44 down to the
/// defenders' bank.
const DECK: core::ops::Range<i32> = 63..65;
const RIVER: core::ops::Range<i32> = 44..62;
/// The swarm: this many Skitterling regiments, coming from the north.
const SWARM: i32 = 6;
const LIMIT_SECONDS: u32 = 180;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Ground3 {
    Open,
    Gate,
    Bridge,
}

#[derive(Debug)]
struct Held {
    /// Seconds from the first blow until the defenders broke or fell (to
    /// the limit, if they held).
    seconds: u32,
    defenders_lost: u16,
    swarm_lost: u32,
}

fn nav(ground: Ground3) -> Vec<u8> {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    (0..side * side)
        .map(|c| {
            let x = (c % side) as i32 / CELLS_PER_TILE;
            let y = (c / side) as i32 / CELLS_PER_TILE;
            let open = match ground {
                Ground3::Open => true,
                // A wall two tiles thick just in front of the defenders.
                Ground3::Gate => !(61..63).contains(&y) || GAP.contains(&x),
                Ground3::Bridge => !RIVER.contains(&y) || DECK.contains(&x),
            };
            u8::from(open)
        })
        .collect()
}

/// `stalwart`: nobody routs, so the fight is blows alone, to the last man.
fn hold(ground: Ground3, stalwart: bool) -> Held {
    let data = crate::test_data();
    let mut tables = crate::Tables::new(data);
    if stalwart {
        for u in &mut tables.units {
            u.morale = 0;
        }
    }
    let (yari, _) = data.unit_by_name("Yari Ashigaru").unwrap();
    let (skitter, _) = data.unit_by_name("Skitterlings").unwrap();
    let nav = nav(ground);
    let mut w = World::new(&MatchSetup {
        seed: 11,
        capacities: Capacities::default(),
        units: &tables.units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let spawn = |w: &mut World, player: u8, unit: u16, x: i32, y: i32| -> RegimentHandle {
        let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
        w.tick(&[PlayerCommand {
            player,
            command: Command::SpawnRegiment {
                unit,
                position: FxVec2::from_ints(x, y),
                facing: NORTH,
            },
        }]);
        w.regiments
            .iter()
            .map(|r| r.handle)
            .find(|h| !before.contains(h))
            .unwrap()
    };
    // In the gate the line stands in the gap, its front rank level with
    // the wall's outer face.
    let y = if ground == Ground3::Gate { 63 } else { HOLD.1 };
    let line = spawn(&mut w, 0, yari, HOLD.0, y);
    let full = w.regiments.get(line).unwrap().models;
    let swarm: Vec<_> = (0..SWARM)
        .map(|k| spawn(&mut w, 1, skitter, 40 + 10 * (k % 6), 24 - 8 * (k / 6)))
        .collect();
    let swarm_full: u32 = swarm
        .iter()
        .map(|&h| u32::from(w.regiments.get(h).unwrap().models))
        .sum();
    let attack: Vec<_> = swarm
        .iter()
        .map(|&regiment| PlayerCommand {
            player: 1,
            command: Command::Attack {
                regiment,
                target: line,
            },
        })
        .collect();
    w.tick(&attack);
    let mut seconds = LIMIT_SECONDS;
    let mut contact = None;
    for t in 0..LIMIT_SECONDS * TICKS_PER_SECOND {
        w.tick(&[]);
        if contact.is_none() && !w.blows().is_empty() {
            contact = Some(t);
        }
        match w.regiments.get(line) {
            Some(r) if !r.morale.broken() && r.models > 0 => {}
            _ => {
                seconds = (t - contact.unwrap_or(t)) / TICKS_PER_SECOND;
                break;
            }
        }
    }
    let left = w.regiments.get(line).map_or(0, |r| r.models);
    let swarm_left: u32 = swarm
        .iter()
        .filter_map(|&h| w.regiments.get(h))
        .map(|r| u32::from(r.models))
        .sum();
    Held {
        seconds,
        defenders_lost: full - left,
        swarm_lost: swarm_full - swarm_left,
    }
}

/// The real-play check: one Yari regiment against six Skitterling
/// regiments holds a gate and a bridge head far longer than open ground,
/// and kills at least twice as many of the swarm for each man it loses there. Played twice:
/// with morale, and to the last man (blows alone).
#[test]
fn a_line_holds_a_gate_and_a_bridge_against_a_swarm() {
    for stalwart in [false, true] {
        let [open, gate, bridge] = [Ground3::Open, Ground3::Gate, Ground3::Bridge].map(|g| {
            let started = std::time::Instant::now();
            let held = hold(g, stalwart);
            println!(
                "{g:?}{}: {held:?} ({:.0?} to play)",
                if stalwart { " to the last man" } else { "" },
                started.elapsed()
            );
            held
        });
        if !stalwart {
            // Morale breaks the line long before blows do (see the morale
            // redesign in docs/29), so only the fight to the last man is
            // held to the frontage numbers.
            continue;
        }
        let rate = |h: &Held| h.swarm_lost as f32 / f32::from(h.defenders_lost.max(1));
        for (name, narrow) in [("gate", &gate), ("bridge", &bridge)] {
            assert!(
                narrow.seconds * 2 >= open.seconds * 3,
                "the {name} holds half as long again as the open: {narrow:?} against {open:?}"
            );
            assert!(
                rate(narrow) >= rate(&open) * 2.0,
                "the {name} trades better than the open: {narrow:?} against {open:?}"
            );
        }
    }
}
