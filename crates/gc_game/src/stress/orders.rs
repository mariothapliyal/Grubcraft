//! Queued orders (docs/29 item 6, "Real use"): four Ember regiments are
//! given, all at once, a march up to their own Gate, a march through it
//! (Shift) and an attack on the enemy beyond (Shift). They file through
//! and fight on arrival, with no order given after the first tick.

use gc_fixed::Fx;
use gc_sim::orders::Step;
use gc_sim::{Command, PlayerCommand, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, WEST, centre, grid, move_to, of, put_up, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "queued-orders",
    about: "4 regiments told at once: up to the Gate, through it, then attack",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 64;
/// The wall's row, and where its Gate starts on it.
const WALL_ROW: i32 = 30;
const GATE_X: i32 = 30;
/// The enemy must be beaten within this long.
const LIMIT_SECONDS: u32 = 100;

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 13, 0);
    let (palisade, gate) = (real.building("Palisade"), real.building("Gate"));
    let stats = &real.table.stats[usize::from(gate)];
    let span = GATE_X..GATE_X + i32::from(stats.width.max(stats.depth));
    let middle = (span.start + span.end) / 2;
    let mut wall: Vec<_> = (0..TILES as i32)
        .filter(|x| !span.contains(x))
        .map(|x| (palisade, (x, WALL_ROW), 0))
        .collect();
    wall.push((gate, (GATE_X, WALL_ROW), 1));
    put_up(&mut world, 0, &wall);

    let [yari, katana, _] = real.ember_line();
    let army: Vec<_> = ([yari, katana, yari, katana].iter().enumerate())
        .map(|(i, &u)| (u, grid((18, 46), 4, (9, 6), i)))
        .collect();
    let army = spawn(&mut world, 0, &army, WEST);
    let foe = spawn(&mut world, 1, &[(katana, centre(middle, 10))], WEST)[0];

    // All three orders in one tick: the first plain, the rest queued.
    let mut orders = Vec::new();
    for (i, &r) in army.iter().enumerate() {
        orders.push(move_to(0, r, centre(middle, 36 + 2 * i as i32)));
        let through = Step::Move {
            target: grid((middle - 9, 20), 4, (6, 0), i),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            hunt: false,
        };
        for step in [through, Step::Attack { target: foe }] {
            orders.push(PlayerCommand {
                player: 0,
                command: Command::Then { regiment: r, step },
            });
        }
    }
    report.tick(&mut world, &orders);

    // Where each regiment stood the first time it fought.
    let mut fought_at = vec![None; army.len()];
    let mut beaten = None;
    let mut most_queued = 0;
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        for (k, &r) in army.iter().enumerate() {
            if let Some(v) = world.regiments.get(r) {
                most_queued = most_queued.max(v.queued);
                if v.engaged && fought_at[k].is_none() {
                    fought_at[k] = Some(v.position.y.floor_int());
                }
            }
        }
        if foe_beaten(&world, foe) {
            beaten = Some(t);
            break;
        }
    }
    let fought = fought_at.iter().flatten().count();
    report.metric("orders queued at most", most_queued);
    report.metric("enemy beaten", seconds(beaten));
    report.metric("regiments that fought", format!("{fought}/{}", army.len()));
    report.check(
        format!("the enemy beaten within {LIMIT_SECONDS} s"),
        beaten.is_some(),
    );
    report.check(
        "every regiment that fought did so through the Gate",
        fought_at.iter().flatten().all(|&y| y < WALL_ROW),
    );
    report.check("the regiments fought", fought >= 2);
    report.check(
        "no regiment lost on the way",
        of(&world, &army).count() == army.len(),
    );
}

/// Whether regiment `foe` is gone or broken.
fn foe_beaten(world: &World, foe: gc_sim::RegimentHandle) -> bool {
    world.regiments.get(foe).is_none_or(|v| v.morale.broken())
}
