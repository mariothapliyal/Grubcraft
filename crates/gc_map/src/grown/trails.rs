//! Worn trails (docs/27, "Worn trails"; docs/41 step 3).
//!
//! Once a grown map's sites stand, trails are worn between them: the first
//! start is the network, then each other start and each site in turn
//! wears the cheapest way to the nearest part of it. Ground a trail has
//! already worn costs a third of fresh ground, so later trails join
//! earlier ones and the trails grow into a network the way real paths do.
//! They keep off steep ground (the walking costs say so), cross rivers
//! where they can be waded, and go round woods, bogs and thicket where
//! they can. A worn tile in the open shows as [`Habitat::Trail`] and its
//! tall grass is trodden flat; under trees or on wet ground the trail is
//! hidden and nothing changes. A trail is two tiles wide, so it shows
//! as a track and not a thread.
//!
//! Lairs get no trail: no one walks to one.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::Habitat;
use super::field::{ONE, value_noise};
use crate::def::Kind;
use crate::def::Land;
use crate::{Ground, Map};
use gc_sim::Cover;

/// What fresh ground costs over worn ground.
const FRESH: u32 = 3;
/// What a tile of woods, scrub or bog costs over open ground.
const ROUGH: u32 = 4;
/// How many tiles apart the points of [`wander`] are.
const WANDER: usize = 9;

/// Wear the trails into `map` between its starts and sites. On a mirrored
/// map each worn tile's turned copy is worn too, so the trails stay their
/// own turned copy.
pub(crate) fn wear(map: &mut Map, seed: u64, mirrored: bool) {
    let n = map.tiles as usize;
    if map.habitat.len() != n * n || map.cover.len() != n * n {
        return;
    }
    let mut ends: Vec<usize> = map
        .objects
        .iter()
        .filter(|o| match o.kind {
            Kind::Start | Kind::Village | Kind::Town | Kind::Bazaar | Kind::Shrine => true,
            Kind::Heart => true,
            Kind::CreepCamp => o.label != "The lair",
            _ => false,
        })
        .filter(|o| (0..n as i32).contains(&o.at.0) && (0..n as i32).contains(&o.at.1))
        .map(|o| o.at.1 as usize * n + o.at.0 as usize)
        // Mirrored land: only one half's ends wear trails, and the other
        // half gets their turned copy below.
        .filter(|&i| !mirrored || i <= n * n - 1 - i)
        .collect();
    if mirrored && let Some(&first) = ends.first() {
        // One trail from the turned copy of the first end joins the halves.
        ends.push(n * n - 1 - first);
    }
    let Some((&first, rest)) = ends.split_first() else {
        return;
    };
    let step = step_costs(map, seed as u32);
    let mut worn = vec![false; n * n];
    worn[first] = true;
    let mut search = Search::new(n);
    for &from in rest {
        if worn[from] {
            continue;
        }
        if let Some(path) = search.way_to_worn(&step, &worn, from) {
            for i in path {
                worn[i] = true;
            }
        }
    }
    // Two tiles wide: each worn tile and the ones east and south of it.
    let wide = |i: usize| {
        let (x, y) = (i % n, i / n);
        worn[i] || (x > 0 && worn[i - 1]) || (y > 0 && worn[i - n])
    };
    let last = n * n - 1;
    for i in 0..n * n {
        if wide(i) || (mirrored && wide(last - i)) {
            tread(map, i);
        }
    }
}

/// What stepping onto each tile costs (0: can't), from the worst of its
/// walkable-ground cells, rougher through woods, scrub and wet ground.
fn step_costs(map: &Map, seed: u32) -> Vec<u32> {
    let n = map.tiles as usize;
    let ground = map.tile_ground(&map.nav_costs());
    (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            let c = match ground[i] {
                Ground::Blocked => return 0,
                Ground::Open => 1,
                Ground::Steep => 2,
                Ground::Ford => 3,
            };
            let rough = if map.cover[i] == Cover::Wood as u8 || soft(map, i) {
                ROUGH
            } else {
                1
            };
            c * rough * wander(seed, x, y)
        })
        .collect()
}

/// A slow wobble in what fresh ground costs, from 6 to 13 (about 1 to 2
/// over its 6), so trails wander as real paths do instead of running
/// dead straight: value noise with a point every [`WANDER`] tiles.
fn wander(seed: u32, x: usize, y: usize) -> u32 {
    let at = |v: usize| (v as i64) * i64::from(ONE) / WANDER as i64;
    6 + value_noise(seed, at(x), at(y)) as u32 * 8 / 65_536
}

/// Ground a trail would be lost in: soft ground (wet woodland and bog).
fn soft(map: &Map, i: usize) -> bool {
    map.footing.get(i) == Some(&(gc_sim::Footing::Soft as u8))
}

/// One Dijkstra's buffers, kept from one trail to the next: only the
/// tiles a search reached are reset after it.
struct Search {
    n: usize,
    cost: Vec<u32>,
    back: Vec<usize>,
    reached: Vec<usize>,
}

