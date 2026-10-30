//! The water engine (docs/39-water-engine.md, stage 1): every river
//! carries its own water.
//!
//! The rivers come from the map's flow layer ([`crate::Flow`]): tiles of
//! running water side by side are one river (joined rivers are one), and
//! each tile's reach is how far it lies up the river from where the river
//! ends (the map's edge, or a lake or pond it runs into). Maps with still
//! water have no rivers, so nothing here runs on them.
//!
//! Each river is cut into **stretches** of [`STRETCH_TILES`] of reach, one
//! tree per river from its top to its mouth (a fork has two stretches
//! above it, never two below). Once a second, rain on a stretch's tiles
//! (and the spring melt) fills its store, wetter ground passing on more,
//! and a share of every store flows on to the stretch below
//! ([`OUTFLOW_DIVISOR`]), so long rain raises a big river's lower reaches
//! most, and later than its head. A stretch's store lifts its water
//! ([`Rivers::swell`]).
//!
//! A storm in changing weather may set off a **front** (a flash flood): a
//! crest that runs down from the upper river at [`WAVE_TILES_PER_SECOND`],
//! drawn from a heavy tail, so most are small and a few are huge
//! ([`Band`]). As it passes a tile the water there stays at the crest for
//! [`HIGH_SECONDS`] and falls over [`FALL_SECONDS`]. Major and
//! catastrophic ones are told map-wide well before they come.
//!
//! Each ford (a connected group of ford tiles on a river, with the summer
//! fords there) belongs to one stretch. It shuts once the water over it
//! (the map's rise, the stretch's swell and any front) reaches a full
//! flood, and opens once it falls under half of one, so the walkable grid
//! changes twice a flood per ford.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::TICKS_PER_SECOND;
use crate::flood::FLOOD_RISE;
use crate::hash::StateHasher;
use crate::sky::{Season, Spell, WET_FULL, WeatherKind};
use crate::terrain::{Flow, HEIGHT_UNITS, Terrain};

/// Tiles of reach in a stretch.
pub const STRETCH_TILES: u16 = 16;
/// How fast a front runs down the river.
pub const WAVE_TILES_PER_SECOND: u32 = 6;
/// How long the water stays up where a front has passed.
pub const HIGH_SECONDS: u32 = 20;
/// How long it then takes to fall back.
pub const FALL_SECONDS: u32 = 30;
/// How long before a front reaches its first ford the warning comes.
pub const WARNING_SECONDS: u32 = 8;
/// A spell of rain at least this strong is a storm.
pub const STORM_STRENGTH: u8 = 220;
/// How far down the river a front carries troops it sweeps away, tiles.
pub const CARRY_TILES: u32 = 10;
/// The share of a regiment swept away that drowns, in percent (rounded
/// down, so the last soldier never does).
pub const DROWN_PERCENT: u32 = 30;
/// The morale a regiment swept away loses.
pub const SWEPT_AWAY_MORALE: i16 = 25;
/// Most fronts running at once; another storm's adds to the biggest.
pub const MAX_FRONTS: usize = 4;
/// A stretch's store flows on to the one below at this share a second
/// (one in eight): a swell runs down a river at about two tiles a second.
pub const OUTFLOW_DIVISOR: u64 = 8;
/// Store that lifts a stretch's water one height unit, per tile of the
/// stretch: long heavy rain lifts the stretch ten stretches down a river
/// about a fifth of a tile.
pub const STORE_PER_UNIT: u64 = 320;
/// The most a stretch's store lifts its water: half a tile.
pub const MAX_SWELL: u16 = (HEIGHT_UNITS / 2) as u16;
/// Rain the spring melt is worth on a stretch, as a strength (0 to 255).
pub const MELT_RAIN: u64 = 120;
/// Furthest above its first ford a front sets off, in tiles: today's
/// warning twice over.
const SET_OFF_ABOVE: u16 = (WARNING_SECONDS * 2 * WAVE_TILES_PER_SECOND) as u16;
/// A front's crest falls an eighth for every this many tiles it runs.
const CREST_FADE_TILES: u32 = 128;

const NONE: u16 = u16::MAX;

/// How big a flood is, by the crest where it reaches a river's first
/// ford (docs/39-water-engine.md, section 2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
    /// Wading is deeper and the current stronger; no ford shuts.
    Minor,
    /// The fords shut (today's flash flood).
    Flood,
    /// The banks go under. Told map-wide [`Band::lead_seconds`] ahead.
    Major,
    /// The valley floor floods. Told map-wide [`Band::lead_seconds`] ahead.
    Catastrophic,
}

