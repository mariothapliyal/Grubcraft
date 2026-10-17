//! Campaign battle maps (random maps step 3, doc 19's C2): the battlefield
//! a campaign battle is fought on, written from the region it happens in.
//!
//! A region has a ground (from the area of the Titan's body it lies in),
//! the route the attacker came along, at most one feature, and maybe a
//! Fort. [`battle_map`] turns those and a seed into a map description
//! ([`MapDef`]) 64 to 96 tiles on a side, so the same region and seed
//! always give the same land. The route shapes the middle (a road, a
//! cliff wall with gaps, a river with fords, or a Brood burrow), the area
//! dresses the ground (crater lakes, ridges, cavern walls, bogs, rock,
//! forest, waste), and the feature stands on the defender's side: the
//! village on its slope, the lair, the Great Place on its hill.
//!
//! Unlike a Field Battle map it is not fair and not turned: the defender
//! holds the north half, with the feature and any Fort, and the attacker
//! deploys in the south, on the side it came from. The campaign turns the
//! view so south faces the route the attacker took.

use gc_fixed::Pcg32;

use crate::def::{
    CoverKind, CoverOp, Kind, MapDef, Mode, Noise, Object, Pos, SetPieceKind, Shape, SizeClass,
    Symmetry,
};
use crate::recipe::{DEEP, FORD, WATER, Writer};
use crate::stamp::dist2;

/// The areas of the Titan's body (doc 19); each has its own ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// The skull: highlands and crater lakes.
    Crown,
    /// Ridges, passes and terraced slopes.
    Ribs,
    /// Vast caverns open to the sky: cliff walls round a wide floor.
    Lungs,
    /// Bogs and slow rivers.
    Gut,
    /// The mountain chain: high, broken, rocky ground.
    Spine,
    /// Fur forests, deep and dark.
    Hands,
    /// Wastes and long roads.
    Tail,
    /// The centre of the Reach.
    Heart,
}

/// The route the attacker came along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// A road through the middle, kept clear of woods.
    Road,
    /// A cliff wall across the middle with one or two gaps.
    Pass,
    /// A river across the middle with one or two fords.
    Ford,
    /// A Brood burrow: the attacker comes up close, with no road.
    Burrow,
}

/// What stands in the region, on the defender's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feature {
    None,
    /// The defender's Seat (a town of theirs).
    Seat,
    /// A settlement: a village on a slope.
    Settlement,
    /// A company's camp.
    CompanyCamp,
    /// A Titan-spawn lair.
    Lair,
    /// A frontier site.
    Site,
    /// One of the Great Places round the Heart, on its hill.
    GreatPlace,
}

impl Area {
    pub const ALL: [Area; 8] = [
        Area::Crown,
        Area::Ribs,
        Area::Lungs,
        Area::Gut,
        Area::Spine,
        Area::Hands,
        Area::Tail,
        Area::Heart,
    ];
}

impl Route {
    pub const ALL: [Route; 4] = [Route::Road, Route::Pass, Route::Ford, Route::Burrow];
}

impl Feature {
    pub const ALL: [Feature; 7] = [
        Feature::None,
        Feature::Seat,
        Feature::Settlement,
        Feature::CompanyCamp,
        Feature::Lair,
        Feature::Site,
        Feature::GreatPlace,
    ];

    /// The object standing for it on the map, and its label.
    fn object(self) -> Option<(Kind, &'static str)> {
        match self {
            Feature::None => None,
            Feature::Seat => Some((Kind::Town, "The Seat")),
            Feature::Settlement => Some((Kind::Village, "The village")),
            Feature::CompanyCamp => Some((Kind::CreepCamp, "The company camp")),
            Feature::Lair => Some((Kind::CreepCamp, "The lair")),
            Feature::Site => Some((Kind::FrontierSlot, "The site")),
            Feature::GreatPlace => Some((Kind::Shrine, "The Great Place")),
        }
    }

    /// The name of the hill it stands on.
    fn hill(self) -> &'static str {
        match self {
            Feature::None => "The hill",
            Feature::Seat => "The Seat's hill",
            Feature::Settlement => "The village slope",
            Feature::CompanyCamp => "The camp hill",
            Feature::Lair => "The lair rocks",
            Feature::Site => "The site hill",
            Feature::GreatPlace => "The Great Place",
        }
    }
}

