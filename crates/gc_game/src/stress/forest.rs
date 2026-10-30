//! Six Ember regiments fall back west through a belt of woods, broken by
//! clumps of thicket, with six enemy regiments coming after them.

use gc_sim::TICKS_PER_SECOND;

use super::arena::{Arena, Real, WEST, attack_nearest, centre, move_to, soldiers, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "forest-retreat",
    about: "6 regiments retreat through woods and thicket, chased by 6",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 96;
/// The woods' columns.
const WOODS: core::ops::Range<i32> = 24..40;
const LIMIT_SECONDS: u32 = 85;

fn run(report: &mut Report) {
    let real = Real::load();
    let mut arena = Arena::new(TILES);
    arena.woods(WOODS, 0..TILES as i32);
    // Thicket clumps, staggered, so the way through winds.
    for (x, y) in [
        (27, 20),
        (27, 34),
        (27, 48),
        (33, 13),
        (33, 27),
        (33, 41),
        (33, 55),
    ] {
        arena.thicket(x..x + 4, y..y + 5);
    }
    let mut world = arena.world(&real, 13, 0);
    let (yari, katana, yumi) = (
        real.unit("Yari Ashigaru"),
        real.unit("Katana Samurai"),
        real.unit("Yumi Ashigaru"),
    );
    let units = [yari, katana, yari, yumi, katana, yari];
    let ours: Vec<_> = (units.iter().enumerate())
        .map(|(i, &u)| (u, centre(46 + (i as i32 / 3) * 4, 26 + (i as i32 % 3) * 9)))
        .collect();
    let ours = spawn(report, &mut world, 0, &ours, super::arena::EAST);
    // The pursuers are all foot: archers would shoot the retreat to pieces
    // and make it a test of arrows, not of the way through.
    let theirs: Vec<_> = ([yari, katana, yari, katana, yari, katana]
        .iter()
        .enumerate())
    .map(|(i, &u)| (u, centre(70 + (i as i32 / 3) * 4, 26 + (i as i32 % 3) * 9)))
    .collect();
    let theirs = spawn(report, &mut world, 1, &theirs, WEST);
    let start = soldiers(&world, &ours);
    report.metric("soldiers retreating", start);

    // Each regiment's rally point, west of the woods.
    let rally = |i: usize| centre(8 + (i as i32 / 3) * 6, 18 + (i as i32 % 3) * 14);
    let mut first = attack_nearest(&world, 1, &theirs, 0);
    first.extend((ours.iter().enumerate()).map(|(i, &r)| move_to(0, r, rally(i))));
    report.tick(&mut world, &first);

    // When each regiment first came out of the woods' west edge, and with
    // how many soldiers: the retreat is over for it then, whatever comes
    // after.
    let mut out_at: Vec<Option<(u32, u16)>> = vec![None; ours.len()];
    let mut in_woods = 0u32;
    let mut done = None;
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        // The pursuers keep after the nearest, and the player sends on any
        // regiment that has stopped short (one that ran and rallied).
        let mut orders = Vec::new();
        if t % (2 * TICKS_PER_SECOND) == 0 {
            orders = attack_nearest(&world, 1, &theirs, 0);
            orders.extend(
                (ours.iter().enumerate())
                    .filter(|&(i, r)| {
                        out_at[i].is_none()
                            && world.regiments.iter().any(|v| v.handle == *r && !v.moving)
                    })
                    .map(|(i, &r)| move_to(0, r, rally(i))),
            );
        }
        report.tick(&mut world, &orders);
        let mut left = 0;
        for (i, &r) in ours.iter().enumerate() {
            let Some(v) = world.regiments.iter().find(|v| v.handle == r) else {
                continue;
            };
            let (x, y) = (v.position.x.floor_int(), v.position.y.floor_int());
            if out_at[i].is_none() && x < WOODS.start {
                out_at[i] = Some((t, v.models));
            }
            if out_at[i].is_none() {
                left += 1;
                in_woods += u32::from(arena.is_woods(x, y));
            }
        }
        if left == 0 {
            done = Some(t);
            break;
        }
    }
    let seconds = |t: u32| format!("{:.1} s", t as f32 / TICKS_PER_SECOND as f32);
    let got_out: Vec<_> = out_at.iter().flatten().collect();
    let saved: u32 = got_out.iter().map(|&&(_, n)| u32::from(n)).sum();
    let first_out = got_out.iter().map(|&&(t, _)| t).min();
    report.metric(
        "first regiment out",
        first_out.map_or("never".into(), seconds),
    );
    report.metric(
        "every regiment out or fallen",
        done.map_or("never".into(), seconds),
    );
    report.metric("regiments out", format!("{}/{}", got_out.len(), ours.len()));
    report.metric("soldiers out", format!("{saved}/{start}"));
    report.metric("regiment-ticks in the woods", in_woods);
    report.check(
        format!("every regiment is out of the woods or fallen within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
    report.check(
        "at least half the regiments get out",
        got_out.len() * 2 >= ours.len(),
    );
}
