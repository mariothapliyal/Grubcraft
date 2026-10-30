//! Drawn paths (docs/29-restart-plan.md, "Dynamic bridges in brief"): the
//! one curve that bridges, walls and blueprints are all drawn along.
//!
//! A [`DrawnPath`] is a few control points, in half tiles, that the player
//! drags out. Between two points the path is a smooth curve (a uniform
//! Catmull-Rom spline through the points), except where a point is marked a
//! *corner*: there it bends sharply, and a stretch between two corners is a
//! straight line. Everything is worked out in whole numbers, so every machine
//! and every replay turns the same path into the same tiles.
//!
//! - A wall follows the tiles under its centre line ([`DrawnPath::wall_tiles`]):
//!   a tile wide, steps meeting at corners, which already seal the way.
//! - A bridge covers every tile whose centre lies within half its width of
//!   the centre line ([`DrawnPath::band`]), so its deck is joined edge to
//!   edge and soldiers can always walk it.
//!
//! Smooth stretches may bend at most [`MAX_BEND_DEGREES`] every two tiles
//! ([`DrawnPath::bends_gently`]), so a bridge reads like a timber bridge and
//! never zig-zags; a corner is a sharp bend on purpose (walls only).

use alloc::vec::Vec;

use crate::StateHasher;
use crate::structures::Refusal;

/// The most control points one drawn path has.
pub const MAX_POINTS: usize = 12;

/// Sub-units a tile is cut into when the curve is worked out.
pub const SUB: i32 = 256;

/// The sharpest a smooth stretch may turn, in degrees, over two tiles.
pub const MAX_BEND_DEGREES: u32 = 15;

/// `cos` of the bend limit with a degree of slack for rounding, times
/// 10 000 (cos 16 degrees).
const MAX_BEND_COS: i128 = 9613;

/// About how many samples the curve takes a tile.
const SAMPLES_PER_TILE: i32 = 4;

/// A bridge's width across, in tiles: narrow, standard or wide. Even the
/// narrow one clears the widest walking unit (the Brood's Broodmother
/// Bulk, 2.7 tiles) all along, round its bends too.
pub const BRIDGE_WIDTHS: [u8; 3] = [3, 4, 5];

/// The width a bridge drawn with the old two-end order
/// (`Command::BuildWall`) has: the two lanes it always had.
pub const OLD_WIDTH: u8 = 2;

/// The narrowest width a player may pick.
pub const NARROW: u8 = BRIDGE_WIDTHS[0];

/// The width a bridge is drawn with unless the player picks another.
pub const STANDARD: u8 = BRIDGE_WIDTHS[1];

/// A path a player draws: up to [`MAX_POINTS`] control points in half
/// tiles (`2 * x + 1` is the centre of tile column `x`, `2 * x` its west
/// edge), and which of them are sharp corners. Both ends are always
/// corners, unless the path is a closed loop (its last point is its first)
/// whose first point is not marked one: then it is smooth all round.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DrawnPath {
    pub points: [(i16, i16); MAX_POINTS],
    /// Bit `k`: point `k` is a sharp corner.
    pub corners: u16,
    /// How many of `points` are used.
    pub len: u8,
}

/// One point the curve passes through, in sub-units ([`SUB`] a tile).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub x: i32,
    pub y: i32,
    /// It is a corner the path bends sharply at (never an end).
    pub corner: bool,
    /// The stretch it starts is straight (between two corners).
    pub straight: bool,
}

/// The tile half-tile coordinate `h` lies in.
fn tile_of_half(h: i32) -> i32 {
    h.div_euclid(2)
}

impl DrawnPath {
    /// A path through `points` (half tiles), each a corner if `sharp` says
    /// so. `None` for fewer than two points or more than [`MAX_POINTS`].
    pub fn new(points: &[(i16, i16)], sharp: impl Fn(usize) -> bool) -> Option<DrawnPath> {
        if points.len() < 2 || points.len() > MAX_POINTS {
            return None;
        }
        let mut path = DrawnPath {
            len: points.len() as u8,
            ..DrawnPath::default()
        };
        path.points[..points.len()].copy_from_slice(points);
        for k in 0..points.len() {
            if sharp(k) {
                path.corners |= 1 << k;
            }
        }
        Some(path)
    }