/// Where a campaign battle is fought.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    pub area: Area,
    pub route: Route,
    pub feature: Feature,
    /// Whether the region's owner has built a Fort there: the defender
    /// gets a palisade round its deployment, with a gate to the south.
    pub fort: bool,
}

impl Region {
    /// Every kind of region: each area, route and feature, with and
    /// without a Fort.
    pub fn all() -> impl Iterator<Item = Region> {
        Area::ALL.into_iter().flat_map(|area| {
            Route::ALL.into_iter().flat_map(move |route| {
                Feature::ALL.into_iter().flat_map(move |feature| {
                    [false, true].map(|fort| Region {
                        area,
                        route,
                        feature,
                        fort,
                    })
                })
            })
        })
    }
}

/// The player numbers of the two sides' starts.
pub const DEFENDER: u8 = 0;
pub const ATTACKER: u8 = 1;
/// The sizes a battle map comes in, tiles on a side.
pub const BATTLE_TILES: std::ops::RangeInclusive<i32> = 64..=96;

/// Ground level, as on the Field Battle maps; the highlands sit higher
/// and a bog barely above the water.
const BASE: i32 = 48;
const HIGHLAND_BASE: i32 = BASE + 16;
const BOG_BASE: i32 = WATER + 10;
/// A knoll's flat top and the slope round it, in tiles (see
/// [`Battle::knoll`]), and so how far it reaches from its centre.
const KNOLL_TOP: (i32, i32) = (2, 3);
const KNOLL_EDGE: (i32, i32) = (6, 8);
const KNOLL_REACH: i32 = KNOLL_TOP.1 + KNOLL_EDGE.1;
/// How far the route's wall or river (with its banks) reaches either side
/// of the middle line.
const ROUTE_BAND: i32 = 7;
/// The Fort's palisade, tiles from the defender's start on an 80-tile map.
pub const FORT_RADIUS: i32 = 13;

/// The battle map for a region and a seed.
pub fn battle_map(region: Region, seed: u64) -> MapDef {
    let mut rng = Pcg32::new(seed, 0x0062_6174);
    let base = match region.area {
        Area::Crown => HIGHLAND_BASE,
        Area::Gut => BOG_BASE,
        _ => BASE,
    };
    // A wall or river needs room either side; a Seat or a Fort, room
    // round the defender; a cavern, room inside its walls.
    let mut tiles = match region.route {
        Route::Road | Route::Burrow => 64,
        Route::Pass | Route::Ford => 80,
    } + 16 * (rng.below(2) as i32);
    if region.fort || region.feature == Feature::Seat || region.area == Area::Lungs {
        tiles = (tiles + 16).min(*BATTLE_TILES.end());
    }
    let mut b = Battle {
        w: Writer::new(&mut rng, tiles, base),
        n: tiles,
        defender: (tiles / 2, tiles / 4),
        // The Brood come up out of the ground close to the defender.
        attacker: if region.route == Route::Burrow {
            (tiles / 2, tiles * 5 / 8)
        } else {
            (tiles / 2, tiles * 3 / 4)
        },
        band: if matches!(region.route, Route::Pass | Route::Ford) {
            ROUTE_BAND
        } else {
            0
        },
        margin: 6,
        keep: Vec::new(),
        no_cover: Vec::new(),
        road: Vec::new(),
        objects: Vec::new(),
    };
    if region.area == Area::Lungs {
        // Clear of the cavern walls (see `Battle::dress`).
        b.margin = b.scaled(10) + 6;
    }
    let (lo, hi) = match region.area {
        Area::Crown | Area::Heart | Area::Hands => (16, 32),
        Area::Ribs => (20, 32),
        Area::Lungs | Area::Tail => (6, 14),
        // Small swells, so the bog's dry ground stays out of the water:
        // the noise reaches half its amplitude either way.
        Area::Gut => (6, 2 * (BOG_BASE - WATER) - 2),
        Area::Spine => (40, 56),
    };
    let noise = Noise {
        seed: b.w.rng.next_u32(),
        amplitude: b.w.range(lo, hi),
        scale: b.w.range(20, 36),
    };
    for (at, owner) in [(b.defender, DEFENDER), (b.attacker, ATTACKER)] {
        let r = b.scaled(8);
        b.w.stamp(Shape::Disc { at, r }, Mode::Set, base, b.scaled(6));
        let clear = b.scaled(10);
        b.keep.push((at, clear));
        b.no_cover.push((at, clear));
        b.put(Kind::Start, at, Some(owner), "");
    }
    if region.route == Route::Road {
        b.lay_road(region.fort);
    }
    // The route's wall or river, then the feature and the Fort, whose
    // stamps go over the river's levelled banks; they and the rest keep
    // clear of the route.
    let crossings = match region.route {
        Route::Pass => b.pass(),
        Route::Ford => b.river(),
        Route::Road | Route::Burrow => Vec::new(),
    };
    b.feature(region.feature, region.fort);
    if region.fort {
        b.fort();
    }
    if region.route == Route::Pass {
        b.gap_hill(crossings[0]);
    }
    // A knoll on the attacker's side too, so both sides have high ground.
    let far = b.mid() + 4..tiles;
    if let Some(at) = b.place(far, KNOLL_REACH) {
        b.knoll(at, "The far knoll");
    }
    b.dress(region.area);
    b.road();
    let Battle { w, objects, .. } = b;
    MapDef {
        name: format!(
            "{:?} {:?} {:?}{} {seed}",
            region.area,
            region.route,
            region.feature,
            if region.fort { " Fort" } else { "" }
        ),
        size: SizeClass::Field(tiles),
        symmetry: Symmetry::None,
        players: 2,
        base,
        water: WATER,
        noise: Some(noise),
        terrain: w.terrain,
        cover: w.cover,
        objects,
        set_pieces: w.set_pieces,
    }
}