impl Search {
    fn new(n: usize) -> Search {
        Search {
            n,
            cost: vec![u32::MAX; n * n],
            back: vec![usize::MAX; n * n],
            reached: Vec::new(),
        }
    }

    /// The cheapest way from `from` to any worn tile, the tiles on it
    /// (worn ones too), or `None` if no worn tile can be reached.
    fn way_to_worn(&mut self, step: &[u32], worn: &[bool], from: usize) -> Option<Vec<usize>> {
        for &i in &self.reached {
            self.cost[i] = u32::MAX;
        }
        self.reached.clear();
        let n = self.n;
        let mut heap = BinaryHeap::new();
        self.cost[from] = 0;
        self.reached.push(from);
        heap.push(Reverse((0u32, from)));
        while let Some(Reverse((c, i))) = heap.pop() {
            if c > self.cost[i] {
                continue;
            }
            if worn[i] {
                let mut path = vec![i];
                let mut at = i;
                while at != from {
                    at = self.back[at];
                    path.push(at);
                }
                return Some(path);
            }
            let (x, y) = ((i % n) as i32, (i / n) as i32);
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let (nx, ny) = (x + dx, y + dy);
                if !(0..n as i32).contains(&nx) || !(0..n as i32).contains(&ny) {
                    continue;
                }
                let j = ny as usize * n + nx as usize;
                // No squeezing diagonally between two tiles that can't be
                // walked.
                let side_a = y as usize * n + nx as usize;
                let side_b = ny as usize * n + x as usize;
                if step[j] == 0 || step[side_a] == 0 || step[side_b] == 0 {
                    continue;
                }
                let fresh = if worn[j] { 1 } else { FRESH };
                // A diagonal step is about 1.4 of a straight one.
                let len = if dx != 0 && dy != 0 { 7 } else { 5 };
                let next = c + step[j] * fresh * len;
                if next < self.cost[j] {
                    if self.cost[j] == u32::MAX {
                        self.reached.push(j);
                    }
                    self.cost[j] = next;
                    self.back[j] = i;
                    heap.push(Reverse((next, j)));
                }
            }
        }
        None
    }
}

/// Wear one tile: open ground off the water shows the trail and loses its
/// tall grass; woods, wet ground and water hide it.
fn tread(map: &mut Map, i: usize) {
    let water = map.heights[i] <= map.water || map.flow.get(i).is_some_and(|&f| f != 0);
    if water || map.cover[i] == Cover::Wood as u8 || soft(map, i) {
        return;
    }
    map.habitat[i] = Habitat::Trail as u8;
    map.cover[i] = Cover::Open as u8;
    if let Some(land) = map.land.get_mut(i) {
        // A river valley stays a valley; elsewhere the trail is meadow.
        if *land != Land::Valley as u8 {
            *land = Habitat::Trail.land() as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grown::{Fairness, describe};
    use crate::wilds::Wildness;

    fn trail_map(fairness: Fairness) -> Map {
        Map::from_def(&describe(256, 3, Wildness::Mixed, fairness))
    }

    /// Every start and every site but the lairs is on the network: a worn
    /// tile lies within a few tiles of each.
    #[test]
    fn trails_reach_the_starts_and_sites() {
        let map = trail_map(Fairness::Measured);
        let n = map.tiles as i32;
        let trail = Habitat::Trail as u8;
        let worn = |p: (i32, i32)| {
            (-6..=6).any(|dy| {
                (-6..=6).any(|dx| {
                    let (x, y) = (p.0 + dx, p.1 + dy);
                    (0..n).contains(&x)
                        && (0..n).contains(&y)
                        && map.habitat[(y * n + x) as usize] == trail
                })
            })
        };
        let mut ends = 0;
        for o in &map.objects {
            let end = matches!(o.kind, Kind::Start | Kind::Village | Kind::Shrine)
                || (o.kind == Kind::CreepCamp && o.label != "The lair");
            if end {
                ends += 1;
                assert!(
                    worn(o.at),
                    "{:?} {} at {:?} has no trail",
                    o.kind,
                    o.label,
                    o.at
                );
            }
        }
        assert!(ends >= 6);
        let share =
            map.habitat.iter().filter(|&&h| h == trail).count() as f64 / map.habitat.len() as f64;
        assert!(share > 0.002 && share < 0.03, "trail share {share}");
    }

    /// A trail tile is open ground, never water, woods or bog.
    #[test]
    fn trails_lie_on_open_dry_ground() {
        let map = trail_map(Fairness::Measured);
        for i in 0..map.habitat.len() {
            if map.habitat[i] == Habitat::Trail as u8 {
                assert_eq!(map.cover[i], Cover::Open as u8);
                assert!(map.heights[i] > map.water);
            }
        }
    }

    /// On mirrored land the trails are their own turned copy.
    #[test]
    fn mirrored_trails_are_their_own_turned_copy() {
        let map = trail_map(Fairness::Mirrored);
        let last = map.habitat.len() - 1;
        let trail = Habitat::Trail as u8;
        for i in 0..map.habitat.len() {
            assert_eq!(
                map.habitat[i] == trail,
                map.habitat[last - i] == trail,
                "tile {i}"
            );
        }
    }
}
