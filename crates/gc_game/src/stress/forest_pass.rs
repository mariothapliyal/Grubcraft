//! Twelve Ember regiments march east past a dense forest, woods packed
//! with thicket, that lies right across the straight way, with open
//! fields north and south of it (Mario's complaint: an army routed
//! through a forest as the shortest way, clumped up and crawled).

use super::arena::{Arena, EAST, Real, grid, march, of, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "forest-pass",
    about: "12 regiments march past a dense forest lying across the way",
    budget_ms: 4_000,
    run,
};

const TILES: u32 = 128;
/// The forest's columns and rows.
const FOREST_X: core::ops::Range<i32> = 44..84;
const FOREST_Y: core::ops::Range<i32> = 36..92;
const LIMIT_SECONDS: u32 = 65;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.woods(FOREST_X, FOREST_Y);
    // Thicket clumps every few tiles, staggered row by row: a forest that
    // only lets a regiment through by winding, single file.
    for (row, y) in (FOREST_Y.start + 2..FOREST_Y.end - 4)
        .step_by(7)
        .enumerate()
    {
        let shift = if row % 2 == 0 { 0 } else { 4 };
        for x in (FOREST_X.start + 2 + shift..FOREST_X.end - 4).step_by(8) {
            arena.thicket(x..x + 4, y..y + 4);
        }
    }
    let mut world = arena.world(real, 17, 0);
    let [yari, katana, yumi] = real.ember_line();
    let units = [yari, katana, yumi, yari, katana, yumi];
    // Four abreast, three deep, across the forest's middle rows.
    let at = |x: i32, i: usize| grid((x, 50), 3, (6, 9), i);
    let placed: Vec<_> = (0..12).map(|i| (units[i % 6], at(14, i))).collect();
    let army = spawn(&mut world, 0, &placed, EAST);
    let goals: Vec<_> = (0..12).map(|i| at(100, i)).collect();

    let (mut in_woods, mut ticks) = (0u32, 0u32);
    let walked = march(
        real,
        &mut world,
        report,
        &army,
        &goals,
        LIMIT_SECONDS,
        |t, w| {
            ticks = t + 1;
            for v in of(w, &army) {
                let (x, y) = (v.position.x.floor_int(), v.position.y.floor_int());
                in_woods += u32::from(arena.is_woods(x, y));
            }
        },
    );
    walked.report(report, 50);
    let share = in_woods * 100 / (ticks * army.len() as u32).max(1);
    report.metric("regiment-ticks in the forest, percent", share);
    report.check(
        "regiments spend under a tenth of the march in the forest",
        share < 10,
    );
}