/// A battle map being written.
struct Battle<'a> {
    w: Writer<'a>,
    n: i32,
    defender: Pos,
    attacker: Pos,
    /// How far either side of the middle line the route's wall or river
    /// reaches (0 when there is none).
    band: i32,
    /// How far in from the map's edge shaped ground and objects keep.
    margin: i32,
    /// Ground already shaped or kept open (starts, hills, the Fort, the
    /// road): the centre and how far it reaches.
    keep: Vec<(Pos, i32)>,
    /// What woods and grass keep off (the starts, the feature, the Fort's
    /// inside), and how far.
    no_cover: Vec<(Pos, i32)>,
    /// The road's turning points, if there is one.
    road: Vec<Pos>,
    objects: Vec<Object>,
}

impl Battle<'_> {
    /// `t` tiles on an 80-tile map, scaled to this one.
    fn scaled(&self, t: i32) -> i32 {
        t * self.n / 80
    }

    fn mid(&self) -> i32 {
        self.n / 2
    }

    fn put(&mut self, kind: Kind, at: Pos, owner: Option<u8>, label: &str) {
        self.objects.push(Object {
            kind,
            at,
            owner,
            label: label.into(),
            once: true,
        });
    }

    /// Whether ground shaped at `at` out to `r` tiles stays clear of the
    /// route, the objects and everything kept.
    fn clear(&self, at: Pos, r: i32) -> bool {
        let keep = self.margin..self.n - self.margin;
        let inside = keep.contains(&at.0) && keep.contains(&at.1);
        let route = self.band == 0 || (at.1 - self.mid()).abs() >= self.band + r;
        inside
            && route
            && self
                .objects
                .iter()
                .all(|o| dist2(o.at, at) >= (r + 2) * (r + 2))
            && self
                .keep
                .iter()
                .all(|&(p, kr)| dist2(p, at) >= (r + kr) * (r + kr))
    }

    /// A random spot clear by `r` (see [`Battle::clear`]), `None` if none
    /// turns up in a few dozen tries.
    fn spot(&mut self, r: i32) -> Option<Pos> {
        for _ in 0..32 {
            let at = (self.w.range(0, self.n - 1), self.w.range(0, self.n - 1));
            if self.clear(at, r) {
                return Some(at);
            }
        }
        None
    }

    /// A random spot among every one clear by `r` (on a two-tile grid)
    /// with `y` in the range, so a crowded map still finds one if there
    /// is one.
    fn place(&mut self, ys: std::ops::Range<i32>, r: i32) -> Option<Pos> {
        let n = self.n;
        let spots: Vec<Pos> = ys
            .step_by(2)
            .flat_map(|y| (0..n).step_by(2).map(move |x| (x, y)))
            .filter(|&at| self.clear(at, r))
            .collect();
        if spots.is_empty() {
            return None;
        }
        let k = self.w.range(0, spots.len() as i32 - 1);
        Some(spots[k as usize])
    }

    /// A small hill a line can hold, named as a set piece. Its slope
    /// reaches [`KNOLL_REACH`] tiles.
    fn knoll(&mut self, at: Pos, label: &str) -> i32 {
        let r = self.w.range(KNOLL_TOP.0, KNOLL_TOP.1);
        let height = self.w.base + self.w.range(28, 36);
        let edge = self.w.range(KNOLL_EDGE.0, KNOLL_EDGE.1);
        self.w
            .stamp(Shape::Disc { at, r }, Mode::Raise, height, edge);
        self.w.set_piece(SetPieceKind::High, at, label);
        self.keep.push((at, r + edge));
        r + edge
    }

    /// Where the ways through the middle line go: one or two, apart.
    fn crossings(&mut self) -> Vec<i32> {
        let n = self.n;
        let first = self.w.range(n / 3, n * 2 / 3);
        let mut xs = vec![first];
        if self.w.range(0, 1) == 1 {
            let side = if first < n / 2 { 1 } else { -1 };
            xs.push(first + side * self.w.range(n / 4, n / 3));
        }
        xs
    }

    /// A cliff wall along the middle line, broken by one or two gaps: where
    /// they are.
    fn pass(&mut self) -> Vec<i32> {
        let mid = self.mid();
        let gaps = self.crossings();
        let width = self.w.range(6, 8);
        let height = self.w.base + self.w.range(80, 112);
        let mut cuts: Vec<(i32, i32)> = gaps.iter().map(|&x| (x - width, x + width)).collect();
        cuts.sort_unstable();
        // The wall runs on past the east edge.
        cuts.push((self.n + 4, self.n + 4));
        let mut from = -4;
        for &(a, b) in &cuts {
            if a > from {
                self.w.stamp(
                    Shape::Line {
                        a: (from, mid),
                        b: (a, mid),
                        half_width: 0,
                    },
                    Mode::Raise,
                    height,
                    3,
                );
            }
            from = from.max(b);
        }
        gaps
    }

    /// A hill behind a gap at `x` on the defender's side, to hold it
    /// from, if the feature and the start leave room.
    fn gap_hill(&mut self, x: i32) {
        let at = (x, self.mid() - self.band - KNOLL_REACH - 1);
        if self.clear(at, KNOLL_REACH) {
            self.knoll(at, "The gap hill");
        }
    }

    /// A river along the middle line with one or two fords: where they are.
    fn river(&mut self) -> Vec<i32> {
        let mid = self.mid();
        let fords = self.crossings();
        let half_width = self.w.range(2, 3);
        let r = half_width + self.w.range(2, 3);
        // The ford's banks slope down over this many tiles, gently enough
        // to walk however high the ground stands.
        let slope = (self.w.base - FORD) / 8 + 2;
        // Level the banks first, so rolling ground can't leave a ford
        // between two cliffs.
        for &x in &fords {
            let at = (x, mid);
            self.w.stamp(
                Shape::Disc {
                    at,
                    r: r + slope + 3,
                },
                Mode::Set,
                self.w.base,
                6,
            );
        }
        let (a, b) = ((-4, mid), (self.n + 4, mid));
        self.w
            .stamp(Shape::Line { a, b, half_width }, Mode::Lower, DEEP, 3);
        for &x in &fords {
            self.w
                .stamp(Shape::Disc { at: (x, mid), r }, Mode::Set, FORD, slope);
        }
        self.w
            .set_piece(SetPieceKind::Ford, (fords[0], mid), "The ford");
        fords
    }

    /// The feature on its hill on the defender's side, clear of the
    /// defender's start, and its object on the slope facing the middle (a
    /// Great Place on the top). With a Fort the hill is the defender's
    /// own, inside the palisade, and the feature stands on its north slope.
    fn feature(&mut self, feature: Feature, fort: bool) {
        let north = self.n / 8..self.mid() - self.band - KNOLL_REACH;
        let (x, y) = if fort {
            self.defender
        } else if let Some(at) = self.place(north, KNOLL_REACH) {
            at
        } else {
            // Not seen on any map: off to the defender's east.
            (self.n * 3 / 4, self.defender.1)
        };
        let reach = self.knoll((x, y), feature.hill());
        if let Some((kind, label)) = feature.object() {
            let at = if fort {
                (x, y - reach / 2 - 1)
            } else if feature == Feature::GreatPlace {
                (x, y)
            } else {
                (x, y + reach / 2 + 1)
            };
            let owner = (feature == Feature::Seat).then_some(DEFENDER);
            self.put(kind, at, owner, label);
            self.no_cover.push((at, 2));
        }
    }

    /// A palisade round the defender's start, for now a steep earth bank
    /// (buildings come with M3), in eight straight runs with the one
    /// facing south left open as the gate.
    fn fort(&mut self) {
        // Corners at 22.5° + k × 45°, in thousandths.
        const CORNERS: [Pos; 8] = [
            (924, 383),
            (383, 924),
            (-383, 924),
            (-924, 383),
            (-924, -383),
            (-383, -924),
            (383, -924),
            (924, -383),
        ];
        let (cx, cy) = self.defender;
        let r = self.scaled(FORT_RADIUS);
        let at = |(x, y): Pos| (cx + x * r / 1000, cy + y * r / 1000);
        let height = self.w.base + 40;
        // The run from the second corner to the third faces south.
        for k in (0..8).filter(|&k| k != 1) {
            let (a, b) = (at(CORNERS[k]), at(CORNERS[(k + 1) % 8]));
            self.w.stamp(
                Shape::Line {
                    a,
                    b,
                    half_width: 1,
                },
                Mode::Raise,
                height,
                1,
            );
        }
        self.keep.push((self.defender, r + 3));
        self.no_cover.push((self.defender, r + 2));
    }

    /// The area's own ground and cover.
    fn dress(&mut self, area: Area) {
        let base = self.w.base;
        match area {
            Area::Crown => {
                // A crater lake.
                let r = self.w.range(4, 6);
                self.patch(r, 3, Mode::Lower, DEEP, 3);
            }
            Area::Ribs => {
                for k in 0..self.w.range(2, 3) {
                    self.ridge((k == 0).then_some("The ridge"));
                }
            }
            Area::Heart => {
                for k in 0..2 {
                    self.ridge((k == 0).then_some("The vein"));
                }
            }
            Area::Lungs => {
                // The cavern walls, east and west, their tops off the map,
                // and a pillar or two.
                let (hw, edge) = (self.scaled(4), self.scaled(6));
                for x in [-hw, self.n + hw] {
                    let (a, b) = ((x, -4), (x, self.n + 4));
                    self.w.stamp(
                        Shape::Line {
                            a,
                            b,
                            half_width: hw,
                        },
                        Mode::Raise,
                        base + 128,
                        edge,
                    );
                }
                for _ in 0..self.w.range(1, 2) {
                    self.rock(base + 128);
                }
            }
            Area::Gut => {
                for _ in 0..self.w.range(3, 5) {
                    let r = self.w.range(3, 5);
                    self.patch(r, 2, Mode::Set, FORD, 3);
                }
            }
            Area::Spine => {
                for _ in 0..self.w.range(2, 4) {
                    let height = base + self.w.range(64, 96);
                    self.rock(height);
                }
            }
            Area::Hands | Area::Tail => {}
        }
        let (woods, meadows) = match area {
            Area::Crown => (1, 2),
            Area::Ribs | Area::Heart => (2, 2),
            Area::Lungs | Area::Tail => (0, 2),
            Area::Gut => (1, 5),
            Area::Spine => (1, 1),
            Area::Hands => (self.w.range(8, 11), 1),
        };
        for _ in 0..woods {
            self.clump(CoverKind::Wood);
        }
        for _ in 0..meadows {
            self.clump(CoverKind::Grass);
        }
    }

    /// A short wall of rock running north to south, its top named.
    fn ridge(&mut self, label: Option<&str>) {
        let half = self.scaled(6);
        for _ in 0..32 {
            let Some(mid) = self.spot(4) else {
                return;
            };
            let (a, b) = ((mid.0, mid.1 - half), (mid.0, mid.1 + half));
            if [a, mid, b].iter().all(|&p| self.clear(p, 4)) {
                let height = self.w.base + self.w.range(40, 56);
                self.w.stamp(
                    Shape::Line {
                        a,
                        b,
                        half_width: 1,
                    },
                    Mode::Raise,
                    height,
                    4,
                );
                if let Some(label) = label {
                    self.w.set_piece(SetPieceKind::High, mid, label);
                }
                self.keep.extend([(a, 5), (mid, 5), (b, 5)]);
                return;
            }
        }
    }

    /// A steep rock no one climbs.
    fn rock(&mut self, height: i32) {
        let r = self.w.range(2, 3);
        self.patch(r, 3, Mode::Raise, height, 2);
    }

    /// A disc of ground `r` tiles across shaped at a spot clear by `pad`
    /// more, kept clear of what comes after.
    fn patch(&mut self, r: i32, pad: i32, mode: Mode, height: i32, edge: i32) {
        if let Some(at) = self.spot(r + pad) {
            self.w.stamp(Shape::Disc { at, r }, mode, height, edge);
            self.keep.push((at, r + pad));
        }
    }

    /// A clump of woods or grass, off the starts, the feature, the Fort
    /// and the route's wall or river.
    fn clump(&mut self, kind: CoverKind) {
        let r = self.w.range(self.scaled(4), self.scaled(7));
        for _ in 0..16 {
            let at = (self.w.range(4, self.n - 4), self.w.range(4, self.n - 4));
            let route = self.band == 0 || (at.1 - self.mid()).abs() >= self.band + r;
            let off = |&(p, d): &(Pos, i32)| dist2(p, at) >= (r + d) * (r + d);
            if route && self.no_cover.iter().all(off) {
                self.w.cover.push(CoverOp {
                    shape: Shape::Disc { at, r },
                    kind,
                });
                return;
            }
        }
    }

    /// Where the road runs, from the south edge through both starts to the
    /// north edge (or into a Fort by its gate), kept clear of what is
    /// shaped after.
    fn lay_road(&mut self, fort: bool) {
        let n = self.n;
        self.road = vec![
            (self.attacker.0 + self.w.range(-6, 6), n + 4),
            self.attacker,
            (n / 2 + self.w.range(-6, 6), self.mid()),
        ];
        let (x, y) = self.defender;
        if fort {
            // Straight in through the gate.
            self.road
                .extend([(x, y + FORT_RADIUS * n / 80 + 4), self.defender]);
        } else {
            self.road
                .extend([self.defender, (x + self.w.range(-6, 6), -4)]);
        }
        for k in 1..self.road.len() {
            let (a, b) = (self.road[k - 1], self.road[k]);
            let steps = (a.0 - b.0).abs().max((a.1 - b.1).abs()) / 4 + 1;
            for i in 0..=steps {
                let p = (a.0 + (b.0 - a.0) * i / steps, a.1 + (b.1 - a.1) * i / steps);
                self.keep.push((p, 3));
            }
        }
    }

    /// The road, kept clear of woods and grass.
    fn road(&mut self) {
        for pair in self.road.windows(2) {
            self.w.cover.push(CoverOp {
                shape: Shape::Line {
                    a: pair[0],
                    b: pair[1],
                    half_width: 1,
                },
                kind: CoverKind::Open,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same region and seed always give the same land, and the seed
    /// changes it.
    #[test]
    fn the_same_region_and_seed_give_the_same_land() {
        for region in Region::all().step_by(7) {
            for seed in 1..=3 {
                let first = battle_map(region, seed);
                let a = first.to_ron();
                assert_eq!(a, battle_map(region, seed).to_ron());
                let mut next = battle_map(region, seed + 1);
                next.name = first.name;
                assert_ne!(a, next.to_ron(), "{region:?} seed {seed}");
            }
        }
    }

    /// Every map is 64 to 96 tiles, has the two sides' starts, the
    /// feature's object and at least two set pieces.
    #[test]
    fn every_map_has_both_starts_and_its_feature() {
        for region in Region::all() {
            let def = battle_map(region, 5);
            let what = format!("{region:?}");
            let SizeClass::Field(tiles) = def.size else {
                panic!("{what}: {:?}", def.size);
            };
            assert!(BATTLE_TILES.contains(&tiles), "{what}: {tiles}");
            for side in [DEFENDER, ATTACKER] {
                let start = def
                    .objects
                    .iter()
                    .find(|o| o.kind == Kind::Start && o.owner == Some(side));
                assert!(start.is_some(), "{what}: no start for {side}");
            }
            let features = def.objects.len() - 2;
            assert_eq!(
                features,
                usize::from(region.feature != Feature::None),
                "{what}"
            );
            assert!(def.set_pieces.len() >= 2, "{what}");
        }
    }
}
