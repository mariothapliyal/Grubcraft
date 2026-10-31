//! Setting woods alight on an enemy (docs/41-finish-living-land.md, step
//! 6): a computer player whose regiment stands at the edge of woods that
//! an enemy army stands or hides in, with the wind not blowing back at it
//! and none of its own side in the way, orders that regiment to burn them
//! ([`gc_sim::Command::Burn`]), as a person would.

use gc_sim::fire::burn_ticks;
use gc_sim::{PlayerId, RegimentHandle, World};
use glam::Vec2;

use crate::perception::{Perception, Seen, segment_distance};

/// How far off an enemy in the woods may be, tiles: the woods from the
/// burner's edge to it must be one wood all the way.
const REACH: f32 = 12.0;
/// Tiles beyond a regiment's edge it lights the woods from (the sim's
/// own reach for [`gc_sim::Command::Burn`] is one tile).
const TINDER: f32 = 1.0;
/// No regiment of its own side (or an ally's) in the woods within this
/// many tiles of the woods between it and the enemy, which would burn
/// with it, nor anywhere this near the enemy (fighting it, say).
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

/// Whether the ground at `p` burns, as the sim's fire judges it.
fn burnable(world: &World, p: Vec2) -> bool {
    let g = world
        .terrain()
        .growth(p.x.floor() as i32, p.y.floor() as i32);
    burn_ticks(g) > 0
}

/// The regiment `party` should set the woods alight with now, if any: of
/// its regiments standing on open ground, free and not burning, the one
/// nearest an enemy it sees or lately saw within [`REACH`] standing in
/// woods that run unbroken from the regiment's edge to it.
pub fn pick(view: &Perception, world: &World, party: PlayerId) -> Option<RegimentHandle> {
    if !world.weather_lets_fire() || world.terrain().growth_side() == 0 {
        return None;
    }
    let burners: Vec<&Seen> = (view.regiments.iter())
        .filter(|r| r.owner == party && r.models > 0)
        .filter(|r| !(r.engaged || r.broken() || r.in_fire || burnable(world, r.pos)))
        .collect();
    if burners.is_empty() {
        return None;
    }
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
    let w = world.weather().wind;
    let wind = Vec2::new(f32::from(w.0), f32::from(w.1));
    let back = (wind.length() >= WIND_FELT).then(|| wind.normalize());
    let mut best: Option<(f32, RegimentHandle)> = None;
    for r in burners {
        for e in &enemies {
            let d = r.pos.distance(e.pos);
            if d > REACH || best.is_some_and(|(b, _)| b <= d) {
                continue;
            }
            let Some(way) = (e.pos - r.pos).try_normalize() else {
                continue;
            };
            if back.is_some_and(|b| b.dot(way) < BACK_WIND) {
                continue;
            }
            // The woods start within its reach and run unbroken to the
            // enemy, walked in half tiles.
            let edge = r.half_extent(way) + TINDER;
            let steps = (d * 2.0).ceil() as i32;
            let at = |k: i32| r.pos + way * (k as f32 * 0.5);
            let reach = ((edge * 2.0) as i32).min(steps);
            let Some(start) = (0..=reach).find(|&k| burnable(world, at(k))) else {
                continue;
            };
            if !(start + 1..=steps).all(|k| burnable(world, at(k))) {
                continue;
            }
            let from = at(start);
            let in_the_way = |p: Vec2| {
                p.distance(e.pos) < FRIEND_NEAR
                    || (burnable(world, p) && segment_distance(p, from, e.pos) < FRIEND_CLEAR)
            };
            if friends.iter().any(|&p| in_the_way(p)) {
                continue;
            }
            best = Some((d, r.handle));
        }
    }
    best.map(|(_, b)| b)
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
