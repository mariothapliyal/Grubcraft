//! Measured fairness (docs/27-living-land.md, stage 3): where the starts
//! stand on land that is not its own turned copy. Each start keeps to a
//! window round its plateau (so north is still north and the middles are
//! still the middles), and within those windows a search on a coarse grid
//! moves them, one at a time, to where the walks to the enemy are as equal
//! as the land allows, for every way the lobby hands the starts out.
//!
//! It runs twice. First on the land's shape before the water and the
//! woods ([`place`]), so the clearings, the open ground round the starts
//! and the rivers all grow round the starts it chose; then on the finished
//! land by its true walks, as the nav rules walk it ([`settle`]), to even
//! out what the rivers, woods and passes changed. `gc_mapcheck` measures
//! the finished map.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use super::field::{Grid, ONE};
use super::scaled;
use crate::def::Pos;
use crate::generate::START_PLATEAUS;

/// Ground rising faster than this between two tiles is too steep to march
/// up for the search (a little under the nav rules' steep).
const STEEP: i32 = ONE * 6 / 10;
/// A coarse cell this full of steep tiles (in sixteenths) can't be
/// crossed.
const SHUT: usize = 9;
/// Sweeps over the six starts at most.
const SWEEPS: usize = 3;

/// A way the lobby hands the starts out (see `gc_match::battle_starts`):
/// each start's team, or [`OUT`] for a start no one plays.
type Handout = [u8; START_PLATEAUS];

/// The walks between the starts (up to six), each way.
pub(super) type Walks = [[u32; START_PLATEAUS]; START_PLATEAUS];

/// A start no one plays in a [`Handout`].
const OUT: u8 = u8::MAX;

/// The ways the lobby hands out the six slots (as [`crate::plateaus`]
/// lists them: north row 0, 1 and 4, south row 2, 3 and 5): two against
/// two on the corners, three against three, four and six for themselves,
/// and two against two against two in columns (0 and 2, 1 and 3, 4 and
/// 5).
const SIX: [Handout; 5] = [
    [0, 0, 1, 1, OUT, OUT],
    [0, 0, 1, 1, 0, 1],
    [0, 1, 2, 3, OUT, OUT],
    [0, 1, 2, 3, 4, 5],
    [0, 1, 0, 1, 2, 2],
];

/// Every way the lobby hands out `k` starts: [`SIX`] for the six slots;
/// for a ring of three or five ([`ring`]), everyone for themselves. (Two
/// teams on a ring stand as neighbours, so the middle of a team of three
/// is always further from the enemy than its ends: that is the shape,
/// not unfairness to search away. One against two is fair when every
/// start's nearest enemy is.)
fn handouts(k: usize) -> impl Iterator<Item = Handout> {
    let six = (k == START_PLATEAUS).then_some(SIX).into_iter().flatten();
    let ring = (k != START_PLATEAUS).then(|| {
        let mut teams = [OUT; START_PLATEAUS];
        for (i, t) in teams.iter_mut().enumerate().take(k) {
            *t = i as u8;
        }
        teams
    });
    six.chain(ring)
}

/// Where `k` starts stand round the middle of an `n`-tile map when the
/// lobby has three or five players (docs/27, 3 and 5 players): evenly
/// round a ring as far out as the six slots' rows, turned by `seed`, the
/// first the one furthest south (where this machine's player starts) and
/// the rest going round from it.
pub(super) fn ring(n: usize, k: usize, seed: u64) -> Vec<Pos> {
    let n = n as i32;
    let inset = if n >= crate::def::BIG_MAP_TILES {
        n / 6
    } else {
        n / 4
    };
    let (middle, radius) = (n / 2, n / 2 - inset);
    let step = (1u32 << 16) / k as u32;
    // A turn of less than half a step either way from due south (+y), so
    // the first start is still the one furthest south.
    let turn = (seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 40) as u32 % step;
    let first = (gc_fixed::Angle::QUARTER.0 as u32 + turn).wrapping_sub(step / 2);
    (0..k as u32)
        .map(|i| {
            let a = gc_fixed::Angle((first + i * step) as u16);
            let along = |v: gc_fixed::Fx| (i64::from(radius) * i64::from(v.raw()) >> 16) as i32;
            (middle + along(a.cos()), middle + along(a.sin()))
        })
        .collect()
}

