//! Laying out a ring of wall round a base (docs/26-fortifications.md,
//! stage 8), for the Engineer.
//!
//! The ring is the shortest one that closes: the fewest new wall tiles
//! that, with the cliffs, the deep water and the walls already standing,
//! cut the ground the base must keep (its buildings with room round them,
//! its seeps and veins and the lanes to them) off from everything further
//! out. That is a minimum cut, found as a maximum flow from the kept
//! ground to the edge of the square it is planned in. The cut is then
//! walked round in order, straightened into lines a few tiles long (so a
//! slanting side is drawn as slanting lines, not as steps), and checked to
//! still close; where straightening would open it, the cut is drawn as it
//! was found.
//!
//! The ring follows the land as a player's drawn wall does: each stretch
//! of the cut between its sharp turns is drawn as one smooth curve
//! ([`gc_sim::drawn::DrawnPath`]) where the curve's tiles can all go up
//! and the ring still closes, so it bends round the hill or along the
//! river it leans on instead of stepping. Ground that holds the wall well
//! costs the cut a little less, so the ring bends out to take it when
//! that is not much longer: a crest with the ground falling away outside
//! (its defenders fight downhill) and a river bank (its attackers stand in
//! the water).
//!
//! Everything here is plain geometry on a square of tiles ([`Site`]), so
//! it is tested without a world.

use gc_sim::drawn::{self, DrawnPath};
use gc_sim::walls::wall_tiles;

/// How far from home the ring is planned, tiles: inside a walled town's
/// reach (`gc_sim::towns::TOWN_REACH`).
pub(super) const REACH: i32 = 30;
/// The longest line drawn at once, tiles.
pub(super) const RUN: i32 = 8;
/// How far a straightened line may stray from the cut, tiles.
const STRAIGHTEN: f32 = 0.8;
/// What a tile of wall costs the cut: on open ground, and on ground that
/// holds a wall well (see [`Site::vantage`]), so a ring goes up to a
/// third further round to stand on that.
const OPEN: i32 = 4;
const VANTAGE: i32 = 3;
/// What a tile that can't be walled now costs the cut: a ring goes a long
/// way round rather than leave a gap. And a tile to clear first: a little
/// way round (three tiles of open ground), not a long one.
const HARD: i32 = 1000 * OPEN;
const GROWN: i32 = 3 * OPEN;
/// A smooth stretch of the ring: at least this many tiles (shorter ones
/// are drawn as lines), at most this many in one drawing, a control point
/// about every [`CURVE_STEP`] tiles along it. It is cut at any turn of
/// the cut sharper than [`SHARP_TURN`] degrees, which stays a corner.
const CURVE_LEAST: usize = 6;
const CURVE_RUN: usize = 16;
const CURVE_STEP: usize = 4;
const SHARP_TURN: f32 = 50.0;
const NO_END: i32 = 1 << 28;

/// How a tile lies for the ring.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Ground {
    /// A town's ground stops here of itself: a cliff, deep water, the
    /// map's edge.
    Shut,
    /// Its own wall or gate, standing, going up or planned (or another
    /// building of its own out of the ring's way, which closes a ring as
    /// a wall does).
    Walled,
    /// Walked, and a wall tile can go up on it.
    Open,
    /// Walked, and a wall tile can go up on it once what grows there is
    /// cleared (scrub, woods, thicket).
    Grown,
    /// Walked, but no wall can go up on it now (a node, another side's
    /// building, no power).
    Hard,
}

/// The square of tiles a ring is planned in.
#[derive(Clone, Debug)]
pub(super) struct Site {
    /// The tile at its corner, and its side in tiles.
    pub origin: (i32, i32),
    pub side: i32,
    pub ground: Vec<Ground>,
    /// The tiles the ring must go round.
    pub keep: Vec<bool>,
    /// Tiles that must stay outside it (the ground in front of its gate).
    pub out: Vec<bool>,
    /// Tiles no wall may go on (the way through its gate).
    pub clear: Vec<bool>,
    /// Tiles a wall of its own is planned on and not begun: open ground
    /// still, so the ring is laid over them as over any other, but soon
    /// shut, so what lies beyond one is no part of the town (see
    /// [`Site::keep_joined`]).
    pub planned: Vec<bool>,
    /// Open tiles that hold a wall well, facing out of the town: a crest
    /// or the top of a slope (the ground a few tiles further out lies at
    /// least half a tile lower, so its defenders fight downhill), or a
    /// river bank (attackers wade to it). Cheaper for the cut.
    pub vantage: Vec<bool>,
}

/// A gate in the ring: its middle tile, and the step from there out of
/// the town (east, west, north or south). Its three tiles lie across that.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Door {
    pub mid: (i32, i32),
    pub out: (i32, i32),
}

impl Door {
    /// Its three tiles, end, middle, end.
    pub fn tiles(self) -> [(i32, i32); 3] {
        let (ax, ay) = (self.out.1.abs(), self.out.0.abs());
        [-1, 0, 1].map(|k| (self.mid.0 + ax * k, self.mid.1 + ay * k))
    }

    /// The tile `k` steps out of it (into the town for a negative `k`).
    pub fn step(self, k: i32) -> (i32, i32) {
        (self.mid.0 + self.out.0 * k, self.mid.1 + self.out.1 * k)
    }

    /// The wall tile beside each of its ends.
    pub fn beside(self) -> [(i32, i32); 2] {
        let (ax, ay) = (self.out.1.abs(), self.out.0.abs());
        [-2, 2].map(|k| (self.mid.0 + ax * k, self.mid.1 + ay * k))
    }
}

/// A line of the ring to draw, from tile to tile: straight, or a curve
/// along the cut.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Piece {
    pub from: (i32, i32),
    pub to: (i32, i32),
    /// The path it is drawn along, from `from` to `to`.
    pub path: DrawnPath,
    /// How far round the ring from its front (its gate) it starts, tiles.
    pub round: u32,
    /// Where its two ends are in the ring's order round the town.
    at: [usize; 2],
}

/// A ring laid out.
#[derive(Clone, Debug, Default)]
pub(super) struct Layout {
    /// The lines to draw, nearest the front first.
    pub pieces: Vec<Piece>,
    /// Whether the ring closes once they are drawn and the ground of
    /// `clear` is cleared and walled (no gap left on ground that can't be
    /// walled at all now).
    pub closes: bool,
    /// The tiles of the ring to clear before wall can go up on them, each
    /// with how far round the ring from its front it is, nearest first.
    pub clear: Vec<((i32, i32), u32)>,
    /// The town's ground once they are drawn, by the site's tiles.
    town: Vec<bool>,
    /// The ring's tiles in order round the town, by the site's tiles.
    order: Vec<usize>,
}

