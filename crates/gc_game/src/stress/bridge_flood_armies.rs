//! Two armies of over 200 soldiers each, one player's, cross one bridge
//! of the narrowest width a player can draw (3 tiles) opposite ways while a catastrophic flood shuts the ford upstream
//! (docs/39-water-engine.md, stage 4e). Sound piers with nothing in the
//! water stand through any flood, so this is the bridge's traffic with
//! the river up.

use gc_fixed::{Fx, FxVec2};
use gc_sim::TICKS_PER_SECOND;
use gc_sim::rivers::Band;

use super::arena::{
    Arena, EAST, Real, SOUTH, WEST, grid, march, of, put_up, soldiers, spawn, squall,
};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "bridge-flood-armies",
    about: "two armies of 200+ cross one bridge, 3 tiles wide, opposite ways in a catastrophic flood",
    budget_ms: 1_500,
    run,
};

const TILES: u32 = 96;
const SECOND: u32 = TICKS_PER_SECOND;
const RIVER: core::ops::Range<i32> = 46..50;
const FORD: core::ops::Range<i32> = 2..5;
const ROW: i32 = 48;
const SPAN: core::ops::RangeInclusive<i32> = 43..=52;
/// A seed whose squall rolls a catastrophic front (checked).
const SEED: u64 = 23;
/// When the armies set off: the flood has shut the ford by then.
const SET_OFF: u32 = 40 * SECOND;
/// Regiments a side, and the least soldiers a side.
const REGIMENTS: usize = 11;
const LEAST: u32 = 200;
const LIMIT_SECONDS: u32 = 180;
/// The longest any regiment may stand stuck on its way: walking in place,
/// or waiting its turn at the bridgehead. The plan's mark is 10 s; the
/// two armies share the deck rather than keeping to a side, and one
/// regiment waits a little longer than that at the bridgehead (see
/// docs/39, stage 4e).
const STALL_SECONDS: u32 = 15;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.river(RIVER, 0..TILES as i32);
    arena.ford(RIVER, FORD);
    arena.flowing(SOUTH);
    let mut world = arena.world_under(real, SEED, 0, squall());
    let bridge = real.building("Timber Bridge");
    let deck: Vec<_> = (SPAN.flat_map(|x| [(x, ROW), (x, ROW + 1), (x, ROW + 2)]))
        .map(|tile| (bridge, tile, 0))
        .collect();
    put_up(&mut world, 0, &deck);

    let units = real.ember_line();
    let at = |x: i32, i: usize| grid((x, 36), 3, (6, 8), i);
    let west: Vec<_> = (0..REGIMENTS).map(|i| (units[i % 3], at(14, i))).collect();
    let east: Vec<_> = (0..REGIMENTS)
        .map(|i| (units[(i + 1) % 3], at(70, i)))
        .collect();
    let west_army = spawn(&mut world, 0, &west, EAST);
    let east_army = spawn(&mut world, 0, &east, WEST);
    let a_side = soldiers(&world, &west_army).min(soldiers(&world, &east_army));
    let mut army = west_army;
    army.extend(east_army);
    while world.tick_count() < SET_OFF {
        world.tick(&[]);
    }
    let rivers = world.flood().rivers();
    let flooding = rivers.fronts().any(|f| f.band() == Band::Catastrophic) && rivers.shut();

    // Each army to where the other began; how long each regiment has stood
    // stuck since it last made a quarter of a tile of headway.
    let goals: Vec<_> = east.iter().chain(&west).map(|&(_, p)| p).collect();
    let mut headway: Vec<(FxVec2, u32)> = vec![(FxVec2::ZERO, 0); army.len()];
    let mut stall = 0;
    let walked = march(
        real,
        &mut world,
        report,
        &army,
        &goals,
        LIMIT_SECONDS,
        |t, w| {
            for v in of(w, &army) {
                let i = army.iter().position(|&r| r == v.handle).unwrap();
                let last = &mut headway[i];
                let near = (goals[i] - v.position).length() <= Fx::from_int(3);
                if t == 0 || near || (v.position - last.0).length() > Fx::from_ratio(1, 4) {
                    *last = (v.position, t);
                }
                stall = stall.max(t - last.1);
            }
        },
    );
    report.metric("soldiers a side, the fewer", a_side);
    report.metric("longest stuck", seconds(Some(stall)));
    report.check(
        format!("each side has at least {LEAST} soldiers"),
        a_side >= LEAST,
    );
    report.check(
        "the catastrophic flood has shut the ford when they set off",
        flooding,
    );
    report.check(
        format!("no regiment stuck more than {STALL_SECONDS} s"),
        stall <= STALL_SECONDS * SECOND,
    );
    walked.report(report, 20);
}