/// The coarse grid the search walks: `c` tiles a cell, `m` cells a side.
struct Coarse {
    c: usize,
    m: usize,
    /// Each cell's height (at its middle tile).
    height: Vec<i32>,
    /// Whether each cell can be crossed, and how rough it is (steep
    /// sixteenths).
    open: Vec<bool>,
    rough: Vec<u32>,
}

impl Coarse {
    /// The grid of `c`-tile cells over `h`, with the tiles in `steep`
    /// counted as ground that can't be crossed.
    fn new(h: &Grid, c: usize, steep: &[bool]) -> Coarse {
        let n = h.n;
        let m = n / c;
        let mut height = Vec::with_capacity(m * m);
        let mut open = Vec::with_capacity(m * m);
        let mut rough = Vec::with_capacity(m * m);
        for cy in 0..m {
            for cx in 0..m {
                let (x0, y0) = (cx * c, cy * c);
                height.push(h.v[(y0 + c / 2) * n + x0 + c / 2]);
                let count = (y0..y0 + c)
                    .flat_map(|y| (x0..x0 + c).map(move |x| y * n + x))
                    .filter(|&i| steep[i])
                    .count();
                let sixteenths = count * 16 / (c * c);
                let edge = cx == 0 || cy == 0 || cx + 1 == m || cy + 1 == m;
                open.push(!edge && sixteenths < SHUT);
                rough.push(sixteenths as u32);
            }
        }
        Coarse {
            c,
            m,
            height,
            open,
            rough,
        }
    }

    /// The cell a tile is in.
    fn cell(&self, at: Pos) -> usize {
        let clamp = |v: i32| (v.max(0) as usize / self.c).min(self.m - 1);
        clamp(at.1) * self.m + clamp(at.0)
    }

    /// A cell's middle tile.
    fn tile(&self, cell: usize) -> Pos {
        let (cx, cy) = (cell % self.m, cell / self.m);
        (
            (cx * self.c + self.c / 2) as i32,
            (cy * self.c + self.c / 2) as i32,
        )
    }

    /// The walk from `from` to each of `to`, eight ways over open cells
    /// (ten a straight step, fourteen a diagonal, more over rough ground
    /// and for every tile of height climbed or dropped); `u32::MAX` for
    /// one it can't reach.
    fn walks(&self, from: usize, to: &[usize]) -> Vec<u32> {
        let m = self.m as i64;
        let mut dist = vec![u32::MAX; self.m * self.m];
        let mut heap = BinaryHeap::new();
        dist[from] = 0;
        heap.push(Reverse((0u32, from as u32)));
        let mut left = to.iter().filter(|&&t| t != from).count();
        while let Some(Reverse((d, i))) = heap.pop() {
            let i = i as usize;
            if d > dist[i] {
                continue;
            }
            if i != from && to.contains(&i) {
                left -= 1;
                if left == 0 {
                    break;
                }
            }
            let (x, y) = ((i % self.m) as i64, (i / self.m) as i64);
            for (dx, dy) in [
                (-1, 0),
                (1, 0),
                (0, -1),
                (0, 1),
                (-1, -1),
                (1, -1),
                (-1, 1),
                (1, 1),
            ] {
                let (px, py) = (x + dx, y + dy);
                if !(0..m).contains(&px) || !(0..m).contains(&py) {
                    continue;
                }
                let j = (py * m + px) as usize;
                if !self.open[j] {
                    continue;
                }
                let climb = (self.height[j] - self.height[i]).abs();
                // Too steep to go straight up, cell to cell.
                if climb > STEEP * self.c as i32 {
                    continue;
                }
                let base: u32 = if dx != 0 && dy != 0 { 14 } else { 10 };
                let cost = base * (16 + self.rough[j]) / 16
                    + (i64::from(climb) * 20 / i64::from(ONE)) as u32;
                let nd = d + cost;
                if nd < dist[j] {
                    dist[j] = nd;
                    heap.push(Reverse((nd, j as u32)));
                }
            }
        }
        to.iter()
            .map(|&t| if t == from { 0 } else { dist[t] })
            .collect()
    }
}

