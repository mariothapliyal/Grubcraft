//! Living Land: grown land (docs/27-living-land.md, stage 1). Instead of
//! stamping lines and discs, the land grows from fields of numbers, each
//! layer reading what the ones before it left:
//!
//! 1. **Shape** ([`shape`]): warped fractal noise for the broad land,
//!    ridged noise for branching crests inside a wandering outline of
//!    mountain country.
//! 2. **Rock** ([`shape`]): strata benches whose edges are cliffs where a
//!    noise field says sharp and ramps where it says soft, and crags on the
//!    highest cores.
//! 3. **Weather** ([`erode`]): simulated raindrops cut gullies and lay
//!    fans, then steep slopes crumble a little.
//! 4. **Water** ([`water`]): every drop's way down to the edge, rivers
//!    where enough gathers (as wide as their flow, bending more on flat
//!    land), lakes in basins that can't drain, valleys easing down to them.
//! 5. **Biomes and habitats** ([`biome`]): wetness, height, which way a
//!    slope faces and the age of a wood choose a [`Habitat`] for every
//!    tile, and each habitat its woods, tall grass and thicket.
//!
//! Fairness comes two ways ([`Fairness`]). **Mirrored**: the land is grown
//! whole, then the south half is made a turned copy of the north along a
//! wandering seam, so every start has the same land round it, and the
//! starts are the seeded map's plateaus. **Measured** (stage 3): the land
//! is grown whole and left so, and a search ([`starts`]) places each start
//! near its plateau where the walks to the enemy come out equal. Either
//! way each start stands in a flat open clearing the mountains keep away
//! from (under Measured, the same clearing for every start).
//!
//! The map's description ([`describe`]) holds only the seed, the
//! wildness, the starts and the sites; the loader grows the land again
//! from them ([`grow`], cached), so a map is the same bytes on every
//! machine. All of it is whole-number maths.

mod biome;
mod erode;
pub(crate) mod field;
mod shape;
mod starts;
mod water;
mod ways;

use std::sync::{Arc, Mutex};

use gc_fixed::{Angle, Fx, FxVec2};
use serde::{Deserialize, Serialize};

use crate::def::{
    CoverKind, CoverOp, Kind, Land, MapDef, Mode, Object, Pos, SetPiece, SetPieceKind, Shape,
    SizeClass, Symmetry, TerrainOp,
};
use crate::generate::{START_PLATEAUS, plateaus};
use crate::recipe::{BASE, WATER, start_objects};
use crate::stamp::dist2;
use crate::wilds::Wildness;
use field::{Grid, ONE, Seeds, fbm, hypot, mul, smoothstep};

/// The water line, in fixed-point tiles: the Wilds' water line (24
/// sixteenths of a tile).
pub(crate) const WATER_LINE: i32 = WATER * ONE / 16;
/// Ground rising faster than this per tile is a cliff (the nav rules').
pub(crate) const CLIFF: i32 = ONE * 12 / 10;
/// The highest grown ground, in fixed-point tiles, a little under the
/// highest the map can hold.
const TOP: i32 = 13 * ONE + ONE / 2;

/// The smallest grown map, in tiles a side (as the Wilds).
pub const MIN_TILES: i32 = crate::wilds::MIN_TILES;

/// What grows on a tile: each biome split by height, wetness, which way a
/// slope faces, rock and the age of the woods (docs/27, "Biome depth").
/// Drawn, and the choice of woods, grass and thicket; the simulation reads
/// the cover, and scrub edge as scrub to clear (`Map::settle_growth`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[repr(u8)]
pub enum Habitat {
    FlowerMeadow,
    #[default]
    Grassland,
    /// Bushes and young trees where meadow meets forest, and on rough
    /// slopes: scrub, which a growing base clears first (idea 22).
    ScrubEdge,
    WetMeadow,
    OldGrowth,
    BirchFringe,
    FernHollow,
    Glade,
    SedgeTussock,
    AlderCarr,
    PeatBog,
    HeatherMoor,
    Scree,
    TreeLine,
    BareRock,
    CrackedFlat,
    BoneDrift,
    RibField,
    DyingGrass,
}

impl Habitat {
    pub const ALL: [Habitat; 19] = [
        Habitat::FlowerMeadow,
        Habitat::Grassland,
        Habitat::ScrubEdge,
        Habitat::WetMeadow,
        Habitat::OldGrowth,
        Habitat::BirchFringe,
        Habitat::FernHollow,
        Habitat::Glade,
        Habitat::SedgeTussock,
        Habitat::AlderCarr,
        Habitat::PeatBog,
        Habitat::HeatherMoor,
        Habitat::Scree,
        Habitat::TreeLine,
        Habitat::BareRock,
        Habitat::CrackedFlat,
        Habitat::BoneDrift,
        Habitat::RibField,
        Habitat::DyingGrass,
    ];

    /// The habitat a tile's byte stands for (grassland for anything unknown).
    pub fn from_u8(b: u8) -> Habitat {
        Habitat::ALL
            .get(usize::from(b))
            .copied()
            .unwrap_or_default()
    }

    /// The biome it belongs to.
    pub fn land(self) -> Land {
        use Habitat::*;
        match self {
            FlowerMeadow | Grassland | ScrubEdge | WetMeadow => Land::Meadow,
            OldGrowth | BirchFringe | FernHollow | Glade => Land::Forest,
            SedgeTussock | AlderCarr | PeatBog => Land::Fen,
            HeatherMoor | Scree | TreeLine | BareRock => Land::Highland,
            CrackedFlat | BoneDrift | RibField | DyingGrass => Land::Waste,
        }
    }
}

/// How wild a grown map is, as numbers for the layers.
pub(crate) struct Dials {
    /// Thousandths of the map that are mountain country.
    pub(crate) mountains: i32,
    /// How high the crests rise over mountain country, fixed-point tiles.
    pub(crate) peaks: i32,
    /// How much sharper than a plain slope the strata edges can be.
    pub(crate) sharpness: i32,
    /// The per-mille height in mountain country above which crags stand.
    pub(crate) crag_line: i32,
    /// How tall the crags are, fixed-point tiles.
    pub(crate) crags: i32,
    /// How wooded the land is: 0 to [`ONE`], about 0.5 for a mixed map.
    pub(crate) forest: i32,
    /// How readily water gathers into rivers: thousandths of the map that
    /// must drain through a tile for a river to start there.
    pub(crate) river_per_mille: i32,
}

impl Dials {
    pub(crate) fn of(wildness: Wildness) -> Dials {
        match wildness {
            Wildness::Plains => Dials {
                mountains: 110,
                peaks: 4 * ONE,
                sharpness: 4 * ONE,
                crag_line: 680,
                crags: ONE * 16 / 10,
                forest: ONE * 4 / 10,
                river_per_mille: 9,
            },
            Wildness::Mixed => Dials {
                mountains: 300,
                peaks: 6 * ONE,
                sharpness: 7 * ONE,
                crag_line: 560,
                crags: ONE * 28 / 10,
                forest: ONE / 2,
                river_per_mille: 11,
            },
            Wildness::Wild => Dials {
                mountains: 440,
                peaks: 7 * ONE,
                sharpness: 8 * ONE + ONE / 2,
                crag_line: 520,
                crags: ONE * 32 / 10,
                forest: ONE * 6 / 10,
                river_per_mille: 11,
            },
        }
    }
}

/// How a grown map is made fair (docs/27-living-land.md, Fairness).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum Fairness {
    /// The south half a turned copy of the north: twins are exact, and the
    /// copy shows at map scale.
    #[default]
    Mirrored,
    /// Land unique all over, the starts placed where the walks to the
    /// enemy come out equal.
    Measured,
}

impl Fairness {
    pub const ALL: [Fairness; 2] = [Fairness::Measured, Fairness::Mirrored];

    pub fn name(self) -> &'static str {
        match self {
            Fairness::Measured => "Measured",
            Fairness::Mirrored => "Mirrored",
        }
    }
}