    /// The straight line from the centre of tile `from` to the centre of
    /// tile `to`: what `Command::BuildWall` draws.
    pub fn straight(from: (i32, i32), to: (i32, i32)) -> DrawnPath {
        let c = |v: i32| (2 * v + 1) as i16;
        DrawnPath::new(&[(c(from.0), c(from.1)), (c(to.0), c(to.1))], |_| true).expect("two points")
    }

    /// The control points used.
    pub fn points(&self) -> &[(i16, i16)] {
        &self.points[..usize::from(self.len).min(MAX_POINTS)]
    }

    /// Whether it is a closed loop, smooth all round.
    pub fn closed(&self) -> bool {
        let p = self.points();
        p.len() > 3 && p[0] == p[p.len() - 1] && self.corners & 1 == 0
    }

    /// Whether the path bends sharply at point `k`. The ends of an open
    /// path always do.
    pub fn corner(&self, k: usize) -> bool {
        let n = self.points().len();
        if self.closed() {
            // The two ends are the same point, smooth.
            return k != 0 && k != n - 1 && self.corners & (1 << k) != 0;
        }
        k == 0 || k + 1 == n || self.corners & (1 << k) != 0
    }

    /// Whether any point other than the ends is a corner.
    pub fn has_corners(&self) -> bool {
        (1..self.points().len().saturating_sub(1)).any(|k| self.corner(k))
    }

    /// The tile of point `k`.
    pub fn tile(&self, k: usize) -> (i32, i32) {
        let (x, y) = self.points()[k];
        (tile_of_half(i32::from(x)), tile_of_half(i32::from(y)))
    }

    /// The path's first and last tiles.
    pub fn ends(&self) -> ((i32, i32), (i32, i32)) {
        (self.tile(0), self.tile(self.points().len() - 1))
    }

    /// A refusal for a path that isn't one: fewer than two points, too
    /// many, or two in a row at the same place.
    pub fn check(&self) -> Result<(), Refusal> {
        let n = usize::from(self.len);
        if !(2..=MAX_POINTS).contains(&n) {
            return Err(Refusal::Unknown);
        }
        if self.points().windows(2).any(|w| w[0] == w[1]) {
            return Err(Refusal::Unknown);
        }
        Ok(())
    }

    /// Point `k` in sub-units.
    fn at(&self, k: usize) -> (i64, i64) {
        let (x, y) = self.points()[k];
        let half = i64::from(SUB / 2);
        (i64::from(x) * half, i64::from(y) * half)
    }

    /// The point before `k` the curve bends towards (itself at a corner).
    fn before(&self, k: usize) -> (i64, i64) {
        let n = self.points().len();
        if self.corner(k) {
            return self.at(k);
        }
        match k {
            // A closed loop: the point before the first is the one before
            // the last (the last is the first again).
            0 => self.at(n - 2),
            _ => self.at(k - 1),
        }
    }

    /// The point after `k` the curve bends towards (itself at a corner).
    fn after(&self, k: usize) -> (i64, i64) {
        let n = self.points().len();
        if self.corner(k) {
            return self.at(k);
        }
        if k + 1 == n {
            self.at(1)
        } else {
            self.at(k + 1)
        }
    }

