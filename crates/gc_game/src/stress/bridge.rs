//! A column of six Ember regiments crosses a two-lane Timber Bridge over a
//! deep river while enemy archers on the far bank shoot at it.

use gc_sim::{TICKS_PER_SECOND, World};

use super::arena::{
    Arena, EAST, Real, WEST, attack_nearest, centre, move_to, put_up, soldiers, spawn,
};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "bridge",
    about: "a column of 6 regiments crosses a two-lane bridge under archer fire",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 64;
/// The river's columns, and the bridge's two rows from bank to bank.
const RIVER: core::ops::Range<i32> = 30..34;
const ROW: i32 = 30;
const SPAN: core::ops::RangeInclusive<i32> = 27..=36;
const LIMIT_SECONDS: u32 = 70;

fn run(report: &mut Report) {
    let real = Real::load();
    let mut arena = Arena::new(TILES);
    arena.river(RIVER, 0..TILES as i32);
    let mut world = arena.world(&real, 12, 0);
    let bridge = real.building("Timber Bridge");
    for x in SPAN {
        for y in [ROW, ROW + 1] {
            put_up(report, &mut world, 0, bridge, (x, y), 0);
        }
    }

    let (yari, katana, yumi) = (
        real.unit("Yari Ashigaru"),
        real.unit("Katana Samurai"),
        real.unit("Yumi Ashigaru"),
    );
    let column: Vec<_> = [yari, katana, yari, yumi, katana, yari]
        .iter()
        .enumerate()
        .map(|(i, &u)| (u, centre(20 - i as i32 * 3, ROW + (i as i32 % 2) * 4 - 2)))
        .collect();
    let column = spawn(report, &mut world, 0, &column, EAST);
    let archers: Vec<_> = (0..3)
        .map(|i| (yumi, centre(46, ROW + 12 + i * 4)))
        .collect();
    let archers = spawn(report, &mut world, 1, &archers, WEST);
    let start = soldiers(&world, &column);
    report.metric("soldiers crossing", start);

    let orders: Vec<_> = (column.iter().enumerate())
        .map(|(i, &r)| {
            move_to(
                0,
                r,
                centre(50 + (i as i32 % 2) * 6, ROW - 6 + (i as i32 / 2) * 6),
            )
        })
        .collect();
    report.tick(&mut world, &orders);

    // Regiment-ticks with the block's middle over the river off the deck.
    let mut in_river = 0u32;
    let mut done = None;
    let across = |w: &World| {
        (w.regiments.iter())
            .filter(|v| column.contains(&v.handle))
            .all(|v| v.position.x.floor_int() > RIVER.end + 3 && !v.moving)
    };
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        let shoot = if t % TICKS_PER_SECOND == 0 {
            attack_nearest(&world, 1, &archers, 0)
        } else {
            Vec::new()
        };
        report.tick(&mut world, &shoot);
        for v in world
            .regiments
            .iter()
            .filter(|v| column.contains(&v.handle))
        {
            let (x, y) = (v.position.x.floor_int(), v.position.y.floor_int());
            if arena.is_river(x, y) && !(ROW..=ROW + 1).contains(&y) {
                in_river += 1;
            }
        }
        if across(&world) {
            done = Some(t);
            break;
        }
    }
    let left = soldiers(&world, &column);
    report.metric(
        "all across and standing",
        done.map_or("never".into(), |t| {
            format!("{:.1} s", t as f32 / TICKS_PER_SECOND as f32)
        }),
    );
    report.metric("soldiers lost to arrows or water", start - left);
    report.metric("regiment-ticks over the river off the deck", in_river);
    report.check(
        format!("the column is across and standing within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
    report.check(
        "no regiment strays off the deck into the river",
        in_river == 0,
    );
}