/// What a description says to grow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Grown {
    pub seed: u64,
    pub wildness: Wildness,
    /// (Descriptions from before stage 3 have none: they were mirrored.)
    #[serde(default)]
    pub fairness: Fairness,
    /// A Conquest map's: the middle is a flat open clearing for the Heart,
    /// as each start is (docs/41, step 1).
    #[serde(default)]
    pub heart: bool,
}

/// Grown land, one value per tile, row by row: what the loader starts a
/// grown map from.
#[derive(Clone, Debug)]
pub(crate) struct Raster {
    /// Heights at tile centres, fixed-point tiles.
    pub(crate) height: Grid,
    /// One `gc_sim::Cover` byte per tile.
    pub(crate) cover: Vec<u8>,
    pub(crate) thicket: Vec<bool>,
    /// One [`Land`] byte per tile.
    pub(crate) land: Vec<u8>,
    /// One [`Habitat`] byte per tile.
    pub(crate) habitat: Vec<u8>,
    /// One `gc_sim::Flow` byte per tile: the rivers' current.
    pub(crate) flow: Vec<u8>,
    /// Where the starts stand, in lobby order.
    pub(crate) starts: [Pos; START_PLATEAUS],
    /// Whether the land is its own turned copy (so every change to it must
    /// be made to a tile and its turned copy alike).
    pub(crate) mirrored: bool,
}

impl Raster {
    /// The heights in simulation units at each height sample of a map
    /// `side` samples across ([`crate::SAMPLES_PER_TILE`] to a tile),
    /// bilinear between the tile centres.
    pub(crate) fn sample_heights(&self, side: usize) -> Vec<i32> {
        let per = i64::from(crate::SAMPLES_PER_TILE);
        let one = i64::from(ONE);
        let units = i64::from(gc_sim::terrain::HEIGHT_UNITS);
        let g = &self.height;
        let last = g.n as i64 - 1;
        // Bilinear, as `Grid::sample`, with each column's and row's
        // tiles and weights worked out once.
        let axis: Vec<(usize, usize, i64)> = (0..side)
            .map(|s| {
                let at = s as i64 * one / per - one / 2;
                let (i, f) = (at.div_euclid(one), at.rem_euclid(one));
                (
                    i.clamp(0, last) as usize,
                    (i + 1).clamp(0, last) as usize,
                    f,
                )
            })
            .collect();
        let mut out = Vec::with_capacity(side * side);
        for &(y0, y1, fy) in &axis {
            let (r0, r1) = (
                &g.v[y0 * g.n..(y0 + 1) * g.n],
                &g.v[y1 * g.n..(y1 + 1) * g.n],
            );
            for &(x0, x1, fx) in &axis {
                let top = i64::from(r0[x0]) * (one - fx) + i64::from(r0[x1]) * fx;
                let bottom = i64::from(r1[x0]) * (one - fx) + i64::from(r1[x1]) * fx;
                let h = (top * (one - fy) + bottom * fy) >> 32;
                out.push((h * units / one) as i32);
            }
        }
        out
    }
}

/// The grown maps made most recently, so describing a map and then
/// loading it (or loading it twice) grows it once.
type Cached = ((u32, Grown), Arc<Raster>);
static RECENT: Mutex<Vec<Cached>> = Mutex::new(Vec::new());
const CACHED: usize = 2;

/// The land a seed grows, `tiles` on a side.
pub(crate) fn grow(tiles: u32, what: Grown) -> Arc<Raster> {
    let key = (tiles, what);
    if let Ok(cache) = RECENT.lock()
        && let Some((_, r)) = cache.iter().find(|(k, _)| *k == key)
    {
        return Arc::clone(r);
    }
    let raster = Arc::new(grow_fair(tiles as usize, what));
    if let Ok(mut cache) = RECENT.lock() {
        cache.retain(|(k, _)| *k != key);
        cache.insert(0, (key, Arc::clone(&raster)));
        cache.truncate(CACHED);
    }
    raster
}

/// The land grown, and under [`Fairness::Measured`] grown once more from
/// another seed if its starts' walks are more unequal than
/// [`MEASURED_LIMIT`], keeping the fairer. Mirrored land is fair by construction.
fn grow_fair(n: usize, what: Grown) -> Raster {
    // How unequal a Measured map's walks are, measured unless its
    // settling already did.
    let grown = |what: Grown| {
        let (r, spread) = grow_uncached(n, what);
        let spread = spread.unwrap_or_else(|| true_unfairness(&r, &standable(&r)));
        (r, spread)
    };
    if what.fairness != Fairness::Measured {
        return grow_uncached(n, what).0;
    }
    let (first, spread) = grown(what);
    if spread <= MEASURED_LIMIT {
        return first;
    }
    let seed = what.seed ^ 0x9E37_79B9_7F4A_7C15;
    let (again, s) = grown(Grown { seed, ..what });
    if s < spread { again } else { first }
}

/// How unequal Measured starts' walks may be (thousandths, as
/// [`starts::unfairness`]: the worst over every way the lobby hands starts
/// out) before the land is grown once more from another seed.
pub(crate) const MEASURED_LIMIT: u32 = 100;

/// `t` tiles on a 512-tile map, scaled to one `n` across (whole tiles).
fn scaled(n: usize, t: i32) -> i32 {
    (i64::from(t) * n as i64 / 512) as i32
}

/// How many places with room a site weighs before it picks the one its
/// kind likes best.
const SITE_CHOICES: usize = 12;

/// How far round each start the land is kept a flat open clearing, in
/// whole tiles on an `n`-tile map.
fn clear_radius(n: usize) -> i32 {
    scaled(n, 22)
}

/// How far a start's open ground reaches, in whole tiles on an `n`-tile map.
fn open_reach(n: usize) -> i32 {
    scaled(n, 40)
}

/// Each tile's distance to the nearest of `starts`, whole tiles.
fn near_starts(n: usize, starts: &[Pos]) -> Grid {
    Grid::from_fn(n, |x, y| {
        starts
            .iter()
            .map(|&(sx, sy)| {
                let (dx, dy) = (i64::from(x as i32 - sx), i64::from(y as i32 - sy));
                dx * dx + dy * dy
            })
            .min()
            // One root for the nearest: the root keeps the order.
            .map_or(i32::MAX, |d2| gc_fixed::isqrt_u64((d2 as u64) << 32) as i32)
    })
}

