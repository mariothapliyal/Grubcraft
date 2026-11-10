//! Twelve Ember regiments come up one behind another to the same rally
//! point beside a row of houses, as the regiments a base trains do, then
//! all march off to one spot, as an army sent out does.

use gc_fixed::{Fx, FxVec2};
use gc_sim::scenario::NORTH;
use gc_sim::{RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, centre, move_to, of, put_up, soldiers, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "rally-point",
    about: "12 regiments gather at one rally point in a base, then march off to one spot",
    budget_ms: 4_000,
    run,
};

const TILES: u32 = 128;
const REGIMENTS: usize = 12;
/// The rally point, the houses' row north of it and where the army goes.
const RALLY: (i32, i32) = (100, 64);
const HOUSES_Y: i32 = 56;
const AWAY: (i32, i32) = (100, 112);
const LIMIT_SECONDS: u32 = 60;
/// The most regiment-seconds in disorder, gathering and marching off.
const DISORDER: [u32; 2] = [200, 400];
/// Everyone must have stood, in good order, this long for it to be over.
const SETTLED: u32 = 5 * TICKS_PER_SECOND;
/// How far from the spot they were sent to the last of them may stand.
const SPREAD: i32 = 30;

/// Send all of `army` to `to` and tick until they have all stood in good
/// order for [`SETTLED`]: when (from the order), and the regiment-ticks
/// spent in disorder.
fn gather(
    world: &mut World,
    report: &mut Report,
    army: &[RegimentHandle],
    to: FxVec2,
) -> (Option<u32>, u32) {
    let orders: Vec<_> = army.iter().map(|&r| move_to(0, r, to)).collect();
    report.tick(world, &orders);
    let (mut disorder, mut quiet) = (0, 0);
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        report.tick(world, &[]);
        let out = of(world, army).filter(|v| v.disorder > 0).count() as u32;
        disorder += out;
        let near = |v: &gc_sim::RegimentView| (v.position - to).length() <= Fx::from_int(SPREAD);
        if out == 0 && of(world, army).all(|v| !v.moving && near(&v)) {
            quiet += 1;
            if quiet >= SETTLED {
                return (Some(t - SETTLED), disorder);
            }
        } else {
            quiet = 0;
        }
    }
    (None, disorder)
}

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 37, 0);
    let farmstead = real.building("Farmstead");
    let houses: Vec<_> = (RALLY.0 - 20..=RALLY.0 + 20)
        .step_by(5)
        .map(|x| (farmstead, (x, HOUSES_Y), 0))
        .collect();
    put_up(&mut world, 0, &houses);

    // In single file from the west, seven tiles apart.
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi];
    let placed: Vec<_> = (0..REGIMENTS)
        .map(|i| (kinds[i % 3], centre(RALLY.0 - 14 - 7 * i as i32, RALLY.1)))
        .collect();
    let army = spawn(&mut world, 0, &placed, NORTH);
    let total = soldiers(&world, &army);

    let per_second = |ticks: u32| format!("{:.1}", ticks as f32 / TICKS_PER_SECOND as f32);
    let (gathered, disorder) = gather(&mut world, report, &army, centre(RALLY.0, RALLY.1));
    report.metric(
        "all at the rally point, standing in good order",
        seconds(gathered),
    );
    report.metric(
        "regiment-seconds in disorder gathering",
        per_second(disorder),
    );
    let (arrived, marching) = gather(&mut world, report, &army, centre(AWAY.0, AWAY.1));
    report.metric(
        "all marched off and standing in good order",
        seconds(arrived),
    );
    report.metric(
        "regiment-seconds in disorder marching off",
        per_second(marching),
    );
    report.check(
        format!("all gather and stand in good order within {LIMIT_SECONDS} s"),
        gathered.is_some(),
    );
    report.check(
        format!("all march off and stand in good order within {LIMIT_SECONDS} s"),
        arrived.is_some(),
    );
    for (k, ticks) in [disorder, marching].into_iter().enumerate() {
        let what = ["gathering", "marching off"][k];
        report.check(
            format!("under {} regiment-seconds in disorder {what}", DISORDER[k]),
            ticks < DISORDER[k] * TICKS_PER_SECOND,
        );
    }
    report.check("no soldier lost", soldiers(&world, &army) == total);
}