    /// The curve, sampled about [`SAMPLES_PER_TILE`] times a tile, from the
    /// first point to the last, every control point among the samples.
    pub fn samples(&self) -> Vec<Sample> {
        let n = self.points().len();
        let mut out = Vec::new();
        for k in 0..n.saturating_sub(1) {
            let (p1, p2) = (self.at(k), self.at(k + 1));
            let straight = self.corner(k) && self.corner(k + 1);
            let corner = k > 0 && self.corner(k);
            if straight {
                out.push(Sample {
                    x: p1.0 as i32,
                    y: p1.1 as i32,
                    corner,
                    straight,
                });
                continue;
            }
            let (p0, p3) = (self.before(k), self.after(k + 1));
            let len = isqrt((p2.0 - p1.0).pow(2) + (p2.1 - p1.1).pow(2));
            let steps = (len * i64::from(SAMPLES_PER_TILE) / i64::from(SUB)).max(2);
            let s3 = steps * steps * steps;
            let along = |a: i64, b: i64, c: i64, d: i64, j: i64| {
                // Uniform Catmull-Rom at t = j / steps, times 2 steps³.
                let n = 2 * b * s3
                    + (c - a) * j * steps * steps
                    + (2 * a - 5 * b + 4 * c - d) * j * j * steps
                    + (3 * b - a - 3 * c + d) * j * j * j;
                (n + s3).div_euclid(2 * s3)
            };
            for j in 0..steps {
                out.push(Sample {
                    x: along(p0.0, p1.0, p2.0, p3.0, j) as i32,
                    y: along(p0.1, p1.1, p2.1, p3.1, j) as i32,
                    corner: corner && j == 0,
                    straight: false,
                });
            }
        }
        let last = self.at(n - 1);
        out.push(Sample {
            x: last.0 as i32,
            y: last.1 as i32,
            corner: false,
            straight: false,
        });
        out
    }

    /// The length of the curve, in sub-units.
    pub fn length(&self) -> i64 {
        arc(&self.samples()).last().copied().unwrap_or(0)
    }

    /// Whether every smooth stretch turns at most [`MAX_BEND_DEGREES`] over
    /// any two tiles (sharp corners left out).
    pub fn bends_gently(&self) -> bool {
        let s = self.samples();
        let arc = arc(&s);
        let sub = i64::from(SUB);
        let (mut j, mut k) = (0, 0);
        for i in 0..s.len() {
            while j < s.len() && arc[j] < arc[i] + sub {
                j += 1;
            }
            if j == s.len() {
                break;
            }
            k = k.max(j);
            while k < s.len() && arc[k] < arc[j] + sub {
                k += 1;
            }
            if k == s.len() {
                break;
            }
            // A sharp corner within: a bend on purpose.
            if s[i + 1..k].iter().any(|p| p.corner) {
                continue;
            }
            let a = (i128::from(s[j].x - s[i].x), i128::from(s[j].y - s[i].y));
            let b = (i128::from(s[k].x - s[j].x), i128::from(s[k].y - s[j].y));
            let dot = a.0 * b.0 + a.1 * b.1;
            let (la, lb) = (a.0 * a.0 + a.1 * a.1, b.0 * b.0 + b.1 * b.1);
            if dot <= 0 || dot * dot * 100_000_000 < MAX_BEND_COS * MAX_BEND_COS * la * lb {
                return false;
            }
        }
        true
    }

    /// The tiles of a wall drawn along the path, in order: a tile wide,
    /// 8-connected (steps meet at corners), each once. A straight stretch
    /// covers just the tiles [`crate::walls::wall_tiles`] gives, so a path
    /// of two points is the old straight line exactly.
    pub fn wall_tiles(&self) -> Vec<(i32, i32)> {
        // Each tile, and whether it stays whatever comes after: the tiles
        // of straight stretches and of the points themselves.
        let mut out: Vec<((i32, i32), bool)> = Vec::new();
        let push = |t: (i32, i32), keep: bool, out: &mut Vec<((i32, i32), bool)>| {
            if out.last().is_some_and(|l| l.0 == t) {
                out.last_mut().expect("a last").1 |= keep;
                return;
            }
            // A tile the curve only grazes is left out: when the one before
            // it already touches this one, it adds nothing to the seal.
            while out.len() >= 2 && !out[out.len() - 1].1 && touch(out[out.len() - 2].0, t) {
                out.pop();
            }
            if !out.iter().any(|o| o.0 == t) {
                out.push((t, keep));
            }
        };
        let s = self.samples();
        let tile = |p: &Sample| (p.x.div_euclid(SUB), p.y.div_euclid(SUB));
        // The ends and sharp corners keep their tiles; a smooth point is
        // just somewhere the curve passes.
        let n = s.len();
        let point = |k: usize| k == 0 || k + 1 == n || s[k].corner;
        for k in 0..n.saturating_sub(1) {
            let (a, b) = (tile(&s[k]), tile(&s[k + 1]));
            // A straight stretch is a line between its two ends' tiles;
            // between samples of a curve, the steps in between.
            for t in crate::walls::line_steps(a, b) {
                let keep = s[k].straight || t == a && point(k) || t == b && point(k + 1);
                push(t, keep, &mut out);
            }
        }
        out.into_iter().map(|(t, _)| t).collect()
    }