fn grow_uncached(n: usize, what: Grown) -> (Raster, Option<u32>) {
    let dials = Dials::of(what.wildness);
    let mut seeds = Seeds::new(what.seed);
    let mirrored = what.fairness == Fairness::Mirrored;
    let slots = plateaus(n as u32);
    let near_slot = near_starts(n, &slots);
    let calm = near_slot.map(|d| smoothstep(scaled(n, 95) * ONE, scaled(n, 55) * ONE, d));
    let (raw, parts) = shape::raw_land(n, &mut seeds, &dials, &calm);
    let benched = shape::terrace(&raw, &mut seeds, &dials);
    let mut h = shape::massifs(&benched, &parts, &mut seeds, &dials).map(soft_top);
    let droplets = 70_000 * n * n / (512 * 512);
    erode::erode(&mut h, seeds.next(), droplets);
    erode::thermal(&mut h, ONE * 18 / 10, 4);
    let (starts, near_start, shape) = if mirrored {
        for &s in &slots {
            let shape = clearing_shape(&mut seeds);
            flatten_clearing(&mut h, s, shape);
        }
        (slots, near_slot, [0; 3])
    } else {
        // Every start gets the same clearing, so building room is equal
        // by construction.
        let starts = starts::place(&h);
        let shape = clearing_shape(&mut seeds);
        for &s in &starts {
            flatten_clearing(&mut h, s, shape);
        }
        (starts, near_starts(n, &starts), shape)
    };
    // A Conquest map's Heart stands in a clearing of its own, kept open
    // as a start's is.
    let mut near_start = near_start;
    if what.heart {
        let centre = heart_tile(n);
        flatten_clearing(&mut h, centre, clearing_shape(&mut seeds));
        let near_centre = near_starts(n, &[centre]);
        for (d, &c) in near_start.v.iter_mut().zip(&near_centre.v) {
            *d = (*d).min(c);
        }
    }
    let open_r = open_reach(n);
    // The open ground round each start, with a ragged edge: its reach
    // wanders from three quarters to one and a quarter of `open_r`.
    let ragged = fbm(n, seeds.next(), scaled(n, 24).max(8) * ONE, 3);
    let keep: Vec<bool> = (near_start.v.iter().zip(&ragged.v))
        .map(|(&d, &r)| d <= mul(open_r * ONE, ONE * 3 / 4 + r / 2))
        .collect();

    let (d, flow) = if mirrored {
        // The south half from the north: blend along a wandering seam, so
        // the water finds the same land on both sides.
        let seam = Seam::new(n, &mut seeds);
        seam.blend(&mut h);
        let mut wet = water::water(&mut h, &mut seeds, &dials, &keep);
        seam.select(&mut h.v);
        seam.smooth(&mut h);
        for v in &mut h.v {
            *v = soft_top(*v).max(0);
        }
        // The biomes read only fields that are their own turned copies, so
        // the woods and lands match across the halves without a seam.
        seam.select(&mut wet.acc);
        seam.select(&mut wet.river);
        seam.select(&mut wet.lake);
        let sym = |mut g: Grid| {
            seam.blend(&mut g);
            g
        };
        let mut d = biome::dress(&h, &wet, &mut seeds, &dials, &keep, &sym);
        seam.select(&mut d.cover);
        seam.select(&mut d.thicket);
        // Knots that meet across the seam can join into one too long.
        biome::drop_long_thicket(&mut d.thicket, n);
        seam.select(&mut d.land);
        seam.select(&mut d.habitat);
        (d, wet.flow)
    } else {
        let wet = water::water(&mut h, &mut seeds, &dials, &keep);
        for v in &mut h.v {
            *v = soft_top(*v).max(0);
        }
        let d = biome::dress(&h, &wet, &mut seeds, &dials, &keep, &|g| g);
        (d, wet.flow)
    };
    let mut raster = Raster {
        height: h,
        cover: d.cover,
        thicket: d.thicket,
        land: d.land,
        habitat: d.habitat,
        flow,
        starts,
        mirrored,
    };
    ways::open_ways(&mut raster, &mut seeds);
    let spread = (!mirrored)
        .then(|| refine_starts(&mut raster, shape, &mut seeds))
        .flatten();
    (raster, spread)
}

/// Measured fairness's second look: on the finished land, move the starts
/// a few tiles where the walks come out more equal, clear and flatten the
/// same clearing at each that moved, and cut the ways again.
fn refine_starts(r: &mut Raster, shape: [u16; 3], seeds: &mut Seeds) -> Option<u32> {
    let mut now = None;
    for round in 0..SETTLE_ROUNDS {
        if now.is_some_and(|u| u <= SETTLED) {
            break;
        }
        let (mut trial, mut s) = (r.clone(), seeds.clone());
        let reach = scaled(r.height.n, if round == 0 { 24 } else { 8 });
        let (before, moved) = move_starts(&mut trial, shape, &mut s, reach);
        let was = *now.get_or_insert(before);
        if !moved {
            break;
        }
        let after = true_unfairness(&trial, &standable(&trial));
        if after >= was {
            break;
        }
        (*r, *seeds, now) = (trial, s, Some(after));
    }
    now
}

/// Measured's settling stops once the starts' walks are this unequal or
/// less (thousandths, as [`starts::unfairness`]), or after this many
/// rounds of moving them.
const SETTLED: u32 = 50;
const SETTLE_ROUNDS: usize = 2;

/// How unequal the walks between the starts are on the land as it
/// stands (`step` from [`standable`]), in thousandths.
fn true_unfairness(r: &Raster, step: &[u8]) -> u32 {
    let n = r.height.n;
    let mut walk = [[0u32; START_PLATEAUS]; START_PLATEAUS];
    for (j, &s) in r.starts.iter().enumerate() {
        let d = ways::walk(step, n, tile_index(n, s), &[]);
        for (row, &k) in walk.iter_mut().zip(&r.starts) {
            row[j] = walk_to(&d, n, k).map_or(u32::MAX, |(d, _)| d);
        }
    }
    starts::unfairness(&walk)
}

/// One round of [`refine_starts`]: move the starts up to `reach` tiles by
/// their true walks, clear and flatten their clearings and cut the ways
/// again. How unequal the walks were before (as [`starts::unfairness`]),
/// and whether any start moved.
fn move_starts(r: &mut Raster, shape: [u16; 3], seeds: &mut Seeds, reach: i32) -> (u32, bool) {
    let n = r.height.n as i32;
    let step = standable(r);
    let half = clear_radius(n as usize) / 2;
    // A clearing goes where its middle is standable and none of it is
    // under water.
    let fits = |p: Pos| {
        let at = |x: i32, y: i32| (y * n + x) as usize;
        (half..n - half).contains(&p.0)
            && (half..n - half).contains(&p.1)
            && step[at(p.0, p.1)] != 0
            && (-half..=half).all(|dy| {
                (-half..=half).all(|dx| r.height.v[at(p.0 + dx, p.1 + dy)] > WATER_LINE + ONE / 4)
            })
    };
    let (moved, before) = starts::settle(&step, n as usize, r.starts, reach, &fits);
    if moved == r.starts {
        return (before, false);
    }
    // The meadow is the part of the clearing flattened however its edge
    // wobbles; the open ground the least a start's ragged edge gives.
    let meadow = clear_radius(n as usize) * 62 / 100;
    let open = open_reach(n as usize) * 3 / 4;
    for (k, &at) in moved.iter().enumerate() {
        if at == r.starts[k] {
            continue;
        }
        flatten_clearing(&mut r.height, at, shape);
        for y in (at.1 - open).max(0)..(at.1 + open + 1).min(n) {
            for x in (at.0 - open).max(0)..(at.0 + open + 1).min(n) {
                let i = (y * n + x) as usize;
                let d2 = dist2((x, y), at);
                if d2 > open * open || r.height.v[i] <= WATER_LINE {
                    continue;
                }
                r.cover[i] = gc_sim::Cover::Open as u8;
                r.thicket[i] = false;
                if d2 <= meadow * meadow {
                    r.habitat[i] = Habitat::Grassland as u8;
                    r.land[i] = Habitat::Grassland.land() as u8;
                }
            }
        }
    }
    r.starts = moved;
    ways::open_ways(r, seeds);
    (before, true)
}

/// What it costs to step onto each tile ([`crate::Ground::cost`], ten on
/// open ground) where a regiment's centre can stand
/// ([`crate::Map::standable`]) on the land as it stands, and 0 where it
/// can't.
fn standable(r: &Raster) -> Vec<u8> {
    let n = r.height.n;
    let side = n * crate::SAMPLES_PER_TILE as usize + 1;
    let samples = r
        .sample_heights(side)
        .iter()
        .map(|&h| crate::map::sample_of_sim(i64::from(h)))
        .collect();
    let water =
        crate::map::sample_of_sim(i64::from(WATER) * i64::from(gc_sim::terrain::HEIGHT_UNITS) / 16);
    let mut map = crate::Map::from_samples(n as u32, samples, water);
    map.thicket.clone_from(&r.thicket);
    let ground = map.tile_ground(&map.nav_costs());
    map.standable(&ground)
        .iter()
        .zip(&ground)
        .map(|(&ok, g)| if ok { g.cost() as u8 } else { 0 })
        .collect()
}