impl Band {
    /// The crest, in height units, a band has at the first ford it reaches.
    pub const fn crest(self) -> u16 {
        match self {
            Band::Minor => (HEIGHT_UNITS / 5) as u16,
            Band::Flood => (HEIGHT_UNITS * 3 / 8) as u16,
            Band::Major => (HEIGHT_UNITS * 6 / 10) as u16,
            Band::Catastrophic => (HEIGHT_UNITS * 9 / 10) as u16,
        }
    }

    /// The band of a crest of `crest` height units.
    pub const fn of(crest: u16) -> Band {
        if crest as u32 >= HEIGHT_UNITS * 8 / 10 {
            Band::Catastrophic
        } else if crest as u32 >= HEIGHT_UNITS / 2 {
            Band::Major
        } else if crest >= FLOOD_RISE {
            Band::Flood
        } else {
            Band::Minor
        }
    }

    /// Seconds at least between a front of this band being told map-wide
    /// and its reaching the first ford (0: only the river's own warning).
    pub const fn lead_seconds(self) -> u32 {
        match self {
            Band::Minor | Band::Flood => 0,
            Band::Major => 15,
            Band::Catastrophic => 30,
        }
    }
}

/// The rivers of a map, their stretches and water, their fords, and the
/// fronts running down them.
#[derive(Clone, Debug, Default)]
pub struct Rivers {
    seed: u64,
    /// The map's side, in tiles.
    side: i32,
    /// One per tile: tiles up the river from where it ends (`NONE` off
    /// the rivers).
    reach: Arc<[u16]>,
    /// One per tile: its stretch (`NONE` off the rivers).
    stretch_of: Arc<[u16]>,
    stretches: Arc<[Stretch]>,
    /// The fords, one group each, by stretch.
    groups: Arc<[Group]>,
    /// Every ford tile on a river, as `(y, x)`, sorted, with its group.
    ford_tiles: Arc<[((i32, i32), u16)]>,
    /// The water in each stretch beyond its normal flow.
    store: Vec<u64>,
    /// What each stretch passes on this second (kept for its room).
    outflow: Vec<u64>,
    /// How far each stretch's store lifts its water, height units.
    swell: Vec<u16>,
    /// Whether each group's fords are shut by the river (not the map-wide
    /// flood, which shuts every ford at once; see [`crate::flood`]).
    shut: Vec<bool>,
    fronts: [Option<Front>; MAX_FRONTS],
    /// The fords the minds keep off: of groups shut, or a front is
    /// coming down on. As `(y, x)`, sorted.
    threatened: Vec<(i32, i32)>,
}

/// A stretch of river.
#[derive(Clone, Copy, Debug)]
struct Stretch {
    /// The stretch below (`NONE` at a river's mouth).
    below: u16,
    /// Its place in a walk of its river's tree from the mouth, and the
    /// last place under it: stretch `a` is downstream of (or is) `b` when
    /// `a.tin <= b.tin` and `b.tout <= a.tout`.
    tin: u16,
    tout: u16,
    /// The stretch above it with the most land (`NONE` at a river's top),
    /// up which a front sets off.
    up: u16,
    /// Its own tiles, and those of every stretch above it.
    own: u32,
    land: u32,
    /// The furthest up the river any of its tiles lies.
    top: u16,
}

impl Stretch {
    /// Whether `self` is downstream of `other` (or is it).
    fn holds(&self, other: &Stretch) -> bool {
        self.tin <= other.tin && other.tout <= self.tout
    }
}

/// One ford and the summer fords on it.
#[derive(Clone, Debug)]
struct Group {
    stretch: u16,
    /// The furthest up and the nearest the mouth any of them lies.
    first: u16,
    last: u16,
    /// The ford tiles, and the summer fords on the river, as `(y, x)`,
    /// sorted.
    fords: Vec<(i32, i32)>,
    /// They as runs along each row (`[x, y, w, 1]`).
    runs: Vec<[i32; 4]>,
}

/// A front running down a river.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Front {
    /// The stretch it set off in, and the reach it set off from.
    start: u16,
    top: u16,
    /// Its crest where it sets off, height units.
    crest: u16,
    /// Its band at the first ford it reaches.
    band: Band,
    /// The first ford's reach.
    first: u16,
    /// Ticks since it was rolled, and the ticks it waits before it sets
    /// off (so a big one is told long enough ahead).
    ticks: u32,
    delay: u32,
    /// Whether the river's warning has been given.
    warned: bool,
}

impl Front {
    /// Its band at the first ford it reaches.
    pub fn band(&self) -> Band {
        self.band
    }

    /// Ticks since it set off (0 while it waits).
    fn running(&self) -> u32 {
        self.ticks.saturating_sub(self.delay)
    }
}

