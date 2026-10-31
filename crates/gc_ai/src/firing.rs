//! Setting woods alight on an enemy (docs/41-finish-living-land.md, step
//! 6): a computer player whose regiment stands at the edge of woods that
//! an enemy army stands or hides in, with the wind not blowing back at it
//! and none of its own side in the way, orders that regiment to burn them
//! ([`gc_sim::Command::Burn`]), as a person would.

use gc_sim::{Growth, PlayerId, RegimentHandle, World};
use glam::Vec2;

use crate::perception::{Perception, Seen};

/// How far off an enemy in the woods may be, tiles: the woods from the
/// burner's edge to it must be one wood all the way.
pub const REACH: f32 = 12.0;
/// Tiles beyond a regiment's edge it lights the woods from (the sim's
/// own reach for [`gc_sim::Command::Burn`] is one tile).
const TINDER: f32 = 1.0;
/// No regiment of its own side (or an ally's) in the woods within this
/// many tiles of the enemy or of the woods between, which would burn with
/// it, nor anywhere this near the enemy (fighting it, say).
const FRIEND_CLEAR: f32 = 6.0;
const FRIEND_NEAR: f32 = 3.0;
/// Wind of at least this many sixteenths of a tile a second has a side:
/// a fire is not lit with it blowing from the woods at the burner.
const WIND_FELT: f32 = 4.0;
/// Cosine of the widest angle between the way to the enemy and the wind
/// blowing back at the burner that it still burns in.
const BACK_WIND: f32 = -0.3;
/// Ticks between looks, and after an order before the next.
pub const LOOK_TICKS: u32 = 2 * gc_sim::TICKS_PER_SECOND;
pub const AFTER_BURN: u32 = 20 * gc_sim::TICKS_PER_SECOND;

/// A fire it means to set: who lights it, and on whom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Burn {
    pub burner: RegimentHandle,
    pub on: RegimentHandle,
}

fn burnable(world: &World, p: Vec2) -> bool {
    let g = world
        .terrain()
        .growth(p.x.floor() as i32, p.y.floor() as i32);
    g != Growth::Open
}

/// The fire `party` should set now, if any: the nearest pair of one of its
/// regiments (standing on open ground, free, not burning) and an enemy it
/// sees or lately saw within [`REACH`] standing in woods that run unbroken
/// from the regiment's edge to it.
pub fn pick(view: &Perception, world: &World, party: PlayerId) -> Option<Burn> {
    if !world.weather_lets_fire() || world.terrain().growth_side() == 0 {
        return None;
    }
    let w = world.weather().wind;
    let wind = Vec2::new(f32::from(w.0), f32::from(w.1));
    let enemies: Vec<&Seen> = (view.regiments.iter().chain(view.remembered()))
        .filter(|e| view.hostile(party, e.owner) && e.models > 0)
        .filter(|e| burnable(world, e.pos))
        .collect();
    if enemies.is_empty() {
        return None;
    }
    let friends: Vec<Vec2> = (view.regiments.iter())
        .filter(|f| !view.hostile(party, f.owner))
        .map(|f| f.pos)
        .collect();
    let mut best: Option<(f32, Burn)> = None;
    for r in view.regiments.iter().filter(|r| r.owner == party) {
        if r.engaged || r.broken() || r.in_fire || r.models == 0 || burnable(world, r.pos) {
            continue;
        }
        for e in &enemies {
            let d = r.pos.distance(e.pos);
            if d > REACH || best.is_some_and(|(b, _)| b <= d) {
                continue;
            }
            let Some(way) = (e.pos - r.pos).try_normalize() else {
                continue;
            };
            if wind.length() >= WIND_FELT && wind.normalize().dot(way) < BACK_WIND {
                continue;
            }
            // The woods start within its reach and run unbroken to the enemy.
            let edge = r.width.max(r.depth) * 0.5 + TINDER;
            let steps = (d * 2.0).ceil() as i32;
            let at = |k: i32| r.pos + way * (k as f32 * 0.5);
            let Some(start) = (0..=steps).find(|&k| burnable(world, at(k))) else {
                continue;
            };
            if (start as f32) * 0.5 > edge || !(start..=steps).all(|k| burnable(world, at(k))) {
                continue;
            }
            let from = at(start);
            let in_the_way = |p: Vec2| {
                p.distance(e.pos) < FRIEND_NEAR
                    || (burnable(world, p)
                        && (p.distance(e.pos) < FRIEND_CLEAR
                            || segment_distance(p, from, e.pos) < FRIEND_CLEAR))
            };
            if friends.iter().any(|&p| in_the_way(p)) {
                continue;
            }
            best = Some((
                d,
                Burn {
                    burner: r.handle,
                    on: e.handle,
                },
            ));
        }
    }
    best.map(|(_, b)| b)
}

/// How far `p` is from the segment `a` to `b`.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_is_measured_to_the_nearest_of_the_segment() {
        let (a, b) = (Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert_eq!(segment_distance(Vec2::new(5.0, 3.0), a, b), 3.0);
        assert_eq!(segment_distance(Vec2::new(-4.0, 3.0), a, b), 5.0);
        assert_eq!(segment_distance(Vec2::new(13.0, 4.0), a, b), 5.0);
    }
}