/// Heights eased under [`TOP`]: the same below the snow line and ever
/// flatter above it, so the highest crests stay peaks instead of being cut
/// off into flat tops.
fn soft_top(h: i32) -> i32 {
    const SNOW: i32 = 9 * ONE + ONE / 2;
    const ROOM: i32 = TOP - SNOW;
    if h <= SNOW {
        return h;
    }
    let over = i64::from(h - SNOW);
    SNOW + (over * i64::from(ROOM) / (over + i64::from(ROOM))) as i32
}

/// The wobble of a clearing's edge: three phases.
fn clearing_shape(seeds: &mut Seeds) -> [u16; 3] {
    [
        seeds.next() as u16,
        seeds.next() as u16,
        seeds.next() as u16,
    ]
}

/// Flatten an irregular clearing (its edge's wobble `k`) round a start, to
/// the ground's middle height there (and well clear of the water), fading
/// out over the clearing's edge.
fn flatten_clearing(h: &mut Grid, at: Pos, k: [u16; 3]) {
    let n = h.n;
    let r = clear_radius(n);
    let fade = scaled(n, 16).max(6);
    let near: Vec<i32> = (-6..=6)
        .flat_map(|dy| (-6..=6).map(move |dx| (dx, dy)))
        .map(|(dx, dy)| h.get(i64::from(at.0 + dx), i64::from(at.1 + dy)))
        .collect();
    let level = field::percentile(near, 500).max(WATER_LINE + ONE * 8 / 10);
    let reach = (r * 13 / 10 + fade + 1).max(1);
    for y in (at.1 - reach).max(0)..(at.1 + reach + 1).min(n as i32) {
        for x in (at.0 - reach).max(0)..(at.0 + reach + 1).min(n as i32) {
            let (dx, dy) = (x - at.0, y - at.1);
            let d = hypot(dx * ONE, dy * ONE);
            let angle = Angle::of_vec(FxVec2::new(Fx::from_int(dx), Fx::from_int(dy))).0;
            let wave = |m: u16, phase: u16, amp: i32| {
                mul(
                    amp,
                    Angle(angle.wrapping_mul(m).wrapping_add(phase)).sin().raw(),
                )
            };
            let wobble = ONE
                + wave(3, k[0], ONE * 18 / 100)
                + wave(5, k[1], ONE * 12 / 100)
                + wave(2, k[2], ONE * 8 / 100);
            let rad = r * wobble;
            let w = smoothstep(rad + fade * ONE, rad, d);
            let i = y as usize * n + x as usize;
            h.v[i] += mul(level - h.v[i], w);
        }
    }
}

/// The seam the south half is copied along: for each tile, how much of it
/// is its own (north) land and how much its turned copy's, from 0 to
/// [`ONE`], with a tile's and its turned copy's shares adding to one.
struct Seam {
    n: usize,
    weight: Vec<i32>,
}

impl Seam {
    fn new(n: usize, seeds: &mut Seeds) -> Seam {
        // A wandering line near the middle: the middle row bent by noise.
        let wander = fbm(n, seeds.next(), (n as i32) * ONE / 5, 4);
        let band = 30 * ONE;
        let amp = 36 * ONE;
        let mut weight = vec![ONE / 2; n * n];
        for i in 0..n * n {
            let j = n * n - 1 - i;
            if i >= j {
                continue;
            }
            let y = (i / n) as i32;
            // (n/2 - 0.5 - y) in fixed point, moved by the noise's
            // difference from its turned copy's.
            let t = (n as i32) * ONE / 2 - ONE / 2 - y * ONE + mul(amp, wander.v[i] - wander.v[j]);
            let w = smoothstep(-band, band, t);
            weight[i] = w;
            weight[j] = ONE - w;
        }
        Seam { n, weight }
    }

    /// Each tile and its turned copy (j = last - i) as the same blend of
    /// the two.
    fn blend(&self, h: &mut Grid) {
        let last = self.n * self.n - 1;
        for i in 0..=last / 2 {
            let j = last - i;
            let v = field::lerp(h.v[j], h.v[i], self.weight[i]);
            h.v[i] = v;
            h.v[j] = v;
        }
    }

    /// Each tile and its turned copy as whichever of the two has the
    /// bigger share.
    fn select<T: Copy>(&self, v: &mut [T]) {
        let last = self.n * self.n - 1;
        for i in 0..=last / 2 {
            let j = last - i;
            if self.weight[i] >= ONE / 2 {
                v[j] = v[i];
            } else {
                v[i] = v[j];
            }
        }
    }

    /// Smooth the land near the seam, where a river cut on one side may
    /// meet uncut ground on the other: an average over a few tiles, the
    /// same each side, so the copy stays exact.
    fn smooth(&self, h: &mut Grid) {
        let soft = h.blur(2, 2);
        for i in 0..self.n * self.n {
            let w = self.weight[i];
            let near = ONE / 2 - (w - ONE / 2).abs(); // 0 far off, 0.5 on the seam
            if near > ONE / 20 {
                h.v[i] = soft.v[i];
            }
        }
    }
}

/// The Field Battle map a seed grows, `tiles` on a side, as wild as
/// `wildness`: the land itself is grown by the loader from the seed; this
/// holds the starts and the sites out in the land (Grubfolk villages,
/// outposts, lairs and standing stones), each on open ground with room
/// round it.
///
/// Under [`Fairness::Measured`] nothing is turned for the south: each site
/// in the north gets a partner of its own in the south, as near the turned
/// spot as the land has room.
pub fn describe(tiles: i32, seed: u64, wildness: Wildness, fairness: Fairness) -> MapDef {
    let what = Grown {
        seed,
        wildness,
        fairness,
        heart: false,
    };
    lay(tiles, what).expect("a Field Battle map always has room for its starts")
}

/// The Conquest map a seed grows on Living Land (docs/41, step 1), `tiles`
/// on a side (one of [`crate::conquest::WILDS_TILES`]), as wild as
/// `wildness`: the Classic Conquest layout
/// ([`crate::conquest::describe_wilds`]) on grown land. Four starts (the
/// corners) under [`crate::def::BIG_MAP_TILES`] and six from it, players
/// round the north starts west to east and their turned copies the rest;
/// each north start's share is placed on the land at the same walk from
/// it as every other start's (see [`shares`]), the Heart stands in a
/// clearing in the middle, and each lair guards a rich field. A seed whose
/// land has no room for a share (or no counter-route) grows another seed's
/// land instead, and after [`CONQUEST_TRIES`] the Classic map of the seed
/// is made; `None` only for a size the Conquest maps don't come in.
pub fn describe_conquest(tiles: i32, seed: u64, wildness: Wildness) -> Option<MapDef> {
    if !crate::conquest::WILDS_TILES.contains(&tiles) {
        return None;
    }
    (0..CONQUEST_TRIES)
        .find_map(|k| {
            let what = Grown {
                // (Next seeds far apart, so a lobby seed and the next don't
                // fall back on the same land.)
                seed: seed ^ k.wrapping_mul(0x9E37_79B9_7F4A_7C15),
                wildness,
                fairness: Fairness::Mirrored,
                heart: true,
            };
            lay(tiles, what)
        })
        // (Land that never has room: the Classic map of the same seed.)
        .or_else(|| crate::conquest::describe_wilds(tiles, seed, wildness))
}

/// How many seeds in a row [`describe_conquest`] tries.
const CONQUEST_TRIES: u64 = 16;

/// The tile a Conquest map's Heart stands on.
fn heart_tile(n: usize) -> Pos {
    (n as i32 / 2, n as i32 / 2)
}