/// Where a flash flood is, as told (see [`crate::events::Event::FlashFlood`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stage {
    /// A front is [`WARNING_SECONDS`] from the river's first ford.
    Coming,
    /// The river has shut fords.
    Here,
    /// The water has fallen and the fords are open again.
    Gone,
}

/// What one tick of the rivers changed (see [`Rivers::tick`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    /// A front is [`WARNING_SECONDS`] from its first ford.
    pub(crate) coming: bool,
    /// A big front was rolled: told map-wide at once.
    pub(crate) great: Option<Band>,
    /// Groups whose fords just shut, and just opened.
    pub(crate) shut: Vec<u16>,
    pub(crate) opened: Vec<u16>,
}

/// The way to a neighbour that a flow points.
fn step_of(flow: u8) -> Option<(i32, i32)> {
    // In the order of the eight ways (an eighth of a turn each from east).
    const STEPS: [(i32, i32); 8] = [
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
    ];
    Flow::of(flow).map(|_| STEPS[usize::from(flow & 7)])
}

impl Rivers {
    /// The rivers of `terrain`'s map, `map_tiles` a side, and which of
    /// the `fords` and summer fords (`shallows`), as `(y, x)` tiles, lie
    /// on each.
    pub(crate) fn new(
        terrain: &Terrain,
        fords: &[(i32, i32)],
        shallows: &[(i32, i32)],
        map_tiles: u32,
        seed: u64,
    ) -> Rivers {
        let flow = terrain.flow_layer();
        let n = map_tiles as i32;
        let none = Rivers {
            seed,
            ..Rivers::default()
        };
        if flow.is_empty() || n == 0 {
            return none;
        }
        let at = |x: i32, y: i32| crate::terrain::index_in(n, x, y);
        let xy = |i: usize| (i as i32 % n, i as i32 / n);
        let tiles = (n * n) as usize;
        let mut reach = alloc::vec![NONE; tiles];
        let mut queue = Vec::new();
        // Where each river ends: running water that runs off the map, or
        // into still water.
        for i in 0..tiles {
            let Some((dx, dy)) = step_of(flow[i]) else {
                continue;
            };
            let (x, y) = xy(i);
            let ends = match at(x + dx, y + dy) {
                None => true,
                Some(j) => flow[j] == 0 && terrain.tile_under_water(x + dx, y + dy),
            };
            if ends {
                reach[i] = 0;
                queue.push(i);
            }
        }
        // How far up each tile lies, through running water.
        let mut head = 0;
        while let Some(&i) = queue.get(head) {
            head += 1;
            let (x, y) = xy(i);
            for (dx, dy) in crate::nav::DIRS {
                if let Some(j) = at(x + dx, y + dy).filter(|&j| flow[j] != 0 && reach[j] == NONE) {
                    reach[j] = reach[i].saturating_add(1).min(NONE - 1);
                    queue.push(j);
                }
            }
        }
        // The stretches: tiles side by side in the same band of reach.
        let band = |i: usize| reach[i] / STRETCH_TILES;
        let mut stretch_of = alloc::vec![NONE; tiles];
        let mut stretches: Vec<Stretch> = Vec::new();
        for start in 0..tiles {
            if reach[start] == NONE || stretch_of[start] != NONE {
                continue;
            }
            let Ok(s) = u16::try_from(stretches.len()).map(|s| s.min(NONE - 1)) else {
                break;
            };
            if s == NONE - 1 {
                break;
            }
            let mut st = Stretch {
                below: NONE,
                tin: 0,
                tout: 0,
                up: NONE,
                own: 0,
                land: 0,
                top: 0,
            };
            stretch_of[start] = s;
            queue.clear();
            queue.push(start);
            while let Some(i) = queue.pop() {
                st.own += 1;
                st.top = st.top.max(reach[i]);
                let (x, y) = xy(i);
                for (dx, dy) in crate::nav::DIRS {
                    if let Some(j) = at(x + dx, y + dy).filter(|&j| {
                        reach[j] != NONE && stretch_of[j] == NONE && band(j) == band(i)
                    }) {
                        stretch_of[j] = s;
                        queue.push(j);
                    }
                }
            }
            stretches.push(st);
        }
        if stretches.is_empty() {
            return none;
        }
        // Each stretch's stretch below: the lowest-numbered one a band
        // further down that a tile of it touches a step down the river.
        for i in 0..tiles {
            let s = stretch_of[i];
            if s == NONE || reach[i] == 0 || !reach[i].is_multiple_of(STRETCH_TILES) {
                continue;
            }
            let (x, y) = xy(i);
            for (dx, dy) in crate::nav::DIRS {
                let Some(j) = at(x + dx, y + dy) else {
                    continue;
                };
                if reach[j] != NONE && reach[j] + 1 == reach[i] {
                    let below = &mut stretches[usize::from(s)].below;
                    *below = (*below).min(stretch_of[j]);
                }
            }
        }
        // Walk each river's tree from its mouth: places, land, and the
        // stretch above with the most land.
        let count = stretches.len();
        let mut above: Vec<Vec<u16>> = alloc::vec![Vec::new(); count];
        for (s, st) in stretches.iter().enumerate() {
            if st.below != NONE {
                above[usize::from(st.below)].push(s as u16);
            }
        }
        let mut place = 0u16;
        let mut stack: Vec<(u16, bool)> = Vec::new();
        for root in 0..count {
            if stretches[root].below != NONE {
                continue;
            }
            stack.push((root as u16, false));
            while let Some((s, done)) = stack.pop() {
                let su = usize::from(s);
                if done {
                    stretches[su].tout = place.saturating_sub(1);
                    let mut land = stretches[su].own;
                    let mut best = (0, NONE);
                    for &a in &above[su] {
                        let l = stretches[usize::from(a)].land;
                        land += l;
                        if l > best.0 {
                            best = (l, a);
                        }
                    }
                    stretches[su].land = land;
                    stretches[su].up = best.1;
                    continue;
                }
                stretches[su].tin = place;
                place = place.saturating_add(1);
                stack.push((s, true));
                for &a in above[su].iter().rev() {
                    stack.push((a, false));
                }
            }
        }
        // The fords: each connected group of ford tiles on a river is one,
        // in the stretch of its middle tile up the river; the summer fords
        // join the group of the nearest ford up or down their branch.
        let mut groups: Vec<Group> = Vec::new();
        let mut taken = alloc::vec![false; tiles];
        let mut ford = alloc::vec![false; tiles];
        for &(y, x) in fords {
            if let Some(i) = at(x, y).filter(|&i| stretch_of[i] != NONE) {
                ford[i] = true;
            }
        }
        for &(y0, x0) in fords {
            let Some(start) = at(x0, y0).filter(|&i| ford[i] && !taken[i]) else {
                continue;
            };
            let mut members = Vec::new();
            taken[start] = true;
            queue.clear();
            queue.push(start);
            while let Some(i) = queue.pop() {
                members.push(i);
                let (x, y) = xy(i);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    if let Some(j) = at(x + dx, y + dy).filter(|&j| ford[j] && !taken[j]) {
                        taken[j] = true;
                        queue.push(j);
                    }
                }
            }
            members.sort_by_key(|&i| (reach[i], i));
            let middle = members[members.len() / 2];
            let tiles_of = members.iter().map(|&i| {
                let (x, y) = xy(i);
                (y, x)
            });
            let reaches = members.iter().map(|&i| reach[i]);
            groups.push(Group {
                stretch: stretch_of[middle],
                first: reaches.clone().max().unwrap_or(0),
                last: reaches.min().unwrap_or(0),
                fords: tiles_of.collect(),
                runs: Vec::new(),
            });
        }
        for &(y, x) in shallows {
            let Some(i) = at(x, y).filter(|&i| stretch_of[i] != NONE) else {
                continue;
            };
            // The nearest ford up or down the same branch of the river.
            let here = stretches[usize::from(stretch_of[i])];
            if let Some(g) = groups
                .iter_mut()
                .filter(|g| {
                    let there = &stretches[usize::from(g.stretch)];
                    here.holds(there) || there.holds(&here)
                })
                .min_by_key(|g| (g.first.abs_diff(reach[i]), g.stretch))
            {
                g.fords.push((y, x));
            }
        }
        if groups.is_empty() {
            return none;
        }
        for g in &mut groups {
            g.fords.sort_unstable();
            g.runs = crate::flood::runs_of(&g.fords);
        }
        groups.sort_by(|a, b| (a.stretch, &a.fords).cmp(&(b.stretch, &b.fords)));
        let (count, groups_n) = (stretches.len(), groups.len());
        let mut ford_tiles: Vec<((i32, i32), u16)> = (groups.iter().enumerate())
            .flat_map(|(g, group)| group.fords.iter().map(move |&t| (t, g as u16)))
            .collect();
        ford_tiles.sort_unstable();
        ford_tiles.dedup_by_key(|t| t.0);
        Rivers {
            ford_tiles: ford_tiles.into(),
            seed,
            side: n,
            reach: reach.into(),
            stretch_of: stretch_of.into(),
            stretches: stretches.into(),
            groups: groups.into(),
            store: alloc::vec![0; count],
            outflow: alloc::vec![0; count],
            swell: alloc::vec![0; count],
            shut: alloc::vec![false; groups_n],
            fronts: [None; MAX_FRONTS],
            threatened: Vec::with_capacity(64),
        }
    }

    /// Whether the map has rivers with fords: everything here runs only
    /// then.
    pub fn any(&self) -> bool {
        !self.groups.is_empty()
    }

    /// How many fords (groups of ford tiles on a river) there are.
    pub fn fords(&self) -> usize {
        self.groups.len()
    }

    /// How many stretches the rivers have.
    pub fn stretches(&self) -> usize {
        self.stretches.len()
    }

    /// The fronts running now.
    pub fn fronts(&self) -> impl Iterator<Item = &Front> + '_ {
        self.fronts.iter().flatten()
    }

    /// Whether a front is running (or waiting to set off).
    pub fn running(&self) -> bool {
        self.fronts.iter().any(Option::is_some)
    }

    /// Whether the river has shut any ford.
    pub fn shut(&self) -> bool {
        self.shut.iter().any(|&s| s)
    }

    /// How far its store lifts the water of the stretch at tile `(x, y)`
    /// (0 off the rivers).
    pub fn swell(&self, x: i32, y: i32) -> u16 {
        self.stretch_at(x, y)
            .map_or(0, |s| self.swell[usize::from(s)])
    }

    fn stretch_at(&self, x: i32, y: i32) -> Option<u16> {
        crate::terrain::index_in(self.side, x, y)
            .map(|i| self.stretch_of[i])
            .filter(|&s| s != NONE)
    }

    /// The group of ford tile `(x, y)`, if a river's.
    fn group_at(&self, x: i32, y: i32) -> Option<usize> {
        let k = (self.ford_tiles)
            .binary_search_by(|t| t.0.cmp(&(y, x)))
            .ok()?;
        Some(usize::from(self.ford_tiles[k].1))
    }

    /// Whether the river has shut the ford tile `(x, y)`.
    pub fn shuts(&self, x: i32, y: i32) -> bool {
        self.group_at(x, y).is_some_and(|g| self.shut[g])
    }

    /// Whether the river has shut the ford tile `(x, y)` and a front is
    /// over it, so troops there are swept away down the river.
    pub fn carries(&self, x: i32, y: i32) -> bool {
        self.shuts(x, y) && self.front_rise(x, y) > 0
    }

    /// How far the fronts raise the water on tile `(x, y)`: the highest
    /// of them.
    fn front_rise(&self, x: i32, y: i32) -> u16 {
        let Some(i) = crate::terrain::index_in(self.side, x, y) else {
            return 0;
        };
        let s = self.stretch_of[i];
        if s == NONE {
            return 0;
        }
        let here = &self.stretches[usize::from(s)];
        (self.fronts.iter().flatten())
            .filter(|f| here.holds(&self.stretches[usize::from(f.start)]))
            .map(|f| rise_after(f.running(), f.top, f.crest, self.reach[i]))
            .max()
            .unwrap_or(0)
    }

    /// The fords and summer fords, as `(row, column)` and sorted, that the
    /// river has shut or a front is coming down on (from its warning till
    /// it reaches them). The computer players keep off them.
    pub fn threatened(&self) -> &[(i32, i32)] {
        &self.threatened
    }

    /// The water the rivers raise, for the terrain (`None` with no
    /// rivers).
    pub(crate) fn water(&self) -> Option<RiverWater> {
        self.any().then(|| RiverWater {
            side: self.side,
            reach: self.reach.clone(),
            stretch_of: self.stretch_of.clone(),
            places: self.stretches.iter().map(|s| (s.tin, s.tout)).collect(),
            swell: self.swell.clone(),
            fronts: [None; MAX_FRONTS],
            most: 0,
        })
    }

    /// Bring the terrain's copy of the water (made by [`Rivers::water`])
    /// up to date.
    pub(crate) fn write_water(&self, water: &mut RiverWater) {
        water.swell.copy_from_slice(&self.swell);
        let mut most = self.swell.iter().copied().max().unwrap_or(0);
        for (k, f) in self.fronts.iter().enumerate() {
            water.fronts[k] = f.map(|f| {
                let st = &self.stretches[usize::from(f.start)];
                most = most.max(f.crest);
                FrontWater {
                    start: (st.tin, st.tout),
                    top: f.top,
                    crest: f.crest,
                    ticks: f.running(),
                }
            });
        }
        water.most = most;
    }

    /// Where a front carries something it sweeps away at tile `(x, y)`:
    /// on down the river by its flow, [`CARRY_TILES`] at most, stopping
    /// at still water or the map's edge.
    pub(crate) fn carried(&self, terrain: &Terrain, x: i32, y: i32) -> (i32, i32) {
        let (flow, n) = (terrain.flow_layer(), self.side);
        let (mut x, mut y) = (x, y);
        for _ in 0..CARRY_TILES {
            let Some(i) = crate::terrain::index_in(n, x, y) else {
                break;
            };
            let Some((dx, dy)) = flow.get(i).and_then(|&f| step_of(f)) else {
                break;
            };
            if crate::terrain::index_in(n, x + dx, y + dy).is_none() {
                break;
            }
            (x, y) = (x + dx, y + dy);
        }
        (x, y)
    }

    /// Group `g`'s fords as runs, to shut or open on the walkable grid.
    pub(crate) fn runs(&self, g: u16) -> &[[i32; 4]] {
        &self.groups[usize::from(g)].runs
    }

    /// The groups the river has shut.
    pub(crate) fn shut_groups(&self) -> impl Iterator<Item = u16> + '_ {
        (0..self.shut.len() as u16).filter(|&g| self.shut[usize::from(g)])
    }

    /// One second of the rivers' stores, with rain of `rain` strength
    /// (0 to 255) on ground `wet`, and the spring melt running or not:
    /// each stretch takes its rain and passes a share of its water on.
    pub(crate) fn second(&mut self, rain: u8, wet: u16, melting: bool) {
        if !self.any() {
            return;
        }
        let pour = u64::from(rain) * u64::from(wet.min(WET_FULL)) / u64::from(WET_FULL)
            + if melting { MELT_RAIN } else { 0 };
        for (s, st) in self.stretches.iter().enumerate() {
            self.outflow[s] = self.store[s] / OUTFLOW_DIVISOR;
            self.store[s] += pour * u64::from(st.own);
        }
        for (s, st) in self.stretches.iter().enumerate() {
            let out = self.outflow[s];
            self.store[s] -= out;
            if st.below != NONE {
                self.store[usize::from(st.below)] += out;
            }
        }
        for (s, st) in self.stretches.iter().enumerate() {
            let per = STORE_PER_UNIT * u64::from(st.own.max(1));
            self.swell[s] = (self.store[s] / per).min(u64::from(MAX_SWELL)) as u16;
        }
    }

    /// A storm `spell` began, in `season` (with seasons), on ground `wet`,
    /// with the map's water `rise`n and `snowpack` still to melt: roll once,
    /// from the seed and the spell's start, for a front, and its size.
    /// None on frozen water.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn storm(
        &mut self,
        spell: &Spell,
        season: Option<Season>,
        frozen: bool,
        wet: u16,
        rise: u16,
        snowpack: u32,
    ) -> Option<Band> {
        let w = spell.weather;
        if !self.any() || frozen || w.kind != WeatherKind::Rain || w.strength < STORM_STRENGTH {
            return None;
        }
        let chance: u64 = match season {
            None => 40,
            Some(Season::Spring | Season::Autumn) => 60,
            Some(Season::Summer) => 40,
            Some(Season::Winter) => 15,
        };
        let roll = roll(self.seed, spell.at);
        if roll % 100 >= chance {
            return None;
        }
        // The heavy tail: most small, a few huge.
        let mut band = match (roll >> 16) % 100 {
            0..35 => Band::Minor,
            35..80 => Band::Flood,
            80..96 => Band::Major,
            _ => Band::Catastrophic,
        };
        // Wet ground or a high river makes a bigger one more often.
        let soaked = wet >= crate::flood::FLOOD_WET || rise >= FLOOD_RISE / 2;
        if soaked && (roll >> 24).is_multiple_of(2) {
            band = match band {
                Band::Minor => Band::Flood,
                Band::Flood => Band::Major,
                _ => Band::Catastrophic,
            };
        }
        let mut crest = band.crest();
        if season == Some(Season::Spring) && snowpack >= crate::flood::MELT_MIN {
            // Rain on deep snow.
            crest = crest.saturating_mul(2).min(Band::Catastrophic.crest());
        } else if season == Some(Season::Summer) && wet < crate::flood::EBB_WET {
            crest /= 2;
        }
        let band = Band::of(crest);
        // Which ford's river: the first ford it reaches is that one.
        let g = &self.groups[((roll >> 32) % self.groups.len() as u64) as usize];
        let first = g.first;
        // Up the river's main stem from the ford, to where it sets off.
        let mut start = g.stretch;
        let want = first.saturating_add(SET_OFF_ABOVE);
        loop {
            let st = &self.stretches[usize::from(start)];
            if st.top >= want || st.up == NONE {
                break;
            }
            start = st.up;
        }
        let top = self.stretches[usize::from(start)].top.min(want);
        // The first ford it reaches may lie above the one rolled for.
        let from = &self.stretches[usize::from(start)];
        let first = (self.groups.iter())
            .filter(|g| g.first <= top && self.stretches[usize::from(g.stretch)].holds(from))
            .map(|g| g.first)
            .max()
            .unwrap_or(first);
        // Its crest where it sets off, so it is `crest` at the first ford.
        let fade = u32::from(top - first.min(top));
        let crest_top = (u32::from(crest) * 8 * CREST_FADE_TILES
            / (8 * CREST_FADE_TILES - fade.min(4 * CREST_FADE_TILES)))
        .min(u32::from(u16::MAX)) as u16;
        let travel = front_ticks(top, first);
        let lead = band.lead_seconds() * TICKS_PER_SECOND;
        let front = Front {
            start,
            top,
            crest: crest_top,
            band,
            first,
            ticks: 0,
            delay: lead.saturating_sub(travel),
            warned: false,
        };
        match self.fronts.iter_mut().find(|f| f.is_none()) {
            Some(slot) => *slot = Some(front),
            // Full: it adds to the biggest running.
            None => {
                if let Some(big) = self.fronts.iter_mut().flatten().max_by_key(|f| f.crest) {
                    big.crest = big.crest.saturating_add(crest_top / 2);
                    big.band = big.band.max(band);
                }
            }
        }
        Some(band)
    }

    /// One tick of the rivers, the map's water `lift` height units above
    /// its line (negative in low water): the fronts run on, and fords shut
    /// and open with the water over them. Fords are weighed every tick
    /// while a front runs, else once a second (`second`).
    pub(crate) fn tick(&mut self, lift: i32, second: bool, turn: &mut Turn) {
        turn.coming = false;
        turn.shut.clear();
        turn.opened.clear();
        if !self.any() {
            return;
        }
        let running = self.running();
        for slot in &mut self.fronts {
            let Some(f) = slot else {
                continue;
            };
            f.ticks += 1;
            let reached = f.delay + front_ticks(f.top, f.first);
            if !f.warned
                && f.band >= Band::Flood
                && f.ticks + WARNING_SECONDS * TICKS_PER_SECOND >= reached
            {
                f.warned = true;
                turn.coming = true;
            }
            // Over once the water has fallen at the river's end.
            let done =
                f.delay + front_ticks(f.top, 0) + (HIGH_SECONDS + FALL_SECONDS) * TICKS_PER_SECOND;
            if f.ticks >= done {
                *slot = None;
            }
        }
        if !(running || second) {
            return;
        }
        let mut changed = false;
        for g in 0..self.groups.len() {
            let group = &self.groups[g];
            let mut water = lift + i32::from(self.swell[usize::from(group.stretch)]);
            let here = &self.stretches[usize::from(group.stretch)];
            let front = (self.fronts.iter().flatten())
                .filter(|f| here.holds(&self.stretches[usize::from(f.start)]))
                .map(|f| {
                    // The highest the front has the water anywhere on the
                    // ford: reached first at its top, fallen last at its foot.
                    rise_after(f.running(), f.top, f.crest, group.first).max(rise_after(
                        f.running(),
                        f.top,
                        f.crest,
                        group.last,
                    ))
                })
                .max()
                .unwrap_or(0);
            water += i32::from(front);
            let need = if self.shut[g] {
                FLOOD_RISE / 2
            } else {
                FLOOD_RISE
            };
            let now = water >= i32::from(need);
            if now != self.shut[g] {
                self.shut[g] = now;
                changed = true;
                if now {
                    turn.shut.push(g as u16);
                } else {
                    turn.opened.push(g as u16);
                }
            }
        }
        if changed || running {
            self.weigh_threats();
        }
    }

    /// The fords the minds keep off, again.
    fn weigh_threats(&mut self) {
        let before = self.threatened.len();
        let mut list = core::mem::take(&mut self.threatened);
        list.clear();
        for (g, group) in self.groups.iter().enumerate() {
            let here = &self.stretches[usize::from(group.stretch)];
            let coming = (self.fronts.iter().flatten()).any(|f| {
                f.warned
                    && here.holds(&self.stretches[usize::from(f.start)])
                    && f.ticks < f.delay + front_ticks(f.top, group.first)
            });
            if self.shut[g] || coming {
                list.extend_from_slice(&group.fords);
            }
        }
        if list.len() != before || !list.is_sorted() {
            list.sort_unstable();
            list.dedup();
        }
        self.threatened = list;
    }

    /// Hashed with the ground only while the rivers hold anything: water
    /// in a store, a shut ford or a front.
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if self.store.iter().any(|&s| s > 0) {
            for &s in &self.store {
                h.u64(s);
            }
        }
        if self.shut() {
            for &s in &self.shut {
                h.u8(u8::from(s));
            }
        }
        for f in self.fronts.iter().flatten() {
            h.u16(f.start);
            h.u16(f.top);
            h.u16(f.crest);
            h.u32(f.ticks);
            h.u32(f.delay);
            h.u8(u8::from(f.warned));
        }
    }
}