impl Layout {
    /// Turn the ring to face tile `front` (the middle of its gate, or
    /// where its gate would be): its lines and the ground to clear are
    /// put in order from there, each line drawn from its end nearer it.
    pub fn face(&mut self, site: &Site, front: Option<(i32, i32)>) {
        let len = self.order.len().max(1);
        let front = front
            .and_then(|f| {
                (self.order.iter().enumerate())
                    .min_by_key(|&(_, &i)| {
                        let a = site.tile(i);
                        (a.0 - f.0).pow(2) + (a.1 - f.1).pow(2)
                    })
                    .map(|(k, _)| k)
            })
            .unwrap_or(0);
        let round_from = |place: usize| {
            let d = (place % len).abs_diff(front);
            d.min(len - d) as u32
        };
        for p in &mut self.pieces {
            let (ra, rb) = (round_from(p.at[0]), round_from(p.at[1]));
            if rb < ra {
                (p.from, p.to) = (p.to, p.from);
                p.at.swap(0, 1);
                p.path = reversed(&p.path);
            }
            p.round = ra.min(rb);
        }
        self.pieces.sort_by_key(|p| p.round);
        for c in &mut self.clear {
            let i = site.index(c.0);
            c.1 = (self.order.iter().position(|&r| Some(r) == i)).map_or(u32::MAX, round_from);
        }
        self.clear.sort_by_key(|c| c.1);
    }

    /// Whether tile `t` of `site` (which this was laid on) is the town's
    /// ground once the ring is drawn.
    pub fn town(&self, site: &Site, t: (i32, i32)) -> bool {
        site.index(t).is_some_and(|i| self.town[i])
    }
}

impl Site {
    /// An empty square of open ground with its corner at `origin`.
    pub fn new(origin: (i32, i32), side: i32) -> Site {
        let n = (side * side) as usize;
        Site {
            origin,
            side,
            ground: vec![Ground::Open; n],
            keep: vec![false; n],
            out: vec![false; n],
            clear: vec![false; n],
            planned: vec![false; n],
            vantage: vec![false; n],
        }
    }

    /// The square [`REACH`] tiles round `home`.
    pub fn round(home: (i32, i32)) -> Site {
        Site::new((home.0 - REACH, home.1 - REACH), 2 * REACH + 1)
    }

    /// Tile `t`'s place in the square, if it is in it.
    pub fn index(&self, t: (i32, i32)) -> Option<usize> {
        let (x, y) = (t.0 - self.origin.0, t.1 - self.origin.1);
        ((0..self.side).contains(&x) && (0..self.side).contains(&y))
            .then(|| (y * self.side + x) as usize)
    }

    pub fn tile(&self, i: usize) -> (i32, i32) {
        let i = i as i32;
        (self.origin.0 + i % self.side, self.origin.1 + i / self.side)
    }

    /// Whether tile `i` is on the square's rim: outside any ring.
    fn rim(&self, i: usize) -> bool {
        let (x, y) = (i as i32 % self.side, i as i32 / self.side);
        x == 0 || y == 0 || x == self.side - 1 || y == self.side - 1
    }

    /// Whether a town's ground runs over tile `i`.
    fn walked(&self, i: usize) -> bool {
        matches!(self.ground[i], Ground::Open | Ground::Grown | Ground::Hard)
    }

    /// The tiles beside tile `i`, east, west, south and north.
    fn beside(&self, i: usize) -> impl Iterator<Item = usize> + use<> {
        let side = self.side as usize;
        let (x, y) = (i % side, i / side);
        [
            (x + 1 < side).then(|| i + 1),
            (x > 0).then(|| i - 1),
            (y + 1 < side).then(|| i + side),
            (y > 0).then(|| i - side),
        ]
        .into_iter()
        .flatten()
    }

    /// Keep tile `t` inside the ring, if it is in the square, off its rim
    /// and walked.
    pub fn keep_tile(&mut self, t: (i32, i32)) {
        if let Some(i) = self.index(t)
            && !self.rim(i)
            && self.walked(i)
        {
            self.keep[i] = true;
        }
    }

    /// Put a gate in the ring: its tiles walled, the tile behind it kept
    /// and the tile in front of it left out, and the way through it kept
    /// clear of wall two tiles either side.
    pub fn set_door(&mut self, door: Door) {
        for t in door.tiles() {
            if let Some(i) = self.index(t) {
                self.ground[i] = Ground::Walled;
                self.keep[i] = false;
            }
        }
        self.keep_tile(door.step(-1));
        if let Some(i) = self.index(door.step(1)) {
            self.out[i] = true;
            self.keep[i] = false;
        }
        for k in [-2, -1, 1, 2] {
            if let Some(i) = self.index(door.step(k)) {
                self.clear[i] = true;
            }
        }
    }

    /// The kept tiles with kept tiles all round them: a straightened line
    /// may take the edge off the kept ground, never these.
    fn core(&self) -> Vec<bool> {
        let side = self.side;
        let kept = |x: i32, y: i32| {
            !(0..side).contains(&x)
                || !(0..side).contains(&y)
                || self.keep[(y * side + x) as usize]
                || !self.walked((y * side + x) as usize)
        };
        let core: Vec<bool> = (0..self.keep.len())
            .map(|i| {
                let (x, y) = (i as i32 % side, i as i32 / side);
                self.keep[i] && (-1..=1).all(|dy| (-1..=1).all(|dx| kept(x + dx, y + dy)))
            })
            .collect();
        if core.iter().any(|&c| c) {
            core
        } else {
            self.keep.clone()
        }
    }

    /// The ground joined to the tiles of `from` that are walked, as a
    /// town's ground spreads: tile to tile by their sides, never over
    /// ground that is shut or walled, nor over a tile `stop` takes.
    fn spread(&self, from: impl Iterator<Item = usize>, stop: impl Fn(usize) -> bool) -> Vec<bool> {
        let mut seen = vec![false; self.ground.len()];
        let mut todo: Vec<usize> = from.filter(|&i| self.walked(i) && !stop(i)).collect();
        for &i in &todo {
            seen[i] = true;
        }
        while let Some(i) = todo.pop() {
            for n in self.beside(i) {
                if !seen[n] && self.walked(n) && !stop(n) {
                    seen[n] = true;
                    todo.push(n);
                }
            }
        }
        seen
    }

