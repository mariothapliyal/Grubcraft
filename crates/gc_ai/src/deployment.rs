//! Where an army stands when a Field Battle begins, worked out from the
//! ground: the standard deployment (line in front, archers behind, fast
//! troops on the wings) is tried where the start is, then shifted, turned a
//! little and folded into more ranks until every regiment stands on open
//! ground on the map; any regiment that still doesn't is moved to the
//! nearest spot that is. Where the standard deployment already fits it is
//! used as it is.

use std::sync::LazyLock;

use gc_sim::nav::OPEN;
use gc_sim::{Cover, Formation, UnitId, UnitStats};
use glam::Vec2;

use crate::army::{GAP, Placement, ROW_GAP};
use crate::perception::{Class, fx, right_of};
use crate::terrain::Terrain;

/// Tiles a regiment's block keeps from the edge of the map.
const EDGE: f32 = 4.0;
/// Most ranks of line regiments a deployment is folded into.
const MAX_RANKS: usize = 3;
/// Cost of a regiment that can't stand where the layout puts it, or stands
/// on another.
const MISFIT: f32 = 40.0;
/// Costs of going to a place: each tile the anchor moves, each degree it
/// turns, each rank beyond the first.
const PER_TILE: f32 = 0.6;
const PER_DEGREE: f32 = 0.15;
const PER_RANK: f32 = 4.0;
/// Cost per tile the anchor stands short of [`EDGE_ROOM`] from the map's
/// edge, so the deployment zone's ring (24 tiles at least) stays on the map
/// when there is room.
const PER_EDGE: f32 = 1.0;
const EDGE_ROOM: f32 = 34.0;
/// Costs of standing on ground that is bad but passable, per sample.
const SLOW: f32 = 1.0;
const WOODED: f32 = 1.5;
const SOFT: f32 = 1.0;
/// How far a misplaced regiment looks for a spot, tiles.
const REPAIR_REACH: i32 = 14;
/// Tiles apart that two regiments keep.
const SPACE: f32 = 1.0;
/// The turns tried, degrees, the anchor's distances from the start, tiles,
/// and the directions round it.
const TURNS: [f32; 5] = [0.0, 20.0, -20.0, 40.0, -40.0];
const RINGS: [f32; 9] = [3.0, 6.0, 9.0, 12.0, 16.0, 20.0, 25.0, 32.0, 40.0];
const BEARINGS: i32 = 12;

/// A regiment's place in a layout: its block's centre across and back from
/// the anchor, and the block's size, tiles.
#[derive(Clone, Copy)]
struct Slot {
    unit: UnitId,
    across: f32,
    back: f32,
    width: f32,
    depth: f32,
}

/// What [`deploy_clear_of`] decided.
pub struct Deployment {
    pub placed: Vec<Placement>,
    /// The middle of the deployment: the start, unless that ground wouldn't
    /// hold the army.
    pub anchor: Vec2,
}