/// The water the rivers raise, as the terrain reads it.
#[derive(Clone, Debug)]
pub(crate) struct RiverWater {
    side: i32,
    reach: Arc<[u16]>,
    stretch_of: Arc<[u16]>,
    /// Each stretch's places in its river's walk (see `Stretch::holds`).
    places: Vec<(u16, u16)>,
    swell: Vec<u16>,
    fronts: [Option<FrontWater>; MAX_FRONTS],
    /// The most it raises the water anywhere, height units.
    most: u16,
}

#[derive(Clone, Copy, Debug)]
struct FrontWater {
    start: (u16, u16),
    top: u16,
    crest: u16,
    ticks: u32,
}

impl RiverWater {
    /// The most it raises the water anywhere: no tile higher than that
    /// above the line need ask.
    pub(crate) fn most(&self) -> u16 {
        self.most
    }

    /// How far it raises the water on tile `(x, y)`.
    pub(crate) fn rise_at(&self, x: i32, y: i32) -> u16 {
        let Some(i) = crate::terrain::index_in(self.side, x, y) else {
            return 0;
        };
        let s = self.stretch_of[i];
        if s == NONE {
            return 0;
        }
        let (tin, tout) = self.places[usize::from(s)];
        let front = (self.fronts.iter().flatten())
            .filter(|f| tin <= f.start.0 && f.start.1 <= tout)
            .map(|f| rise_after(f.ticks, f.top, f.crest, self.reach[i]))
            .max()
            .unwrap_or(0);
        self.swell[usize::from(s)].saturating_add(front)
    }
}