/// How unequal a set of walks is, in thousandths: (most - least) / mean.
/// Any walk that can't be made counts as very unequal.
fn spread(values: impl Iterator<Item = u32>) -> u32 {
    let v: Vec<u32> = values.collect();
    if v.contains(&u32::MAX) {
        return 100_000;
    }
    let (lo, hi) = (
        v.iter().copied().min().unwrap_or(0),
        v.iter().copied().max().unwrap_or(0),
    );
    let mean = (v.iter().map(|&d| u64::from(d)).sum::<u64>() / v.len().max(1) as u64).max(1);
    (u64::from(hi - lo) * 1000 / mean) as u32
}

/// The worst spread, in thousandths, over every way the lobby hands the
/// starts out ([`handouts`]): each start's walk to its nearest enemy,
/// and, between teams of the same size, the longest walk between two
/// allies (so help is as far away on every team), at half weight: allies'
/// walks may be twice as far apart as the enemies' before they count the
/// same, since an army reaching a foe a minute sooner matters more than
/// help arriving a minute later.
pub(super) fn unfairness(walk: &Walks, k: usize) -> u32 {
    handouts(k)
        .map(|teams| {
            let playing = |i: &usize| teams[*i] != OUT;
            let nearest = spread((0..k).filter(playing).map(|i| {
                ((0..k).filter(playing))
                    .filter(|&j| teams[j] != teams[i])
                    .map(|j| walk[i][j])
                    .min()
                    .unwrap_or(u32::MAX)
            }));
            // Each team's size and longest walk between two allies.
            let (mut size, mut longest) = ([0usize; START_PLATEAUS], [0u32; START_PLATEAUS]);
            for i in (0..k).filter(playing) {
                let t = teams[i] as usize;
                size[t] += 1;
                for j in (0..k).filter(|&j| j != i && teams[j] == teams[i]) {
                    longest[t] = longest[t].max(walk[i][j]);
                }
            }
            let allies = (2..=k)
                .map(|s| {
                    let same = (0..k).filter(|&t| size[t] == s);
                    if same.clone().count() < 2 {
                        0
                    } else {
                        spread(same.map(|t| longest[t]))
                    }
                })
                .max()
                .unwrap_or(0);
            nearest.max(allies / 2)
        })
        .max()
        .unwrap_or(0)
}

/// Where the starts stand on `h`, in lobby order, each kept near its slot
/// in `slots` (the six [`crate::plateaus`], or a [`ring`]).
pub(super) fn place(h: &Grid, slots: &[Pos]) -> Vec<Pos> {
    let n = h.n;
    let steep: Vec<bool> = (0..n * n)
        .map(|i| {
            let (x, y) = (i % n, i / n);
            let d = |j: usize| (h.v[j] - h.v[i]).abs() > STEEP;
            (x + 1 < n && d(i + 1)) || (y + 1 < n && d(i + n))
        })
        .collect();
    let coarse = Coarse::new(h, (n / 96).max(4), &steep);
    search(&coarse, slots, scaled(n, 28))
}

