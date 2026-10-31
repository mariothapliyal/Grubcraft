//! Setting woods alight on an enemy (docs/41-finish-living-land.md, step
//! 6): a computer player whose regiment stands at the edge of woods that
//! an enemy army stands or hides in, with the wind not blowing back at it
//! and none of its own side in the way, orders that regiment to burn them
//! ([`gc_sim::Command::Burn`]), as a person would.

use gc_sim::fire::burn_ticks;
use gc_sim::{PlayerId, RegimentHandle, World};
use glam::Vec2;

use crate::orders::{Act, Want};
use crate::perception::{Perception, Seen, segment_distance};

/// How far off an enemy in the woods may be, tiles: the woods from the
/// burner's edge to it must be one wood all the way.
const REACH: f32 = 16.0;
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
    let burners: Vec<&Seen> = burners(view, world, party).collect();
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
    let back = felt_wind(world);
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

/// How far off, tiles, an enemy hidden in the woods may be for it to
/// smoke it out rather than go in after it.
const SMOKE_REACH: f32 = 40.0;
/// How long it holds its troops out of woods it means to burn, ticks:
/// long enough to bring a burner to the edge and for the fire to take.
pub const SMOKE_TICKS: u32 = 45 * gc_sim::TICKS_PER_SECOND;
/// The priority of its smoking-out orders: above the battle's own, below
/// marching out of a fire.
const SMOKE_PRIORITY: u8 = u8::MAX - 1;
/// Open ground at least this deep (tiles) past a woods' edge makes it an
/// edge, not a clearing or a track through the trees.
const EDGE_DEPTH: f32 = 2.0;

/// Where `r` should stand to burn the woods that `e` hides in: just out of
/// them on its own side, facing `e`, within reach of a fire set there
/// and not with the wind blowing back at it (`back`, the felt wind's
/// direction). None if there is no such place.
fn approach(world: &World, r: &Seen, e: &Seen, back: Option<Vec2>) -> Option<Vec2> {
    let way = (r.pos - e.pos).try_normalize()?;
    if back.is_some_and(|b| b.dot(-way) < BACK_WIND) {
        return None;
    }
    let edge = walk_out(world, e.pos, way)?;
    if edge.distance(e.pos) > REACH - 4.0 {
        return None;
    }
    // Its front at the trees once it faces them (orders count a march
    // done 2 tiles short).
    Some(edge + way * (r.depth * 0.5 - 1.0).max(0.6))
}

/// The direction of a wind strong enough to have a side, if any.
fn felt_wind(world: &World) -> Option<Vec2> {
    let w = world.weather().wind;
    let wind = Vec2::new(f32::from(w.0), f32::from(w.1));
    (wind.length() >= WIND_FELT).then(|| wind.normalize())
}

/// Its regiments that could bring a fire: free, steady, on open ground.
fn burners<'a>(
    view: &'a Perception,
    world: &'a World,
    party: PlayerId,
) -> impl Iterator<Item = &'a Seen> + 'a {
    (view.regiments.iter())
        .filter(move |r| r.owner == party && r.models > 0)
        .filter(move |r| !(r.engaged || r.broken() || r.in_fire || burnable(world, r.pos)))
}

/// An enemy hidden in woods that `party` could smoke out: in burnable
/// ground, within [`SMOKE_REACH`] of one of its regiments that has a place
/// to burn it from ([`approach`]), with none of its own side fighting it
/// and no ally (whom it can't order out) in the woods by it. The nearest
/// such, or `keep` while it still is one.
pub fn quarry(
    view: &Perception,
    world: &World,
    party: PlayerId,
    keep: Option<RegimentHandle>,
) -> Option<RegimentHandle> {
    if !world.weather_lets_fire() || world.terrain().growth_side() == 0 {
        return None;
    }
    let back = felt_wind(world);
    let friends: Vec<&Seen> = (view.regiments.iter())
        .filter(|f| !view.hostile(party, f.owner))
        .collect();
    let mut best: Option<(f32, RegimentHandle)> = None;
    for e in (view.regiments.iter().chain(view.remembered()))
        .filter(|e| view.hostile(party, e.owner) && e.models > 0)
        .filter(|e| burnable(world, e.pos))
    {
        let blocked = friends.iter().any(|f| {
            let near = f.pos.distance(e.pos) < FRIEND_NEAR + f.radius() + e.radius();
            (f.engaged && near)
                || (f.owner != party
                    && burnable(world, f.pos)
                    && f.pos.distance(e.pos) < FRIEND_CLEAR * 2.0 + f.radius())
        });
        if blocked {
            continue;
        }
        let Some(d) = burners(view, world, party)
            .filter(|r| r.pos.distance(e.pos) <= SMOKE_REACH)
            .filter(|r| approach(world, r, e, back).is_some())
            .map(|r| r.pos.distance(e.pos))
            .min_by(f32::total_cmp)
        else {
            continue;
        };
        let d = if Some(e.handle) == keep { -1.0 } else { d };
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, e.handle));
        }
    }
    best.map(|(_, h)| h)
}

