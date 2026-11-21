//! A group order marches as a brigade (docs/42-armies-that-march.md):
//! eight regiments over a ford, round the corner of a thicket and through
//! a Gate, and twelve and twenty-four through a Gate. Each march is played twice on
//! the same ground, as separate moves and as one brigade, and the brigade
//! is held to what it is for: everyone arrives, no later than apart (but
//! for a margin), closer together and in less disorder.
//!
//! Two of them a brigade cannot yet march better than its regiments do
//! apart, so it lets go and they march as they did before there were
//! brigades (docs/42 has both under stage 3): a way that doubles back
//! round a corner (the body would turn its whole order about on the way:
//! 74 s against 58 s apart when it tried), and a big army in wide rows
//! at one Gate (its rows are long coming in to the file: 85 s against
//! 51 s). Those scenarios hold it to marching about as its regiments do
//! apart.

use gc_fixed::FxVec2;
use gc_sim::{TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, WEST, gated_wall, grid, march_as, spawn};
use super::{Report, Scenario, seconds};

pub(super) const FORD: Scenario = Scenario {
    name: "brigade-ford",
    about: "8 regiments ordered as one group wade a narrow ford",
    budget_ms: 6_000,
    run: ford,
};

pub(super) const CORNER: Scenario = Scenario {
    name: "brigade-corner",
    about: "8 regiments ordered as one group march back round the corner of a thicket",
    budget_ms: 6_000,
    run: corner,
};

pub(super) const GATE: Scenario = Scenario {
    name: "brigade-gate",
    about: "8 regiments ordered as one group march through one Gate",
    budget_ms: 8_000,
    run: |real, report| gate(real, report, 8, 4, 96, BETTER),
};

pub(super) const COLUMN_GATE: Scenario = Scenario {
    name: "brigade-column-gate",
    about: "12 regiments ordered as one group, two abreast, march through one Gate",
    budget_ms: 8_000,
    run: |real, report| gate(real, report, 12, 2, 80, BETTER),
};

pub(super) const BIG_GATE: Scenario = Scenario {
    name: "brigade-big-gate",
    about: "24 regiments ordered as one group, eight abreast, march through one Gate",
    budget_ms: 12_000,
    run: |real, report| gate(real, report, 24, 8, 96, AS_APART),
};

const TILES: u32 = 128;
const LIMIT: u32 = 200;
/// What a brigade is held to against the same march apart.
struct Limits {
    /// How much later its last regiment may arrive, percent of the time
    /// apart.
    later: u32,
    /// Its time from the first regiment arriving to the last, and its
    /// regiment-seconds in disorder, percent of those apart.
    spread: u32,
    disorder: u32,
}

const BETTER: Limits = Limits {
    later: 15,
    spread: 100,
    disorder: 100,
};
/// Where a brigade lets go (see the module's notes).
const AS_APART: Limits = Limits {
    later: 10,
    spread: 125,
    disorder: 125,
};

/// Play the march of `placed` (unit, where) to `goals` on the ground
/// `build` makes, apart and then as a brigade, and hold the brigade to
/// the march apart.
fn compare(
    real: &Real,
    report: &mut Report,
    build: impl Fn() -> World,
    placed: &[(u16, FxVec2)],
    goals: &[FxVec2],
    limits: Limits,
) {
    let mut runs = Vec::new();
    for (name, number) in [("apart", 0), ("as a brigade", 1)] {
        let mut world = build();
        let army = spawn(&mut world, 0, placed, WEST);
        let run = march_as(
            real,
            &mut world,
            report,
            &army,
            goals,
            number,
            LIMIT,
            |_, _| {},
        );
        report.metric(format!("{name}: last regiment arrives"), seconds(run.done));
        let spread = run.done.zip(run.first).map(|(last, first)| last - first);
        report.metric(format!("{name}: first to last"), seconds(spread));
        report.metric(
            format!("{name}: regiment-seconds in disorder"),
            format!("{:.1}", run.disorder as f32 / TICKS_PER_SECOND as f32),
        );
        report.check(
            format!("{name}: every regiment arrives within {LIMIT} s"),
            run.done.is_some(),
        );
        report.check(
            format!("{name}: no soldier lost"),
            run.soldiers.1 == run.soldiers.0,
        );
        runs.push((run, spread));
    }
    let [(apart, wide), (together, close)] = &runs[..] else {
        unreachable!("two runs");
    };
    if let (Some(a), Some(b)) = (apart.done, together.done) {
        report.check(
            format!(
                "the brigade's last arrives no more than {}% later than apart",
                limits.later
            ),
            b * 100 <= a * (100 + limits.later),
        );
    }
    let within = |what: &str, percent: u32| {
        if percent <= 100 {
            format!("the brigade {what} than apart")
        } else {
            format!("the brigade {what} than apart, within {}%", percent - 100)
        }
    };
    report.check(
        within("arrives no further apart", limits.spread),
        (close.zip(*wide)).is_some_and(|(close, wide)| close * 100 <= wide * limits.spread),
    );
    report.check(
        within("marches in no more disorder", limits.disorder),
        together.disorder * 100 <= apart.disorder * limits.disorder,
    );
}

/// Where `count` regiments stand `abreast` to a row, the rows going
/// south from `at`, the middle of the front one.
fn places(count: usize, abreast: usize, at: (i32, i32)) -> Vec<FxVec2> {
    let west = at.0 - 5 * (abreast as i32 - 1);
    (0..count)
        .map(|i| grid((west, at.1), abreast, (10, 8), i))
        .collect()
}

/// `count` regiments of the Ember line standing so.
fn army(real: &Real, count: usize, abreast: usize, at: (i32, i32)) -> Vec<(u16, FxVec2)> {
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari, katana, yari, yumi, katana];
    (places(count, abreast, at).into_iter().enumerate())
        .map(|(i, at)| (kinds[i % kinds.len()], at))
        .collect()
}

fn ford(real: &Real, report: &mut Report) {
    // A river across the map with one ford, the army south of it and its
    // goal north.
    let build = || {
        let mut arena = Arena::new(TILES);
        arena.river(0..TILES as i32, 60..64);
        arena.shelving_ford(60..68, 60..64, 3);
        // (The ford's shoulders are deep water again.)
        arena.river(0..60, 60..64);
        arena.river(68..TILES as i32, 60..64);
        arena.world(real, 41, 0)
    };
    compare(
        real,
        report,
        build,
        &army(real, 8, 4, (64, 96)),
        &places(8, 4, (64, 24)),
        BETTER,
    );
}

fn corner(real: &Real, report: &mut Report) {
    // A thicket from the west edge two thirds across: from its south
    // side to its north the way is round its east corner.
    let build = || {
        let mut arena = Arena::new(TILES);
        arena.thicket(0..84, 48..76);
        arena.world(real, 42, 0)
    };
    compare(
        real,
        report,
        build,
        &army(real, 8, 4, (40, 100)),
        &places(8, 4, (40, 20)),
        AS_APART,
    );
}

/// `count` regiments, `abreast` to a row, their front row at `south`,
/// through one Gate to the north of the map.
fn gate(
    real: &Real,
    report: &mut Report,
    count: usize,
    abreast: usize,
    south: i32,
    limits: Limits,
) {
    const WALL_ROW: i32 = 60;
    const GATE_X: i32 = 62;
    let build = || {
        let mut world = Arena::new(TILES).world(real, 43, 0);
        gated_wall(real, &mut world, TILES, WALL_ROW, GATE_X);
        world
    };
    compare(
        real,
        report,
        build,
        &army(real, count, abreast, (64, south)),
        &places(count, abreast, (64, 8)),
        limits,
    );
}