/// A grown map's description (see [`describe`]); for Conquest (a map
/// with a Heart) the shares and the Heart too, or `None` if the land has
/// no room for a share or a start has no counter-route.
fn lay(tiles: i32, what: Grown) -> Option<MapDef> {
    let seed = what.seed;
    let n = tiles as usize;
    let measured = what.fairness == Fairness::Measured;
    let conquest = what.heart;
    // Four starts (the corners) on a small Conquest map, else six.
    let six = !conquest || tiles >= crate::def::BIG_MAP_TILES;
    let starts: Vec<Pos> = if measured {
        grow(tiles as u32, what).starts.to_vec()
    } else {
        plateaus(tiles as u32)[..if six { 6 } else { 4 }].to_vec()
    };
    let mut def = MapDef {
        name: if conquest {
            format!("Conquest Land {seed}")
        } else {
            format!("Land {seed}")
        },
        size: SizeClass::Field(tiles),
        symmetry: if measured {
            Symmetry::None
        } else {
            Symmetry::Rot2
        },
        players: starts.len() as u8,
        base: BASE,
        water: WATER,
        noise: None,
        grown: Some(what),
        terrain: Vec::new(),
        cover: Vec::new(),
        land: Vec::new(),
        objects: start_objects(&starts),
        set_pieces: Vec::new(),
    };
    // Where a site can stand: open, walkable ground with a regiment's room
    // round it, from the land as the loader builds it.
    let map = crate::Map::from_def(&def);
    let ground = map.tile_ground(&map.nav_costs());
    let standable = map.standable(&ground);
    let room = |at: Pos, r: i32| {
        (-r..=r).all(|dy| {
            (-r..=r).all(|dx| {
                let (x, y) = (at.0 + dx, at.1 + dy);
                (0..tiles).contains(&x)
                    && (0..tiles).contains(&y)
                    && standable[(y * tiles + x) as usize]
                    && !map.is_thicket((y * tiles + x) as usize)
            })
        })
    };
    // Where a Conquest map's piece has room: standable, no thicket, no ford
    // under or beside it.
    let fits = |p: Pos| {
        room(p, 1)
            && (-1..=1).all(|dy| {
                (-1..=1).all(|dx| {
                    ground[((p.1 + dy) * tiles + p.0 + dx) as usize] != crate::Ground::Ford
                })
            })
    };
    // A Conquest map's north starts, west to east: the corners, and the
    // middle between them with six.
    let north = if six {
        vec![starts[0], starts[4], starts[1]]
    } else {
        vec![starts[0], starts[1]]
    };
    // A Conquest map's shares and Heart, in place of the starts.
    if conquest {
        let (cost, walks) = walks_from(&map, &north);
        if !counter_routes(n, &cost, &starts, &north, &walks) {
            return None;
        }
        def.objects = shares(n, &north, &walks, &fits)?;
        def.objects.push(Object {
            kind: Kind::Heart,
            at: heart_tile(n),
            owner: None,
            label: String::new(),
            once: true,
        });
    }
    def.set_pieces = set_pieces(&map, &ground, &standable, &starts, measured);
    let mut rng = gc_fixed::Pcg32::new(seed, 0x0073_6974);
    // The nearest spot to the turned copy of `at` that passes `fits`,
    // within `r` tiles of it.
    let partner = |at: Pos, r: i32, fits: &dyn Fn(Pos) -> bool| {
        nearest((tiles - 1 - at.0, tiles - 1 - at.1), r, fits)
    };
    let partner_reach = (tiles / 12).max(8);
    // (Under Measured both halves' pieces are listed.)
    let enough = if measured { 4 } else { 2 };
    // Too few places to fight over: raise a knoll or two on open ground
    // where the land has none.
    for label in ["The knoll", "The far knoll"] {
        if def.set_pieces.len() >= enough {
            break;
        }
        let far = |p: Pos, d: &MapDef| {
            d.set_pieces
                .iter()
                .all(|s| dist2(s.at, p) > (tiles / 5) * (tiles / 5))
                && starts
                    .iter()
                    .all(|&s| dist2(s, p) > (open_reach(n) + 12).pow(2))
                && d.objects.iter().all(|o| {
                    // (A Conquest map's Heart keeps its clearing flat.)
                    let keep = if o.kind == Kind::Heart {
                        open_reach(n) + 12
                    } else {
                        16
                    };
                    dist2(o.at, p) > keep * keep
                })
        };
        let spot = (0..64).find_map(|_| {
            let at = (
                16 + rng.below((tiles - 32) as u32) as i32,
                16 + rng.below((tiles / 2 - 28) as u32) as i32,
            );
            (far(at, &def) && room(at, 8)).then_some(at)
        });
        let Some(at) = spot else { break };
        let mut knolls = vec![at];
        if measured {
            let inside = |q: Pos| (8..tiles - 8).contains(&q.0) && (8..tiles - 8).contains(&q.1);
            let Some(other) = partner(at, partner_reach, &|q| {
                inside(q) && far(q, &def) && room(q, 8)
            }) else {
                break;
            };
            knolls.push(other);
        }
        for at in knolls {
            let here = i32::from(map.heights[(at.1 * tiles + at.0) as usize]) / 16;
            def.terrain.push(TerrainOp {
                shape: Shape::Disc { at, r: 4 },
                mode: Mode::Raise,
                height: here + 48,
                edge: 12,
            });
            def.set_pieces.push(SetPiece {
                kind: SetPieceKind::High,
                at,
                label: label.into(),
            });
        }
    }
    let reach = scaled(n, 12).max(6);
    let open = open_reach(n);
    let mut taken: Vec<(Pos, i32)> = starts.iter().map(|&s| (s, open + reach)).collect();
    // (A Conquest map's shares and Heart too.)
    if conquest {
        taken.extend(def.objects.iter().map(|o| match o.kind {
            Kind::Heart => (o.at, open),
            _ => (o.at, 4),
        }));
    }
    let per_area = |rng: &mut gc_fixed::Pcg32, lo: i32, hi: i32| {
        let k = lo + rng.below((hi - lo + 1) as u32) as i32;
        (k * tiles * tiles + 256 * 256) / (512 * 512)
    };
    let lairs = per_area(&mut rng, 1, 2);
    let sites = [
        (Kind::Village, "Grubfolk village", per_area(&mut rng, 1, 2)),
        (
            Kind::CreepCamp,
            "The company camp",
            per_area(&mut rng, 0, 1),
        ),
        (Kind::CreepCamp, "The old fort", per_area(&mut rng, 1, 1)),
        (Kind::CreepCamp, "The lair", lairs),
        (Kind::Shrine, "The standing stones", 1),
    ];
    // How much a site of each kind likes a spot: villages water (a lake
    // shore, where rivers meet), the old fort the ground standing over its
    // surroundings, lairs deep woods far from any start, the standing
    // stones the highest ground. The company camp takes the first place
    // with room.
    let height = |p: Pos| i32::from(map.heights[(p.1 * tiles + p.0) as usize]);
    let count = |p: Pos, r: i32, f: &dyn Fn(usize) -> bool| {
        (-r..=r)
            .flat_map(|dy| (-r..=r).map(move |dx| (p.0 + dx, p.1 + dy)))
            .filter(|&q| (0..tiles).contains(&q.0) && (0..tiles).contains(&q.1))
            .filter(|&q| dist2(q, p) <= r * r && f((q.1 * tiles + q.0) as usize))
            .count() as i32
    };
    let reason = |label: &str, p: Pos| match label {
        "Grubfolk village" => count(p, 10, &|i| map.heights[i] < map.water),
        "The old fort" => {
            let ring: i32 = [
                (10, 0),
                (-10, 0),
                (0, 10),
                (0, -10),
                (7, 7),
                (-7, 7),
                (7, -7),
                (-7, -7),
            ]
            .iter()
            .map(|&(dx, dy)| {
                height((
                    (p.0 + dx).clamp(0, tiles - 1),
                    (p.1 + dy).clamp(0, tiles - 1),
                ))
            })
            .sum();
            height(p) - ring / 8
        }
        "The lair" => {
            let woods = count(p, 10, &|i| {
                map.cover.get(i) == Some(&(gc_sim::Cover::Wood as u8))
            });
            let far = starts.iter().map(|&s| dist2(s, p)).min().unwrap_or(0);
            woods + gc_fixed::isqrt_u64(far as u64) as i32 / 2
        }
        "The standing stones" => height(p),
        _ => 0,
    };
    'kinds: for (kind, label, count) in sites {
        for _ in 0..count {
            let ok = |at: Pos, taken: &[(Pos, i32)]| {
                let copy = (tiles - 1 - at.0, tiles - 1 - at.1);
                dist2(at, copy) >= 4 * reach * reach
                    && taken.iter().all(|&(p, r)| {
                        let d = (r + reach) * (r + reach);
                        dist2(p, at) >= d && (measured || dist2(p, copy) >= d)
                    })
                    && room(at, 3)
                    // (A Conquest lair's rich field has room too.)
                    && (!(conquest && label == "The lair")
                        || [(-3, 3), (3, 3)].iter().all(|&(dx, dy)| fits((at.0 + dx, at.1 + dy))))
            };
            // Sites go where the land gives a reason (docs/27, Settlers):
            // of the places with room, the one its kind likes best.
            let mut choices: Vec<(i32, Pos)> = Vec::new();
            for _ in 0..96 {
                let at = (
                    8 + rng.below((tiles - 16) as u32) as i32,
                    8 + rng.below((tiles / 2 - 12) as u32) as i32,
                );
                if ok(at, &taken) {
                    choices.push((reason(label, at), at));
                    if choices.len() == SITE_CHOICES {
                        break;
                    }
                }
            }
            // Best first (of equals, the first found); under Measured the
            // best that has a partner in the south.
            choices.sort_by_key(|&(score, _)| std::cmp::Reverse(score));
            let inside = |q: Pos| (8..tiles - 8).contains(&q.0) && (8..tiles - 8).contains(&q.1);
            let spots = choices.iter().find_map(|&(_, at)| {
                if !measured {
                    return Some(vec![at]);
                }
                let mut with = taken.clone();
                with.push((at, reach));
                partner(at, partner_reach, &|q| inside(q) && ok(q, &with)).map(|o| vec![at, o])
            });
            // (No room for one is no room for the rest.)
            let Some(spots) = spots else { break 'kinds };
            for at in spots {
                taken.push((at, reach));
                def.objects.push(Object {
                    kind,
                    at,
                    owner: None,
                    label: label.into(),
                    once: false,
                });
                def.cover.push(CoverOp {
                    shape: Shape::Disc { at, r: reach / 2 },
                    kind: CoverKind::Open,
                });
                // On a Conquest map a lair guards a rich field (on the
                // room the site keeps round it).
                if conquest && label == "The lair" {
                    for (kind, dx) in [(Kind::GrubSeep, -3), (Kind::MarrowVein, 3)] {
                        def.objects.push(Object {
                            kind,
                            at: (at.0 + dx, at.1 + 3),
                            owner: None,
                            label: String::new(),
                            once: false,
                        });
                    }
                }
            }
        }
    }
    // The sites' open ground and the knolls change the land a little: the
    // shares must still be fair on the land as it ships.
    if conquest && !still_fair(&def, &starts, &north) {
        return None;
    }
    Some(def)
}

