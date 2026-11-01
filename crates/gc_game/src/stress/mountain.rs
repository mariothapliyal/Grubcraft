//! Eight regiments are ordered onto the steep side of a hill: none may
//! end up standing strung out on the slope. Each stands on the nearest
//! ground that isn't steep (the hill's flat top or the plain at its foot).

use gc_fixed::FxVec2;
use gc_sim::nav::STEEP;
use gc_sim::scenario::NORTH;

use super::arena::{Arena, Real, centre, grid, march, of, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "mountain",
    about: "8 regiments ordered onto a hill's steep side stand on flat ground",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 96;
/// The hill: its middle, flat top, height and the tiles its sides fall over.
const HILL: (i32, i32) = (48, 40);
const TOP: i32 = 6;
const HEIGHT: i32 = 4;
const SIDE: i32 = 8;
const LIMIT_SECONDS: u32 = 40;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.hill(HILL, TOP, HEIGHT, SIDE);
    let mut world = arena.world(real, 41, 0);
    let [yari, katana, yumi] = real.ember_line();
    let units = [yari, katana, yumi, yari, katana, yari, yumi, katana];
    let placed: Vec<_> = (0..8)
        .map(|i| (units[i], grid((20, 76), 4, (14, 8), i)))
        .collect();
    let army = spawn(&mut world, 0, &placed, NORTH);
    // Each to a spot on steep ground on the hill's south side, spread
    // along it.
    let cost = arena.costs();
    let steep: Vec<FxVec2> = (HILL.1 + TOP..HILL.1 + TOP + SIDE)
        .flat_map(|y| (HILL.0 - 12..HILL.0 + 12).map(move |x| centre(x, y)))
        .filter(|&p| cost(p) == STEEP)
        .collect();
    report.check("the hill has a steep side", steep.len() >= 8);
    if steep.len() < 8 {
        return;
    }
    let on_slope: Vec<FxVec2> = (0..8).map(|i| steep[i * steep.len() / 8]).collect();
    // The sim moves each order off the slope, so none reaches the spot
    // it was sent to and the march runs its full time: what counts is
    // where each stands at the end.
    let mut stood = Vec::new();
    march(
        real,
        &mut world,
        report,
        &army,
        &on_slope,
        LIMIT_SECONDS,
        |_, w| {
            stood = of(w, &army).map(|v| (v.position, v.moving)).collect();
        },
    );
    let still = stood.iter().filter(|(_, moving)| !moving).count();
    let on_steep = stood.iter().filter(|(p, _)| cost(*p) == STEEP).count();
    report.metric(
        "standing on steep ground at the end",
        format!("{on_steep}/8"),
    );
    report.check("every regiment stops", still == 8);
    report.check("no regiment stands on the steep slope", on_steep == 0);
}
