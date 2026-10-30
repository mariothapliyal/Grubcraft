//! 200 soldiers of the Ember Clans, foot, archers and horse, march through
//! their own Gate in a Palisade wall run right across the map.

use gc_sim::{RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, centre, move_to, put_up, soldiers, spawn};
use super::{Report, Scenario};

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

fn run(report: &mut Report) {
    let real = Real::load();
    let mut world = Arena::new(TILES).world(&real, 11, 0);
    let palisade = real.building("Palisade");
    let gate = real.building("Gate");
    for x in (0..TILES as i32).filter(|x| !(GATE_X..GATE_X + 3).contains(x)) {
        put_up(report, &mut world, 0, palisade, (x, WALL_ROW), 0);
    }
    put_up(report, &mut world, 0, gate, (GATE_X, WALL_ROW), 1);

    // 4 x 28 spears, 3 x 16 swords, 2 x 16 bows and 8 horse: 200.
    let army: Vec<u16> = [
        ("Yari Ashigaru", 4),
        ("Katana Samurai", 3),
        ("Yumi Ashigaru", 2),
        ("Mounted Samurai", 1),
    ]
    .iter()
    .flat_map(|&(name, n)| std::iter::repeat_n(real.unit(name), n))
    .collect();
    let placed: Vec<_> = (army.iter().enumerate())
        .map(|(i, &u)| (u, centre(14 + (i as i32 % 5) * 9, 42 + (i as i32 / 5) * 6)))
        .collect();
    let army = spawn(report, &mut world, 0, &placed, super::arena::WEST);
    let start = soldiers(&world, &army);
    report.metric("soldiers", start);

    let orders: Vec<_> = (army.iter().enumerate())
        .map(|(i, &r)| {
            move_to(
                0,
                r,
                centre(14 + (i as i32 % 5) * 9, 12 + (i as i32 / 5) * 6),
            )
        })
        .collect();
    report.tick(&mut world, &orders);

    let through = |w: &World, r: &[RegimentHandle]| {
        (w.regiments.iter())
            .filter(|v| r.contains(&v.handle) && v.position.y.floor_int() < WALL_ROW)
            .count()
    };
    let limit = LIMIT_SECONDS * TICKS_PER_SECOND;
    let mut first = None;
    let mut done = None;
    for t in 1..=limit {
        report.tick(&mut world, &[]);
        let n = through(&world, &army);
        if n > 0 && first.is_none() {
            first = Some(t);
        }
        let standing = (world.regiments.iter())
            .filter(|v| army.contains(&v.handle))
            .all(|v| !v.moving);
        if n == army.len() && standing {
            done = Some(t);
            break;
        }
    }
    let seconds = |t: Option<u32>| {
        t.map_or("never".into(), |t| {
            format!("{:.1} s", t as f32 / TICKS_PER_SECOND as f32)
        })
    };
    report.metric("first regiment through", seconds(first));
    report.metric("all through and standing", seconds(done));
    report.metric(
        "regiments through",
        format!("{}/{}", through(&world, &army), army.len()),
    );
    report.check(
        format!("every regiment through and standing within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
    report.check("no soldier lost", soldiers(&world, &army) == start);
}
