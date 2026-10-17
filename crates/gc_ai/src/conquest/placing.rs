//! Where a Conquest mind puts its buildings: near where it wants them, a
//! tile apart from its other buildings, and off the paths its workers walk
//! between its halls and their seeps and veins, so a base never walls in
//! its own workers or armies.
//!
//! Every spot is checked with [`World::can_lay`], the same check the
//! simulation makes when the order lands; the ground may still change in
//! the reaction time between (someone stands there), and then the Steward
//! simply tries again.

use gc_sim::structures::BuildingStats;
use gc_sim::{BuildingId, PlayerId, World};
use glam::Vec2;

use crate::perception::v2;

/// Tiles kept free round a building, so paths run between buildings.
pub const GAP: i32 = 1;
/// Half-width of the lane kept free from a hall to each of its nodes.
pub const LANE: f32 = 2.5;
/// How far out from `near` a spot is looked for, in tiles.
pub const REACH: i32 = 18;

/// Footprint `[x, y, width, depth]` in tiles.
pub type Foot = [i32; 4];

/// What the spot must keep clear of.
#[derive(Clone, Debug, Default)]
pub struct Keep {
    /// Lanes `(from, to)` in tiles: a hall's middle to a node it serves.
    pub lanes: Vec<(Vec2, Vec2)>,
    /// Footprints already taken or about to be (laid but not yet seen).
    pub taken: Vec<Foot>,
}

/// The north-west tile for a building of type `building` (`stats`) that
/// `party` could lay now, as near `near` as can be found within [`REACH`]
/// tiles, keeping [`GAP`] tiles from its own buildings and everything in
/// `keep`. `None` when nothing fits.
pub fn spot_for(
    world: &World,
    party: PlayerId,
    building: BuildingId,
    stats: &BuildingStats,
    near: Vec2,
    keep: &Keep,
) -> Option<(i32, i32)> {
    let (w, d) = (i32::from(stats.width), i32::from(stats.depth));
    let mut taken = keep.taken.clone();
    if let Some(s) = world.structures() {
        taken.extend(
            s.iter()
                .filter(|v| v.owner == party)
                .map(|v| [v.tile.0, v.tile.1, i32::from(v.width), i32::from(v.depth)]),
        );
    }
    let cx = (near.x - w as f32 / 2.0).round() as i32;
    let cy = (near.y - d as f32 / 2.0).round() as i32;
    for r in 0..=REACH {
        let mut best: Option<((i32, i32), f32)> = None;
        for t in ring(cx, cy, r) {
            let foot = [t.0, t.1, w, d];
            if taken.iter().any(|&f| overlaps(f, foot, GAP)) {
                continue;
            }
            if keep.lanes.iter().any(|&(a, b)| crosses(foot, a, b)) {
                continue;
            }
            if world.can_lay(party, building, t).is_err() {
                continue;
            }
            let mid = Vec2::new(t.0 as f32 + w as f32 / 2.0, t.1 as f32 + d as f32 / 2.0);
            let score = mid.distance(near);
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((t, score));
            }
        }
        if let Some((t, _)) = best {
            return Some(t);
        }
    }
    None
}

/// Tiles on the square ring `r` steps out from `(x, y)`.
fn ring(x: i32, y: i32, r: i32) -> Vec<(i32, i32)> {
    if r == 0 {
        return vec![(x, y)];
    }
    let mut out: Vec<(i32, i32)> = (-r..=r).flat_map(|i| [(x + i, y - r), (x + i, y + r)]).collect();
    out.extend((1 - r..r).flat_map(|i| [(x - r, y + i), (x + r, y + i)]));
    out
}

/// Whether footprints `a` and `b` come within `gap` tiles of each other.
pub fn overlaps(a: Foot, b: Foot, gap: i32) -> bool {
    a[0] < b[0] + b[2] + gap
        && b[0] < a[0] + a[2] + gap
        && a[1] < b[1] + b[3] + gap
        && b[1] < a[1] + a[3] + gap
}

/// Whether footprint `f` comes within [`LANE`] of the segment `a`-`b`.
fn crosses(f: Foot, a: Vec2, b: Vec2) -> bool {
    let min = Vec2::new(f[0] as f32, f[1] as f32);
    let max = min + Vec2::new(f[2] as f32, f[3] as f32);
    // Sample along the lane; lanes are short (a hall to its seeps).
    let steps = (a.distance(b) / 0.5).ceil().max(1.0) as i32;
    (0..=steps).any(|i| {
        let p = a.lerp(b, i as f32 / steps as f32);
        let q = p.clamp(min, max);
        q.distance(p) < LANE
    })
}

/// The middle of a structure's footprint, in tiles.
pub fn centre_of(v: &gc_sim::structures::StructureView) -> Vec2 {
    v2(v.centre())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_cover_each_tile_once() {
        let mut seen = std::collections::HashSet::new();
        for r in 0..4 {
            for t in ring(0, 0, r) {
                assert!(seen.insert(t), "{t:?} twice");
            }
        }
        assert_eq!(seen.len(), 49);
    }

    #[test]
    fn a_lane_keeps_buildings_off_it() {
        let lane = (Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0));
        assert!(crosses([4, -1, 2, 2], lane.0, lane.1));
        assert!(!crosses([4, 5, 2, 2], lane.0, lane.1));
    }
}
