//! Two armies of over 200 soldiers each, one player's, cross one bridge
//! of the narrowest width a player can draw (3 tiles) opposite ways while a catastrophic flood shuts the ford upstream
//! (docs/39-water-engine.md, stage 4e). Sound piers with nothing in the
//! water stand through any flood, so this is the bridge's traffic with
//! the river up.

use gc_sim::TICKS_PER_SECOND;
use gc_sim::rivers::Band;

use super::arena::{
    Arena, CATASTROPHIC_SEED, EAST, Real, SOUTH, WEST, grid, march, soldiers, spawn, squall,
    timber_bridge,
};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "bridge-flood-armies",
    about: "two armies of 200+ cross one bridge, 3 tiles wide, opposite ways in a catastrophic flood",
    budget_ms: 1_500,
    run,
};

const TILES: u32 = 96;
const SECOND: u32 = TICKS_PER_SECOND;
const RIVER: core::ops::Range<i32> = 46..50;
const ROW: i32 = 48;
const SPAN: core::ops::RangeInclusive<i32> = 43..=52;
/// When the armies set off: the flood has shut the ford by then.
const SET_OFF: u32 = 40 * SECOND;
/// Regiments a side, and the least soldiers a side.
const REGIMENTS: usize = 11;
const LEAST: u32 = 200;
const LIMIT_SECONDS: u32 = 180;
/// The longest any regiment may stand stuck on its way: walking in place,
/// or waiting its turn at the bridgehead (docs/39's mark).
const STALL_SECONDS: u32 = 10;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.river(RIVER, 0..TILES as i32);
    arena.ford(RIVER, 2..5);
    arena.flowing(SOUTH);
    let mut world = arena.world_under(real, CATASTROPHIC_SEED, 0, squall());
    timber_bridge(real, &mut world, SPAN, ROW..ROW + 3);

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
        report.tick(&mut world, &[]);
    }
    let rivers = world.flood().rivers();
    let flooding = rivers.fronts().any(|f| f.band() == Band::Catastrophic) && rivers.shut();

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
    report.metric("soldiers a side, the fewer", a_side);
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
        walked.stuck <= STALL_SECONDS * SECOND,
    );
    walked.report(report, 20);
}
