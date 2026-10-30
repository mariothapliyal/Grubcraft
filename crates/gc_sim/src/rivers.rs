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
//!
//! **The floodplain** (stage 2): land within [`PLAIN_TILES`] of a river
//! and less than a tile above its line belongs to the stretch of the
//! river tile nearest it, so the same water rises over it: once the river
//! stands higher than the land, it is wet (wading ground: slow, no
//! charges). Each stretch's floodplain is grouped by height (bands of
//! [`PLAIN_STEP`]); a group shuts on the walkable grid once the water
//! stands [`WADING_DEPTH`] over it and opens again [`PLAIN_MARGIN`] lower,
//! weighed once a second, so a big flood switches each group at most
//! twice. Troops on shut floodplain are hurt and sent to dry ground, and
//! swept down the river while a front is over it; buildings there lose
//! [`DROWNED_PERCENT`] of their hit points a second.

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
/// How far from a river its floodplain reaches, tiles.
pub const PLAIN_TILES: u8 = 6;
/// The floodplain's height bands, height units.
pub const PLAIN_STEP: u16 = (HEIGHT_UNITS / 10) as u16;
/// How deep water over floodplain shuts it, height units (the wading
/// depth).
pub const WADING_DEPTH: u16 = (HEIGHT_UNITS * 35 / 100) as u16;
/// How far under [`WADING_DEPTH`] the water falls before shut floodplain
/// opens again, height units.
pub const PLAIN_MARGIN: u16 = (HEIGHT_UNITS / 10) as u16;
/// The share of its hit points, in hundredths, a building on shut
/// floodplain loses each second.
pub const DROWNED_PERCENT: u32 = 2;
/// How far down the river a front carries troops it sweeps away, tiles.
pub const CARRY_TILES: u32 = 10;
/// The share of a regiment swept away that drowns, in percent (rounded
/// down, so the last soldier never does).
pub const DROWN_PERCENT: u32 = 30;
/// The morale a regiment swept away loses.
pub const SWEPT_AWAY_MORALE: i16 = 25;
/// Most fronts running at once; a storm while all run sets none off.
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
    /// The stretches, each before those above it.
    order: Arc<[u16]>,
    /// The fords, one group each, by stretch.
    groups: Arc<[Group]>,
    /// Every ford tile on a river, as `(y, x)`, sorted, with its group.
    ford_tiles: Arc<[((i32, i32), u16)]>,
    /// The floodplain, one group per stretch and height band.
    plains: Arc<[Plain]>,
    /// Every floodplain tile, as `(y, x)`, sorted, with its group and the
    /// river tile nearest it (as `(x, y)`).
    plain_tiles: Arc<[PlainTile]>,
    /// Whether each floodplain group is shut.
    plain_shut: Vec<bool>,
    /// The water in each stretch beyond its normal flow.
    store: Vec<u64>,
    /// How far each stretch's store lifts its water, height units.
    swell: Vec<u16>,
    /// Whether each group's fords are shut by the river (not the map-wide
    /// flood, which shuts every ford at once; see [`crate::flood`]).
    shut: Vec<bool>,
    /// Whether a warned front is coming down on each group's fords.
    coming: Vec<bool>,
    fronts: [Option<Front>; MAX_FRONTS],
    /// The fords the minds keep off: of groups shut, or a front is
    /// coming down on. As `(y, x)`, sorted.
    threatened: Vec<(i32, i32)>,
    /// How many times `threatened` has changed, and room to build it anew.
    threats: u32,
    threat_room: Vec<(i32, i32)>,
    /// What the last tick changed, and runs gathered to shut or open
    /// (both kept for their room).
    turn: Turn,
    runs: Vec<[i32; 4]>,
}

