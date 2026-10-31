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

/// The centre of the burnable tile nearest `r` whose edge is within
/// [`TINDER`] of its block: where a [`gc_sim::Command::Burn`] from it
/// catches (the sim's `Fires::tinder`, near enough).
fn tinder(world: &World, r: &Seen) -> Option<Vec2> {
    let reach = (r.radius() + TINDER).ceil() as i32 + 1;
    let (cx, cy) = (r.pos.x.floor() as i32, r.pos.y.floor() as i32);
    let mut best: Option<(f32, Vec2)> = None;
    for y in cy - reach..=cy + reach {
        for x in cx - reach..=cx + reach {
            let c = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            if !burnable(world, c) {
                continue;
            }
            let d = c.distance(r.pos);
            let Some(dir) = (c - r.pos).try_normalize() else {
                continue;
            };
            // A tile's half width, outward, at most √2/2.
            if d > r.half_extent(dir) + TINDER + 0.7 || best.is_some_and(|(b, _)| b <= d) {
                continue;
            }
            best = Some((d, c));
        }
    }
    best.map(|(_, c)| c)
}

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
    let friends: Vec<&Seen> = (view.regiments.iter())
        .filter(|f| !view.hostile(party, f.owner))
        .collect();
    let w = world.weather().wind;
    let wind = Vec2::new(f32::from(w.0), f32::from(w.1));
    let back = (wind.length() >= WIND_FELT).then(|| wind.normalize());
    let mut best: Option<(f32, RegimentHandle)> = None;
    for r in burners {
        // Where its torches would catch: the nearest burnable tile within
        // reach of its block, as the sim picks it.
        let Some(lit) = tinder(world, r) else {
            continue;
        };
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
            // That tile is on the way to the enemy, and the woods run
            // unbroken from it to the enemy, walked in half tiles.
            if segment_distance(lit, r.pos, e.pos) > 1.0 {
                continue;
            }
            let steps = (lit.distance(e.pos) * 2.0).ceil() as i32;
            let Some(on) = (e.pos - lit).try_normalize() else {
                continue;
            };
            if !(1..=steps).all(|k| burnable(world, lit + on * (k as f32 * 0.5))) {
                continue;
            }
            // None of its own side fighting the enemy, nor in the woods by
            // the way the fire runs.
            let in_the_way = |f: &&Seen| {
                f.handle != r.handle
                    && (f.pos.distance(e.pos) < FRIEND_NEAR + f.radius() + e.radius()
                        || (burnable(world, f.pos) || f.engaged)
                            && segment_distance(f.pos, lit, e.pos) < FRIEND_CLEAR + f.radius())
            };
            if friends.iter().any(in_the_way) {
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
