//! Two armies of the same player, six regiments each, march head on
//! through each other along the same rows, each to where the other began.

use super::arena::{Arena, EAST, Real, WEST, grid, march, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "armies-crossing",
    about: "two armies of 6 regiments swap places head on along the same rows",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 96;
const LIMIT_SECONDS: u32 = 30;

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 19, 0);
    let [yari, katana, yumi] = real.ember_line();
    let units = [yari, katana, yumi, yari, katana, yari];
    // Three abreast, two deep: west at x 16 and 22, east at 70 and 76.
    let at = |x: i32, i: usize| grid((x, 36), 2, (6, 10), i);
    let west: Vec<_> = (0..6).map(|i| (units[i], at(16, i))).collect();
    let east: Vec<_> = (0..6).map(|i| (units[5 - i], at(70, i))).collect();
    let mut army = spawn(&mut world, 0, &west, EAST);
    army.extend(spawn(&mut world, 0, &east, WEST));
    // Each army to where the other began.
    let goals: Vec<_> = east.iter().chain(&west).map(|&(_, p)| p).collect();
    let walked = march(
        real,
        &mut world,
        report,
        &army,
        &goals,
        LIMIT_SECONDS,
        |_, _| {},
    );
    walked.report(report, 70);
}