/// The cost of stepping onto each tile of `map` (as [`ways::walk`] takes
/// it, 0 where a regiment can't stand) and the walks from each of `from`.
fn walks_from(map: &crate::Map, from: &[Pos]) -> (Vec<u8>, Vec<Vec<u32>>) {
    let n = map.tiles as usize;
    let ground = map.tile_ground(&map.nav_costs());
    let cost: Vec<u8> = (map.standable(&ground).iter().zip(&ground))
        .map(|(&ok, g)| if ok { g.cost() as u8 } else { 0 })
        .collect();
    let walks = from
        .iter()
        .map(|&s| ways::walk(&cost, n, tile_index(n, s), &[]))
        .collect();
    (cost, walks)
}

/// Whether a Conquest map's shares are still fair on the land its
/// description makes (sites and knolls included): every start's walk to
/// each piece of its share within two of [`shares`]' slacks of every
/// other's, and every start still with a counter-route.
fn still_fair(def: &MapDef, starts: &[Pos], north: &[Pos]) -> bool {
    let map = crate::Map::from_def(def);
    let n = map.tiles as usize;
    let (cost, walks) = walks_from(&map, north);
    if !counter_routes(n, &cost, starts, north, &walks) {
        return false;
    }
    // The shares come first, piece by piece, one per north start.
    let k = north.len();
    def.objects[k..k * (1 + crate::conquest::SHARE.len())]
        .chunks(k)
        .filter(|piece| piece[0].kind != Kind::CreepCamp)
        .all(|piece| {
            let d: Vec<u32> = (piece.iter().zip(&walks))
                .map(|(o, w)| walk_to(w, n, o.at).map_or(u32::MAX, |(d, _)| d))
                .collect();
            let (lo, hi) = (d.iter().min().unwrap_or(&0), d.iter().max().unwrap_or(&0));
            *hi != u32::MAX && hi - lo <= 2 * (hi / 100).max(2)
        })
}

/// The tile `p` as an index, row by row, on an `n`-tile map.
fn tile_index(n: usize, p: Pos) -> usize {
    p.1 as usize * n + p.0 as usize
}

/// The walk (as [`ways::walk`] gives them) to a thing on tile `p`: to the
/// tile or one beside it, as the map checker measures it, and that tile.
fn walk_to(dist: &[u32], n: usize, p: Pos) -> Option<(u32, Pos)> {
    let tiles = n as i32;
    (-1..=1)
        .flat_map(|dy| (-1..=1).map(move |dx| (p.0 + dx, p.1 + dy)))
        .filter(|&(x, y)| (0..tiles).contains(&x) && (0..tiles).contains(&y))
        .map(|q| (dist[tile_index(n, q)], q))
        .filter(|&(d, _)| d != u32::MAX)
        .min()
}