/// A stretch of river.
#[derive(Clone, Copy, Debug)]
struct Stretch {
    /// The stretch below (`NONE` at a river's mouth).
    below: u16,
    /// Its place in a walk of its river's tree from the mouth, and the
    /// last place under it (see [`Stretch::holds`]).
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
    /// Its river (joined rivers are one), counted from 0.
    river: u16,
    /// A tile of it about halfway along it, as `(x, y)`: where the
    /// players look for it.
    at: (i32, i32),
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

/// Floodplain of one stretch in one height band.
#[derive(Clone, Debug)]
struct Plain {
    stretch: u16,
    /// The middle of its band, height units above the line.
    height: u16,
    /// The furthest up and the nearest the mouth its river tiles lie.
    first: u16,
    last: u16,
    /// Its tiles as runs along each row (`[x, y, w, 1]`).
    runs: Vec<[i32; 4]>,
}

/// A floodplain tile: where, its group, and the river tile nearest it.
#[derive(Clone, Copy, Debug)]
struct PlainTile {
    at: (i32, i32),
    plain: u16,
    river: (i32, i32),
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

    /// Whether it has set off.
    fn moving(&self) -> bool {
        self.ticks >= self.delay
    }

    /// How far it raises the water at `reach`.
    fn rise(&self, reach: u16) -> u16 {
        rise_after(self.running(), self.top, self.crest, reach)
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
    /// The rivers first shut a ford, or opened the last.
    pub(crate) stage: Option<Stage>,
    /// The water the terrain reads changed.
    pub(crate) water: bool,
    /// Groups whose fords just shut, and just opened.
    shut: Vec<u16>,
    opened: Vec<u16>,
    /// Floodplain groups that just shut, and just opened.
    plain_shut: Vec<u16>,
    plain_opened: Vec<u16>,
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

/// The highest any of `fronts` raises the water at `reach` of stretch
/// `here`: only those that set off at or above it count.
fn fronts_rise<'a>(
    stretches: &[Stretch],
    fronts: impl IntoIterator<Item = &'a Front>,
    here: u16,
    reach: u16,
) -> u16 {
    let here = &stretches[usize::from(here)];
    (fronts.into_iter())
        .filter(|f| here.holds(&stretches[usize::from(f.start)]))
        .map(|f| f.rise(reach))
        .max()
        .unwrap_or(0)
}

/// How far stretch `here`'s water stands above the line at `reach`: its
/// swell and the highest of `fronts` there.
fn water_rise<'a>(
    stretches: &[Stretch],
    swell: &[u16],
    fronts: impl IntoIterator<Item = &'a Front>,
    here: u16,
    reach: u16,
) -> u16 {
    swell[usize::from(here)].saturating_add(fronts_rise(stretches, fronts, here, reach))
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
            if stretches.len() >= usize::from(NONE - 1) {
                break;
            }
            let s = stretches.len() as u16;
            let mut st = Stretch {
                below: NONE,
                tin: 0,
                tout: 0,
                up: NONE,
                own: 0,
                land: 0,
                top: 0,
                river: 0,
                at: xy(start),
            };
            stretch_of[start] = s;
            queue.clear();
            queue.push(start);
            // Halfway along the band, and the nearest tile to it so far.
            let middle = band(start) * STRETCH_TILES + STRETCH_TILES / 2;
            let mut nearest = (reach[start].abs_diff(middle), start);
            while let Some(i) = queue.pop() {
                st.own += 1;
                st.top = st.top.max(reach[i]);
                nearest = nearest.min((reach[i].abs_diff(middle), i));
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
            st.at = xy(nearest.1);
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
        // stretch above with the most land; and the stretches in the order
        // walked, so each comes before those above it.
        let count = stretches.len();
        let mut order: Vec<u16> = Vec::with_capacity(count);
        let mut above: Vec<Vec<u16>> = alloc::vec![Vec::new(); count];
        for (s, st) in stretches.iter().enumerate() {
            if st.below != NONE {
                above[usize::from(st.below)].push(s as u16);
            }
        }
        let mut place = 0u16;
        let mut river = 0u16;
        let mut stack: Vec<(u16, bool)> = Vec::new();
        for root in 0..count {
            if stretches[root].below != NONE {
                continue;
            }
            stack.push((root as u16, false));
            let this_river = river;
            river += 1;
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
                stretches[su].river = this_river;
                place = place.saturating_add(1);
                order.push(s);
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
                for (dx, dy) in crate::nav::DIRS {
                    // Side by side only.
                    if dx != 0 && dy != 0 {
                        continue;
                    }
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
        let mut ford_tiles: Vec<((i32, i32), u16)> = (groups.iter().enumerate())
            .flat_map(|(g, group)| group.fords.iter().map(move |&t| (t, g as u16)))
            .collect();
        ford_tiles.sort_unstable();
        ford_tiles.dedup_by_key(|t| t.0);
        let (plains, plain_tiles) = floodplain(terrain, n, &mut reach, &mut stretch_of);
        let runs = (groups.iter().map(|g| g.runs.len()))
            .chain(plains.iter().map(|p| p.runs.len()))
            .sum();
        let (groups_n, plains_n) = (groups.len(), plains.len());
        Rivers {
            seed,
            side: n,
            reach: reach.into(),
            stretch_of: stretch_of.into(),
            stretches: stretches.into(),
            order: order.into(),
            groups: groups.into(),
            threatened: Vec::with_capacity(ford_tiles.len()),
            threat_room: Vec::with_capacity(ford_tiles.len()),
            ford_tiles: ford_tiles.into(),
            plains: plains.into(),
            plain_tiles: plain_tiles.into(),
            plain_shut: alloc::vec![false; plains_n],
            store: alloc::vec![0; count],
            swell: alloc::vec![0; count],
            shut: alloc::vec![false; groups_n],
            coming: alloc::vec![false; groups_n],
            fronts: [None; MAX_FRONTS],
            threats: 0,
            turn: Turn {
                shut: Vec::with_capacity(groups_n),
                opened: Vec::with_capacity(groups_n),
                plain_shut: Vec::with_capacity(plains_n),
                plain_opened: Vec::with_capacity(plains_n),
                ..Turn::default()
            },
            runs: Vec::with_capacity(runs),
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
        self.stretch_at(x, y).map_or(0, |s| self.swell[s])
    }

    /// The group of ford tile `(x, y)`, if a river's.
    fn group_at(&self, x: i32, y: i32) -> Option<usize> {
        let k = (self.ford_tiles)
            .binary_search_by(|t| t.0.cmp(&(y, x)))
            .ok()?;
        Some(usize::from(self.ford_tiles[k].1))
    }

    /// The floodplain tile `(x, y)`, if it is one.
    fn plain_at(&self, x: i32, y: i32) -> Option<&PlainTile> {
        let k = (self.plain_tiles)
            .binary_search_by(|t| t.at.cmp(&(y, x)))
            .ok()?;
        Some(&self.plain_tiles[k])
    }

    /// Whether the river has shut the ford tile `(x, y)`.
    pub fn shuts_ford(&self, x: i32, y: i32) -> bool {
        self.group_at(x, y).is_some_and(|g| self.shut[g])
    }

    /// Whether the river has shut the tile `(x, y)`: a ford, or deep
    /// water over its floodplain.
    pub fn shuts(&self, x: i32, y: i32) -> bool {
        self.shuts_ford(x, y)
            || self
                .plain_at(x, y)
                .is_some_and(|t| self.plain_shut[usize::from(t.plain)])
    }

    /// Whether the river has shut any tile: a ford or floodplain.
    pub fn flooding(&self) -> bool {
        self.shut() || self.plain_shut.iter().any(|&s| s)
    }

    /// How many floodplain groups there are, and how many tiles.
    pub fn floodplain(&self) -> (usize, usize) {
        (self.plains.len(), self.plain_tiles.len())
    }

    /// Whether tile `(x, y)` is floodplain (land beside a river, under a
    /// tile above its line).
    pub fn on_floodplain(&self, x: i32, y: i32) -> bool {
        self.plain_at(x, y).is_some()
    }

    /// Whether the river has shut the tile `(x, y)` (see
    /// [`Rivers::shuts`]) and a front is over it, so troops there are
    /// swept away down the river.
    pub fn carries(&self, x: i32, y: i32) -> bool {
        let Some(i) = crate::terrain::index_in(self.side, x, y) else {
            return false;
        };
        self.shuts(x, y)
            && fronts_rise(
                &self.stretches,
                self.fronts(),
                self.stretch_of[i],
                self.reach[i],
            ) > 0
    }

    /// The fords and summer fords, as `(row, column)` and sorted, that the
    /// river has shut or a front is coming down on (from its warning till
    /// it reaches them). The computer players keep off them.
    pub fn threatened(&self) -> &[(i32, i32)] {
        &self.threatened
    }

    /// How many times [`Rivers::threatened`] has changed: a mind reads the
    /// list again when this moves.
    pub fn threats(&self) -> u32 {
        self.threats
    }

    /// How many rivers there are (joined rivers are one).
    pub fn river_count(&self) -> usize {
        // (The last stretch walked is in the last river.)
        (self.order.last()).map_or(0, |&s| {
            usize::from(self.stretches[usize::from(s)].river) + 1
        })
    }

    /// The stretch tile `(x, y)` lies in, if it is on a river or its
    /// floodplain.
    pub fn stretch_at(&self, x: i32, y: i32) -> Option<usize> {
        crate::terrain::index_in(self.side, x, y)
            .map(|i| self.stretch_of[i])
            .filter(|&s| s != NONE)
            .map(usize::from)
    }

    /// The stretch below stretch `s` (`None` at a river's mouth, or for
    /// a stretch not under [`Rivers::stretches`]).
    pub fn below(&self, s: usize) -> Option<usize> {
        (self.stretches.get(s))
            .map(|st| st.below)
            .filter(|&b| b != NONE)
            .map(usize::from)
    }

    /// Whether stretch `s` has a ford.
    pub fn has_ford(&self, s: usize) -> bool {
        // (The groups are sorted by stretch.)
        let k = self.groups.partition_point(|g| usize::from(g.stretch) < s);
        self.groups
            .get(k)
            .is_some_and(|g| usize::from(g.stretch) == s)
    }

    /// What the players read of stretch `s` (see [`Reading`]), with the
    /// map-wide flood raising every river `lift` height units.
    ///
    /// # Panics
    ///
    /// If `s` is not under [`Rivers::stretches`].
    pub fn reading(&self, s: usize, lift: u16) -> Reading {
        let st = &self.stretches[s];
        let rise = water_rise(
            &self.stretches,
            &self.swell,
            self.fronts(),
            s as u16,
            st.top,
        );
        // The worst front still to reach it (the soonest of equals).
        let coming = (self.fronts())
            .filter(|f| st.holds(&self.stretches[usize::from(f.start)]) && st.top <= f.top)
            .filter_map(|f| {
                let ticks = (front_ticks(f.top, st.top) + f.delay).checked_sub(f.ticks)?;
                (ticks > 0).then(|| Coming {
                    seconds: ticks.div_ceil(TICKS_PER_SECOND),
                    crest: faded(f.crest, f.top - st.top),
                })
            })
            .min_by_key(|c| (core::cmp::Reverse(Band::of(c.crest)), c.seconds));
        Reading {
            river: st.river,
            at: st.at,
            rise: lift.saturating_add(rise),
            coming,
        }
    }

    /// The water the rivers raise, for the terrain (`None` with no
    /// rivers).
    pub(crate) fn water(&self) -> Option<RiverWater> {
        self.any().then(|| RiverWater {
            side: self.side,
            reach: self.reach.clone(),
            stretch_of: self.stretch_of.clone(),
            stretches: self.stretches.clone(),
            swell: self.swell.clone(),
            fronts: [None; MAX_FRONTS],
            most: 0,
        })
    }

    /// Bring the terrain's copy of the water (made by [`Rivers::water`])
    /// up to date.
    pub(crate) fn write_water(&self, water: &mut RiverWater) {
        water.swell.copy_from_slice(&self.swell);
        water.fronts = self.fronts;
        let swell = self.swell.iter().copied().max().unwrap_or(0);
        let crest = self.fronts().map(|f| f.crest).max().unwrap_or(0);
        water.most = swell.saturating_add(crest);
    }

    /// Where a front carries something it sweeps away at tile `(x, y)`:
    /// on down the river by its flow, [`CARRY_TILES`] at most, stopping
    /// at still water or the map's edge.
    /// From floodplain, it is carried from the river tile nearest it.
    pub(crate) fn carried(&self, terrain: &Terrain, x: i32, y: i32) -> (i32, i32) {
        let (flow, n) = (terrain.flow_layer(), self.side);
        let (mut x, mut y) = self.plain_at(x, y).map_or((x, y), |t| t.river);
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

    /// The runs, to shut or open on the walkable grid, of the floodplain
    /// the last tick shut (`shut`) or opened.
    pub(crate) fn changed_plain_runs(&mut self, shut: bool) -> &[[i32; 4]] {
        let list = if shut {
            &self.turn.plain_shut
        } else {
            &self.turn.plain_opened
        };
        self.runs.clear();
        for &p in list {
            self.runs
                .extend_from_slice(&self.plains[usize::from(p)].runs);
        }
        &self.runs
    }

    /// The runs, to shut or open on the walkable grid, of the fords the
    /// last tick shut (`shut`) or opened.
    pub(crate) fn changed_runs(&mut self, shut: bool) -> &[[i32; 4]] {
        let list = if shut {
            &self.turn.shut
        } else {
            &self.turn.opened
        };
        self.runs.clear();
        for &g in list {
            self.runs
                .extend_from_slice(&self.groups[usize::from(g)].runs);
        }
        &self.runs
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
        // Each stretch comes before those above it, so what flows into it
        // this second flows on only the next.
        for &s in self.order.iter() {
            let st = &self.stretches[usize::from(s)];
            let out = self.store[usize::from(s)].div_ceil(OUTFLOW_DIVISOR);
            let store = &mut self.store[usize::from(s)];
            *store = *store + pour * u64::from(st.own) - out;
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
        // Separate bits of the roll for each choice.
        let mut band = match (roll >> 16 & 0xffff) % 100 {
            0..35 => Band::Minor,
            35..80 => Band::Flood,
            80..96 => Band::Major,
            _ => Band::Catastrophic,
        };
        // Wet ground or a high river makes a bigger one more often.
        let soaked = wet >= crate::flood::FLOOD_WET || rise >= FLOOD_RISE / 2;
        if soaked && (roll >> 32).is_multiple_of(2) {
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
        // Which ford's river.
        let g = &self.groups[((roll >> 40) % self.groups.len() as u64) as usize];
        // Up the river's main stem from the ford, to where it sets off.
        let mut start = g.stretch;
        let want = g.first.saturating_add(SET_OFF_ABOVE);
        loop {
            let st = &self.stretches[usize::from(start)];
            if st.top >= want || st.up == NONE {
                break;
            }
            start = st.up;
        }
        let from = &self.stretches[usize::from(start)];
        let top = from.top.min(want);
        // The first ford it reaches may lie above the one rolled for.
        let first = (self.groups.iter())
            .filter(|g| g.first <= top && self.stretches[usize::from(g.stretch)].holds(from))
            .map(|g| g.first)
            .max()
            .unwrap_or(g.first.min(top));
        // Its crest where it sets off, so it is `crest` at the first ford.
        let crest_top = unfaded(crest, top - first);
        let lead = band.lead_seconds() * TICKS_PER_SECOND;
        let front = Front {
            start,
            top,
            crest: crest_top,
            band,
            first,
            ticks: 0,
            delay: lead.saturating_sub(front_ticks(top, first)),
            warned: false,
        };
        // With every slot taken, the storm passes with the fronts it has.
        *self.fronts.iter_mut().find(|f| f.is_none())? = Some(front);
        Some(band)
    }

    /// One tick of the rivers, the map's water `lift` height units above
    /// its line (negative in low water), and whether it is a whole
    /// `second`: the fronts run on, and fords shut and open with the water
    /// over them. Fords are weighed every tick while a front moves, else
    /// once a second.
    pub(crate) fn tick(&mut self, lift: i32, second: bool) -> &Turn {
        let any = self.any();
        let turn = &mut self.turn;
        turn.coming = false;
        turn.stage = None;
        turn.water = second;
        turn.shut.clear();
        turn.opened.clear();
        turn.plain_shut.clear();
        turn.plain_opened.clear();
        if !any {
            return &self.turn;
        }
        let mut moving = false;
        for slot in &mut self.fronts {
            let Some(f) = slot else {
                continue;
            };
            f.ticks += 1;
            moving |= f.moving();
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
                turn.water = true;
            }
        }
        turn.water |= moving;
        if !(moving || second) {
            return &self.turn;
        }
        let was_shut = self.shut();
        for (g, group) in self.groups.iter().enumerate() {
            // The highest the water is anywhere on the ford: a front
            // reaches its top first and falls last at its foot.
            let front = [group.first, group.last]
                .map(|r| {
                    fronts_rise(
                        &self.stretches,
                        self.fronts.iter().flatten(),
                        group.stretch,
                        r,
                    )
                })
                .into_iter()
                .max()
                .unwrap_or(0);
            let water = lift + i32::from(self.swell[usize::from(group.stretch)]) + i32::from(front);
            let need = if self.shut[g] {
                FLOOD_RISE / 2
            } else {
                FLOOD_RISE
            };
            let now = water >= i32::from(need);
            if now != self.shut[g] {
                self.shut[g] = now;
                if now {
                    self.turn.shut.push(g as u16);
                } else {
                    self.turn.opened.push(g as u16);
                }
            }
        }
        if second {
            self.weigh_plains(lift);
        }
        let now_shut = self.shut();
        if now_shut != was_shut {
            self.turn.stage = Some(if now_shut { Stage::Here } else { Stage::Gone });
        }
        // Which fords a warned front is still coming down on.
        let mut changed = !self.turn.shut.is_empty() || !self.turn.opened.is_empty();
        for (g, group) in self.groups.iter().enumerate() {
            let here = &self.stretches[usize::from(group.stretch)];
            let coming = (self.fronts.iter().flatten()).any(|f| {
                f.warned
                    && here.holds(&self.stretches[usize::from(f.start)])
                    && f.ticks < f.delay + front_ticks(f.top, group.first)
            });
            if coming != self.coming[g] {
                self.coming[g] = coming;
                changed = true;
            }
        }
        if changed {
            self.weigh_threats();
        }
        &self.turn
    }

    /// Shut each floodplain group the water stands [`WADING_DEPTH`] over,
    /// and open each it has fallen [`PLAIN_MARGIN`] under that, with the
    /// map's water `lift`.
    fn weigh_plains(&mut self, lift: i32) {
        for (p, plain) in self.plains.iter().enumerate() {
            let water = [plain.first, plain.last]
                .map(|r| {
                    water_rise(
                        &self.stretches,
                        &self.swell,
                        self.fronts.iter().flatten(),
                        plain.stretch,
                        r,
                    )
                })
                .into_iter()
                .max()
                .unwrap_or(0);
            let water = lift + i32::from(water);
            let mut need = i32::from(plain.height) + i32::from(WADING_DEPTH);
            if self.plain_shut[p] {
                need -= i32::from(PLAIN_MARGIN);
            }
            let now = water >= need;
            if now != self.plain_shut[p] {
                self.plain_shut[p] = now;
                if now {
                    self.turn.plain_shut.push(p as u16);
                } else {
                    self.turn.plain_opened.push(p as u16);
                }
            }
        }
    }

    /// The fords the minds keep off, again, built in the room kept for
    /// them; [`Rivers::threats`] moves when they change.
    fn weigh_threats(&mut self) {
        let mut list = core::mem::take(&mut self.threat_room);
        list.clear();
        for (g, group) in self.groups.iter().enumerate() {
            if self.shut[g] || self.coming[g] {
                list.extend_from_slice(&group.fords);
            }
        }
        list.sort_unstable();
        list.dedup();
        if list != self.threatened {
            self.threats = self.threats.wrapping_add(1);
            core::mem::swap(&mut list, &mut self.threatened);
        }
        self.threat_room = list;
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
        if self.plain_shut.iter().any(|&s| s) {
            for &s in &self.plain_shut {
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
    stretches: Arc<[Stretch]>,
    swell: Vec<u16>,
    fronts: [Option<Front>; MAX_FRONTS],
    /// The most it raises the water anywhere, height units.
    most: u16,
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
        water_rise(
            &self.stretches,
            &self.swell,
            self.fronts.iter().flatten(),
            s,
            self.reach[i],
        )
    }
}

/// What the players read of a stretch of river (see [`Rivers::reading`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Its river, counted from 0 (under [`Rivers::river_count`]).
    pub river: u16,
    /// A tile of it about halfway along it, as `(x, y)`.
    pub at: (i32, i32),
    /// How far its water stands above the map's line at its top, height
    /// units: the map-wide flood, its swell and any front over it.
    pub rise: u16,
    /// The worst front still to reach it (the soonest of the biggest
    /// band), if one is running or waiting.
    pub coming: Option<Coming>,
}

/// A front on its way to a stretch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Coming {
    /// Seconds until its crest reaches the stretch's top.
    pub seconds: u32,
    /// Its crest there, height units ([`Band::of`] tells its band).
    pub crest: u16,
}

/// How high a river runs, as its gauge reads (docs/39-water-engine.md,
/// "What the player sees").
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Gauge {
    /// Below its line in a dry summer.
    Low,
    /// At its line, or barely over it.
    Normal,
    /// Up: wading in it is deeper and slower ([`DEEP_RISE`] or more).
    Up,
    /// Over its fords ([`FLOOD_RISE`] or more).
    InFlood,
    /// Over its banks (half a tile or more).
    Bursting,
}

/// The rise at which wading gets deeper and slower: a tenth of a tile
/// (a minor flood; docs/39-water-engine.md, section 2).
pub const DEEP_RISE: u16 = (HEIGHT_UNITS / 10) as u16;

impl Gauge {
    /// The gauge of water `rise` height units above the line, or `low`
    /// below it.
    pub const fn of(rise: u16, low: u16) -> Gauge {
        if rise as u32 >= HEIGHT_UNITS / 2 {
            Gauge::Bursting
        } else if rise >= FLOOD_RISE {
            Gauge::InFlood
        } else if rise >= DEEP_RISE {
            Gauge::Up
        } else if low > 0 && rise == 0 {
            Gauge::Low
        } else {
            Gauge::Normal
        }
    }
}

/// A crest of `crest` once it has run `ran` tiles: an eighth lower every
/// [`CREST_FADE_TILES`], at most by half.
/// The floodplain of the rivers on `terrain`'s map, `n` tiles a side
/// (see the module docs): its groups, by stretch and height band, and its
/// tiles, sorted. Each floodplain tile takes the `reach` and `stretch_of`
/// of the river tile nearest it.
fn floodplain(
    terrain: &Terrain,
    n: i32,
    reach: &mut [u16],
    stretch_of: &mut [u16],
) -> (Vec<Plain>, Vec<PlainTile>) {
    let at = |x: i32, y: i32| crate::terrain::index_in(n, x, y);
    let xy = |i: usize| (i as i32 % n, i as i32 / n);
    let line = i32::from(terrain.water_level());
    // Out from every river tile over dry land, nearest first.
    let mut from = alloc::vec![u32::MAX; reach.len()];
    let mut steps = alloc::vec![0u8; reach.len()];
    let mut queue: Vec<usize> = (0..reach.len()).filter(|&i| reach[i] != NONE).collect();
    for &i in &queue {
        from[i] = i as u32;
    }
    let mut found: Vec<(u16, u16, usize)> = Vec::new();
    let mut head = 0;
    while let Some(&i) = queue.get(head) {
        head += 1;
        if steps[i] >= PLAIN_TILES {
            continue;
        }
        let (x, y) = xy(i);
        for (dx, dy) in crate::nav::DIRS {
            let Some(j) = at(x + dx, y + dy) else {
                continue;
            };
            if from[j] != u32::MAX || terrain.tile_under_water(x + dx, y + dy) {
                continue;
            }
            from[j] = from[i];
            steps[j] = steps[i] + 1;
            queue.push(j);
            let above = terrain.height(gc_fixed::FxVec2::from_ints(x + dx, y + dy)) - line;
            if (0..HEIGHT_UNITS as i32).contains(&above) {
                let r = from[j] as usize;
                let band = above as u16 / PLAIN_STEP;
                found.push((stretch_of[r], band, j));
            }
        }
    }
    found.sort_unstable_by_key(|&(s, b, j)| (s, b, j));
    let mut plains: Vec<Plain> = Vec::new();
    let mut tiles: Vec<PlainTile> = Vec::with_capacity(found.len());
    for run in found.chunk_by(|a, b| (a.0, a.1) == (b.0, b.1)) {
        let (stretch, band) = (run[0].0, run[0].1);
        let p = plains.len() as u16;
        let mut cells: Vec<(i32, i32)> = Vec::with_capacity(run.len());
        let (mut first, mut last) = (0, NONE);
        for &(_, _, j) in run {
            let r = from[j] as usize;
            (first, last) = (first.max(reach[r]), last.min(reach[r]));
            let (x, y) = xy(j);
            cells.push((y, x));
            tiles.push(PlainTile {
                at: (y, x),
                plain: p,
                river: xy(r),
            });
        }
        cells.sort_unstable();
        plains.push(Plain {
            stretch,
            height: band * PLAIN_STEP + PLAIN_STEP / 2,
            first,
            last,
            runs: crate::flood::runs_of(&cells),
        });
    }
    for t in &tiles {
        let (y, x) = t.at;
        let (j, r) = (at(x, y), at(t.river.0, t.river.1));
        if let (Some(j), Some(r)) = (j, r) {
            reach[j] = reach[r];
            stretch_of[j] = stretch_of[r];
        }
    }
    tiles.sort_unstable_by_key(|t| t.at);
    (plains, tiles)
}

fn faded(crest: u16, ran: u16) -> u16 {
    (u32::from(crest) * (FADE_WHOLE - fade_of(ran)) / FADE_WHOLE) as u16
}

/// The crest that has faded to `crest` after running `ran` tiles.
fn unfaded(crest: u16, ran: u16) -> u16 {
    (u32::from(crest) * FADE_WHOLE / (FADE_WHOLE - fade_of(ran))).min(u32::from(u16::MAX)) as u16
}

const FADE_WHOLE: u32 = 8 * CREST_FADE_TILES;

fn fade_of(ran: u16) -> u32 {
    u32::from(ran).min(FADE_WHOLE / 2)
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
    let crest = u32::from(faded(crest, top - reach));
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