    /// The tiles joined to the kept ground: the only ones a ring is laid
    /// on.
    pub fn joined(&self) -> Vec<usize> {
        let n = self.ground.len();
        let near = self.spread((0..n).filter(|&i| self.keep[i]), |_| false);
        (0..n).filter(|&i| near[i]).collect()
    }

    /// Drop the kept tiles not joined to tile `home` through kept ground
    /// alone: a building over a cliff from the rest, or far off with no
    /// kept lane to it, is no part of this town, and is left outside. So
    /// is one beyond a wall of its own, standing or planned: a ring laid
    /// round both would shut it, and all it trains, in a yard with no
    /// gate once the wall between them is whole.
    pub fn keep_joined(&mut self, home: (i32, i32)) {
        let Some(h) = self.index(home) else { return };
        let joined = self.spread(std::iter::once(h), |i| !self.keep[i] || self.planned[i]);
        self.keep = joined;
    }
}

/// A flow network on the tiles: each walked tile is a way in and a way
/// out joined by an edge as wide as walling it costs.
struct Flow {
    to: Vec<u32>,
    cap: Vec<i32>,
    next: Vec<u32>,
    head: Vec<u32>,
    level: Vec<i32>,
    it: Vec<u32>,
}

const NONE: u32 = u32::MAX;

impl Flow {
    fn new(nodes: usize) -> Flow {
        Flow {
            to: Vec::new(),
            cap: Vec::new(),
            next: Vec::new(),
            head: vec![NONE; nodes],
            level: vec![-1; nodes],
            it: vec![NONE; nodes],
        }
    }

    fn edge(&mut self, a: usize, b: usize, cap: i32) {
        for (from, to, cap) in [(a, b, cap), (b, a, 0)] {
            self.to.push(to as u32);
            self.cap.push(cap);
            self.next.push(self.head[from]);
            self.head[from] = self.to.len() as u32 - 1;
        }
    }

    /// The nodes still reached from `s`, each with how many edges away.
    fn levels(&mut self, s: usize) {
        self.level.fill(-1);
        self.level[s] = 0;
        let mut todo = std::collections::VecDeque::from([s]);
        while let Some(u) = todo.pop_front() {
            let mut e = self.head[u];
            while e != NONE {
                let v = self.to[e as usize] as usize;
                if self.cap[e as usize] > 0 && self.level[v] < 0 {
                    self.level[v] = self.level[u] + 1;
                    todo.push_back(v);
                }
                e = self.next[e as usize];
            }
        }
    }

    /// Push flow along one more shortest way from `s` to `t`; how much.
    fn push(&mut self, s: usize, t: usize) -> i32 {
        let mut path: Vec<u32> = Vec::new();
        let mut u = s;
        loop {
            if u == t {
                let f = path
                    .iter()
                    .map(|&e| self.cap[e as usize])
                    .min()
                    .unwrap_or(0);
                for &e in &path {
                    self.cap[e as usize] -= f;
                    self.cap[e as usize ^ 1] += f;
                }
                return f;
            }
            let mut on = false;
            while self.it[u] != NONE {
                let e = self.it[u] as usize;
                let v = self.to[e] as usize;
                if self.cap[e] > 0 && self.level[v] == self.level[u] + 1 {
                    path.push(e as u32);
                    u = v;
                    on = true;
                    break;
                }
                self.it[u] = self.next[e];
            }
            if on {
                continue;
            }
            // A dead end: back a step, and never this way again.
            self.level[u] = -1;
            let Some(e) = path.pop() else { return 0 };
            u = self.to[e as usize ^ 1] as usize;
        }
    }

    /// The most that flows from `s` to `t`, giving up (`None`) past `most`.
    fn max(&mut self, s: usize, t: usize, most: i32) -> Option<i32> {
        let mut flow = 0;
        loop {
            self.levels(s);
            if self.level[t] < 0 {
                return Some(flow);
            }
            self.it.copy_from_slice(&self.head);
            loop {
                let f = self.push(s, t);
                if f == 0 {
                    break;
                }
                flow += f;
                if flow > most {
                    return None;
                }
            }
        }
    }
}

/// The cheapest set of walked tiles that cuts the site's kept ground off
/// from its rim and the tiles it leaves out: `None` when nothing can (the
/// kept ground runs to the rim).
fn cut(site: &Site) -> Option<Vec<usize>> {
    let n = site.ground.len();
    // Only the ground joined to what is kept matters.
    let near = site.spread((0..n).filter(|&i| site.keep[i]), |_| false);
    let (s, t) = (2 * n, 2 * n + 1);
    let mut flow = Flow::new(2 * n + 2);
    let end = |i: usize| site.out[i] || site.rim(i);
    for i in (0..n).filter(|&i| near[i]) {
        let cap = match site.ground[i] {
            _ if site.keep[i] || end(i) || site.clear[i] => NO_END,
            Ground::Open if site.vantage[i] => VANTAGE,
            Ground::Open => OPEN,
            Ground::Grown => GROWN,
            _ => HARD,
        };
        flow.edge(2 * i, 2 * i + 1, cap);
        if site.keep[i] {
            flow.edge(s, 2 * i, NO_END);
        }
        if end(i) {
            flow.edge(2 * i + 1, t, NO_END);
            continue;
        }
        for b in site.beside(i).filter(|&b| near[b]) {
            flow.edge(2 * i + 1, 2 * b, NO_END);
        }
    }
    flow.max(s, t, NO_END / 2)?;
    flow.levels(s);
    Some(
        (0..n)
            .filter(|&i| near[i] && flow.level[2 * i] >= 0 && flow.level[2 * i + 1] < 0)
            .collect(),
    )
}

