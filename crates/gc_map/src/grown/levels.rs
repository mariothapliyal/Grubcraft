//! Water that runs downhill (docs/27 stage 5): the water's own level on
//! each tile, by body of water.
//!
//! High water is cut into **bodies**, each flat: a highland lake, or a
//! reach of about [`REACH`] tiles of a river in the hills. Every tile of a
//! body has its level; water stands on the tile where the ground is under
//! it. Water at the map's line (body 0) is the low water every map had
//! before.
//!
//! Two rules keep the water believable, and are kept again whenever the
//! land changes under it ([`settle`]):
//!
//! - **It stays in its banks.** No dry tile next to a body's water lies
//!   under that water's level: a body whose bank is cut lower drops to it.
//! - **It runs downhill.** Along each river, through any lake it crosses,
//!   no body stands higher than the one above it.
//!
//! Falls are where one reach meets a lower one.

use super::WATER_LINE;
use super::field::{Grid, ONE, nearest};

/// About how many tiles of river one flat reach covers.
pub(super) const REACH: usize = 16;
/// Water whose own level would be less than this above the map's line
/// stands at the line, as all water did before stage 5.
pub(super) const HIGH: i32 = ONE / 2;
/// How far under the lowest dry ground beside it a body's water stands.
const MARGIN: i32 = ONE / 16;
/// Dry tiles this many whole tiles from high water, and no lower than its
/// level, take its level: the water's surface stays flat out to its shore,
/// and a flood raises it over its own banks.
const SHORE_BAND: i32 = 6;
/// The most rounds of keeping the water in before giving up on the bodies
/// still leaking (they drop to the line).
const ROUNDS: usize = 64;

/// The bodies of high water and the order the rivers run through them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Bodies {
    /// Each body's level (fixed-point tiles); body 0 is the map's line.
    level: Vec<i32>,
    /// The bodies along each river, from its source down (0 where it runs
    /// at the line).
    chains: Vec<Vec<u16>>,
}

impl Bodies {
    pub(super) fn new() -> Bodies {
        Bodies {
            level: vec![WATER_LINE],
            chains: Vec::new(),
        }
    }

    /// A new body at `level`, or body 0 if that is under [`HIGH`] over the
    /// line. Panics past `u16::MAX` bodies (a map has a few hundred).
    pub(super) fn add(&mut self, level: i32) -> u16 {
        if level < WATER_LINE + HIGH {
            return 0;
        }
        self.level.push(level);
        u16::try_from(self.level.len() - 1).expect("bodies of water")
    }

    /// Body `b`'s level.
    pub(super) fn level(&self, b: u16) -> i32 {
        self.level[usize::from(b)]
    }

    /// A river through `bodies` in order from its source (repeats are
    /// dropped).
    pub(super) fn chain(&mut self, bodies: impl IntoIterator<Item = u16>) {
        let mut chain: Vec<u16> = Vec::new();
        for b in bodies {
            if chain.last() != Some(&b) {
                chain.push(b);
            }
        }
        if chain.len() > 1 {
            self.chains.push(chain);
        }
    }

    /// Ease every level with `f` (a map's heights eased under its top are
    /// eased the same way, which keeps their order).
    pub(super) fn map(&mut self, f: impl Fn(i32) -> i32) {
        for l in self.level.iter_mut().skip(1) {
            *l = f(*l);
        }
    }

    /// Lower body `b` to `level`, or to the line if that is under
    /// [`HIGH`] over it. Whether it changed.
    fn lower(&mut self, b: u16, level: i32) -> bool {
        let at = &mut self.level[usize::from(b)];
        let to = if level < WATER_LINE + HIGH {
            WATER_LINE
        } else {
            level
        };
        if to < *at {
            *at = to;
            true
        } else {
            false
        }
    }

    /// Make the water run downhill: down each river, no body higher than
    /// the lowest above it. Whether anything changed.
    pub(super) fn order(&mut self) -> bool {
        let mut changed = false;
        loop {
            let mut again = false;
            for c in 0..self.chains.len() {
                let mut low = i32::MAX;
                for k in 0..self.chains[c].len() {
                    let b = self.chains[c][k];
                    if b == 0 {
                        low = WATER_LINE;
                    } else {
                        again |= self.lower(b, low);
                        low = low.min(self.level(b));
                    }
                }
            }
            changed |= again;
            if !again {
                return changed;
            }
        }
    }

    /// Every pair of high bodies (upper, lower) where a river runs from
    /// the upper on down into the lower, sorted.
    fn falls(&self) -> Vec<(u16, u16)> {
        let mut pairs: Vec<(u16, u16)> = (self.chains.iter())
            .flat_map(|c| {
                c.iter()
                    .enumerate()
                    .flat_map(move |(k, &a)| c[k + 1..].iter().map(move |&b| (a, b)))
            })
            .filter(|&(_, b)| b != 0)
            .collect();
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }

    /// How many bodies stand over the line, for reports.
    #[cfg(test)]
    pub(super) fn lakes_and_reaches(&self) -> usize {
        self.level
            .iter()
            .skip(1)
            .filter(|&&l| l > WATER_LINE)
            .count()
    }

