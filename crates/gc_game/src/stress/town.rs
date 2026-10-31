//! Eight Ember regiments march north through a town of Farmsteads built
//! right across the map, its streets two tiles wide and staggered row by
//! row, so the way through winds.

use gc_sim::scenario::NORTH;

use super::arena::{Arena, Real, grid, march, of, put_up, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "town-gaps",
    about: "8 regiments march through a town's narrow, winding streets",
    budget_ms: 4_000,
    run,
};

const TILES: u32 = 96;
/// The town's first row of houses, and how many rows.
const TOWN_Y: i32 = 38;
const ROWS: i32 = 4;
/// A house is 3 tiles; each street and lane 2.
const HOUSE: i32 = 3;
const STREET: i32 = 2;
const LIMIT_SECONDS: u32 = 40;

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 23, 0);
    let farmstead = real.building("Farmstead");
    let step = HOUSE + STREET;
    let mut town = Vec::new();
    for row in 0..ROWS {
        let y = TOWN_Y + row * step;
        // Every other row shifted along, so no street runs straight
        // through.
        let shift = if row % 2 == 0 { 0 } else { STREET + 1 };
        town.extend(
            (shift..=TILES as i32 - HOUSE)
                .step_by(step as usize)
                .map(|x| (farmstead, (x, y), 0)),
        );
    }
    // The army's own town: a test of the streets, not of a fight.
    let houses = put_up(&mut world, 0, &town);
    report.metric("houses", houses.len());

    let [yari, katana, yumi] = real.ember_line();
    let units = [yari, katana, yumi, yari, katana, yari, yumi, katana];
    let at = |y: i32, i: usize| grid((30, y), 4, (10, 7), i);
    let placed: Vec<_> = (0..8).map(|i| (units[i], at(70, i))).collect();
    let army = spawn(&mut world, 0, &placed, NORTH);
    let goals: Vec<_> = (0..8).map(|i| at(16, i)).collect();

    let mut first = None;
    let walked = march(
        real,
        &mut world,
        report,
        &army,
        &goals,
        LIMIT_SECONDS,
        |t, w| {
            if of(w, &army).any(|v| v.position.y.floor_int() < TOWN_Y) {
                first.get_or_insert(t);
            }
        },
    );
    report.metric("first regiment through", seconds(first));
    walked.report(report, 50);
}