    /// The tiles a bridge `width` tiles across covers along the path:
    /// every tile whose centre lies less than half `width` from the centre
    /// line, ordered along it (by the nearest sample, then row, then
    /// column).
    pub fn band(&self, width: u8) -> Vec<(i32, i32)> {
        let s = self.samples();
        let half = i64::from(width) * i64::from(SUB) / 2;
        let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for p in &s {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        let reach = half as i32 + SUB;
        let (tx0, ty0) = ((x0 - reach).div_euclid(SUB), (y0 - reach).div_euclid(SUB));
        let (tx1, ty1) = ((x1 + reach).div_euclid(SUB), (y1 + reach).div_euclid(SUB));
        let mut tiles: Vec<(usize, i32, i32)> = Vec::new();
        for ty in ty0..=ty1 {
            for tx in tx0..=tx1 {
                let c = (tx * SUB + SUB / 2, ty * SUB + SUB / 2);
                if let Some(k) = within(&s, c, half) {
                    tiles.push((k, ty, tx));
                }
            }
        }
        tiles.sort_unstable();
        tiles.into_iter().map(|(_, y, x)| (x, y)).collect()
    }

    /// The tiles of a bridge `width` across at either end of the path: those
    /// whose centres lie less than half `width` from its first point, and
    /// from its last.
    pub fn band_ends(&self, width: u8) -> [Vec<(i32, i32)>; 2] {
        let half = i64::from(width) * i64::from(SUB) / 2;
        let n = self.points().len();
        [0, n - 1].map(|k| {
            let (px, py) = self.at(k);
            let (cx, cy) = ((px as i32).div_euclid(SUB), (py as i32).div_euclid(SUB));
            let r = i32::from(width);
            let mut out = Vec::new();
            for ty in cy - r..=cy + r {
                for tx in cx - r..=cx + r {
                    let c = (i64::from(tx * SUB + SUB / 2), i64::from(ty * SUB + SUB / 2));
                    if (c.0 - px).pow(2) + (c.1 - py).pow(2) < half * half {
                        out.push((tx, ty));
                    }
                }
            }
            out
        })
    }

    /// The local line a tile laid along the path at `tile` runs along, its
    /// two end tiles: on a straight stretch, the stretch's own ends (so a
    /// wall drawn straight keeps the line it was drawn in); on a curve, the
    /// centre line two tiles either side of the point nearest the tile.
    /// Either is moved across, if need be, so that `tile` is one of its
    /// steps (see [`crate::walls::wall_tiles`]): a bridge's outer lanes
    /// run beside the centre line.
    pub fn chord(&self, samples: &[Sample], tile: (i32, i32)) -> [i16; 4] {
        let c = (tile.0 * SUB + SUB / 2, tile.1 * SUB + SUB / 2);
        let k = nearest(samples, c);
        let t = |p: &Sample| [p.x.div_euclid(SUB) as i16, p.y.div_euclid(SUB) as i16];
        // On a straight stretch: from its start to the next sample (its end).
        let on = |k: usize| samples[k].straight && k + 1 < samples.len();
        let k = if k > 0 && !on(k) && on(k - 1) && near_seg(samples, k - 1, c) {
            k - 1
        } else {
            k
        };
        if on(k) {
            let (a, b) = (t(&samples[k]), t(&samples[k + 1]));
            return through([a[0], a[1], b[0], b[1]], tile);
        }
        let arc = arc(samples);
        let sub = i64::from(2 * SUB);
        let lo = (0..=k).rev().find(|&j| arc[k] - arc[j] >= sub).unwrap_or(0);
        let hi = (k..samples.len())
            .find(|&j| arc[j] - arc[k] >= sub)
            .unwrap_or(samples.len() - 1);
        let (a, b) = (t(&samples[lo]), t(&samples[hi]));
        through([a[0], a[1], b[0], b[1]], tile)
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.len);
        h.u16(self.corners);
        for &(x, y) in self.points() {
            h.u16(x as u16);
            h.u16(y as u16);
        }
    }
}

