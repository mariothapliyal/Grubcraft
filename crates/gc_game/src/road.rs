//! Draw the road (docs/42-armies-that-march.md, stage 4, item 18): the
//! player holds Ctrl (Cmd on a Mac) and right-drags the way the army
//! should go. The drawn line becomes a few waypoints, and the army is
//! sent along them leg by leg, each leg a Shift-queued group order (so a
//! brigade of its own) that stands the army in its shape at the waypoint,
//! facing on along the road. Between waypoints the simulation's own route
//! search walks the leg, so the waypoints are kept close enough at every
//! bend that the shortest way between two of them is the drawn one.
//!
//! All of it is orders the game sends for the player: nothing new reaches
//! the simulation.

use gc_fixed::{Angle, FxVec2};
use gc_sim::Command;
use gc_sim::formation::{GroupMember, GroupOrder, centre_of};
use glam::Vec2;

/// A drawn point is kept once the cursor's ground is this far from the
/// last, in tiles.
pub const STEP: f32 = 1.0;
/// A drawn road shorter than this is no road: the drag is an ordinary
/// order.
pub const MIN_ROAD: f32 = 12.0;
/// How far the drawn line may stray from the straight leg between two
/// waypoints before a bend between them is kept as a waypoint, in tiles.
const BEND: f32 = 3.0;
/// The shortest leg kept, in tiles: closer waypoints merge into one (the
/// later) so the army is not halted to re-form every few steps.
const LEG: f32 = 10.0;