/// Ticks from a front setting off at `top` until it reaches `reach`.
fn front_ticks(top: u16, reach: u16) -> u32 {
    u32::from(top.saturating_sub(reach)) * TICKS_PER_SECOND / WAVE_TILES_PER_SECOND
}

/// How far a front `ticks` running that set off at `top` with `crest`
/// has raised the water at `reach`: nothing above where it set off or
/// before it gets there, the crest (fading on the way down) for a while,
/// then falling.
fn rise_after(ticks: u32, top: u16, crest: u16, reach: u16) -> u16 {
    if reach > top {
        return 0;
    }
    let Some(since) = ticks.checked_sub(front_ticks(top, reach)) else {
        return 0;
    };
    let ran = u32::from(top - reach).min(4 * CREST_FADE_TILES);
    let crest = u32::from(crest) * (8 * CREST_FADE_TILES - ran) / (8 * CREST_FADE_TILES);
    let (high, fall) = (
        HIGH_SECONDS * TICKS_PER_SECOND,
        FALL_SECONDS * TICKS_PER_SECOND,
    );
    if since < high {
        crest as u16
    } else if since < high + fall {
        (crest * (high + fall - since) / fall) as u16
    } else {
        0
    }
}

/// A roll from the match seed and a tick: no random stream moves.
fn roll(seed: u64, tick: u32) -> u64 {
    crate::fire::splitmix(seed ^ u64::from(tick).rotate_left(29) ^ 0xf1a5_4f10_0d00_0000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_water_rises_only_below_where_the_front_set_off() {
        assert_eq!(rise_after(1, 5, 100, 5), 100);
        assert_eq!(rise_after(1, 5, 100, 6), 0);
        // Not yet reached further down.
        assert_eq!(rise_after(1, 5, 100, 0), 0);
    }

    #[test]
    fn the_water_falls_after_it_has_been_high() {
        let high = HIGH_SECONDS * TICKS_PER_SECOND;
        let fall = FALL_SECONDS * TICKS_PER_SECOND;
        assert_eq!(rise_after(0, 10, 80, 10), 80);
        assert_eq!(rise_after(high + fall / 2, 10, 80, 10), 40);
        assert_eq!(rise_after(high + fall, 10, 80, 10), 0);
    }

    #[test]
    fn the_crest_fades_down_the_river() {
        let far = front_ticks(128, 0);
        // An eighth lower after 128 tiles.
        assert_eq!(rise_after(far, 128, 800, 0), 700);
    }

    #[test]
    fn bands_follow_their_crests() {
        for band in [Band::Minor, Band::Flood, Band::Major, Band::Catastrophic] {
            assert_eq!(Band::of(band.crest()), band);
        }
        assert!(Band::Flood.crest() >= FLOOD_RISE);
        assert!(Band::Minor.crest() < FLOOD_RISE);
    }
}