/// The search: each start, one at a time, moved to the cell within
/// `reach` tiles of where it is that makes the walks most equal, trying
/// every other cell and then the cells round the best; then again,
/// while any moved.
fn search(coarse: &Coarse, from: &[Pos], reach: i32) -> Vec<Pos> {
    let count = from.len();
    let reach = (reach as usize / coarse.c).max(1) as i64;
    let home: Vec<usize> = from.iter().map(|&s| coarse.cell(s)).collect();
    let mut at = home.clone();
    let mut walk: Walks = [[0; START_PLATEAUS]; START_PLATEAUS];
    let fill = |walk: &mut Walks, k: usize, row: &[u32]| {
        for j in 0..count {
            if j != k {
                walk[k][j] = row[j];
                walk[j][k] = row[j];
            }
        }
    };
    for k in 0..count {
        let row = coarse.walks(at[k], &at);
        fill(&mut walk, k, &row);
    }
    // A start pays a little for each cell it wanders and for rough ground,
    // so of two equally fair places the nearer, smoother one wins.
    let price = |cell: usize, k: usize| {
        let m = coarse.m as i64;
        let (dx, dy) = (
            (cell as i64 % m - home[k] as i64 % m).abs(),
            (cell as i64 / m - home[k] as i64 / m).abs(),
        );
        (dx.max(dy) as u32) * 2 + coarse.rough[cell] * 4
    };
    let mut score = unfairness(&walk, count) + (0..count).map(|k| price(at[k], k)).sum::<u32>();
    for _ in 0..SWEEPS {
        let mut moved = false;
        for k in 0..count {
            let m = coarse.m as i64;
            let (hx, hy) = (home[k] as i64 % m, home[k] as i64 / m);
            let rest: u32 = (0..count)
                .filter(|&j| j != k)
                .map(|j| price(at[j], j))
                .sum();
            let mut best: Option<(u32, usize, Vec<u32>)> = None;
            // Every other cell first, then the ones round the best of
            // those.
            let try_cell = |cell: usize, best: &mut Option<(u32, usize, Vec<u32>)>| {
                if !coarse.open[cell] || at.iter().enumerate().any(|(j, &a)| j != k && a == cell) {
                    return;
                }
                let mut targets = at.clone();
                targets[k] = cell;
                let row = coarse.walks(cell, &targets);
                let mut trial = walk;
                fill(&mut trial, k, &row);
                let s = unfairness(&trial, count) + rest + price(cell, k);
                if best.as_ref().is_none_or(|(b, _, _)| s < *b) {
                    *best = Some((s, cell, row));
                }
            };
            for dy in (-reach..=reach).step_by(2) {
                for dx in (-reach..=reach).step_by(2) {
                    let (x, y) = (hx + dx, hy + dy);
                    if (0..m).contains(&x) && (0..m).contains(&y) {
                        try_cell((y * m + x) as usize, &mut best);
                    }
                }
            }
            if let Some((_, c, _)) = best.as_ref() {
                let c = *c;
                let (cx, cy) = (c as i64 % m, c as i64 / m);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let (x, y) = (cx + dx, cy + dy);
                        let near_home = (x - hx).abs() <= reach && (y - hy).abs() <= reach;
                        if (dx, dy) != (0, 0)
                            && near_home
                            && (0..m).contains(&x)
                            && (0..m).contains(&y)
                        {
                            try_cell((y * m + x) as usize, &mut best);
                        }
                    }
                }
            }
            if let Some((s, cell, row)) = best
                && s < score
            {
                score = s;
                at[k] = cell;
                fill(&mut walk, k, &row);
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
    // A start that never moved keeps its own tile.
    let mut out = from.to_vec();
    for k in 0..count {
        if at[k] != home[k] {
            out[k] = coarse.tile(at[k]);
        }
    }
    out
}

/// What a move must make the walks more equal by (thousandths) in
/// [`settle`].
const MIN_GAIN: u32 = 2;

/// The starts moved on the finished land by its true walks (`standable`
/// as `super::standable` gives it, walked as `ways::walk` walks), each at
/// most `reach` tiles from where it is, onto ground where `fits` says a
/// clearing can go, where the walks to the enemy come out most equal.
/// One full walk from each start gives every tile's walk to it, so any
/// tile can be tried for a start at once; a start that moves is walked
/// from again. Also how unequal the walks were before (as
/// [`unfairness`]).
pub(super) fn settle(
    standable: &[u8],
    n: usize,
    from: &[Pos],
    reach: i32,
    fits: &dyn Fn(Pos) -> bool,
) -> (Vec<Pos>, u32) {
    let count = from.len();
    let field = |p: Pos| super::ways::walk(standable, n, super::tile_index(n, p), &[]);
    let mut at = from.to_vec();
    let mut fields: Vec<Vec<u32>> = at.iter().map(|&p| field(p)).collect();
    // The walk to a start from a tile next to it, as starts stand on
    // their clearing's middle.
    let to = |f: &[u32], p: Pos| super::walk_to(f, n, p).map_or(u32::MAX, |(d, _)| d);
    let matrix = |at: &[Pos], fields: &[Vec<u32>]| {
        let mut walk: Walks = [[0; START_PLATEAUS]; START_PLATEAUS];
        for k in 0..count {
            for j in 0..count {
                if j != k {
                    walk[k][j] = to(&fields[j], at[k]);
                }
            }
        }
        walk
    };
    let moved =
        |p: Pos, k: usize| ((p.0 - from[k].0).abs().max((p.1 - from[k].1).abs())) as u32 / 4;
    let mut walk = matrix(&at, &fields);
    let before = unfairness(&walk, count);
    let mut score = before + (0..count).map(|k| moved(at[k], k)).sum::<u32>();
    for _ in 0..8 {
        let mut any = false;
        for k in 0..count {
            let rest: u32 = (0..count)
                .filter(|&j| j != k)
                .map(|j| moved(at[j], j))
                .sum();
            let mut best: Option<(u32, Pos)> = None;
            for y in from[k].1 - reach..=from[k].1 + reach {
                for x in from[k].0 - reach..=from[k].0 + reach {
                    let p = (x, y);
                    // Off the map, on ground no one stands on, or on a start.
                    let inside = (0..n as i32).contains(&x) && (0..n as i32).contains(&y);
                    if !inside || standable[super::tile_index(n, p)] == 0 || at.contains(&p) {
                        continue;
                    }
                    // Walks from the other starts to `p` taken as the same as
                    // from `p` to them: a guess, checked once a move is made.
                    let mut trial = walk;
                    for j in 0..count {
                        if j != k {
                            let d = to(&fields[j], p);
                            trial[k][j] = d;
                            trial[j][k] = d;
                        }
                    }
                    let s = unfairness(&trial, count) + rest + moved(p, k);
                    // Whether a clearing fits costs the most, so last.
                    if best.is_none_or(|(b, _)| s < b) && fits(p) {
                        best = Some((s, p));
                    }
                }
            }
            // Each move costs a full walk, so a move must gain a little.
            if let Some((s, p)) = best
                && s + MIN_GAIN < score
            {
                at[k] = p;
                fields[k] = field(p);
                walk = matrix(&at, &fields);
                // Scored again on the true walks, so later moves are
                // weighed against what the land really gives.
                score = unfairness(&walk, count) + (0..count).map(|j| moved(at[j], j)).sum::<u32>();
                any = true;
            }
        }
        if !any {
            break;
        }
    }
    (at, before)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ring's starts stand evenly round the middle, inside the map, the
    /// first the furthest south.
    #[test]
    fn a_ring_stands_evenly_round_the_middle() {
        for n in [256usize, 320, 640, 768] {
            for k in [3, 5] {
                for seed in 0..20 {
                    let ring = ring(n, k, seed);
                    assert_eq!(ring.len(), k);
                    let m = n as i32 / 2;
                    let r2: Vec<i32> = ring
                        .iter()
                        .map(|&(x, y)| (x - m).pow(2) + (y - m).pow(2))
                        .collect();
                    let side2 = |a: Pos, b: Pos| (a.0 - b.0).pow(2) + (a.1 - b.1).pow(2);
                    let sides: Vec<i32> =
                        (0..k).map(|i| side2(ring[i], ring[(i + 1) % k])).collect();
                    for v in [&r2, &sides] {
                        let (lo, hi) = (*v.iter().min().unwrap(), *v.iter().max().unwrap());
                        // (Whole tiles: within three tiles in length.)
                        let root = gc_fixed::isqrt_u64(hi as u64) as i32;
                        assert!(hi - lo <= 6 * root + 9, "{n} {k} {seed}: {v:?}");
                    }
                    for &(x, y) in &ring {
                        assert!((n as i32 / 8..n as i32 * 7 / 8).contains(&x), "{ring:?}");
                        assert!((n as i32 / 8..n as i32 * 7 / 8).contains(&y), "{ring:?}");
                    }
                    assert!(ring.iter().all(|s| s.1 <= ring[0].1), "{ring:?}");
                }
            }
        }
    }

    /// Everyone for themselves on a ring is all a ring is scored on, and
    /// the six slots on their five ways; teams of the same size compare
    /// their allies' walks at half weight.
    #[test]
    fn unfairness_reads_every_handout() {
        let mut walk: Walks = [[100; START_PLATEAUS]; START_PLATEAUS];
        assert_eq!(unfairness(&walk, 3), 0);
        // One start of three further from everyone: 110 against 100 and
        // 100, 10 over a mean of 103.
        for j in 0..3 {
            if j != 0 {
                walk[0][j] = 110;
                walk[j][0] = 110;
            }
        }
        assert_eq!(unfairness(&walk, 3), 97);
        // The six slots: allies 0 and 1 (north) further apart than 2 and
        // 3, with every foe as near: 40 over a mean of 120, halved.
        let mut walk: Walks = [[100; START_PLATEAUS]; START_PLATEAUS];
        walk[0][1] = 140;
        walk[1][0] = 140;
        assert_eq!(unfairness(&walk, 6), 166);
    }
}
