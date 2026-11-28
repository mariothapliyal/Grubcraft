//! Column of march (docs/42-armies-that-march.md, stage 4, item 19): six
//! regiments in line march north, on an ordinary order and in column of
//! march as the game sends it (Alt+right-click), over open country and
//! through narrow country (two hedges of thicket across the field, each
//! with one gap, the gaps far apart). The column should stand deployed in
//! line at the end, and get there sooner over open country. Then the
//! column marches again with an enemy line standing beside the way: it
//! should deploy facing the enemy before any of it is caught in column.

use gc_fixed::Angle;
use gc_sim::arrange::{Arrangement, Preset, arranged_move};
use gc_sim::formation::{GroupMember, GroupOrder};
use gc_sim::scenario::NORTH;
use gc_sim::{Command, Formation, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, centre, of, order, spawn};
use super::{Report, Scenario, seconds};
use crate::column;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "column-of-march",
    about: "6 in line march on an ordinary order and in column, over open and narrow country; the column deploys at the end, and when an enemy comes near",
    budget_ms: 8_000,
    run,
};

/// The narrow field's size, and the open field's (longer: room for a
/// column to form before it meets the enemy).
const NARROW_TILES: u32 = 128;
const OPEN_TILES: u32 = 192;
const LIMIT: u32 = 150;
/// The hedges' rows, and each one's gap.
const HEDGES: [(core::ops::Range<i32>, core::ops::Range<i32>); 2] =
    [(76..79, 18..28), (46..49, 98..108)];
const GOAL: (i32, i32) = (64, 16);

#[derive(Clone, Copy)]
struct How {
    narrow: bool,
    column: bool,
    enemy: bool,
}

#[derive(Default)]
struct Walk {
    /// When all stood formed at the end.
    done: Option<u32>,
    /// Regiment-ticks in disorder.
    disorder: u32,
    /// When the column deployed, and how far the nearest enemy was then.
    deployed: Option<(u32, f32)>,
    /// Regiment-ticks fighting in column.
    caught: u32,
    /// Of ours, how many stood facing the enemy within 30 degrees at the
    /// end.
    facing_enemy: usize,
}

/// The field: with hedges across it, `narrow`, else open.
fn world(real: &Real, narrow: bool) -> World {
    let tiles = if narrow { NARROW_TILES } else { OPEN_TILES };
    let mut arena = Arena::new(tiles);
    for (ys, gap) in HEDGES.into_iter().filter(|_| narrow) {
        arena.thicket(0..gap.start, ys.clone());
        arena.thicket(gap.end..tiles as i32, ys);
    }
    arena.world(real, 61, 0)
}

fn line(m: &mut [GroupMember], o: GroupOrder, out: &mut Vec<Command>) {
    arranged_move(m, o, Arrangement::Preset(Preset::Line), out);
}

fn send(world: &mut World, report: &mut Report, commands: &[Command]) {
    let orders: Vec<_> = commands.iter().map(|&c| order(0, c)).collect();
    report.tick(world, &orders);
}