/// The tiles just outside the ground `inside`, in order round it,
/// clockwise from the north of its most northerly tile: each tile that
/// shares a side with it, as often as the walk round passes it.
fn outline(site: &Site, inside: &[bool]) -> Vec<usize> {
    let side = site.side;
    let Some(start) = inside.iter().position(|&i| i) else {
        return Vec::new();
    };
    let at = |x: i32, y: i32| {
        (0..side).contains(&x) && (0..side).contains(&y) && inside[(y * side + x) as usize]
    };
    // Sides north, east, south, west; walking with the ground on the right
    // hand, along side `d` the walk heads the way of side `d + 1`.
    const STEP: [(i32, i32); 4] = [(0, -1), (1, 0), (0, 1), (-1, 0)];
    let (sx, sy) = (start as i32 % side, start as i32 / side);
    let (mut x, mut y, mut d) = (sx, sy, 0usize);
    let mut round = Vec::new();
    loop {
        let (ox, oy) = (x + STEP[d].0, y + STEP[d].1);
        // Never off the square: the rim is no part of any inside.
        let o = (oy * side + ox) as usize;
        if round.last() != Some(&o) {
            round.push(o);
        }
        let f = STEP[(d + 1) % 4];
        let (qx, qy) = (x + f.0, y + f.1);
        let (rx, ry) = (qx + STEP[d].0, qy + STEP[d].1);
        if at(qx, qy) && at(rx, ry) {
            (x, y, d) = (rx, ry, (d + 3) % 4);
        } else if at(qx, qy) {
            (x, y) = (qx, qy);
        } else {
            d = (d + 1) % 4;
        }
        if (x, y, d) == (sx, sy, 0) {
            break;
        }
    }
    if round.len() > 1 && round.first() == round.last() {
        round.pop();
    }
    round
}

/// The corners of the tiles `chain` (in order) straightened: the fewest
/// of them, first and last kept, that no tile between strays more than
/// `stray` tiles from the lines through.
fn straighten(chain: &[(i32, i32)], stray: f32) -> Vec<usize> {
    let mut keep = vec![0, chain.len().saturating_sub(1)];
    let mut todo = vec![(0, chain.len().saturating_sub(1))];
    while let Some((a, b)) = todo.pop() {
        if b <= a + 1 {
            continue;
        }
        let (p, q) = (chain[a], chain[b]);
        let (dx, dy) = ((q.0 - p.0) as f32, (q.1 - p.1) as f32);
        let len = (dx * dx + dy * dy).sqrt().max(1e-6);
        let far = |t: (i32, i32)| {
            let (ex, ey) = ((t.0 - p.0) as f32, (t.1 - p.1) as f32);
            if p == q {
                (ex * ex + ey * ey).sqrt()
            } else {
                (ex * dy - ey * dx).abs() / len
            }
        };
        let (k, most) = (a + 1..b)
            .map(|k| (k, far(chain[k])))
            .fold((a, 0.0f32), |m, c| if c.1 > m.1 { c } else { m });
        if most > stray {
            keep.push(k);
            todo.push((a, k));
            todo.push((k, b));
        }
    }
    keep.sort_unstable();
    keep.dedup();
    keep
}

/// A line from tile to tile, and each end's place along its chain.
type Run = ((i32, i32), (i32, i32), [usize; 2]);

/// As good as no straying: only tiles in a row make a line.
const EXACT: f32 = 0.01;

/// `chain` as lines at most [`RUN`] tiles long through its straightened
/// corners, no tile straying more than `stray` from them; a stretch whose
/// lines `fits` won't have is straightened less, down to as it was found.
fn lines(
    chain: &[(i32, i32)],
    stray: f32,
    fits: &dyn Fn((i32, i32), (i32, i32)) -> bool,
) -> Vec<Run> {
    if chain.len() == 1 {
        return vec![(chain[0], chain[0], [0, 0])];
    }
    let mut runs_out = Vec::new();
    for w in straighten(chain, stray).windows(2) {
        let (a, b) = (chain[w[0]], chain[w[1]]);
        let len = (b.0 - a.0).abs().max((b.1 - a.1).abs());
        let parts = ((len + RUN - 1) / RUN).max(1);
        let at = |k: i32| {
            // Rounded as a wall's own line is.
            let r = |d: i32| {
                let n = 2 * k * d;
                (n.abs() + parts) / (2 * parts) * n.signum()
            };
            (a.0 + r(b.0 - a.0), a.1 + r(b.1 - a.1))
        };
        let place = |k: i32| w[0] + (w[1] - w[0]) * k as usize / parts as usize;
        let runs = (0..parts).map(|k| (at(k), at(k + 1), [place(k), place(k + 1)]));
        if stray > EXACT && !runs.clone().all(|(a, b, _)| fits(a, b)) {
            let less = if stray > 0.3 { stray * 0.5 } else { EXACT };
            let part = lines(&chain[w[0]..=w[1]], less, fits);
            runs_out.extend(
                part.into_iter()
                    .map(|(a, b, [pa, pb])| (a, b, [pa + w[0], pb + w[0]])),
            );
        } else {
            runs_out.extend(runs);
        }
    }
    runs_out
}

/// The stretches of `chain` (in order) each drawn as one curve, as their
/// first and last places along it: cut where the cut turns sharper than
/// [`SHARP_TURN`] (between its straightened lines), and into runs of at
/// most [`CURVE_RUN`] tiles. Neighbouring stretches share their end tile.
fn stretches(chain: &[(i32, i32)]) -> Vec<[usize; 2]> {
    let last = chain.len().saturating_sub(1);
    let mut cuts = vec![0];
    for w in straighten(chain, STRAIGHTEN).windows(3) {
        let (a, b, c) = (chain[w[0]], chain[w[1]], chain[w[2]]);
        if turn(a, b, c) > SHARP_TURN {
            cuts.push(w[1]);
        }
    }
    cuts.push(last);
    let mut out = Vec::new();
    for w in cuts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let parts = (b - a).div_ceil(CURVE_RUN).max(1);
        out.extend((0..parts).map(|k| [a + (b - a) * k / parts, a + (b - a) * (k + 1) / parts]));
    }
    out
}

/// How far, in degrees, the way from `a` to `b` turns to go on to `c`.
fn turn(a: (i32, i32), b: (i32, i32), c: (i32, i32)) -> f32 {
    let (u, v) = (
        ((b.0 - a.0) as f32, (b.1 - a.1) as f32),
        ((c.0 - b.0) as f32, (c.1 - b.1) as f32),
    );
    let cross = u.0 * v.1 - u.1 * v.0;
    let dot = u.0 * v.0 + u.1 * v.1;
    cross.atan2(dot).abs().to_degrees()
}