/// The standard deployment in `ranks` ranks of line regiments.
fn layout(units: &[UnitStats], army: &[UnitId], ranks: usize) -> Vec<Slot> {
    let size = |u: UnitId| block(units, u);
    let of_class = |c: Class| -> Vec<UnitId> {
        army.iter()
            .copied()
            .filter(|&u| Class::of(&units[u as usize]) == c)
            .collect()
    };
    let mut line = of_class(Class::Line);
    let mut shock = of_class(Class::Shock);
    let ranged = of_class(Class::Ranged);
    // All fast troops: the middle ones stand in the line.
    if line.is_empty() && shock.len() >= 3 {
        let wings = shock.len() / 3;
        line = shock.split_off(wings);
    }
    let row_width = |row: &[UnitId]| {
        row.iter().map(|&u| size(u).0 + GAP).sum::<f32>() - GAP.min(row.len() as f32 * GAP)
    };
    let line_depth = line.iter().map(|&u| size(u).1).fold(0.0, f32::max);
    let ranks = ranks.clamp(1, line.len().max(1));
    let mut out = Vec::with_capacity(army.len());
    let mut slot = |unit: UnitId, across: f32, back: f32| {
        let (width, depth) = size(unit);
        out.push(Slot {
            unit,
            across,
            back,
            width,
            depth,
        });
    };
    // The line, a rank at a time, each rank a share of every kind.
    let mut front_width = 0.0;
    for k in 0..ranks {
        let rank: Vec<UnitId> = line.iter().copied().skip(k).step_by(ranks).collect();
        let width = row_width(&rank);
        if k == 0 {
            front_width = width;
        }
        let mut x = -width * 0.5;
        for &u in &rank {
            let w = size(u).0;
            slot(u, x + w * 0.5, k as f32 * (line_depth + ROW_GAP));
            x += w + GAP;
        }
    }
    // Archers behind the last rank.
    let behind = (ranks - 1) as f32 * (line_depth + ROW_GAP) + line_depth * 0.5 + ROW_GAP;
    let mut x = -row_width(&ranged) * 0.5;
    for &u in &ranged {
        let (w, d) = size(u);
        slot(u, x + w * 0.5, behind + d * 0.5);
        x += w + GAP;
    }
    // Fast troops on the wings, alternating sides from the line outward.
    let (mut left, mut right) = (
        -front_width * 0.5 - GAP * 2.0,
        front_width * 0.5 + GAP * 2.0,
    );
    for (k, &u) in shock.iter().enumerate() {
        let w = size(u).0;
        if k % 2 == 0 {
            slot(u, right + w * 0.5, 0.0);
            right += w + GAP;
        } else {
            slot(u, left - w * 0.5, 0.0);
            left -= w + GAP;
        }
    }
    out
}

/// What standing with the zone's ring over the map's edge costs.
fn edge_cost(terrain: &Terrain, anchor: Vec2) -> f32 {
    let n = terrain.map_tiles() as f32;
    if n == 0.0 {
        return 0.0;
    }
    let gap = anchor.x.min(anchor.y).min(n - anchor.x).min(n - anchor.y);
    (EDGE_ROOM - gap).max(0.0) * PER_EDGE
}

/// Unit `u`'s block in a line, across and deep, tiles.
fn block(units: &[UnitStats], u: UnitId) -> (f32, f32) {
    let s = &units[u as usize];
    let (w, d) = s.block_size_in(Formation::Line, s.files, s.models);
    (fx(w), fx(d))
}

/// A layout set down: where it stands, facing which way.
#[derive(Clone, Copy)]
struct Frame {
    anchor: Vec2,
    forward: Vec2,
}

impl Frame {
    fn right(self) -> Vec2 {
        right_of(self.forward)
    }

    fn at(self, s: &Slot) -> Vec2 {
        self.anchor + self.right() * s.across - self.forward * s.back
    }
}

/// How well the ground holds a block of `width` by `depth` tiles centred on
/// `p` facing `forward`: `None` if any of it is off the map, in water, on
/// cliffs or cut off from the start's own ground, else a cost for rough
/// going. Sampled every few tiles over the block.
fn footing(
    terrain: &Terrain,
    joined: Option<&[bool]>,
    p: Vec2,
    forward: Vec2,
    (width, depth): (f32, f32),
) -> Option<f32> {
    let right = right_of(forward);
    let across = (width / 4.0).ceil().max(1.0) as i32;
    let deep = (depth / 4.0).ceil().max(1.0) as i32;
    let mut cost = 0.0;
    for i in 0..=across {
        for j in 0..=deep {
            let a = (i as f32 / across as f32 - 0.5) * width;
            let b = (j as f32 / deep as f32 - 0.5) * depth;
            let q = p + right * a + forward * b;
            if !terrain.walkable(q)
                || !terrain.inside(q, EDGE)
                || terrain.water(q)
                || joined.is_some_and(|j| !terrain.is_joined(j, q))
            {
                return None;
            }
            if terrain.cost(q) > OPEN {
                cost += SLOW;
            }
            if terrain.cover(q) == Cover::Wood {
                cost += WOODED;
            }
            if terrain.soft(q) {
                cost += SOFT;
            }
        }
    }
    Some(cost)
}