fn walk(real: &Real, report: &mut Report, how: How) -> Walk {
    let mut world = world(real, how.narrow);
    let start = if how.narrow { NARROW_TILES } else { OPEN_TILES } as i32 - 16;
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari, katana, yumi];
    let placed: Vec<_> = (0..6)
        .map(|i| (kinds[i], centre(34 + 12 * i as i32, start)))
        .collect();
    let army = spawn(&mut world, 0, &placed, NORTH);
    let enemy = if how.enemy {
        // Three spearmen in line west of the way, facing it, 28 tiles
        // off it.
        let at: Vec<_> = (0..3).map(|i| (yari, centre(36, 40 + 10 * i))).collect();
        spawn(&mut world, 1, &at, EAST)
    } else {
        Vec::new()
    };
    let mut members: Vec<GroupMember> = of(&world, &army)
        .map(|v| GroupMember::of(&v, &real.units[usize::from(v.unit)]))
        .collect();
    let goal = GroupOrder {
        target: centre(GOAL.0, GOAL.1),
        facing: Some(NORTH),
        width: None,
        brigade: 1,
    };
    let mut out = Vec::new();
    let mut march = None;
    if how.column {
        column::ready(&world, &mut members);
        let clear = |a, b| world.nav().clear_line(0, a, b);
        column::arrange(&mut members, goal, &clear, &mut out);
        let mut legs = vec![out];
        let (changes, m) = column::begin(
            &world,
            0,
            &mut legs,
            goal.target,
            NORTH,
            column::Form::Preset(Preset::Line),
        );
        let mut all = changes;
        all.extend(legs.concat());
        send(&mut world, report, &all);
        march = Some(m);
    } else {
        line(&mut members, goal, &mut out);
        send(&mut world, report, &out);
    }
    let mut w = Walk::default();
    for t in 1..=LIMIT * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        if let Some(m) = &mut march {
            let mut commands = Vec::new();
            if let Some(why) = column::watch(&world, m, &mut commands) {
                let near = (of(&world, &enemy))
                    .flat_map(|e| {
                        of(&world, &army).map(move |v| (e.position - v.position).length())
                    })
                    .min()
                    .map_or(f32::NAN, crate::fx_to_f32);
                w.deployed = Some((t, near));
                commands = column::deploy(&world, m, why, 2, &[], line);
            }
            if m.is_empty() {
                march = None;
            }
            if !commands.is_empty() {
                send(&mut world, report, &commands);
            }
        }
        let mut formed = 0;
        for v in of(&world, &army) {
            // Its last order carried out (as near its place as it can
            // stand), and out of column.
            formed += usize::from(
                march.is_none()
                    && v.formation != Formation::Column
                    && !v.moving
                    && world.goal(v.handle).is_none(),
            );
            w.disorder += u32::from(v.disorder > 0);
            w.caught += u32::from(v.engaged && v.formation == Formation::Column);
        }
        if formed == army.len() {
            w.done = Some(t);
            break;
        }
    }
    if let Some(foe) = of(&world, &enemy).map(|e| e.position).nth(1) {
        w.facing_enemy = of(&world, &army)
            .filter(|v| {
                let to = Angle::of_vec(foe - v.position);
                let off = to.0.wrapping_sub(v.facing.0) as i16;
                i32::from(off).abs() <= 0x10000 / 12
            })
            .count();
    }
    w
}

fn run(real: &Real, report: &mut Report) {
    let tps = TICKS_PER_SECOND as f32;
    let mut open = [None; 2];
    let how = |narrow, column, enemy| How {
        narrow,
        column,
        enemy,
    };
    for (name, how) in [
        ("open, ordinary order", how(false, false, false)),
        ("open, column of march", how(false, true, false)),
        ("narrow, ordinary order", how(true, false, false)),
        ("narrow, column of march", how(true, true, false)),
        ("open, column, enemy by the way", how(false, true, true)),
    ] {
        let w = walk(real, report, how);
        report.metric(format!("{name}: all formed at the end"), seconds(w.done));
        report.metric(
            format!("{name}: regiment-seconds in disorder"),
            format!("{:.1}", w.disorder as f32 / tps),
        );
        if how.column {
            report.metric(
                format!("{name}: deployed at"),
                seconds(w.deployed.map(|d| d.0)),
            );
        }
        if !how.narrow && !how.enemy {
            open[usize::from(how.column)] = w.done;
        }
        if how.enemy {
            let at = w.deployed.map_or(0, |d| d.0);
            report.metric(
                format!("{name}: nearest enemy when it deployed, tiles"),
                format!("{:.1}", w.deployed.map_or(f32::NAN, |d| d.1)),
            );
            report.metric(
                format!("{name}: formed, seconds after deploying"),
                w.done
                    .map_or("never".into(), |d| format!("{:.1}", (d - at) as f32 / tps)),
            );
            report.metric(
                format!("{name}: regiment-seconds fighting in column"),
                format!("{:.1}", w.caught as f32 / tps),
            );
            report.metric(format!("{name}: facing the enemy, of 6"), w.facing_enemy);
            report.check(
                format!("{name}: deploys before contact"),
                w.deployed.is_some() && w.caught == 0,
            );
            report.check(
                format!("{name}: stands formed facing the enemy"),
                w.done.is_some() && w.facing_enemy >= 5,
            );
        } else if how.column {
            report.check(
                format!("{name}: all formed at the end within {LIMIT} s"),
                w.done.is_some(),
            );
        }
    }
    if let [Some(ordinary), Some(column)] = open {
        report.check(
            "open: the column is formed at the end sooner than the ordinary order".to_string(),
            column < ordinary,
        );
    }
}
