//! Making way (docs/42-armies-that-march.md, stage 2): eight regiments
//! ordered as one group march north straight through an army of twelve
//! standing at ease in two lines. The army steps aside out of their lanes,
//! facing as it faces, and back again once they are by, so neither falls
//! out of order pushing through the other.

use gc_fixed::{Fx, FxVec2};
use gc_sim::TICKS_PER_SECOND;
use gc_sim::scenario::NORTH;

use super::arena::{Arena, Real, grid, march_as, of, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "make-way",
    about: "8 regiments ordered as one group march through 12 standing at ease",
    budget_ms: 6_000,
    run,
};

const TILES: u32 = 128;
const LIMIT: u32 = 120;
/// How long the army at ease is given, once the march is in, to stand
/// where it stood again, in seconds.
const SETTLE: u32 = 15;
/// Regiment-seconds the army at ease may spend in disorder: none making
/// way, 77 when it stood its ground and was pushed through.
const DISORDER: u32 = 10;

fn run(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world(real, 44, 0);
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari, katana, yari];
    // The army at ease: two lines of six, ten tiles apart file to file.
    let standing: Vec<(u16, FxVec2)> = (0..12)
        .map(|i| (kinds[i % kinds.len()], grid((39, 56), 6, (10, 8), i)))
        .collect();
    let standing = spawn(&mut world, 0, &standing, NORTH);
    let homes: Vec<FxVec2> = of(&world, &standing).map(|v| v.position).collect();
    // The march: two columns of four, from south of the army to north of it.
    let column =
        |at: (i32, i32)| -> Vec<FxVec2> { (0..8).map(|i| grid(at, 2, (12, 9), i)).collect() };
    let marchers: Vec<(u16, FxVec2)> = (column((58, 92)).into_iter().enumerate())
        .map(|(i, at)| (kinds[i % kinds.len()], at))
        .collect();
    let marchers = spawn(&mut world, 0, &marchers, NORTH);
    let (mut aside, mut standers_disorder) = (Fx::ZERO, 0u32);
    let run = march_as(
        real,
        &mut world,
        report,
        &marchers,
        &column((58, 8)),
        1,
        LIMIT,
        |_, world| {
            for (v, home) in of(world, &standing).zip(&homes) {
                aside = aside.max((v.position - *home).length());
                standers_disorder += u32::from(v.disorder > 0);
            }
        },
    );
    // Then the army at ease steps back.
    let mut back = None;
    for t in 0..SETTLE * TICKS_PER_SECOND {
        let all_back = of(&world, &standing)
            .zip(&homes)
            .all(|(v, home)| v.made_way.is_none() && (v.position - *home).length() <= Fx::HALF);
        if all_back {
            back = Some(t);
            break;
        }
        report.tick(&mut world, &[]);
        standers_disorder += of(&world, &standing).filter(|v| v.disorder > 0).count() as u32;
    }
    let per_second = |ticks: u32| format!("{:.1}", ticks as f32 / TICKS_PER_SECOND as f32);
    report.metric("march: last regiment arrives", seconds(run.done));
    report.metric(
        "march: regiment-seconds in disorder",
        per_second(run.disorder),
    );
    report.metric(
        "army at ease: regiment-seconds in disorder",
        per_second(standers_disorder),
    );
    report.metric(
        "army at ease: furthest stepped aside, tiles",
        format!("{:.1}", crate::fx_to_f32(aside)),
    );
    report.metric("army at ease: back in place after the march", seconds(back));
    report.check(
        format!("every marcher arrives within {LIMIT} s"),
        run.done.is_some(),
    );
    report.check("no soldier lost", run.soldiers.1 == run.soldiers.0);
    report.check(
        format!("the army at ease stands where it stood again within {SETTLE} s"),
        back.is_some(),
    );
    report.check("the army at ease stepped aside", aside > Fx::ONE);
    report.check(
        format!("the army at ease is in disorder under {DISORDER} regiment-seconds"),
        standers_disorder < DISORDER * TICKS_PER_SECOND,
    );
}