/// A block that stands where it is: its centre and half extents, as seen
/// from a frame (taken regiments are read as facing the way it does).
struct Held {
    at: Vec2,
    half: Vec2,
}

fn held_by(units: &[UnitStats], p: &Placement) -> Held {
    let (w, d) = block(units, p.unit);
    Held {
        at: p.position,
        half: Vec2::new(w, d) * 0.5,
    }
}

/// Whether blocks `a` and `b` (centre, half extents across and deep) in
/// `frame` come within [`SPACE`] of each other.
fn crowd(frame: Frame, a: (Vec2, Vec2), b: (Vec2, Vec2)) -> bool {
    let d = a.0 - b.0;
    let across = d.dot(frame.right()).abs();
    let deep = d.dot(frame.forward).abs();
    across < a.1.x + b.1.x + SPACE && deep < a.1.y + b.1.y + SPACE
}

/// Cost of `slots` in `frame`: misfits and rough ground, summed.
fn cost_of(
    terrain: &Terrain,
    joined: Option<&[bool]>,
    frame: Frame,
    slots: &[Slot],
    taken: &[Held],
) -> f32 {
    slots
        .iter()
        .map(|s| {
            let p = frame.at(s);
            let half = Vec2::new(s.width, s.depth) * 0.5;
            let hit = taken
                .iter()
                .any(|t| crowd(frame, (p, half), (t.at, t.half)));
            let ground = footing(terrain, joined, p, frame.forward, (s.width, s.depth));
            let overlap = if hit { MISFIT } else { 0.0 };
            overlap + ground.unwrap_or(MISFIT)
        })
        .sum()
}

/// [`deploy`](crate::army::deploy) with the regiments already `taken` (by an
/// earlier rank, say) left room.
pub fn deploy_clear_of(
    units: &[UnitStats],
    army: &[UnitId],
    centre: Vec2,
    forward: Vec2,
    terrain: &Terrain,
    taken: &[Placement],
) -> Deployment {
    let forward = forward.normalize_or(Vec2::Y);
    let held: Vec<Held> = taken.iter().map(|p| held_by(units, p)).collect();
    // Only ground joined to the start's own counts: an island is no place
    // for an army.
    let joined = terrain.ground_of(centre);
    let joined = joined.as_deref();
    let layouts: Vec<Vec<Slot>> = (1..=MAX_RANKS).map(|r| layout(units, army, r)).collect();

    // Cheapest first try wins ties: the start as it is, then each rank
    // count in turn over every nearby anchor and turn.
    let mut best = (
        f32::INFINITY,
        0,
        Frame {
            anchor: centre,
            forward,
        },
    );
    'search: for (ranks, slots) in layouts.iter().enumerate() {
        let folded = ranks as f32 * PER_RANK;
        for ring in std::iter::once(0.0).chain(RINGS) {
            // Moving further can't pay once the move alone costs more.
            if ring * PER_TILE + folded >= best.0 {
                break;
            }
            // A ring of nothing is the start itself.
            let bearings = if ring == 0.0 { 1 } else { BEARINGS };
            for k in 0..bearings {
                let bearing = Vec2::from_angle(k as f32 / BEARINGS as f32 * std::f32::consts::TAU);
                let anchor = centre + bearing * ring;
                for turn in TURNS {
                    let frame = Frame {
                        anchor,
                        forward: Vec2::from_angle(turn.to_radians()).rotate(forward),
                    };
                    let c = ring * PER_TILE
                        + edge_cost(terrain, anchor)
                        + turn.abs() * PER_DEGREE
                        + folded
                        + cost_of(terrain, joined, frame, slots, &held);
                    if c < best.0 {
                        best = (c, ranks, frame);
                        if c == 0.0 {
                            break 'search;
                        }
                    }
                }
            }
        }
    }
    let (_, ranks, frame) = best;
    let slots = &layouts[ranks];
    let mut spots: Vec<Vec2> = slots.iter().map(|s| frame.at(s)).collect();
    // Whoever still can't stand where the layout puts them, or stands on a
    // neighbour, goes to the nearest spot that will hold them.
    for i in 0..slots.len() {
        let s = &slots[i];
        let half = Vec2::new(s.width, s.depth) * 0.5;
        let others = |p: Vec2, spots: &[Vec2]| {
            held.iter().any(|t| crowd(frame, (p, half), (t.at, t.half)))
                || spots.iter().enumerate().any(|(j, &q)| {
                    let o = &slots[j];
                    j != i && crowd(frame, (p, half), (q, Vec2::new(o.width, o.depth) * 0.5))
                })
        };
        let stands = |p: Vec2| footing(terrain, joined, p, frame.forward, (s.width, s.depth));
        if stands(spots[i]).is_some() && !others(spots[i], &spots) {
            continue;
        }
        let found = NUDGES.iter().find_map(|&(dx, dy)| {
            let p = spots[i] + frame.right() * dx as f32 + frame.forward * dy as f32;
            (stands(p).is_some() && !others(p, &spots)).then_some(p)
        });
        spots[i] = found.unwrap_or_else(|| {
            terrain
                .nearest_walkable(spots[i], REPAIR_REACH as f32)
                .unwrap_or_else(|| terrain.clamp(spots[i], EDGE))
        });
    }
    Deployment {
        placed: slots
            .iter()
            .zip(spots)
            .map(|(s, position)| Placement {
                unit: s.unit,
                position,
                facing: frame.forward,
            })
            .collect(),
        anchor: frame.anchor,
    }
}