/// One smooth path along the tiles `span` of the cut (in order), as a
/// player drags a wall: from the middle of its first tile to the middle of
/// its last, through a point about every [`CURVE_STEP`] tiles taken from
/// the tiles round it (so the cut's steps are smoothed away), with as few
/// sharp corners as keep it within the sim's bend limit. `None` for a
/// stretch shorter than [`CURVE_LEAST`].
fn smooth(span: &[(i32, i32)]) -> Option<DrawnPath> {
    let n = span.len();
    if n < CURVE_LEAST {
        return None;
    }
    let count = ((n - 1).div_ceil(CURVE_STEP) + 1).clamp(3, drawn::MAX_POINTS);
    let half = |v: f32| (2.0 * v + 1.0).round() as i16;
    let mut points: Vec<(i16, i16)> = Vec::with_capacity(count);
    for j in 0..count {
        let k = (j * (n - 1) + (count - 1) / 2) / (count - 1);
        let round = if j == 0 || j + 1 == count {
            &span[k..=k]
        } else {
            &span[k.saturating_sub(2)..=(k + 2).min(n - 1)]
        };
        let m = round.len() as f32;
        let (x, y) = round.iter().fold((0.0, 0.0), |(x, y), t| {
            (x + t.0 as f32 / m, y + t.1 as f32 / m)
        });
        let p = (half(x), half(y));
        if points.last() != Some(&p) {
            points.push(p);
        }
    }
    let last = points.len().checked_sub(1)?;
    // Smooth all along; else a corner at its middle point, two smooth
    // halves; else a corner at every point (still the cut's line without
    // its steps).
    let mid = last / 2;
    let sharp: [&dyn Fn(usize) -> bool; 3] = [&|_| false, &|k| k == mid, &|_| true];
    sharp.into_iter().find_map(|sharp| {
        let path = DrawnPath::new(&points, |k| k == 0 || k == last || sharp(k))?;
        (path.check().is_ok() && path.bends_gently() && path.ends() == (span[0], span[n - 1]))
            .then_some(path)
    })
}

/// `path` drawn the other way, if that covers the same tiles (the curve's
/// rounding could move one); else `path` as it is.
fn reversed(path: &DrawnPath) -> DrawnPath {
    let points: Vec<(i16, i16)> = path.points().iter().rev().copied().collect();
    let last = points.len() - 1;
    let Some(back) = DrawnPath::new(&points, |k| path.corner(last - k)) else {
        return *path;
    };
    let sorted = |p: &DrawnPath| {
        let mut t = p.wall_tiles();
        t.sort_unstable();
        t
    };
    if sorted(&back) == sorted(path) {
        back
    } else {
        *path
    }
}

/// Lay the ring out on `site`, its front (the middle of its gate, or
/// where its gate would be) at tile `front`. `None` when no ring can be
/// laid: nothing is kept, or the kept ground runs to the square's rim.
pub(super) fn lay(site: &Site, front: Option<(i32, i32)>) -> Option<Layout> {
    let n = site.ground.len();
    if !site.keep.iter().any(|&k| k) {
        return None;
    }
    let cut = cut(site)?;
    let mut in_cut = vec![false; n];
    for &i in &cut {
        in_cut[i] = true;
    }
    let inside = site.spread((0..n).filter(|&i| site.keep[i]), |i| in_cut[i]);
    // The cut in order round the town (round each part of it, where the
    // cut leaves it in parts), in chains between the cliffs and walls that
    // close the rest; ground that can't be walled now is a gap.
    let wall = |i: usize| in_cut[i] && site.ground[i] == Ground::Open;
    let mut round: Vec<usize> = Vec::new();
    let mut chains: Vec<(Vec<(i32, i32)>, usize)> = Vec::new();
    let mut seen = vec![false; n];
    let mut left = inside.clone();
    while let Some(start) = left.iter().position(|&l| l) {
        let part = site.spread(std::iter::once(start), |i| !inside[i]);
        for i in (0..n).filter(|&i| part[i]) {
            left[i] = false;
        }
        let edge = outline(site, &part);
        let base = round.len();
        let first = edge.iter().position(|&i| !wall(i)).unwrap_or(0);
        let mut open: Option<(Vec<(i32, i32)>, usize)> = None;
        for k in 0..edge.len() {
            let i = edge[(first + k) % edge.len()];
            if wall(i) && !seen[i] {
                seen[i] = true;
                let chain =
                    open.get_or_insert_with(|| (Vec::new(), base + (first + k) % edge.len()));
                chain.0.push(site.tile(i));
            } else {
                chains.extend(open.take());
            }
        }
        chains.extend(open.take());
        round.extend(edge);
    }
    let in_cut = &in_cut;
    let gap = |g: Ground| (0..n).filter(move |&i| in_cut[i] && site.ground[i] == g);
    let core = site.core();
    let ok = |t: (i32, i32)| {
        site.index(t).is_some_and(|i| {
            site.ground[i] == Ground::Open
                && !core[i]
                && !site.out[i]
                && !site.clear[i]
                && !site.rim(i)
        })
    };
    let fits =
        |a: (i32, i32), b: (i32, i32)| wall_tiles(a, b).is_some_and(|t| t.into_iter().all(ok));
    // Each stretch of the cut as one smooth curve where it can be (with
    // `curved`), else as lines straightened by `stray`.
    let build = |stray: f32, curved: bool| {
        let mut pieces = Vec::new();
        let mut laid = vec![false; n];
        let mut lay_path = |path: DrawnPath, at: [usize; 2]| {
            for t in path.wall_tiles() {
                if let Some(i) = site.index(t) {
                    laid[i] = true;
                }
            }
            let (from, to) = path.ends();
            pieces.push(Piece {
                from,
                to,
                path,
                round: 0,
                at,
            });
        };
        for (chain, from) in &chains {
            let spans = if curved {
                stretches(chain)
            } else {
                vec![[0, chain.len() - 1]]
            };
            for [sa, sb] in spans {
                let span = &chain[sa..=sb];
                let curve = (curved.then(|| smooth(span)).flatten())
                    .filter(|p| p.wall_tiles().into_iter().all(ok));
                if let Some(path) = curve {
                    lay_path(path, [from + sa, from + sb]);
                    continue;
                }
                for (a, b, [pa, pb]) in lines(span, stray, &fits) {
                    lay_path(DrawnPath::straight(a, b), [from + sa + pa, from + sa + pb]);
                }
            }
        }
        // The ring must still close: the kept ground never reaches the
        // rim, the gaps on ground to clear or that can't be walled aside.
        let gap = |i: usize| in_cut[i] && site.ground[i] != Ground::Open;
        let town = site.spread((0..n).filter(|&i| core[i]), |i| laid[i] || gap(i));
        let open = (0..n).any(|i| town[i] && (site.rim(i) || site.out[i]));
        (!open).then_some((pieces, town))
    };
    let (pieces, town) = build(STRAIGHTEN, true)
        .or_else(|| build(STRAIGHTEN, false))
        .or_else(|| build(EXACT, false))?;
    let mut layout = Layout {
        pieces,
        closes: gap(Ground::Hard).next().is_none(),
        clear: gap(Ground::Grown).map(|i| (site.tile(i), 0)).collect(),
        town,
        order: round,
    };
    layout.face(site, front);
    Some(layout)
}