/// The shares round the north starts of a Conquest map (west to east,
/// players 0 up; the loader turns them for the south): the Classic
/// Conquest share ([`crate::conquest::SHARE`]). Grown land isn't flat, so
/// each piece stands where the walk to it from its start (`walks`, one per
/// north start) is what it would be on open ground, as near its place on
/// open ground as the land allows: every start has the same walks to the
/// same things. Where the land makes every start go round, all get the
/// same longer walk. A creep camp stands by the field it guards. `fits`
/// says where a piece has room. `None` if one has nowhere to go.
fn shares(
    n: usize,
    north: &[Pos],
    walks: &[Vec<u32>],
    fits: &dyn Fn(Pos) -> bool,
) -> Option<Vec<Object>> {
    use crate::conquest::SHARE;
    use crate::def::{BASE_AREA, EXPANSION_AREA};
    let tiles = n as i32;
    // The share's reach, as the Classic maps' clearing.
    let z = scaled(n, 60);
    // Things stand at least this far apart, and in the north half.
    const APART: i32 = 4;
    let free = |p: Pos, out: &[Object]| {
        (8..tiles - 8).contains(&p.0)
            && (8..tiles / 2 - 8).contains(&p.1)
            && fits(p)
            && out
                .iter()
                .all(|o| (o.at.0 - p.0).abs().max((o.at.1 - p.1).abs()) >= APART)
    };
    let mut out: Vec<Object> = (north.iter().zip(0u8..))
        .map(|(&at, player)| Object {
            kind: Kind::Start,
            at,
            owner: Some(player),
            label: String::new(),
            once: false,
        })
        .collect();
    // Where each start's last piece went, and where it would have on open
    // ground.
    let mut last: Vec<(Pos, Pos)> = north.iter().map(|&s| (s, (0, 0))).collect();
    for piece in SHARE {
        let off = if piece.owned {
            piece.at
        } else {
            (z * piece.at.0 / 100, z * piece.at.1 / 100)
        };
        let guard = piece.kind == Kind::CreepCamp;
        let (ax, ay) = (off.0.abs(), off.1.abs());
        // The walk on open ground: ten a step, fourteen a diagonal one,
        // less the step onto it.
        let open = (10 * ax.max(ay) + 4 * ax.min(ay) - 10).max(0) as u32;
        let base = ax * ax + ay * ay <= BASE_AREA * BASE_AREA;
        let reach = (ax.max(ay) / 2).max(10);
        let kept = out.len();
        // Every start's piece at the same walk: the open ground's, or a
        // little longer where the land makes every start go round.
        let spots = (0..=6).find_map(|stretch| {
            let target = open + open * stretch / 20;
            let slack = (target / 100).max(2);
            out.truncate(kept);
            let mut spots = Vec::new();
            for (k, &s) in north.iter().enumerate() {
                let spot = if guard {
                    // Beside the field it guards, as on open ground.
                    let (field, field_off) = last[k];
                    let want = (field.0 + off.0 - field_off.0, field.1 + off.1 - field_off.1);
                    nearest(want, 8, &|p| free(p, &out))
                } else {
                    // A start's own seeps and vein stay in its base area,
                    // its natural expansion out past it.
                    let area = |p: Pos| {
                        let d = dist2(p, s);
                        !piece.owned
                            || if base {
                                d <= BASE_AREA * BASE_AREA
                            } else {
                                d > BASE_AREA * BASE_AREA && d <= EXPANSION_AREA * EXPANSION_AREA
                            }
                    };
                    let want = (s.0 + off.0, s.1 + off.1);
                    (-reach..=reach)
                        .flat_map(|dy| (-reach..=reach).map(move |dx| (want.0 + dx, want.1 + dy)))
                        .filter(|&p| area(p) && free(p, &out))
                        .filter_map(|p| {
                            let (d, _) = walk_to(&walks[k], n, p)?;
                            let miss = d.abs_diff(target);
                            (miss <= slack).then_some((miss, dist2(p, want), p))
                        })
                        .min()
                        .map(|(_, _, p)| p)
                }?;
                out.push(Object {
                    kind: piece.kind,
                    at: spot,
                    owner: piece.owned.then_some(k as u8),
                    label: piece.label.into(),
                    once: false,
                });
                spots.push(spot);
            }
            Some(spots)
        })?;
        for (k, spot) in spots.into_iter().enumerate() {
            last[k] = (spot, off);
        }
    }
    Some(out)
}

/// Whether every start's way to its nearest enemy has a counter-route:
/// with the way walled off (six tiles round it, but near either start),
/// the walk round is at most twice as long. The map checker
/// (`gc_mapcheck`'s `routes`) holds a Conquest map to 2.2 times; the
/// margin covers its picking another of two equal ways. Only the north
/// starts are walked (`walks`, from each of `north`): on mirrored land
/// the south's are their turned copies.
fn counter_routes(
    n: usize,
    cost: &[u8],
    starts: &[Pos],
    north: &[Pos],
    walks: &[Vec<u32>],
) -> bool {
    const BLOCK: i32 = 6;
    let tiles = n as i32;
    let ok = |q: Pos| {
        (0..tiles).contains(&q.0) && (0..tiles).contains(&q.1) && cost[tile_index(n, q)] != 0
    };
    let mut walled = cost.to_vec();
    north.iter().zip(walks).all(|(&s, dist)| {
        let Some((rush, foe, end)) = starts
            .iter()
            .filter(|&&f| f != s)
            .filter_map(|&f| walk_to(dist, n, f).map(|(d, q)| (d, f, q)))
            .min()
        else {
            return true;
        };
        // The way back from the foe: each step to a tile the walk reached
        // that much sooner.
        let mut way = vec![end];
        let mut at = end;
        while at != s {
            let d = dist[tile_index(n, at)];
            let back = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
                .filter(|&d| d != (0, 0))
                .map(|(dx, dy)| ((at.0 + dx, at.1 + dy), dx != 0 && dy != 0))
                .find(|&(q, diagonal)| {
                    let step =
                        if diagonal { 14 } else { 10 } * u32::from(cost[tile_index(n, at)]) / 10;
                    ok(q)
                        && (!diagonal || (ok((q.0, at.1)) && ok((at.0, q.1))))
                        && dist[tile_index(n, q)].checked_add(step) == Some(d)
                });
            let Some((q, _)) = back else { return false };
            way.push(q);
            at = q;
        }
        let mut shut = Vec::new();
        for &t in &way {
            if dist2(t, s) <= 16 * 16 || dist2(t, foe) <= 16 * 16 {
                continue;
            }
            for y in (t.1 - BLOCK).max(0)..=(t.1 + BLOCK).min(tiles - 1) {
                for x in (t.0 - BLOCK).max(0)..=(t.0 + BLOCK).min(tiles - 1) {
                    let i = tile_index(n, (x, y));
                    if walled[i] != 0 {
                        walled[i] = 0;
                        shut.push(i);
                    }
                }
            }
        }
        // (The walk round stops once the tiles by the foe are final.)
        let by_foe: Vec<usize> = (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (foe.0 + dx, foe.1 + dy)))
            .filter(|&q| ok(q))
            .map(|q| tile_index(n, q))
            .collect();
        let round = ways::walk(&walled, n, tile_index(n, s), &by_foe);
        for i in shut {
            walled[i] = cost[i];
        }
        walk_to(&round, n, foe).is_some_and(|(d, _)| d <= rush * 2)
    })
}

/// The nearest tile to `at` within `r` that passes `ok` (of equals, the
/// northmost, then westmost).
fn nearest(at: Pos, r: i32, ok: &dyn Fn(Pos) -> bool) -> Option<Pos> {
    let mut near: Vec<Pos> = (-r..=r)
        .flat_map(|dy| (-r..=r).map(move |dx| (at.0 + dx, at.1 + dy)))
        .filter(|&q| dist2(q, at) <= r * r)
        .collect();
    near.sort_by_key(|&q| (dist2(q, at), q.1, q.0));
    near.into_iter().find(|&q| ok(q))
}