/// Whether tiles `a` and `b` touch, edge or corner.
fn touch(a: (i32, i32), b: (i32, i32)) -> bool {
    (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1
}

/// `line` moved across so that `tile` is one of its steps: by how far
/// `tile` is from the nearest of them (the first on a tie).
fn through(line: [i16; 4], tile: (i32, i32)) -> [i16; 4] {
    let [x0, y0, x1, y1] = line.map(i32::from);
    let near = crate::walls::line_steps((x0, y0), (x1, y1))
        .min_by_key(|&(x, y)| (x - tile.0).pow(2) + (y - tile.1).pow(2))
        .unwrap_or((x0, y0));
    let (dx, dy) = (tile.0 - near.0, tile.1 - near.1);
    [x0 + dx, y0 + dy, x1 + dx, y1 + dy].map(|v| v as i16)
}

/// Whether tile centre `c` is nearer segment `k` (samples `k` to `k + 1`)
/// than to segment `k + 1`.
fn near_seg(s: &[Sample], k: usize, c: (i32, i32)) -> bool {
    let d = |k: usize| seg_dist(s, k, c);
    k + 2 >= s.len() || d(k) <= d(k + 1)
}

/// The square of the distance from `c` to segment `k` (samples `k` to
/// `k + 1`), as a fraction: numerator and denominator.
fn seg_dist(s: &[Sample], k: usize, c: (i32, i32)) -> (i128, i128) {
    let a = (i128::from(s[k].x), i128::from(s[k].y));
    let b = (i128::from(s[k + 1].x), i128::from(s[k + 1].y));
    let p = (i128::from(c.0), i128::from(c.1));
    let ab = (b.0 - a.0, b.1 - a.1);
    let ap = (p.0 - a.0, p.1 - a.1);
    let len = ab.0 * ab.0 + ab.1 * ab.1;
    let dot = ap.0 * ab.0 + ap.1 * ab.1;
    if len == 0 || dot <= 0 {
        return (ap.0 * ap.0 + ap.1 * ap.1, 1);
    }
    if dot >= len {
        let bp = (p.0 - b.0, p.1 - b.1);
        return (bp.0 * bp.0 + bp.1 * bp.1, 1);
    }
    let cross = ap.0 * ab.1 - ap.1 * ab.0;
    (cross * cross, len)
}

/// The segment of the curve `s` nearest `c`, if `c` lies within the band
/// `half` either side of it, or `None`. Where `c` lies square to a segment,
/// the band runs from `half` on one side up to but not including `half` on
/// the other (so a band two tiles wide on a line through tile centres is
/// two tiles across, not one or three), and a little further where the
/// segment slants; beyond a segment's ends, within `half` of the end.
fn within(s: &[Sample], c: (i32, i32), half: i64) -> Option<usize> {
    let h = i128::from(half);
    let mut best: Option<(i128, i128, usize)> = None;
    for k in 0..s.len().saturating_sub(1) {
        let (n, d) = seg_dist(s, k, c);
        // The nearest segment, the first on a tie.
        if best.is_none_or(|(bn, bd, _)| n * bd < bn * d) {
            best = Some((n, d, k));
        }
    }
    let (_, _, k) = best?;
    let a = (i128::from(s[k].x), i128::from(s[k].y));
    let b = (i128::from(s[k + 1].x), i128::from(s[k + 1].y));
    let p = (i128::from(c.0), i128::from(c.1));
    let ab = (b.0 - a.0, b.1 - a.1);
    let ap = (p.0 - a.0, p.1 - a.1);
    let len2 = ab.0 * ab.0 + ab.1 * ab.1;
    let dot = ap.0 * ab.0 + ap.1 * ab.1;
    if len2 > 0 && (0..=len2).contains(&dot) {
        // Square to it: the signed distance across, times its length. A
        // tile is a square, and across a slanting line it reaches further
        // than half a tile (up to half a diagonal): the band takes that in
        // too, so a slanting deck is as wide across as a straight one.
        let cross = ab.0 * ap.1 - ab.1 * ap.0;
        let len = i128::from(isqrt(len2.min(i128::from(i64::MAX)) as i64)).max(1);
        let half_tile = i128::from(SUB / 2);
        let reach = h * len + 3 * half_tile * (ab.0.abs() + ab.1.abs() - len);
        let inside = if cross >= 0 {
            cross < reach
        } else {
            -cross <= reach
        };
        return inside.then_some(k);
    }
    let (n, d) = seg_dist(s, k, c);
    (n < h * h * d).then_some(k)
}

/// The sample nearest `c`, the first on a tie.
fn nearest(s: &[Sample], c: (i32, i32)) -> usize {
    (0..s.len())
        .min_by_key(|&k| {
            let (dx, dy) = (i64::from(s[k].x - c.0), i64::from(s[k].y - c.1));
            (dx * dx + dy * dy, k)
        })
        .unwrap_or(0)
}

/// The distance along the curve `s` to each sample, in sub-units.
fn arc(s: &[Sample]) -> Vec<i64> {
    let mut out = Vec::with_capacity(s.len());
    let mut sum = 0;
    for (k, p) in s.iter().enumerate() {
        if k > 0 {
            let q = &s[k - 1];
            sum += isqrt(i64::from(p.x - q.x).pow(2) + i64::from(p.y - q.y).pow(2));
        }
        out.push(sum);
    }
    out
}

fn isqrt(n: i64) -> i64 {
    gc_fixed::isqrt_u64(n.max(0) as u64) as i64
}

/// One drawn line, kept for as long as the match lasts: the path, the
/// width it was drawn at (bridges; 1 for walls), its owner and the seed
/// its look is laid out from. Every tile laid along it names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drawing {
    pub owner: crate::PlayerId,
    pub path: DrawnPath,
    pub width: u8,
    pub seed: u32,
}

