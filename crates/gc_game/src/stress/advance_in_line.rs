//! Advancing in line (docs/42-armies-that-march.md, stage 4): six
//! regiments in line ordered as one group to attack-move through six of
//! the enemy's standing in line: straight opposite, shifted 15 tiles
//! east, a line of four, a line with its middle pushed forward and a line
//! slanting away. Each should take the enemy opposite it (where one
//! stands straight opposite within reach) and none stand by while the
//! rest fight, so the line meets theirs as a line, not as a
//! crowd on whichever of them it saw first.

use gc_fixed::{Fx, FxVec2};
use gc_sim::{Command, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, SOUTH, centre, of, order, soldiers, spawn};
use super::{Report, Scenario, seconds};
use gc_sim::scenario::NORTH;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "advance-in-line",
    about: "6 in line attack-move as one group into lines of 6 opposite, shifted, bulging and slanting, and a line of 4",
    budget_ms: 6_000,
    run,
};

const TILES: u32 = 128;
/// How long each fight is given, in seconds.
const LIMIT: u32 = 60;
/// By when all six of ours must be fighting, in seconds.
const ALL_IN: u32 = 30;
/// Tiles between regiments in each line, file to file.
const STEP: i32 = 10;

/// The enemy's line: how many, how far east of straight opposite, and how
/// many tiles further back (away from us) each stands, by its place in
/// the line.
struct Theirs {
    count: usize,
    shift: i32,
    back: fn(usize) -> i32,
}

struct Clash {
    /// Of the enemy, how many our regiments took on first.
    taken: usize,
    /// How far, across the line, each of ours went from where it stood
    /// to the first foe it took, as a mean and the furthest, in tiles.
    mean_across: f32,
    most_across: f32,
    /// The most of ours that took on one foe first.
    crowded: usize,
    /// When every one of ours had come to blows.
    all_in: Option<u32>,
    /// Soldiers left, ours and theirs, at the limit.
    left: (u32, u32),
    /// Soldiers at the start, ours and theirs.
    start: (u32, u32),
}

fn clash(real: &Real, report: &mut Report, theirs: &Theirs) -> Clash {
    let mut world: World = Arena::new(TILES).world(real, 45, 0);
    let (yari, katana) = (real.unit("Yari Ashigaru"), real.unit("Katana Samurai"));
    let line = |n: usize, x0: i32, y: i32, back: fn(usize) -> i32| -> Vec<(u16, FxVec2)> {
        (0..n)
            .map(|i| {
                let unit = if i % 2 == 0 { yari } else { katana };
                (unit, centre(x0 + i as i32 * STEP, y - back(i)))
            })
            .collect()
    };
    let ours = spawn(&mut world, 0, &line(6, 64 - 5 * STEP / 2, 96, |_| 0), NORTH);
    let their_x0 = 64 - (theirs.count as i32 - 1) * STEP / 2 + theirs.shift;
    let foes = spawn(
        &mut world,
        1,
        &line(theirs.count, their_x0, 44, theirs.back),
        SOUTH,
    );
    let starts: Vec<FxVec2> = of(&world, &ours).map(|v| v.position).collect();
    let foe_starts: Vec<(RegimentHandle, FxVec2)> =
        of(&world, &foes).map(|v| (v.handle, v.position)).collect();
    let start = (soldiers(&world, &ours), soldiers(&world, &foes));
    // Each straight on, well past the enemy, as one group.
    let orders: Vec<_> = (ours.iter().zip(&starts))
        .map(|(&regiment, at)| {
            order(
                0,
                Command::AttackMove {
                    regiment,
                    target: FxVec2::new(at.x, Fx::from_int(16)),
                    facing: None,
                    files: 0,
                    pace: Fx::ZERO,
                    brigade: 1,
                },
            )
        })
        .collect();
    report.tick(&mut world, &orders);
    let mut first: Vec<Option<RegimentHandle>> = vec![None; ours.len()];
    // Whether each of ours has come to blows yet.
    let mut fought = vec![false; ours.len()];
    let mut all_in = None;
    for t in 1..=LIMIT * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        for (f, &h) in first.iter_mut().zip(&ours) {
            if f.is_none() {
                *f = world.regiments.foe_of(h);
            }
        }
        for v in of(&world, &ours) {
            let k = ours.iter().position(|&h| h == v.handle).unwrap();
            fought[k] |= v.engaged;
        }
        if all_in.is_none() && fought.iter().all(|&f| f) {
            all_in = Some(t);
        }
    }
    let gaps: Vec<f32> = (first.iter().zip(&starts))
        .filter_map(|(f, at)| {
            let (_, there) = foe_starts.iter().find(|(h, _)| Some(*h) == *f)?;
            Some(crate::fx_to_f32((at.x - there.x).abs()))
        })
        .collect();
    let on: Vec<usize> = (foe_starts.iter())
        .map(|(h, _)| first.iter().filter(|f| **f == Some(*h)).count())
        .collect();
    Clash {
        taken: on.iter().filter(|&&n| n > 0).count(),
        mean_across: gaps.iter().sum::<f32>() / gaps.len().max(1) as f32,
        most_across: gaps.iter().copied().fold(0.0, f32::max),
        crowded: on.iter().copied().max().unwrap_or(0),
        all_in,
        left: (soldiers(&world, &ours), soldiers(&world, &foes)),
        start,
    }
}

fn run(real: &Real, report: &mut Report) {
    let level: fn(usize) -> i32 = |_| 0;
    for (name, theirs, opposite) in [
        (
            "straight opposite",
            Theirs {
                count: 6,
                shift: 0,
                back: level,
            },
            true,
        ),
        (
            "shifted 15 tiles east",
            Theirs {
                count: 6,
                shift: 15,
                back: level,
            },
            false,
        ),
        (
            "a line of 4",
            Theirs {
                count: 4,
                shift: 0,
                back: level,
            },
            false,
        ),
        (
            "its middle two 8 tiles forward",
            Theirs {
                count: 6,
                shift: 0,
                back: |i| if i == 2 || i == 3 { -8 } else { 0 },
            },
            true,
        ),
        (
            "slanting back 3 tiles a regiment",
            Theirs {
                count: 6,
                shift: 0,
                back: |i| 3 * i as i32,
            },
            false,
        ),
    ] {
        let c = clash(real, report, &theirs);
        report.metric(
            format!("{name}: enemy taken on first, of {}", theirs.count),
            c.taken,
        );
        report.metric(
            format!("{name}: tiles across to the first foe, mean / most"),
            format!("{:.1} / {:.1}", c.mean_across, c.most_across),
        );
        report.metric(format!("{name}: most on one foe"), c.crowded);
        report.metric(format!("{name}: all six fighting"), seconds(c.all_in));
        report.metric(
            format!("{name}: soldiers left after {LIMIT} s, ours / theirs"),
            format!(
                "{} of {} / {} of {}",
                c.left.0, c.start.0, c.left.1, c.start.1
            ),
        );
        report.check(
            format!("{name}: all six of ours fighting within {ALL_IN} s"),
            c.all_in.is_some_and(|t| t <= ALL_IN * TICKS_PER_SECOND),
        );
        if opposite {
            report.check(
                format!("{name}: each of ours takes the enemy straight opposite it first"),
                c.taken == theirs.count && c.most_across < 1.0,
            );
        }
    }
}
