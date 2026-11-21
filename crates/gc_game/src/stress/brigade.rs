//! A group order marches as a brigade (docs/42-armies-that-march.md):
//! eight regiments over a ford, round the corner of a thicket and through
//! a Gate. Each march is played twice on the same ground, as separate
//! moves and as one brigade, and the brigade is held to what it is for:
//! everyone arrives, no later than apart (but for a margin), closer
//! together and in less disorder.
//!
//! Round a sharp corner a brigade is not there yet: the body turns its
//! whole order about on the way and jams at the corner, so it is later
//! than apart and in more disorder (74 s against 58 s when this was
//! written). That scenario holds what it does today so it gets no worse;
//! docs/42 has corners under stage 3.

use gc_fixed::{Fx, FxVec2};
use gc_sim::{Command, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, WEST, grid, of, put_up, soldiers, spawn};
use super::{Report, Scenario, seconds};

pub(super) const FORD: Scenario = Scenario {
    name: "brigade-ford",
    about: "8 regiments ordered as one group wade a narrow ford",
    budget_ms: 6_000,
    run: ford,
};

pub(super) const CORNER: Scenario = Scenario {
    name: "brigade-corner",
    about: "8 regiments ordered as one group march round the corner of a thicket",
    budget_ms: 6_000,
    run: corner,
};

pub(super) const GATE: Scenario = Scenario {
    name: "brigade-gate",
    about: "8 regiments ordered as one group march through one Gate",
    budget_ms: 8_000,
    run: gate,
};

const TILES: u32 = 128;
const LIMIT: u32 = 200;
/// How near its goal a regiment must stand to have arrived.
const NEAR: Fx = Fx::from_int(3);
/// What a brigade is held to against the same march apart.
struct Limits {
    /// How much later its last regiment may arrive, percent of the time
    /// apart.
    later: u32,
    /// Its regiment-seconds in disorder, percent of those apart.
    disorder: u32,
}

const BETTER: Limits = Limits {
    later: 15,
    disorder: 100,
};
/// Round a sharp corner (see the module's notes).
const CORNER_TODAY: Limits = Limits {
    later: 30,
    disorder: 200,
};

/// What one march measured.
struct Run {
    /// Ticks until the first and the last regiment stood at its goal.
    first: Option<u32>,
    last: Option<u32>,
    /// Regiment-ticks in disorder (nobody fights, so all from crowding).
    disorder: u32,
    lost: bool,
}

/// Order `army` to `goals` as brigade `number` (0: separate moves) and
/// tick until each stands at its goal.
fn play(
    world: &mut World,
    report: &mut Report,
    army: &[RegimentHandle],
    goals: &[FxVec2],
    number: u16,
) -> Run {
    let orders: Vec<_> = (army.iter().zip(goals))
        .map(|(&regiment, &target)| PlayerCommand {
            player: 0,
            command: Command::MoveTo {
                regiment,
                target,
                facing: None,
                files: 0,
                pace: Fx::ZERO,
                brigade: number,
            },
        })
        .collect();
    let start = soldiers(world, army);
    report.tick(world, &orders);
    let mut arrived = vec![None; army.len()];
    let mut disorder = 0;
    for t in 1..=LIMIT * TICKS_PER_SECOND {
        report.tick(world, &[]);
        for v in of(world, army) {
            let i = army.iter().position(|&r| r == v.handle).unwrap();
            disorder += u32::from(v.disorder > 0);
            let there = !v.moving && (goals[i] - v.position).length() <= NEAR;
            arrived[i] = if there { arrived[i].or(Some(t)) } else { None };
        }
        if arrived.iter().all(Option::is_some) {
            break;
        }
    }
    let all = arrived.iter().all(Option::is_some);
    Run {
        first: arrived.iter().flatten().min().copied().filter(|_| all),
        last: arrived.iter().flatten().max().copied().filter(|_| all),
        disorder,
        lost: soldiers(world, army) != start,
    }
}

/// Play the march of `placed` (unit, where) to `goals` on the ground
/// `build` makes, apart and then as a brigade, and hold the brigade to
/// the march apart.
fn compare(
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
        let run = play(&mut world, report, &army, goals, number);
        report.metric(format!("{name}: last regiment arrives"), seconds(run.last));
        let spread = run.last.zip(run.first).map(|(last, first)| last - first);
        report.metric(format!("{name}: first to last"), seconds(spread));
        report.metric(
            format!("{name}: regiment-seconds in disorder"),
            format!("{:.1}", run.disorder as f32 / TICKS_PER_SECOND as f32),
        );
        report.check(
            format!("{name}: every regiment arrives within {LIMIT} s"),
            run.last.is_some(),
        );
        report.check(format!("{name}: no soldier lost"), !run.lost);
        runs.push((run, spread));
    }
    let [(apart, wide), (together, close)] = &runs[..] else {
        unreachable!("two runs");
    };
    if let (Some(a), Some(b)) = (apart.last, together.last) {
        report.check(
            format!(
                "the brigade's last arrives no more than {}% later than apart",
                limits.later
            ),
            b * 100 <= a * (100 + limits.later),
        );
    }
    report.check(
        "the brigade arrives closer together than apart",
        close.zip(*wide).is_some_and(|(close, wide)| close <= wide),
    );
    report.check(
        if limits.disorder <= 100 {
            "the brigade marches in no more disorder than apart".to_string()
        } else {
            format!(
                "the brigade marches in no more than {}% of the disorder apart",
                limits.disorder
            )
        },
        together.disorder * 100 <= apart.disorder * limits.disorder,
    );
}

/// Eight regiments, four abreast and two deep, about `at`.
fn eight(real: &Real, at: (i32, i32)) -> Vec<(u16, FxVec2)> {
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari, katana, yari, yumi, katana];
    (0..8)
        .map(|i| (kinds[i], grid((at.0 - 15, at.1), 4, (10, 8), i)))
        .collect()
}

fn goals(at: (i32, i32)) -> Vec<FxVec2> {
    (0..8)
        .map(|i| grid((at.0 - 15, at.1), 4, (10, 8), i))
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
        report,
        build,
        &eight(real, (64, 96)),
        &goals((64, 24)),
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
        report,
        build,
        &eight(real, (40, 100)),
        &goals((40, 20)),
        CORNER_TODAY,
    );
}

fn gate(real: &Real, report: &mut Report) {
    const WALL_ROW: i32 = 60;
    const GATE_X: i32 = 62;
    let build = || {
        let mut world = Arena::new(TILES).world(real, 43, 0);
        let (palisade, gate) = (real.building("Palisade"), real.building("Gate"));
        let stats = &real.table.stats[usize::from(gate)];
        let span = GATE_X..GATE_X + i32::from(stats.width.max(stats.depth));
        let mut wall: Vec<_> = (0..TILES as i32)
            .filter(|x| !span.contains(x))
            .map(|x| (palisade, (x, WALL_ROW), 0))
            .collect();
        wall.push((gate, (GATE_X, WALL_ROW), 1));
        put_up(&mut world, 0, &wall);
        world
    };
    compare(
        report,
        build,
        &eight(real, (64, 96)),
        &goals((64, 24)),
        BETTER,
    );
}