/// The places a Field Battle is best fought over, in the north half (the
/// loader turns them for the south): the most commanding heights, two far
/// apart, and the ford with the most wadeable ground round it, all on
/// standable ground away from the starts. When `measured`, the south gets
/// its own: for each, the best of its kind near the turned spot (or
/// neither, if the south has none).
fn set_pieces(
    map: &crate::Map,
    ground: &[crate::Ground],
    standable: &[bool],
    starts: &[Pos],
    measured: bool,
) -> Vec<SetPiece> {
    let n = map.tiles as i32;
    let at = |p: Pos| (p.1 * n + p.0) as usize;
    let open = open_reach(n as usize);
    let clear_of_starts = |p: Pos| starts.iter().all(|&s| dist2(p, s) > open * open);
    let spots: Vec<Pos> = (16..n / 2 - 8)
        .step_by(4)
        .flat_map(|y| (16..n - 16).step_by(4).map(move |x| (x, y)))
        .filter(|&p| standable[at(p)] && clear_of_starts(p))
        .collect();
    let south: Vec<Pos> = if measured {
        (n / 2 + 8..n - 16)
            .step_by(4)
            .flat_map(|y| (16..n - 16).step_by(4).map(move |x| (x, y)))
            .filter(|&p| standable[at(p)] && clear_of_starts(p))
            .collect()
    } else {
        Vec::new()
    };
    let reach = (n / 10).max(12);
    // The south spots near the turned copy of `p`.
    let across = |p: Pos| {
        let copy = (n - 1 - p.0, n - 1 - p.1);
        south
            .iter()
            .copied()
            .filter(move |&q| dist2(q, copy) <= reach * reach)
    };
    // How far a spot stands over the ring of ground ten tiles out
    // (sixteen points round it), in simulation height units.
    let rise = |p: Pos| {
        let ring: Vec<i32> = (0..16u16)
            .map(|k| {
                let v = Angle(k * 4096).to_vec();
                let (dx, dy) = (v.x.raw() * 10, v.y.raw() * 10);
                let q = (p.0 + (dx + ONE / 2) / ONE, p.1 + (dy + ONE / 2) / ONE);
                i32::from(map.heights[at((q.0.clamp(0, n - 1), q.1.clamp(0, n - 1)))])
            })
            .collect();
        i32::from(map.heights[at(p)]) - ring.iter().sum::<i32>() / 16
    };
    let mut highs: Vec<(i32, Pos)> = spots.iter().map(|&p| (rise(p), p)).collect();
    highs.sort_by_key(|&(r, p)| (-r, p.1, p.0));
    let enough = gc_sim::terrain::HIGH_GROUND + gc_sim::terrain::HIGH_GROUND / 4;
    let mut out: Vec<SetPiece> = Vec::new();
    let mut north_highs = 0;
    for (r, p) in highs {
        if r < enough || north_highs == 2 {
            break;
        }
        if out.iter().all(|s| dist2(s.at, p) > (n / 5) * (n / 5)) {
            let label = if north_highs == 0 {
                "The heights"
            } else {
                "The far heights"
            };
            let other = measured.then(|| {
                across(p)
                    .map(|q| (rise(q), q))
                    .filter(|&(r, _)| r >= enough)
                    .max_by_key(|&(r, q)| (r, -q.1, -q.0))
            });
            if other == Some(None) {
                continue;
            }
            north_highs += 1;
            for q in std::iter::once(p).chain(other.flatten().map(|(_, q)| q)) {
                out.push(SetPiece {
                    kind: SetPieceKind::High,
                    at: q,
                    label: label.into(),
                });
            }
        }
    }
    let fords = |p: Pos| {
        (-4..=4)
            .flat_map(|dy| (-4..=4).map(move |dx| (p.0 + dx, p.1 + dy)))
            .filter(|&q| (0..n).contains(&q.0) && (0..n).contains(&q.1))
            .filter(|&q| ground[at(q)] == crate::Ground::Ford)
            .count()
    };
    let ford = spots
        .iter()
        .copied()
        .map(|p| (fords(p), p))
        .filter(|&(k, _)| k >= 16)
        .max_by_key(|&(k, p)| (k, -p.1, -p.0));
    if let Some((_, p)) = ford {
        let other = measured.then(|| {
            across(p)
                .map(|q| (fords(q), q))
                .filter(|&(k, _)| k >= 16)
                .max_by_key(|&(k, q)| (k, -q.1, -q.0))
        });
        if other != Some(None) {
            for q in std::iter::once(p).chain(other.flatten().map(|(_, q)| q)) {
                out.push(SetPiece {
                    kind: SetPieceKind::Ford,
                    at: q,
                    label: "The ford".into(),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habitats_round_trip_through_bytes() {
        for h in Habitat::ALL {
            assert_eq!(Habitat::from_u8(h as u8), h);
        }
        assert_eq!(Habitat::from_u8(200), Habitat::Grassland);
    }

    /// The south half is the north turned: heights, cover and land.
    #[test]
    fn grown_land_is_its_own_turned_copy() {
        let r = grow(
            256,
            Grown {
                seed: 5,
                wildness: Wildness::Wild,
                fairness: Fairness::Mirrored,
                heart: false,
            },
        );
        let n = 256;
        let last = n * n - 1;
        for i in 0..n * n {
            assert_eq!(r.height.v[i], r.height.v[last - i], "height {i}");
            assert_eq!(r.cover[i], r.cover[last - i], "cover {i}");
            assert_eq!(r.land[i], r.land[last - i], "land {i}");
            assert_eq!(r.habitat[i], r.habitat[last - i], "habitat {i}");
        }
    }
}

#[cfg(test)]
mod load_time {
    /// The loader's samples are the land's bilinear heights, as
    /// `Grid::sample` gives them.
    #[test]
    fn samples_are_the_grids_heights() {
        use crate::grown::{Grown, field::ONE, grow};
        use crate::wilds::Wildness;
        let r = grow(
            64,
            Grown {
                seed: 2,
                wildness: Wildness::Wild,
                fairness: crate::grown::Fairness::Measured,
                heart: false,
            },
        );
        let side = 64 * crate::SAMPLES_PER_TILE as usize + 1;
        let samples = r.sample_heights(side);
        let per = i64::from(crate::SAMPLES_PER_TILE);
        let one = i64::from(ONE);
        let at = |s: usize| s as i64 * one / per - one / 2;
        let units = i64::from(gc_sim::terrain::HEIGHT_UNITS);
        for sy in 0..side {
            for sx in 0..side {
                let h = i64::from(r.height.sample(at(sx), at(sy)));
                assert_eq!(samples[sy * side + sx], (h * units / one) as i32);
            }
        }
    }

    /// A map file may ask for an odd side: the land still fits it.
    #[test]
    fn an_odd_side_grows() {
        let def = crate::grown::describe(
            257,
            3,
            crate::wilds::Wildness::Mixed,
            crate::grown::Fairness::Measured,
        );
        let map = crate::Map::from_def(&def);
        assert_eq!(map.tiles, 257);
        assert_eq!(map.habitat.len(), 257 * 257);
        // Its rivers run, and the simulation gets the current with it.
        assert_eq!(map.flow.len(), 257 * 257);
        assert!(!gc_sim::Flow::layer(map.ground().flow, 257).is_empty());
    }

    /// The doc 27 target: a 768 map is made in under 3 seconds (release,
    /// one core). `cargo test -p gc_map --release --lib grown_load_time -- --ignored --nocapture`
    #[test]
    #[ignore = "a timing, run by hand in release"]
    fn grown_load_time() {
        for (wild, choice) in [
            ("Plains", crate::GROWN + 1),
            ("Mixed", crate::GROWN + 2),
            ("Wild", crate::GROWN + 3),
            ("Measured Plains", crate::MEASURED + 1),
            ("Measured Mixed", crate::MEASURED + 2),
            ("Measured Wild", crate::MEASURED + 3),
        ] {
            let t = std::time::Instant::now();
            let map = crate::FieldMap {
                choice,
                tiles: 768,
                seed: 9,
            }
            .make();
            let made = t.elapsed();
            let _nav = map.nav_costs();
            println!(
                "{wild} 768: made {made:?}, with the nav grid {:?}",
                t.elapsed()
            );
        }
    }
}

#[cfg(test)]
mod fairness_probe {
    use super::*;

    /// Measured land's starts walk to their nearest enemies within 10% of
    /// each other, for every way the lobby hands them out, on the finished
    /// land as the nav rules walk it.
    #[test]
    fn measured_starts_are_within_the_limit() {
        for tiles in [256u32, 320] {
            for w in Wildness::ALL {
                for seed in 1..=2 {
                    let what = Grown {
                        seed,
                        wildness: w,
                        fairness: Fairness::Measured,
                        heart: false,
                    };
                    let r = grow(tiles, what);
                    let spread = true_unfairness(&r, &standable(&r));
                    assert!(
                        spread <= MEASURED_LIMIT,
                        "{w:?} {tiles} seed {seed}: walks {spread} thousandths apart"
                    );
                }
            }
        }
    }

    /// How unequal Measured maps come out, and how long they take: the
    /// numbers in docs/41.
    #[test]
    #[ignore = "a probe: PROBE=N cargo test --release -p gc_map fairness_probe -- --ignored --nocapture"]
    fn probe() {
        let seeds: u64 = std::env::var("PROBE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4);
        for tiles in [320u32, 512, 768] {
            for w in Wildness::ALL {
                let mut v = Vec::new();
                for seed in 0..seeds {
                    let t = std::time::Instant::now();
                    let r = grow(
                        tiles,
                        Grown {
                            seed: 100 + seed,
                            wildness: w,
                            fairness: Fairness::Measured,
                            heart: false,
                        },
                    );
                    v.push((true_unfairness(&r, &standable(&r)), t.elapsed().as_millis()));
                }
                println!("{tiles} {w:?}: {v:?}");
            }
        }
    }
}