/// Where the way out of the town toward `toward` (a tile, far off or
/// near) crosses the ring `layout` laid on `site`, and the way it goes
/// there: the place for a gate. `None` when no way leads out (the ground
/// closes the town of itself, or the ring is closed already).
pub(super) fn way_out(site: &Site, layout: &Layout, toward: (i32, i32)) -> Option<Door> {
    let n = site.ground.len();
    let inside = &layout.town;
    // The shortest ways from the kept ground, walls not yet built aside.
    let mut came = vec![usize::MAX; n];
    let mut dist = vec![u32::MAX; n];
    let mut todo = std::collections::VecDeque::new();
    for i in (0..n).filter(|&i| site.keep[i]) {
        dist[i] = 0;
        todo.push_back(i);
    }
    while let Some(i) = todo.pop_front() {
        for b in site.beside(i) {
            if dist[b] == u32::MAX && site.walked(b) {
                dist[b] = dist[i] + 1;
                came[b] = i;
                todo.push_back(b);
            }
        }
    }
    // The way out that is shortest in all: to the rim, then straight on.
    let on = |i: usize| {
        let t = site.tile(i);
        let (dx, dy) = ((t.0 - toward.0) as f32, (t.1 - toward.1) as f32);
        (dx * dx + dy * dy).sqrt()
    };
    let exit = (0..n)
        .filter(|&i| site.rim(i) && dist[i] != u32::MAX)
        .min_by(|&a, &b| (dist[a] as f32 + on(a)).total_cmp(&(dist[b] as f32 + on(b))))?;
    let mut way = vec![exit];
    while came[*way.last()?] != usize::MAX {
        way.push(came[*way.last()?]);
    }
    way.reverse();
    let k = way.iter().rposition(|&i| inside[i])? + 1;
    if k >= way.len() {
        return None;
    }
    let (a, b) = (
        site.tile(way[k.saturating_sub(3)]),
        site.tile(way[(k + 3).min(way.len() - 1)]),
    );
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let out = if dx.abs() >= dy.abs() {
        (dx.signum(), 0)
    } else {
        (0, dy.signum())
    };
    Some(Door {
        mid: site.tile(way[k]),
        out: if out == (0, 0) { (1, 0) } else { out },
    })
}

/// The places to try for a gate at `door`: there, then a little along the
/// wall either way, then a little further out or in.
pub(super) fn door_places(door: Door) -> impl Iterator<Item = Door> {
    let (ax, ay) = (door.out.1.abs(), door.out.0.abs());
    [0, 1, -1, 2].into_iter().flat_map(move |o| {
        [0, 1, -1, 2, -2, 3, -3].into_iter().map(move |a| Door {
            mid: (
                door.mid.0 + ax * a + door.out.0 * o,
                door.mid.1 + ay * a + door.out.1 * o,
            ),
            out: door.out,
        })
    })
}