/// Smoke out enemy `on`, hidden in the woods: rather than send its troops
/// in after it, `party` keeps them out of those woods (the ones in there
/// and free march out by the nearest edge, the rest stop rather than go
/// in) and brings its nearest regiment that has a place to burn from
/// ([`approach`]) there, so that [`pick`] can set the fire. Rewrites
/// `wants` to match.
pub fn smoke_out(
    view: &Perception,
    world: &World,
    party: PlayerId,
    on: RegimentHandle,
    wants: &mut Vec<Want>,
) {
    let Some(e) = (view.regiments.iter().chain(view.remembered())).find(|e| e.handle == on) else {
        return;
    };
    let back = felt_wind(world);
    // Woods near the enemy, that a fire there would run through.
    let near = |p: Vec2| burnable(world, p) && p.distance(e.pos) < FRIEND_CLEAR * 2.0;
    // Whether the way from `a` to `b` goes through those woods.
    let through = |a: Vec2, b: Vec2| {
        let steps = (a.distance(b) * 2.0).ceil().max(1.0) as i32;
        (0..=steps).any(|k| near(a.lerp(b, k as f32 / steps as f32)))
    };
    let target_of = |w: &Want| match w.act {
        Act::Move { to, .. } => Some(to),
        Act::Attack(h) => view.regiments.iter().find(|x| x.handle == h).map(|x| x.pos),
        _ => None,
    };
    for r in view
        .regiments
        .iter()
        .filter(|r| r.owner == party && r.models > 0)
    {
        if r.engaged || r.broken() {
            continue;
        }
        let its = |w: &Want| w.regiment == r.handle;
        if burnable(world, r.pos) && r.pos.distance(e.pos) < FRIEND_CLEAR * 2.0 + r.radius() {
            // In those woods: out by the nearest edge.
            let out = (0..8)
                .map(|k| Vec2::from_angle(k as f32 * core::f32::consts::FRAC_PI_4))
                .filter_map(|dir| walk_out(world, r.pos, dir).map(|p| (p, dir)))
                .min_by(|a, b| a.0.distance(r.pos).total_cmp(&b.0.distance(r.pos)));
            if let Some((edge, dir)) = out {
                wants.retain(|w| !its(w));
                wants.push(hold(r.handle, edge + dir * (r.radius() + 1.0), None));
            }
        } else if wants
            .iter()
            .any(|w| its(w) && target_of(w).is_some_and(|t| through(r.pos, t)))
        {
            // Not into them.
            wants.retain(|w| !its(w));
            wants.push(hold(r.handle, r.pos, None));
        }
    }
    let burner = burners(view, world, party)
        .filter_map(|r| approach(world, r, e, back).map(|stand| (r, stand)))
        .min_by(|a, b| a.0.pos.distance(a.1).total_cmp(&b.0.pos.distance(b.1)));
    if let Some((r, stand)) = burner {
        wants.retain(|w| w.regiment != r.handle);
        wants.push(hold(r.handle, stand, (e.pos - stand).try_normalize()));
    }
}

/// From `from`, the first point walking `dir` in half tiles (at most
/// [`SMOKE_REACH`] tiles out) past which there is open ground at least
/// [`EDGE_DEPTH`] deep: the woods' edge that way, if any.
fn walk_out(world: &World, from: Vec2, dir: Vec2) -> Option<Vec2> {
    let at = |k: i32| from + dir * (k as f32 * 0.5);
    let deep = (EDGE_DEPTH * 2.0) as i32;
    (1..=(SMOKE_REACH * 2.0) as i32)
        .find(|&k| (k..=k + deep).all(|j| !burnable(world, at(j))))
        .map(at)
}

fn hold(regiment: RegimentHandle, to: Vec2, facing: Option<Vec2>) -> Want {
    Want {
        regiment,
        act: Act::Move {
            to,
            facing,
            pace: 0.0,
            hunt: false,
        },
        priority: SMOKE_PRIORITY,
        group: 0,
    }
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