/// No drawing: a tile not laid along one.
pub const NO_DRAWING: u32 = u32::MAX;

/// Every line drawn in the match, in the order drawn, and a digest of them
/// for the state hash (they never change once drawn).
#[derive(Clone, Default)]
pub(crate) struct Drawings {
    pub list: Vec<Drawing>,
    digest: u64,
}

impl Drawings {
    /// Keep a new drawing; its id.
    pub(crate) fn add(&mut self, owner: crate::PlayerId, path: DrawnPath, width: u8) -> u32 {
        let id = self.list.len() as u32;
        // A seed that differs from line to line and needs no randomness.
        let seed = (id.wrapping_mul(0x9E37_79B9)) ^ u32::from(owner) << 24;
        let d = Drawing {
            owner,
            path,
            width,
            seed,
        };
        let mut h = StateHasher::new();
        h.u64(self.digest);
        h.u8(owner);
        h.u8(width);
        path.hash_into(&mut h);
        self.digest = h.finish();
        self.list.push(d);
        id
    }

    pub(crate) fn get(&self, id: u32) -> Option<&Drawing> {
        self.list.get(id as usize)
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u32(self.list.len() as u32);
        h.u64(self.digest);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn half(t: (i32, i32)) -> (i16, i16) {
        ((2 * t.0 + 1) as i16, (2 * t.1 + 1) as i16)
    }

    #[test]
    fn a_straight_path_covers_the_old_straight_line() {
        for (from, to) in [
            ((0, 0), (9, 3)),
            ((5, 5), (5, -4)),
            ((2, 7), (13, 7)),
            ((3, 3), (3, 3)),
        ] {
            let p = DrawnPath::straight(from, to);
            if from == to {
                assert!(p.check().is_err());
                continue;
            }
            assert_eq!(
                p.wall_tiles(),
                crate::walls::wall_tiles(from, to).unwrap(),
                "{from:?} {to:?}"
            );
        }
    }

    #[test]
    fn a_curve_passes_through_its_points_and_its_tiles_join() {
        let pts = [half((0, 0)), half((8, 2)), half((14, 8))];
        let p = DrawnPath::new(&pts, |_| false).unwrap();
        let s = p.samples();
        for &(x, y) in &pts {
            let at = (i32::from(x) * SUB / 2, i32::from(y) * SUB / 2);
            assert!(s.iter().any(|q| (q.x, q.y) == at), "{at:?}");
        }
        let tiles = p.wall_tiles();
        for w in tiles.windows(2) {
            let (a, b) = (w[0], w[1]);
            assert!(
                (a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1,
                "{a:?} {b:?}"
            );
        }
        assert_eq!(tiles.first(), Some(&(0, 0)));
        assert_eq!(tiles.last(), Some(&(14, 8)));
        // No tile it only grazes: none touches the one two before it.
        for w in tiles.windows(3) {
            assert!(!touch(w[0], w[2]), "{w:?}");
        }
        // A corner keeps its tile.
        let corner = DrawnPath::new(&[(1, 1), (11, 1), (11, 11)], |_| true).unwrap();
        assert!(corner.wall_tiles().contains(&(5, 0)));
        // A curve that just reaches the next row adds no spur there.
        let bow = DrawnPath::new(&[(1, 1), (8, 2), (15, 1)], |_| false).unwrap();
        assert_eq!(bow.wall_tiles(), (0..8).map(|x| (x, 0)).collect::<Vec<_>>());
    }

    #[test]
    fn a_gentle_curve_passes_and_a_sharp_one_does_not() {
        let gentle = DrawnPath::new(&[half((0, 0)), half((10, 1)), half((20, 4))], |_| false);
        assert!(gentle.unwrap().bends_gently());
        let sharp = DrawnPath::new(&[half((0, 0)), half((6, 0)), half((6, 6))], |_| false);
        assert!(!sharp.unwrap().bends_gently());
        // The same bend as a corner is on purpose.
        let corner = DrawnPath::new(&[half((0, 0)), half((6, 0)), half((6, 6))], |_| true);
        assert!(corner.unwrap().bends_gently());
    }

    #[test]
    fn a_band_is_as_wide_as_asked_and_joined_edge_to_edge() {
        // Along a tile edge, two and four tiles; along tile centres, three.
        let edge = DrawnPath::new(&[(1, 20), (21, 20)], |_| true).unwrap();
        let centre = DrawnPath::new(&[(1, 21), (21, 21)], |_| true).unwrap();
        let across = |tiles: &[(i32, i32)], x: i32| tiles.iter().filter(|t| t.0 == x).count();
        assert_eq!(across(&edge.band(2), 5), 2);
        assert_eq!(across(&centre.band(3), 5), 3);
        assert_eq!(across(&edge.band(4), 5), 4);
        // Two across through tile centres too, not one or three.
        assert_eq!(across(&centre.band(2), 5), 2);
        // A curve: every tile meets the next ones edge to edge, so a band
        // has no corner-only steps a soldier couldn't take.
        let curve = DrawnPath::new(&[(1, 21), (17, 25), (33, 37)], |_| false).unwrap();
        for w in [2, 3, 4] {
            let band = curve.band(w);
            assert!(!band.is_empty());
            for &(x, y) in &band {
                let edge = |dx: i32, dy: i32| band.contains(&(x + dx, y + dy));
                for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                    if edge(dx, dy) {
                        assert!(edge(dx, 0) || edge(0, dy), "corner step at {x},{y} w{w}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_closed_loop_is_smooth_all_round() {
        let ring = [
            half((0, 4)),
            half((4, 0)),
            half((8, 4)),
            half((4, 8)),
            half((0, 4)),
        ];
        let p = DrawnPath::new(&ring, |_| false).unwrap();
        assert!(p.closed());
        assert!(!p.has_corners());
        let tiles = p.wall_tiles();
        // A tile no more than once, and it comes back round to where it
        // started.
        let mut seen = tiles.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), tiles.len());
        let (a, b) = (tiles[0], *tiles.last().unwrap());
        assert!((a.0 - b.0).abs() <= 1 && (a.1 - b.1).abs() <= 1);
    }
}