/// Whether a gate can stand at `door` on `site` going by the ground: its
/// tiles open and not kept, and walked ground behind and in front.
pub(super) fn door_fits(site: &Site, door: Door) -> bool {
    let open = |t: (i32, i32), kept: bool| {
        site.index(t).is_some_and(|i| {
            !site.rim(i) && site.walked(i) && (kept || !site.keep[i]) && !site.out[i]
        })
    };
    door.tiles().into_iter().all(|t| {
        open(t, false)
            && site
                .index(t)
                .is_some_and(|i| site.ground[i] == Ground::Open)
    }) && open(door.step(-1), true)
        && open(door.step(1), false)
        && open(door.step(2), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A site from a picture: `.` open, `^` shut, `#` walled, `o` hard,
    /// `K` kept (open).
    fn site(rows: &[&str]) -> Site {
        let side = rows.len() as i32;
        let mut s = Site::new((100, 200), side);
        for (y, row) in rows.iter().enumerate() {
            assert_eq!(row.len() as i32, side, "a square picture");
            for (x, c) in row.chars().enumerate() {
                let i = y * side as usize + x;
                s.ground[i] = match c {
                    '^' => Ground::Shut,
                    '#' => Ground::Walled,
                    'o' => Ground::Hard,
                    'g' => Ground::Grown,
                    _ => Ground::Open,
                };
                s.keep[i] = c == 'K';
            }
        }
        s
    }

    /// The site with the layout's lines drawn on it as `w`.
    fn drawn(s: &Site, l: &Layout) -> Vec<String> {
        let mut rows: Vec<Vec<char>> = (0..s.side)
            .map(|y| {
                (0..s.side)
                    .map(|x| {
                        let i = (y * s.side + x) as usize;
                        match s.ground[i] {
                            _ if s.keep[i] => 'K',
                            Ground::Shut => '^',
                            Ground::Walled => '#',
                            Ground::Hard => 'o',
                            Ground::Grown => 'g',
                            Ground::Open => '.',
                        }
                    })
                    .collect()
            })
            .collect();
        for p in &l.pieces {
            for t in p.path.wall_tiles() {
                let i = s.index(t).unwrap();
                rows[i / s.side as usize][i % s.side as usize] = 'w';
            }
        }
        rows.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    /// The wall tiles the layout draws.
    fn tiles(l: &Layout) -> usize {
        let mut all: Vec<_> = (l.pieces.iter())
            .flat_map(|p| p.path.wall_tiles())
            .collect();
        all.sort_unstable();
        all.dedup();
        all.len()
    }

    /// Whether the kept ground is closed in once the layout is drawn.
    fn closed(s: &Site, l: &Layout) -> bool {
        let mut laid = vec![false; s.ground.len()];
        for p in &l.pieces {
            for t in p.path.wall_tiles() {
                laid[s.index(t).unwrap()] = true;
            }
        }
        let n = s.ground.len();
        let core = s.core();
        let town = s.spread((0..n).filter(|&i| core[i]), |i| laid[i]);
        !(0..n).any(|i| town[i] && s.rim(i))
    }

    #[test]
    fn a_ring_on_open_ground_goes_tight_round_what_is_kept() {
        let s = site(&[
            ".........",
            ".........",
            ".........",
            "...KKK...",
            "...KKK...",
            "...KKK...",
            ".........",
            ".........",
            ".........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        // Four sides of three; the corners close on the slant.
        assert_eq!(tiles(&l), 12, "{:#?}", drawn(&s, &l));
    }

    #[test]
    fn cliffs_and_standing_walls_are_part_of_the_ring_for_nothing() {
        let s = site(&[
            "...........",
            "..^^^^^^^..",
            "..^.....^..",
            "..^.KKK.^..",
            "..^.KKK.^..",
            "..^.....^..",
            "..^.....#..",
            "..^.....#..",
            "..^.....^..",
            "...........",
            "...........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        // Only the mouth of the valley is walled, five tiles across.
        assert_eq!(tiles(&l), 5, "{:#?}", drawn(&s, &l));
    }

    #[test]
    fn ground_that_cant_be_walled_is_gone_round() {
        let s = site(&[
            "...........",
            "..^^^^^^^..",
            "..^.....^..",
            "..^.KKK.^..",
            "..^.....^..",
            "..^..o..^..",
            "..^.....^..",
            "..^.....^..",
            "...........",
            "...........",
            "...........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        assert_eq!(tiles(&l), 5, "{:#?}", drawn(&s, &l));
        // With nowhere else to close, the gap is left and said.
        let s = site(&[
            ".........",
            ".^^^^^^^.",
            ".^.....^.",
            ".^.KKK.^.",
            ".^ooooo^.",
            ".^ooooo^.",
            ".^ooooo^.",
            ".^ooooo^.",
            ".........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(!l.closes);
    }

    #[test]
    fn woods_are_gone_round_a_little_way_or_cleared() {
        // A copse a tile deep across the valley's mouth: the wall goes a
        // tile further out, round it.
        let s = site(&[
            "...........",
            "..^^^^^^^..",
            "..^.....^..",
            "..^.KKK.^..",
            "..^.KKK.^..",
            "..^.KKK.^..",
            "..^ggggg^..",
            "..^.....^..",
            "..^.....^..",
            "...........",
            "...........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(
            l.closes && l.clear.is_empty() && closed(&s, &l),
            "{:#?}",
            drawn(&s, &l)
        );
        assert_eq!(tiles(&l), 5);
        // Woods all the way out: cleared, where the ring is shortest.
        let s = site(&[
            "...........",
            "..^^^^^^^..",
            "..^.....^..",
            "..^.KKK.^..",
            "..^.KKK.^..",
            "..^KKKKK^..",
            "..^ggggg^..",
            "..^ggggg^..",
            "..^ggggg^..",
            "ggggggggggg",
            "...........",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && l.pieces.is_empty(), "{:#?}", drawn(&s, &l));
        assert_eq!(l.clear.len(), 5);
    }

    #[test]
    fn a_town_the_cut_leaves_in_two_parts_is_ringed_round_both() {
        // Two kept blocks far apart: two short rings, not one long one.
        let mut rows = vec![".".repeat(31); 31];
        for (x0, y0) in [(4, 14), (24, 14)] {
            for row in rows.iter_mut().skip(y0).take(3) {
                row.replace_range(x0..x0 + 3, "KKK");
            }
        }
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let s = site(&rows);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        assert_eq!(tiles(&l), 24, "{:#?}", drawn(&s, &l));
    }

    #[test]
    fn a_slanting_side_is_drawn_as_long_lines() {
        // A long building row lying on a slant of one in two.
        let mut rows = vec![String::new(); 23];
        for (y, row) in rows.iter_mut().enumerate() {
            for x in 0..23i32 {
                let on = (2 * (y as i32 - 11) - (x - 11)).abs() <= 2 && (4..=18).contains(&x);
                row.push(if on { 'K' } else { '.' });
            }
        }
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let s = site(&rows);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        // Its two long sides are lines, not a step a tile.
        assert!(
            l.pieces.len() * 3 <= tiles(&l),
            "{} lines for {} tiles: {:#?}",
            l.pieces.len(),
            tiles(&l),
            drawn(&s, &l)
        );
    }

    #[test]
    fn a_round_town_is_ringed_with_smooth_curves() {
        // A disc of kept ground, radius 7, on open ground.
        let side = 25i32;
        let rows: Vec<String> = (0..side)
            .map(|y| {
                (0..side)
                    .map(|x| {
                        let (dx, dy) = (x - 12, y - 12);
                        if dx * dx + dy * dy <= 49 { 'K' } else { '.' }
                    })
                    .collect()
            })
            .collect();
        let rows: Vec<&str> = rows.iter().map(String::as_str).collect();
        let s = site(&rows);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        // Most of it is curves through several points, not lines.
        let curved: usize = (l.pieces.iter())
            .filter(|p| p.path.points().len() > 2)
            .map(|p| p.path.wall_tiles().len())
            .sum();
        assert!(
            curved * 2 >= tiles(&l),
            "{:#?} {:?}",
            drawn(&s, &l),
            l.pieces
        );
        // Every curve is one the sim takes, and no longer than the stepped
        // cut it smooths (about a tile a tile round, 2 pi times 8).
        assert!(l.pieces.iter().all(|p| p.path.bends_gently()));
        assert!(tiles(&l) <= 56, "{} tiles: {:#?}", tiles(&l), drawn(&s, &l));
    }

    #[test]
    fn a_stepped_slant_is_smoothed_into_one_curve() {
        // A cut stepping one across, two down: one path, a few points,
        // never more than a tile off the steps.
        let chain: Vec<(i32, i32)> = (0..15).map(|k| (10 + k / 2, 20 + k)).collect();
        let path = smooth(&chain).unwrap();
        assert!(path.points().len() <= 5, "{:?}", path.points());
        assert_eq!(path.ends(), (chain[0], chain[14]));
        for t in path.wall_tiles() {
            let near = chain
                .iter()
                .any(|c| (c.0 - t.0).abs().max((c.1 - t.1).abs()) <= 1);
            assert!(near, "{t:?} strays: {:?}", path.wall_tiles());
        }
        // Too short to be worth a curve.
        assert!(smooth(&chain[..4]).is_none());
        // A right-angled turn is cut there and stays a corner.
        let mut l: Vec<(i32, i32)> = (0..8).map(|k| (k, 0)).collect();
        l.extend((1..8).map(|k| (7, k)));
        assert_eq!(stretches(&l), vec![[0, 7], [7, 14]]);
        // Long stretches are cut into runs.
        let long: Vec<(i32, i32)> = (0..40).map(|k| (k, 0)).collect();
        let runs = stretches(&long);
        assert!(runs.len() == 3 && runs.iter().all(|r| r[1] - r[0] <= CURVE_RUN));
        // Turned the other way, it covers the same tiles.
        let mut a = path.wall_tiles();
        let mut b = reversed(&path).wall_tiles();
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }

    #[test]
    fn the_ring_bends_out_to_a_crest_or_bank_close_by() {
        // A valley five across: open ground two tiles out from the kept
        // ground's edge holds a wall well (a crest, say).
        let rows = [
            "...........",
            "..^^^^^^^..",
            "..^.....^..",
            "..^.KKK.^..",
            "..^.KKK.^..",
            "..^.....^..",
            "..^.....^..",
            "..^.....^..",
            "..^.....^..",
            "...........",
            "...........",
        ];
        let row = |l: &Layout, s: &Site| {
            let mut ys: Vec<i32> = (l.pieces.iter())
                .flat_map(|p| p.path.wall_tiles())
                .map(|t| t.1 - s.origin.1)
                .collect();
            ys.sort_unstable();
            ys.dedup();
            ys
        };
        let s = site(&rows);
        let near = row(&lay(&s, None).unwrap(), &s);
        assert!(
            near.iter().all(|&y| y <= 5),
            "tight round what is kept: {near:?}"
        );
        // A crest across the valley two rows further out: the wall stands
        // on it, as long as it is.
        let mut s = site(&rows);
        for x in 3..8 {
            let i = s.index((100 + x, 207)).unwrap();
            s.vantage[i] = true;
        }
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        assert_eq!(row(&l, &s), vec![7], "{:#?}", drawn(&s, &l));
        // A crest far round a town in the open is much longer to wall
        // along than the tight ring, and is left.
        let mut s = site(&[".".repeat(15).as_str(); 15]);
        for y in 6..9 {
            for x in 6..9 {
                s.keep[(y * 15 + x) as usize] = true;
            }
        }
        for i in 0..s.ground.len() {
            let (x, y) = ((i % 15) as i32, (i / 15) as i32);
            s.vantage[i] = (x - 7).abs().max((y - 7).abs()) == 5;
        }
        let l = lay(&s, None).unwrap();
        assert!(l.closes && closed(&s, &l));
        assert_eq!(tiles(&l), 12, "{:#?}", drawn(&s, &l));
    }

    #[test]
    fn the_gate_goes_where_the_way_out_crosses_the_ring() {
        let mut s = site(&[
            "...............",
            "...............",
            "...............",
            "...............",
            "...............",
            ".....KKKKK.....",
            ".....KKKKK.....",
            ".....KKKKK.....",
            ".....KKKKK.....",
            ".....KKKKK.....",
            "...............",
            "...............",
            "...............",
            "...............",
            "...............",
        ]);
        // The enemy is far to the east.
        let door = way_out(&s, &lay(&s, None).unwrap(), (400, 207)).unwrap();
        assert_eq!(door.out, (1, 0));
        assert_eq!(door.mid.0, 110, "just east of the kept ground");
        assert!(door_fits(&s, door));
        s.set_door(door);
        let l = lay(&s, Some(door.mid)).unwrap();
        assert!(l.closes && closed(&s, &l), "{:#?}", drawn(&s, &l));
        // The wall runs on from both ends of the gate, nearest first.
        let near: Vec<_> = l.pieces.iter().take(2).collect();
        for end in door.beside() {
            assert!(
                near.iter().any(|p| p.from == end || p.to == end),
                "{:#?} {:?}",
                drawn(&s, &l),
                l.pieces
            );
        }
        // The ground in front of the gate is outside the town.
        let n = s.ground.len();
        let mut laid = vec![false; n];
        for p in &l.pieces {
            for t in p.path.wall_tiles() {
                laid[s.index(t).unwrap()] = true;
            }
        }
        let town = s.spread((0..n).filter(|&i| s.keep[i]), |i| laid[i]);
        assert!(!town[s.index(door.step(1)).unwrap()]);
        assert!(town[s.index(door.step(-1)).unwrap()]);
    }

    #[test]
    fn nothing_is_laid_where_the_ground_closes_the_town_itself() {
        let s = site(&[
            ".......", ".^^^^^.", ".^...^.", ".^.K.^.", ".^...^.", ".^^^^^.", ".......",
        ]);
        let l = lay(&s, None).unwrap();
        assert!(l.closes && l.pieces.is_empty());
        assert_eq!(way_out(&s, &l, (300, 203)), None);
    }

    #[test]
    fn no_ring_where_the_kept_ground_runs_to_the_rim() {
        let mut s = site(&[".....", ".....", "..K..", ".....", "....."]);
        s.keep[2 * 5 + 1] = true;
        s.keep[2 * 5] = true;
        assert!(lay(&s, None).is_none());
    }

    #[test]
    fn a_town_on_two_shelves_keeps_the_one_its_hall_is_on() {
        let rows = [
            ".........",
            ".........",
            "..KK^KK..",
            "..KK^KK..",
            "....^....",
            "....^....",
            "....^....",
            ".........",
            ".........",
        ];
        // Kept ground over a cliff from the hall's is left out.
        let mut s = site(&rows);
        s.keep_joined((102, 202));
        assert!(!s.keep[s.index((106, 202)).unwrap()]);
        assert!(s.keep[s.index((103, 203)).unwrap()]);
        // With a kept lane round the cliff's end, both are one town.
        let mut s = site(&rows);
        for x in 102..=106 {
            s.keep_tile((x, 201));
        }
        s.keep_joined((102, 202));
        assert!(s.keep[s.index((106, 203)).unwrap()]);
        // A wall of its own planned across the lane will shut it: what is
        // past it is left out, though the tile is still open ground.
        let mut s = site(&rows);
        for x in 102..=106 {
            s.keep_tile((x, 201));
        }
        let gap = s.index((104, 201)).unwrap();
        s.planned[gap] = true;
        s.keep_joined((102, 202));
        assert!(!s.keep[s.index((106, 203)).unwrap()]);
        assert!(s.keep[s.index((103, 203)).unwrap()]);
    }
}