    /// The level on each tile of bodies `body` (fixed-point tiles).
    pub(super) fn per_tile(&self, body: &[u16]) -> Vec<i32> {
        body.iter().map(|&b| self.level(b)).collect()
    }
}

/// Keep the water in its banks and running downhill on land `h` (see the
/// module docs), then give each tile its level, shores included: the
/// levels the map keeps. Where a body drops, its bed drops with it, so a
/// river never runs dry.
pub(super) fn settle(h: &mut Grid, body: &[u16], bodies: &mut Bodies, keep: &[bool]) -> Vec<i32> {
    contain(h, body, bodies);
    let mut level = bodies.per_tile(body);
    shores(h, body, keep, &mut level);
    level
}

/// Lower each body until no dry land beside its water lies under it, and
/// keep the rivers running downhill, until nothing changes. Dry land is
/// any tile above its own water, whichever body it belongs to: a lake
/// beside another river's dry bank spills over it as over open ground.
/// (A river's own lower reach beside it is not a bank, wet or dry: water
/// falls into it.)
fn contain(h: &mut Grid, body: &[u16], bodies: &mut Bodies) {
    let n = h.n as i32;
    let falls_into = bodies.falls();
    let mut was = bodies.level.clone();
    bodies.order();
    for round in 0..=ROUNDS {
        sink_beds(h, body, &was, &bodies.level);
        was.clone_from(&bodies.level);
        // The lowest dry land beside each body's water.
        let mut rim = vec![i32::MAX; bodies.level.len()];
        for y in 0..n {
            for x in 0..n {
                let i = (y * n + x) as usize;
                let b = body[i];
                if b == 0 || h.v[i] >= bodies.level(b) {
                    continue;
                }
                for (dx, dy) in NEIGHBOURS {
                    let (px, py) = (x + dx, y + dy);
                    if !(0..n).contains(&px) || !(0..n).contains(&py) {
                        continue;
                    }
                    let k = (py * n + px) as usize;
                    let c = body[k];
                    if h.v[k] >= bodies.level(c) && falls_into.binary_search(&(b, c)).is_err() {
                        let r = &mut rim[usize::from(b)];
                        *r = (*r).min(h.v[k]);
                    }
                }
            }
        }
        let mut changed = false;
        for (b, &r) in rim.iter().enumerate().skip(1) {
            let b = b as u16;
            if r < bodies.level(b) + MARGIN {
                // (Past the last round, what still leaks drops to the line.)
                let to = if round == ROUNDS {
                    WATER_LINE
                } else {
                    r - MARGIN
                };
                changed |= bodies.lower(b, to);
            }
        }
        changed |= bodies.order();
        if !changed {
            break;
        }
    }
    sink_beds(h, body, &was, &bodies.level);
}

/// A body's bed sinks with its water: each tile under the water at the
/// level it `was` keeps its depth under the level it has `now`.
fn sink_beds(h: &mut Grid, body: &[u16], was: &[i32], now: &[i32]) {
    if was == now {
        return;
    }
    for (i, &b) in body.iter().enumerate() {
        let b = usize::from(b);
        if b != 0 && was[b] != now[b] && h.v[i] < was[b] {
            h.v[i] = (h.v[i] - (was[b] - now[b])).max(0);
        }
    }
}

