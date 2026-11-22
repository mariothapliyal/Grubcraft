//! Water that runs downhill (docs/27-living-land.md, stage 5): six
//! regiments on a plateau cross a highland river, whose water stands at its
//! own level four and a half tiles over the map's line. Its deep reach is
//! blocked to the march, so they go round by its ford, and wade it: the
//! simulation, the routes and the computer players' view of the ground all
//! read the river's own level.

use gc_sim::nav::{BLOCKED, FORD};
use gc_sim::scenario::NORTH;

use super::arena::{Arena, Real, centre, grid, march, of, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "high-water",
    about: "6 regiments cross a highland river by its ford, wading water at its own level",
    budget_ms: 4_000,
    run,
};

const TILES: u32 = 96;
/// The plateau, six and a quarter tiles up (sixteenths of a tile, as a
/// map file has them): a quarter tile over the river, so the step down
/// into its ford is steep going, not a cliff.
const PLATEAU: i32 = 100;
/// The river across it: its water six tiles up (the map's line is a tile
/// and a half), deep but for a ford a quarter of a tile deep.
const RIVER: (core::ops::Range<i32>, core::ops::Range<i32>) = (0..96, 46..52);
const LEVEL: i32 = 96;
const DEEP_BED: i32 = 70;
const FORD_XS: core::ops::Range<i32> = 44..52;
const FORD_BED: i32 = 92;
const LIMIT_SECONDS: u32 = 60;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.plateau(6..90, 16..80, PLATEAU, 6);
    arena.high_water(RIVER.0, RIVER.1, DEEP_BED, LEVEL);
    arena.high_water(FORD_XS, RIVER.1, FORD_BED, LEVEL);
    let mut world = arena.world(real, 53, 0);
    let cost = arena.costs();
    let (deep, ford, bank) = (centre(24, 48), centre(47, 48), centre(24, 58));
    {
        let t = world.terrain();
        report.check(
            "the river is water to the simulation",
            t.wet(deep) && t.wet(ford),
        );
        report.check("its banks are dry", !t.wet(bank));
    }
    report.check(
        "its deep reach is blocked to the march",
        cost(deep) == BLOCKED,
    );
    report.check("its ford is wading ground", cost(ford) == FORD);
    let ai = arena.ai_terrain();
    report.check(
        "the computer players see the river as water",
        ai.water(glam::Vec2::new(24.5, 48.5)) && !ai.water(glam::Vec2::new(24.5, 58.5)),
    );
    // Three abreast, two deep, south of the river, spread wide of the ford;
    // each to the same place north of it.
    let [yari, katana, yumi] = real.ember_line();
    let units = [yari, katana, yumi, yari, katana, yumi];
    let start: Vec<_> = (0..6)
        .map(|i| (units[i], grid((26, 62), 3, (22, 6), i)))
        .collect();
    let army = spawn(&mut world, 0, &start, NORTH);
    let goals: Vec<_> = (0..6).map(|i| grid((26, 30), 3, (22, 6), i)).collect();
    let mut waded = vec![false; army.len()];
    let mut in_deep = 0u32;
    let walked = march(
        real,
        &mut world,
        report,
        &army,
        &goals,
        LIMIT_SECONDS,
        |_, w| {
            for v in of(w, &army) {
                let i = army.iter().position(|&r| r == v.handle).unwrap();
                waded[i] |= w.terrain().wet(v.position);
                in_deep += u32::from(cost(v.position) == BLOCKED);
            }
        },
    );
    walked.report(report, 25);
    let wading = waded.iter().filter(|&&w| w).count();
    report.metric("regiments that waded the ford", format!("{wading}/6"));
    report.check("every regiment waded the highland ford", wading == 6);
    report.check("no regiment stood in the deep reach", in_deep == 0);
}
