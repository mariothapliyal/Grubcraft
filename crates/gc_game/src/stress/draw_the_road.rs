//! Draw the road (docs/42-armies-that-march.md, stage 4, item 18): a
//! river across the field with two fords, one straight ahead and one 44
//! tiles west. Six regiments in line are sent north across it, once by an
//! ordinary group order (which takes the near ford) and once along a road
//! drawn by the far ford, turned into legs as the game turns a player's
//! drawn road. The road's army should cross by the far ford, keep to the
//! road, arrive in its shape and not stand waiting on the way.

use gc_fixed::{Fx, FxVec2};
use gc_sim::arrange::{Arrangement, Preset, arranged_move};
use gc_sim::formation::{GroupMember, GroupOrder};
use gc_sim::orders::Step;
use gc_sim::scenario::NORTH;
use gc_sim::{Command, RegimentHandle, TICKS_PER_SECOND, World};
use glam::Vec2;

use super::arena::{Arena, Real, centre, of, order, spawn};
use super::{Report, Scenario, seconds};
use crate::road;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "draw-the-road",
    about: "6 in line cross a river by the near ford on an ordinary order, and by the far ford along a drawn road",
    budget_ms: 6_000,
    run,
};

const TILES: u32 = 128;
const LIMIT: u32 = 150;
/// The river's rows, and the two fords' columns.
const RIVER: core::ops::Range<i32> = 60..64;
const NEAR_FORD: core::ops::Range<i32> = 58..70;
const FAR_FORD: core::ops::Range<i32> = 14..26;
/// The road drawn: from the army, west to the far ford, across, and back
/// east to the goal.
const ROAD: [(f32, f32); 5] = [
    (64.0, 98.0),
    (20.0, 82.0),
    (20.0, 50.0),
    (40.0, 30.0),
    (64.0, 24.0),
];
const GOAL: (i32, i32) = (64, 24);

struct Walk {
    /// Of ours, how many crossed by the far ford.
    far: usize,
    /// When all stood at their places at the end.
    done: Option<u32>,
    /// The furthest the army's middle strayed from the road drawn, tiles.
    strayed: f32,
    /// Regiment-ticks standing still on the way (not yet at the end).
    stood: u32,
    /// Regiment-ticks in disorder.
    disorder: u32,
}

fn world(real: &Real) -> World {
    let mut arena = Arena::new(TILES);
    arena.river(0..TILES as i32, RIVER);
    for xs in [NEAR_FORD, FAR_FORD] {
        arena.shelving_ford(xs, RIVER, 3);
    }
    // The fords' shoulders are deep water again.
    arena.river(0..FAR_FORD.start, RIVER);
    arena.river(FAR_FORD.end..NEAR_FORD.start, RIVER);
    arena.river(NEAR_FORD.end..TILES as i32, RIVER);
    arena.world(real, 47, 0)
}

/// The army's moves as `legs` of group orders, each queued after the one
/// before.
fn send(legs: Vec<Vec<Command>>) -> Vec<gc_sim::PlayerCommand> {
    let mut orders = Vec::new();
    for (k, leg) in legs.into_iter().enumerate() {
        for c in leg {
            let Command::MoveTo {
                regiment,
                target,
                facing,
                files,
                pace,
                brigade,
            } = c
            else {
                continue;
            };
            let step = Step::Move {
                target,
                facing,
                files,
                pace,
                hunt: false,
                brigade,
            };
            orders.push(order(0, Command::of_step(regiment, step, k > 0)));
        }
    }
    orders
}

