//! 200 soldiers of the Ember Clans, foot, archers and horse, march through
//! their own Gate in a Palisade wall run right across the map.

use gc_sim::{TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, WEST, grid, move_to, of, put_up, soldiers, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "gate",
    about: "200 Ember soldiers (spears, swords, bows, horse) march through one Gate",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 64;
/// The wall's row, and the Gate's tiles on it (turned to run along it).
const WALL_ROW: i32 = 30;
const GATE_X: i32 = 30;
/// Every regiment must be through and standing within this long.
const LIMIT_SECONDS: u32 = 75;

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 11, 0);
    let (palisade, gate) = (real.building("Palisade"), real.building("Gate"));
    // Turned a quarter, the Gate runs east to west along its long side.
    let stats = &real.table.stats[usize::from(gate)];
    let span = GATE_X..GATE_X + i32::from(stats.width.max(stats.depth));
    let mut wall: Vec<_> = (0..TILES as i32)
        .filter(|x| !span.contains(x))
        .map(|x| (palisade, (x, WALL_ROW), 0))
        .collect();
    wall.push((gate, (GATE_X, WALL_ROW), 1));
    put_up(&mut world, 0, &wall);

    // 4 x 28 spears, 3 x 16 swords, 2 x 16 bows and 8 horse: 200.
    let [yari, katana, yumi] = real.ember_line();
    let horse = real.unit("Mounted Samurai");
    let army = [
        yari, yari, yari, yari, katana, katana, katana, yumi, yumi, horse,
    ];
    // Five abreast south of the wall, marching to the same grid north of it.
    let at = |y: i32, i: usize| grid((14, y), 5, (9, 6), i);
    let placed: Vec<_> = (army.iter().enumerate())
        .map(|(i, &u)| (u, at(42, i)))
        .collect();
    let army = spawn(&mut world, 0, &placed, WEST);
    let start = soldiers(&world, &army);
    report.metric("soldiers", start);
    let orders: Vec<_> = (army.iter().enumerate())
        .map(|(i, &r)| move_to(0, r, at(12, i)))
        .collect();
    report.tick(&mut world, &orders);

    let through = |w: &World| {
        of(w, &army)
            .filter(|v| v.position.y.floor_int() < WALL_ROW)
            .count()
    };
    let mut first = None;
    let mut done = None;
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        let n = through(&world);
        if n > 0 && first.is_none() {
            first = Some(t);
        }
        // Every regiment still on the field through and standing (a lost
        // one fails the check on losses below, not this one).
        let alive = of(&world, &army).count();
        if alive > 0 && n == alive && of(&world, &army).all(|v| !v.moving) {
            done = Some(t);
            break;
        }
    }
    report.metric("first regiment through", seconds(first));
    report.metric("all through and standing", seconds(done));
    report.metric(
        "regiments through",
        format!("{}/{}", through(&world), army.len()),
    );
    report.check(
        format!("every regiment through and standing within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
    report.check("no soldier lost", soldiers(&world, &army) == start);
}