/// The eight tiles round one.
const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Dry tiles near high water take its level out to [`SHORE_BAND`] tiles
/// where they stand no lower than it: never in the water, but flooded with
/// it. The north decides, and the south takes the same where it is no
/// lower either.
fn shores(h: &Grid, body: &[u16], keep: &[bool], level: &mut [i32]) {
    let n = h.n;
    let nn = n * n;
    let high: Vec<bool> = (0..nn)
        .map(|i| level[i] > WATER_LINE && h.v[i] < level[i])
        .collect();
    if !high.contains(&true) {
        return;
    }
    let (d, near) = nearest(&high, level, n, SHORE_BAND, WATER_LINE);
    // A shore tile stands no lower than the water, and so does all round
    // it, but for that water itself: the last tile before land falls away
    // (or before lower water) keeps the line, so the surface drawn between
    // tiles never climbs over lower ground and no ground below the level
    // reads as water.
    let shore = |i: usize, lv: i32| {
        let (x, y) = ((i % n) as i32, (i / n) as i32);
        body[i] == 0
            && h.v[i] >= lv
            && NEIGHBOURS.iter().all(|&(dx, dy)| {
                let (px, py) = (x + dx, y + dy);
                if !(0..n as i32).contains(&px) || !(0..n as i32).contains(&py) {
                    return true;
                }
                let k = py as usize * n + px as usize;
                h.v[k] >= lv || (high[k] && level[k] == lv)
            })
    };
    let last = nn - 1;
    let mut sets = Vec::new();
    for i in 0..nn.div_ceil(2) {
        let lv = near[i];
        // (Neither a tile nor its turned copy kept for a start, so mirrored
        // land stays mirrored.)
        if lv <= WATER_LINE || d.v[i] > SHORE_BAND * ONE || keep[i] || keep[last - i] {
            continue;
        }
        for k in [i, last - i] {
            if shore(k, lv) {
                sets.push((k, lv));
            }
        }
    }
    for (k, lv) in sets {
        level[k] = lv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bowl in a 24-tile plateau at 6 tiles, its floor at 3, with a notch
    /// cut in its rim down to `notch`.
    fn bowl(notch: i32) -> (Grid, Vec<u16>) {
        let n = 24;
        let mut h = Grid::from_fn(n, |x, y| {
            let (dx, dy) = (x as i32 - 12, y as i32 - 12);
            if dx * dx + dy * dy <= 25 {
                3 * ONE
            } else {
                6 * ONE
            }
        });
        h.v[12 * n + 18] = notch;
        let body = (0..n * n)
            .map(|i| {
                let (dx, dy) = ((i % n) as i32 - 12, (i / n) as i32 - 12);
                u16::from(dx * dx + dy * dy <= 25)
            })
            .collect();
        (h, body)
    }

    #[test]
    fn water_stays_in_its_banks() {
        let keep = vec![false; 24 * 24];
        let (mut h, body) = bowl(6 * ONE);
        let mut bodies = Bodies::new();
        assert_eq!(bodies.add(5 * ONE), 1);
        let level = settle(&mut h, &body, &mut bodies, &keep);
        assert_eq!(bodies.level(1), 5 * ONE, "a whole rim holds");
        assert_eq!(level[12 * 24 + 12], 5 * ONE);
        // A notch cut to 4.5 tiles drops the lake to just under it.
        let (mut h, body) = bowl(4 * ONE + ONE / 2);
        let mut bodies = Bodies::new();
        bodies.add(5 * ONE);
        settle(&mut h, &body, &mut bodies, &keep);
        assert_eq!(bodies.level(1), 4 * ONE + ONE / 2 - MARGIN);
        // Cut to the floor, it drains to the line.
        let (mut h, body) = bowl(2 * ONE);
        let mut bodies = Bodies::new();
        bodies.add(5 * ONE);
        settle(&mut h, &body, &mut bodies, &keep);
        assert_eq!(bodies.level(1), WATER_LINE);
    }

    #[test]
    fn another_bodys_dry_bank_is_a_bank_but_a_lower_reach_is_not() {
        let keep = vec![false; 24 * 24];
        // Beside the lake, a tile of another body (a lake of its own, not
        // on any river with this one) left dry at 4.5 tiles: the lake
        // spills over it.
        let (mut h, mut body) = bowl(6 * ONE);
        let bank = 12 * 24 + 18;
        h.v[bank] = 4 * ONE + ONE / 2;
        let mut bodies = Bodies::new();
        bodies.add(5 * ONE);
        body[bank] = bodies.add(3 * ONE);
        settle(&mut h, &body, &mut bodies, &keep);
        assert_eq!(bodies.level(1), 4 * ONE + ONE / 2 - MARGIN);
        // The same tile as the lake's own lower reach: the water falls
        // into it, and the lake keeps its level.
        let (mut h, mut body) = bowl(6 * ONE);
        h.v[bank] = 4 * ONE + ONE / 2;
        let mut bodies = Bodies::new();
        let lake = bodies.add(5 * ONE);
        let reach = bodies.add(3 * ONE);
        body[bank] = reach;
        bodies.chain([lake, reach, 0]);
        settle(&mut h, &body, &mut bodies, &keep);
        assert_eq!(bodies.level(lake), 5 * ONE);
    }

    #[test]
    fn rivers_run_downhill() {
        let mut bodies = Bodies::new();
        let (a, b, c) = (
            bodies.add(6 * ONE),
            bodies.add(7 * ONE),
            bodies.add(4 * ONE),
        );
        bodies.chain([a, a, b, c, 0]);
        assert!(bodies.order());
        assert_eq!(
            [bodies.level(a), bodies.level(b), bodies.level(c)],
            [6 * ONE, 6 * ONE, 4 * ONE]
        );
        // Once at the line, it stays there.
        let d = bodies.add(5 * ONE);
        bodies.chain([c, 0, d]);
        bodies.order();
        assert_eq!(bodies.level(d), WATER_LINE);
        assert_eq!(
            bodies.add(WATER_LINE + HIGH - 1),
            0,
            "low water is the line"
        );
    }

    #[test]
    fn shores_take_the_level_but_stay_dry() {
        let keep = vec![false; 24 * 24];
        let (mut h, body) = bowl(6 * ONE);
        // A low dip beside the lake, under its level: no shore level there.
        h.v[12 * 24 + 19] = 4 * ONE;
        let mut bodies = Bodies::new();
        bodies.add(4 * ONE + ONE / 2);
        let level = settle(&mut h, &body, &mut bodies, &keep);
        let lv = bodies.level(1);
        for (i, &l) in level.iter().enumerate() {
            if body[i] == 0 && l != WATER_LINE {
                assert!(h.v[i] >= l, "shore tile {i} under its level");
            }
        }
        assert_eq!(
            level[4 * 24 + 12],
            lv,
            "the rim beside takes the lake's level"
        );
    }
}