/// How long a drawn line is, in tiles.
pub fn length(points: &[Vec2]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// The waypoints of a road drawn as `points`, for an army whose middle is
/// at `from`: the bends of the line (Douglas-Peucker, within [`BEND`]),
/// none nearer than [`LEG`] to the one before (or to `from`), always
/// ending at the line's end. The line's start is a waypoint only when it
/// lies a leg or more away from the army.
pub fn waypoints(from: Vec2, points: &[Vec2]) -> Vec<Vec2> {
    let Some(&end) = points.last() else {
        return Vec::new();
    };
    let mut keep = vec![false; points.len()];
    keep[0] = true;
    keep[points.len() - 1] = true;
    bends(points, 0, points.len() - 1, &mut keep);
    let mut out: Vec<Vec2> = Vec::new();
    for (k, &p) in points.iter().enumerate() {
        if !keep[k] {
            continue;
        }
        let last = out.last().copied().unwrap_or(from);
        if p.distance(last) >= LEG {
            out.push(p);
        }
    }
    // The end always (the loop kept it, or one less than a leg short of
    // it, which it takes the place of).
    match out.last_mut() {
        Some(last) => *last = end,
        None => out.push(end),
    }
    out
}

/// Douglas-Peucker: mark the points of `points[a..=b]` that stand out of
/// the straight line from `a` to `b` by more than [`BEND`].
fn bends(points: &[Vec2], a: usize, b: usize, keep: &mut [bool]) {
    if b <= a + 1 {
        return;
    }
    let (p, d) = (points[a], points[b] - points[a]);
    let len = d.length();
    let off = |x: Vec2| {
        if len < 1e-3 {
            x.distance(p)
        } else {
            (d.perp_dot(x - p) / len).abs()
        }
    };
    let Some((k, far)) = ((a + 1)..b)
        .map(|k| (k, off(points[k])))
        .max_by(|x, y| x.1.total_cmp(&y.1))
    else {
        return;
    };
    if far > BEND {
        keep[k] = true;
        bends(points, a, k, keep);
        bends(points, k, b, keep);
    }
}

/// The group orders that send `members` along `waypoints`, one list for
/// each leg (its brigade the leg's number in `brigades`, else 0), laid out
/// by `arrange` (the order's shape: a preset, a saved shape or a standing
/// army). At each waypoint the army faces on toward
/// the next, and at the end along the road's last leg. Each leg is
/// arranged from where the one before leaves the army, turned with the
/// road, so a regiment keeps its side of the army round a bend (a line
/// wheeling left keeps its right on the right) instead of being dealt a
/// place by where it stood before the march began.
pub fn legs(
    members: &mut [GroupMember],
    waypoints: &[FxVec2],
    brigades: &[u16],
    mut arrange: impl FnMut(&mut [GroupMember], GroupOrder, &mut Vec<Command>),
) -> Vec<Vec<Command>> {
    let mut out = Vec::with_capacity(waypoints.len());
    let Some(first) = members.first() else {
        return out;
    };
    let mut from = centre_of(members);
    let mut facing = first.facing;
    for (k, &at) in waypoints.iter().enumerate() {
        let next = waypoints.get(k + 1).copied();
        let way = match next {
            Some(n) => n - at,
            None => at - from,
        };
        let face = if way == FxVec2::ZERO {
            facing
        } else {
            Angle::of_vec(way)
        };
        // Turn the army about its middle as the road turns.
        let turn = Angle(face.0.wrapping_sub(facing.0));
        let c = centre_of(members);
        for m in members.iter_mut() {
            m.position = c + turn.rotate(m.position - c);
            m.facing = m.facing.wrapping_add(turn);
        }
        let mut leg = Vec::with_capacity(members.len());
        arrange(
            members,
            GroupOrder {
                target: at,
                facing: Some(face),
                width: None,
                brigade: brigades.get(k).copied().unwrap_or(0),
            },
            &mut leg,
        );
        // Where this leg leaves each of them.
        for c in &leg {
            if let Command::MoveTo {
                regiment,
                target,
                facing: f,
                ..
            } = *c
                && let Some(m) = members.iter_mut().find(|m| m.handle == regiment)
            {
                m.position = target;
                m.facing = f.unwrap_or(face);
            }
        }
        out.push(leg);
        from = at;
        facing = face;
    }
    out
}

/// The ground points a drag along straight lines between `corners`
/// records, a tile or less apart.
#[cfg(test)]
pub(crate) fn stepped(corners: &[Vec2]) -> Vec<Vec2> {
    let mut out = vec![corners[0]];
    for w in corners.windows(2) {
        let n = w[0].distance(w[1]).ceil().max(1.0) as usize;
        out.extend((1..=n).map(|i| w[0].lerp(w[1], i as f32 / n as f32)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(points: &[(f32, f32)]) -> Vec<Vec2> {
        stepped(&points.iter().map(|&p| Vec2::from(p)).collect::<Vec<_>>())
    }

    #[test]
    fn a_dogleg_keeps_its_corners_and_its_end() {
        let drawn = line(&[(64.0, 100.0), (24.0, 80.0), (24.0, 40.0), (64.0, 24.0)]);
        let w = waypoints(Vec2::new(64.0, 104.0), &drawn);
        // The start lies within a leg of the army; the three corners stay.
        assert_eq!(w.len(), 3, "{w:?}");
        assert!(w[0].distance(Vec2::new(24.0, 80.0)) < 0.5);
        assert!(w[1].distance(Vec2::new(24.0, 40.0)) < 0.5);
        assert_eq!(w[2], Vec2::new(64.0, 24.0));
    }

    #[test]
    fn a_wobbly_straight_line_is_one_leg() {
        let drawn: Vec<Vec2> = (0..60)
            .map(|i| Vec2::new(10.0 + i as f32, 50.0 + if i % 2 == 0 { 1.0 } else { -1.0 }))
            .collect();
        let w = waypoints(Vec2::new(0.0, 50.0), &drawn);
        assert_eq!(w.len(), 2, "{w:?}");
        assert_eq!(*w.last().unwrap(), *drawn.last().unwrap());
    }

    #[test]
    fn close_bends_merge_into_the_later() {
        let drawn = line(&[(0.0, 0.0), (4.0, 6.0), (8.0, 0.0), (40.0, 0.0)]);
        let w = waypoints(Vec2::new(-20.0, 0.0), &drawn);
        assert!(w.windows(2).all(|p| p[0].distance(p[1]) >= LEG), "{w:?}");
        assert_eq!(*w.last().unwrap(), Vec2::new(40.0, 0.0));
    }

    #[test]
    fn a_turn_keeps_each_regiment_on_its_side() {
        use gc_sim::RegimentHandle;
        use gc_sim::arrange::{Arrangement, Preset, arranged_move};
        use gc_sim::scenario::NORTH;
        // Three in line facing north, sent north then west: the one on the
        // right (east) stands on the right (north) after the turn.
        let member = |index: u32, x: i32| GroupMember {
            handle: RegimentHandle {
                index,
                generation: 0,
            },
            position: FxVec2::from_ints(x, 100),
            facing: NORTH,
            files: 6,
            models: 24,
            file_spacing: gc_fixed::Fx::ONE,
            rank_spacing: gc_fixed::Fx::ONE,
            speed: gc_fixed::Fx::from_int(3),
            role: gc_sim::arrange::Role::Melee,
            raised_files: 6,
        };
        let mut members = vec![member(0, 50), member(1, 60), member(2, 70)];
        let way = [FxVec2::from_ints(60, 60), FxVec2::from_ints(20, 60)];
        let legs = legs(&mut members, &way, &[], |m, order, out| {
            arranged_move(m, order, Arrangement::Preset(Preset::Line), out);
        });
        assert_eq!(legs.len(), 2);
        let at = |leg: &[Command], index: u32| {
            leg.iter()
                .find_map(|c| match *c {
                    Command::MoveTo {
                        regiment, target, ..
                    } if regiment.index == index => Some(target),
                    _ => None,
                })
                .unwrap()
        };
        // At the corner they already face west: east-most is north-most.
        assert!(at(&legs[0], 2).y < at(&legs[0], 1).y && at(&legs[0], 1).y < at(&legs[0], 0).y);
        assert!(at(&legs[1], 2).y < at(&legs[1], 0).y);
    }
}