/// Whole-tile steps within [`REPAIR_REACH`], nearest first.
static NUDGES: LazyLock<Vec<(i32, i32)>> = LazyLock::new(|| {
    let r = REPAIR_REACH;
    let mut all: Vec<(i32, i32)> = (-r..=r)
        .flat_map(|x| (-r..=r).map(move |y| (x, y)))
        .filter(|&(x, y)| x * x + y * y <= r * r && (x, y) != (0, 0))
        .collect();
    all.sort_by_key(|&(x, y)| (x * x + y * y, x, y));
    all
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::army::deploy;
    use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
    use gc_sim::scenario::DRILL_UNITS;

    /// A map `tiles` across whose tile (x, y) is open when `land` says so.
    fn map(tiles: u32, land: impl Fn(i32, i32) -> bool) -> Terrain {
        let side = tiles as i32 * CELLS_PER_TILE;
        let nav: Vec<u8> = (0..side * side)
            .map(|c| {
                let (x, y) = (c % side / CELLS_PER_TILE, c / side / CELLS_PER_TILE);
                if land(x, y) { OPEN } else { BLOCKED }
            })
            .collect();
        Terrain::new(&nav, tiles)
    }

    fn army() -> Vec<UnitId> {
        vec![0, 0, 0, 0, 0, 0, 1, 1, 1, 2, 2, 2]
    }

    /// Every regiment's block stands on ground, apart from the others.
    fn assert_placed(t: &Terrain, placed: &[Placement]) {
        assert_eq!(placed.len(), army().len());
        for (i, a) in placed.iter().enumerate() {
            assert!(t.walkable(a.position) && t.inside(a.position, 3.0), "{a:?}");
            for b in &placed[i + 1..] {
                assert!(a.position.distance(b.position) > 4.0, "{a:?} {b:?}");
            }
        }
    }

    #[test]
    fn a_start_on_open_ground_keeps_the_standard_deployment() {
        let t = map(128, |_, _| true);
        let centre = Vec2::new(60.0, 60.0);
        let at = deploy_clear_of(&DRILL_UNITS, &army(), centre, Vec2::NEG_Y, &t, &[]);
        assert_eq!(at.anchor, centre);
        assert!(at.placed.iter().all(|p| p.facing == Vec2::NEG_Y));
        assert_placed(&t, &at.placed);
    }

    #[test]
    fn a_start_on_the_map_edge_moves_in() {
        let t = map(128, |_, _| true);
        for corner in [(1.0, 1.0), (127.0, 1.0), (1.0, 127.0), (64.0, 0.5)] {
            let centre = Vec2::new(corner.0, corner.1);
            let at = deploy_clear_of(&DRILL_UNITS, &army(), centre, Vec2::NEG_Y, &t, &[]);
            assert_placed(&t, &at.placed);
            // The zone's ring comes in off the edge too.
            let gap = at
                .anchor
                .x
                .min(at.anchor.y)
                .min(128.0 - at.anchor.x)
                .min(128.0 - at.anchor.y);
            assert!(gap > 12.0, "{corner:?}: {gap}");
        }
    }

    #[test]
    fn a_narrow_strip_of_land_folds_the_line_instead_of_running_off_it() {
        // A coast 24 tiles wide, running north to south, then the sea.
        let t = map(160, |x, _| (20..44).contains(&x));
        let centre = Vec2::new(32.0, 80.0);
        let placed = deploy(&DRILL_UNITS, &army(), centre, Vec2::new(1.0, 0.0), &t);
        assert_placed(&t, &placed);
        // The ideal line is wider than the coast.
        let width = layout(&DRILL_UNITS, &army(), 1)
            .iter()
            .map(|s| s.across.abs() + s.width * 0.5)
            .fold(0.0, f32::max);
        assert!(width * 2.0 > 24.0, "{width}");
    }

    #[test]
    fn an_island_is_no_place_for_an_army() {
        // The start is on a pocket of land the rest of the map is cut off from by cliffs...
        let t = map(160, |x, y| {
            !(70..=74).contains(&x) || !(60..100).contains(&y) && y < 130 || y >= 130 && x < 70
        });
        let centre = Vec2::new(40.0, 80.0);
        let placed = deploy(&DRILL_UNITS, &army(), centre, Vec2::X, &t);
        let joined = t.ground_of(centre).unwrap();
        assert_placed(&t, &placed);
        assert!(placed.iter().all(|p| t.is_joined(&joined, p.position)));
    }

    #[test]
    fn thicket_and_cliffs_are_steered_round() {
        // Impassable thicket across the start, open land either side of it.
        let t = map(160, |x, y| {
            !((60..100).contains(&x) && (70..90).contains(&y))
        });
        let centre = Vec2::new(80.0, 80.0);
        let placed = deploy(&DRILL_UNITS, &army(), centre, Vec2::NEG_Y, &t);
        assert_placed(&t, &placed);
    }

    #[test]
    fn deployments_are_the_same_every_time() {
        let t = map(128, |x, _| x < 90);
        let centre = Vec2::new(88.0, 64.0);
        let a = deploy(&DRILL_UNITS, &army(), centre, Vec2::X, &t);
        let b = deploy(&DRILL_UNITS, &army(), centre, Vec2::X, &t);
        assert_eq!(a, b);
    }

    #[test]
    fn a_rank_stays_clear_of_the_one_before() {
        let t = map(160, |x, _| (50..90).contains(&x));
        let centre = Vec2::new(70.0, 100.0);
        let first = deploy_clear_of(&DRILL_UNITS, &army(), centre, Vec2::NEG_Y, &t, &[]);
        let second = deploy_clear_of(
            &DRILL_UNITS,
            &army(),
            centre + Vec2::Y * 18.0,
            Vec2::NEG_Y,
            &t,
            &first.placed,
        );
        for a in &first.placed {
            for b in &second.placed {
                assert!(a.position.distance(b.position) > 3.0, "{a:?} {b:?}");
            }
        }
    }
}