fn walk(real: &Real, report: &mut Report, by_road: bool) -> Walk {
    let mut world = world(real);
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari, katana, yumi];
    let line: Vec<_> = (0..6)
        .map(|i| (kinds[i], centre(39 + 10 * i as i32, 100)))
        .collect();
    let army = spawn(&mut world, 0, &line, NORTH);
    let mut members: Vec<GroupMember> = of(&world, &army)
        .map(|v| GroupMember::of(&v, &real.units[usize::from(v.unit)]))
        .collect();
    let arrange = |m: &mut [GroupMember], o: GroupOrder, out: &mut Vec<Command>| {
        arranged_move(m, o, Arrangement::Preset(Preset::Line), out);
    };
    let drawn = road::stepped(&ROAD.map(Vec2::from));
    let from = crate::from_fx_vec(gc_sim::formation::centre_of(&members));
    let legs = if by_road {
        let way: Vec<FxVec2> = (road::waypoints(from, &drawn).into_iter())
            .map(crate::fx_vec)
            .collect();
        let brigades: Vec<u16> = (1..=way.len() as u16).collect();
        road::legs(&mut members, &way, &brigades, arrange)
    } else {
        let mut out = Vec::new();
        let goal = centre(GOAL.0, GOAL.1);
        let o = GroupOrder {
            target: goal,
            facing: Some(NORTH),
            width: None,
            brigade: 1,
        };
        arrange(&mut members, o, &mut out);
        vec![out]
    };
    let places: Vec<(RegimentHandle, FxVec2)> = (legs.last().unwrap().iter())
        .filter_map(|c| match *c {
            Command::MoveTo {
                regiment, target, ..
            } => Some((regiment, target)),
            _ => None,
        })
        .collect();
    report.tick(&mut world, &send(legs));
    let mut far = vec![false; army.len()];
    let (mut done, mut strayed, mut stood, mut disorder) = (None, 0.0f32, 0, 0);
    for t in 1..=LIMIT * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        let views: Vec<_> = of(&world, &army).collect();
        let mut there = 0;
        for v in &views {
            let k = army.iter().position(|&h| h == v.handle).unwrap();
            let (x, y) = (
                crate::fx_to_f32(v.position.x),
                crate::fx_to_f32(v.position.y),
            );
            if (RIVER.start as f32..RIVER.end as f32).contains(&y) && x < FAR_FORD.end as f32 + 4.0
            {
                far[k] = true;
            }
            let place = places.iter().find(|(h, _)| *h == v.handle).unwrap().1;
            let at_end = (place - v.position).length() <= Fx::from_int(3);
            there += usize::from(at_end && !v.moving);
            stood += u32::from(!v.moving && !at_end);
            disorder += u32::from(v.disorder > 0);
        }
        let middle = views
            .iter()
            .map(|v| crate::from_fx_vec(v.position))
            .sum::<Vec2>()
            / views.len().max(1) as f32;
        if by_road {
            strayed = strayed.max(off_road(middle, &drawn));
        }
        if there == army.len() {
            done = Some(t);
            break;
        }
    }
    Walk {
        far: far.iter().filter(|&&f| f).count(),
        done,
        strayed,
        stood,
        disorder,
    }
}

/// How far `p` lies from the nearest point of the drawn line.
fn off_road(p: Vec2, line: &[Vec2]) -> f32 {
    (line.windows(2))
        .map(|w| gc_ai::perception::segment_distance(p, w[0], w[1]))
        .fold(f32::MAX, f32::min)
}

fn run(real: &Real, report: &mut Report) {
    let tps = TICKS_PER_SECOND as f32;
    for (name, by_road) in [("ordinary order", false), ("drawn road", true)] {
        let w = walk(real, report, by_road);
        report.metric(format!("{name}: crossed by the far ford, of 6"), w.far);
        report.metric(format!("{name}: all in their places"), seconds(w.done));
        report.metric(
            format!("{name}: regiment-seconds standing on the way"),
            format!("{:.1}", w.stood as f32 / tps),
        );
        report.metric(
            format!("{name}: regiment-seconds in disorder"),
            format!("{:.1}", w.disorder as f32 / tps),
        );
        if by_road {
            report.metric(
                format!("{name}: furthest the army's middle strayed from it, tiles"),
                format!("{:.1}", w.strayed),
            );
            report.check(format!("{name}: all six cross by the far ford"), w.far == 6);
            report.check(
                format!("{name}: all in their places within {LIMIT} s"),
                w.done.is_some(),
            );
            report.check(
                format!("{name}: the army's middle keeps within 12 tiles of it"),
                w.strayed <= 12.0,
            );
        } else {
            report.check(format!("{name}: takes the near ford"), w.far == 0);
        }
    }
}
