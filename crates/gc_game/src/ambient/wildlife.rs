//! Wildlife, stage 4a and "more fauna" (docs/34-ambient-life.md): small
//! herds of deer graze the open land near the camera, mountain goats browse
//! the Highland's scree and rock, hares sit about the open grass, herons
//! wade the fen, wild boar root in the forest and bone jackals lope the
//! waste. They walk to new pasture now and then, and bolt from soldiers the
//! viewer sees, each in its own way (a heron takes wing and lands again
//! some way off). They are a picture: not in the simulation, not saved,
//! not sent, in nobody's way (soldiers walk through them), and drawn only
//! while the camera is near.
//!
//! A herd belongs to a 20 by 20 tile cell of the map (one herd of each kind
//! at most), where a hash of the cell says whether there is one and where
//! its home is, so coming back to a place finds the same herds in the same
//! spots. Only the cells near the camera hold herds; the rest are forgotten
//! and made again, at home, when the camera returns.

use std::collections::{HashMap, HashSet};

use gc_map::Map;
use gc_map::def::Land;
use gc_map::grown::Habitat;
use glam::Vec2;

/// A cell of the map, in tiles.
pub const CELL: i32 = 20;
/// How far from the camera's focus herds are kept: this at no distance, and
/// this much more per tile of camera distance, at most; cells farther than
/// this and `KEEP` more are forgotten.
const REACH_NEAR: f32 = 36.0;
const REACH_PER_ZOOM: f32 = 0.9;
const REACH_MOST: f32 = 75.0;
const KEEP: f32 = 12.0;
/// How the clips are played to match the speed: a trot is a clip a second,
/// a bound is faster.
pub const RUN_RATE: f32 = 1.5;
/// Herds are not made within this many tiles of a building.
pub const TAME: f32 = 14.0;
/// The ground must be this far above the water (a heron may stand in the
/// shallows instead: see `Traits::floor`).
const SHORE: f32 = 0.12;
/// A flying heron's height above the ground, in tiles, how quickly it
/// climbs or sinks, its speed in tiles a second, and how far from its
/// landing it starts to come down.
const CRUISE: f32 = 1.7;
const CLIMB: f32 = 1.3;
const FLY: f32 = 5.5;
const GLIDE: f32 = 9.0;
/// How far from the soldier a flying herd lands, and how far it looks.
const LAND_FAR: (f32, f32) = (25.0, 40.0);
const LAND_CLEAR: f32 = 20.0;
/// How far a step is looked at when judging the slope.
const STEP: f32 = 0.4;

/// The kinds of wild animal: each has its own land, herd, speed and fear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Deer,
    Goat,
    Hare,
    /// Herons of the fen: they wade, and fly off.
    Heron,
    /// Wild boar of the forest: a sounder with one big one.
    Boar,
    /// Bone jackals of the waste.
    Jackal,
}

/// How a kind lives. Speeds are in tiles a second, turns in radians a
/// second, times in seconds, distances in tiles.
pub struct Traits {
    /// The share of cells where the land suits that have a herd of this kind.
    chance: f32,
    /// Animals in a herd, the least and how many more at most.
    size: (u32, u32),
    walk: f32,
    run: f32,
    turn_walk: f32,
    turn_run: f32,
    /// A herd bolts from a soldier this close, runs until it is `safe` from
    /// every one, and no soldier near for `calm` seconds lets it settle.
    spook: f32,
    safe: f32,
    calm: f32,
    /// The longest a bolt lasts.
    bolt_most: f32,
    /// How far from home a herd roams, and from its own pasture one stands.
    roam: f32,
    scatter: f32,
    /// How long one grazes or stands before it does something else: the
    /// least, and how much more at most.
    graze: (f32, f32),
    stand: (f32, f32),
    /// How long before a herd moves to new pasture.
    move_every: (f32, f32),
    /// A step may climb this much over `STEP` tiles.
    steepest: f32,
    /// How much a bolting animal swerves from a straight run, in radians,
    /// on top of the random jitter (a hare jinks).
    zigzag: f32,
    /// Size, the least and how much more at most.
    scale: (f32, f32),
    /// The least height of ground, above the water, an animal will stand on:
    /// a little above it for those that keep dry, below it for a wader.
    floor: f32,
    /// A kind that takes wing when frightened (it runs a little first, then
    /// flies to land elsewhere) instead of bolting along the ground.
    flies: bool,
    /// How much bigger the first of a herd is (a sounder's sow).
    lead: f32,
}

impl Species {
    pub const ALL: [Species; 6] = [
        Species::Deer,
        Species::Goat,
        Species::Hare,
        Species::Heron,
        Species::Boar,
        Species::Jackal,
    ];

    /// The model's name in the creature pack.
    pub fn model(self) -> &'static str {
        match self {
            Species::Deer => "Deer",
            Species::Goat => "Goat",
            Species::Hare => "Hare",
            Species::Heron => "Heron",
            Species::Boar => "Boar",
            Species::Jackal => "Jackal",
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// Whether this kind lives on a grown map's habitat `h`: deer on open
    /// land and light woods, goats on the Highland's scree, rock, heather and
    /// tree line, hares on open grass and its edges, herons in the marsh's
    /// reeds, sedge, bog, carr and wet meadow, boar in old woods and fern
    /// hollows, jackals on the waste's flats, bone drifts and rib fields.
    fn lives_on(self, h: Habitat) -> bool {
        use Habitat::*;
        match self {
            Species::Deer => matches!(
                h,
                FlowerMeadow
                    | Grassland
                    | ScrubEdge
                    | OldGrowth
                    | BirchFringe
                    | FernHollow
                    | Glade
                    | HeatherMoor
                    | TreeLine
                    | WetMeadow
            ),
            Species::Goat => matches!(h, Scree | BareRock | HeatherMoor | TreeLine),
            Species::Hare => matches!(h, FlowerMeadow | Grassland | ScrubEdge | Glade | DyingGrass),
            Species::Heron => matches!(h, ReedBed | SedgeTussock | PeatBog | AlderCarr | WetMeadow),
            Species::Boar => matches!(h, OldGrowth | FernHollow | BirchFringe | Glade),
            Species::Jackal => matches!(h, CrackedFlat | BoneDrift | RibField | DyingGrass),
        }
    }

    /// Whether this kind lives on a painted map's land `l`.
    fn lives_in(self, l: Land) -> bool {
        match self {
            Species::Deer => !matches!(l, Land::Fen | Land::Waste),
            Species::Goat => l == Land::Highland,
            Species::Hare => matches!(l, Land::Meadow | Land::Valley),
            Species::Heron => l == Land::Fen,
            Species::Boar => l == Land::Forest,
            Species::Jackal => l == Land::Waste,
        }
    }

    pub fn traits(self) -> &'static Traits {
        const DEER: Traits = Traits {
            chance: 0.4,
            size: (2, 3),
            walk: 0.9,
            run: 5.5,
            turn_walk: 2.5,
            turn_run: 9.0,
            spook: 15.0,
            safe: 30.0,
            calm: 5.0,
            bolt_most: 9.0,
            roam: 9.0,
            scatter: 3.0,
            graze: (5.0, 11.0),
            stand: (1.5, 4.0),
            move_every: (25.0, 45.0),
            steepest: 0.3,
            zigzag: 0.0,
            scale: (0.9, 0.2),
            floor: SHORE,
            flies: false,
            lead: 1.0,
        };
        // Goats: sure-footed on steep rock, steady rather than quick, and
        // not easily frightened off their ledges.
        const GOAT: Traits = Traits {
            chance: 0.55,
            size: (2, 3),
            walk: 0.7,
            run: 4.2,
            turn_walk: 2.5,
            turn_run: 7.0,
            spook: 11.0,
            safe: 22.0,
            calm: 6.0,
            bolt_most: 7.0,
            roam: 8.0,
            scatter: 2.5,
            graze: (5.0, 12.0),
            stand: (2.0, 5.0),
            move_every: (30.0, 55.0),
            steepest: 0.55,
            zigzag: 0.0,
            scale: (0.9, 0.2),
            floor: SHORE,
            flies: false,
            lead: 1.0,
        };
        // Hares: singly or in pairs, tiny, sitting tight until a soldier is
        // close, then off in a fast zig-zag, and back to feeding soon.
        const HARE: Traits = Traits {
            chance: 0.75,
            size: (1, 1),
            walk: 0.8,
            run: 8.0,
            turn_walk: 3.5,
            turn_run: 12.0,
            spook: 8.0,
            safe: 18.0,
            calm: 3.0,
            bolt_most: 5.0,
            roam: 7.0,
            scatter: 4.0,
            graze: (3.0, 8.0),
            stand: (2.0, 6.0),
            move_every: (15.0, 30.0),
            steepest: 0.3,
            zigzag: 0.9,
            scale: (0.9, 0.25),
            floor: SHORE,
            flies: false,
            lead: 1.0,
        };
        // Herons: alone or in threes, stalking the shallows on slow steps
        // and standing for a long time, shy; frightened, they run a few
        // steps and take wing (`run` is the run-up's speed), and land again
        // some way off.
        const HERON: Traits = Traits {
            chance: 0.55,
            size: (1, 2),
            walk: 0.45,
            run: 3.0,
            turn_walk: 2.0,
            turn_run: 6.0,
            spook: 17.0,
            safe: 30.0,
            calm: 4.0,
            bolt_most: 1.1,
            roam: 8.0,
            scatter: 3.5,
            graze: (4.0, 9.0),
            stand: (3.0, 8.0),
            move_every: (20.0, 40.0),
            steepest: 0.3,
            zigzag: 0.0,
            scale: (0.95, 0.15),
            floor: -0.4,
            flies: true,
            lead: 1.0,
        };
        // Boar: a sounder of three to six, tight together, rooting with
        // their heads down, less skittish than deer, and off in a bunch.
        const BOAR: Traits = Traits {
            chance: 0.5,
            size: (3, 3),
            walk: 0.95,
            run: 5.0,
            turn_walk: 2.5,
            turn_run: 8.0,
            spook: 9.0,
            safe: 20.0,
            calm: 4.0,
            bolt_most: 8.0,
            roam: 7.0,
            scatter: 2.0,
            graze: (6.0, 14.0),
            stand: (1.5, 4.0),
            move_every: (25.0, 45.0),
            steepest: 0.35,
            zigzag: 0.0,
            scale: (0.62, 0.2),
            floor: SHORE,
            flies: false,
            lead: 1.6,
        };
        // Bone jackals: a pack of two to four, loping about, sniffing and
        // sitting; wary of soldiers from further off than goats, but back
        // at it sooner.
        const JACKAL: Traits = Traits {
            chance: 0.5,
            size: (2, 2),
            walk: 1.1,
            run: 6.5,
            turn_walk: 3.0,
            turn_run: 10.0,
            spook: 20.0,
            safe: 34.0,
            calm: 2.5,
            bolt_most: 6.0,
            roam: 14.0,
            scatter: 3.0,
            graze: (3.0, 8.0),
            stand: (3.0, 9.0),
            move_every: (10.0, 22.0),
            steepest: 0.35,
            zigzag: 0.0,
            scale: (0.85, 0.2),
            floor: SHORE,
            flies: false,
            lead: 1.0,
        };
        match self {
            Species::Deer => &DEER,
            Species::Goat => &GOAT,
            Species::Hare => &HARE,
            Species::Heron => &HERON,
            Species::Boar => &BOAR,
            Species::Jackal => &JACKAL,
        }
    }
}

/// How far from the focus herds are made, in tiles, for a camera
/// `distance` away.
pub fn reach(distance: f32) -> f32 {
    (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST)
}

/// What an animal is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Doing {
    Graze,
    Stand,
    Walk,
    Bolt,
    /// In the air (a heron on the wing).
    Fly,
}

#[derive(Clone, Debug)]
struct Animal {
    pos: Vec2,
    facing: Vec2,
    doing: Doing,
    /// Seconds left in this doing (a walk ends on reaching `goal`, or when
    /// this runs out, so an animal that cannot get there gives up).
    left: f32,
    goal: Vec2,
    /// A walk just ordered, whose goal is not chosen yet.
    fresh: bool,
    /// Offsets the clip so a herd does not move as one, and the animal's size.
    phase: f32,
    scale: f32,
    /// Height above the ground, in tiles (a heron in flight).
    lift: f32,
}

#[derive(Clone, Debug)]
struct Herd {
    kind: Species,
    home: Vec2,
    pasture: Vec2,
    animals: Vec<Animal>,
    rng: u32,
    /// Seconds until the herd moves to new pasture, and until a bolting
    /// herd stops (zero when it is not bolting).
    move_in: f32,
    bolt: f32,
    /// Seconds with no soldier near, while the herd is wary after a bolt.
    wary: f32,
    /// A flying kind that has been frightened: where it will land (the
    /// herd runs up, then is in the air, until it does), seconds of run-up
    /// left, and the direction it runs.
    landing: Option<Vec2>,
    runup: f32,
    flee: Vec2,
}

/// An animal as it is drawn this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub kind: Species,
    pub pos: Vec2,
    pub facing: Vec2,
    pub doing: Doing,
    pub phase: f32,
    pub scale: f32,
    /// Height above the ground, in tiles.
    pub lift: f32,
}

#[derive(Default)]
pub struct Wildlife {
    herds: HashMap<((i32, i32), Species), Herd>,
    /// Cells looked at that hold no herd of a kind (the land is wrong or a
    /// spot was not found), so they are not looked at again every frame.
    empty: HashSet<((i32, i32), Species)>,
    seen: Vec<Seen>,
}

/// A number from 0 to 1 that stays the same for a cell and a `salt`.
pub(super) fn hash(cell: (i32, i32), salt: u32) -> f32 {
    let mut x = (cell.0 as u32)
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add((cell.1 as u32).wrapping_mul(0x85eb_ca6b))
        .wrapping_add(salt.wrapping_mul(0xc2b2_ae35));
    x ^= x >> 15;
    x = x.wrapping_mul(0x2c1b_3c6d);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297a_2d39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1 << 24) as f32
}

/// Whether a `kind` lives at `p`: by the habitat of a grown map, else by the
/// land of a painted one (a map with neither is open meadow).
pub(super) fn suits_at(kind: Species, map: &Map, p: Vec2) -> bool {
    let tiles = map.tiles as i32;
    let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
    if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
        return false;
    }
    let tile = y as usize * tiles as usize + x as usize;
    match map.habitat.get(tile) {
        Some(&b) => kind.lives_on(Habitat::from_u8(b)),
        None => kind.lives_in(map.land_at(tile).unwrap_or_default()),
    }
}

/// Whether an animal may stand at `p`: on the map, on dry ground.
pub(super) fn open(map: &Map, p: Vec2) -> bool {
    open_at(map, p, SHORE)
}

/// Whether an animal may stand at `p`: on the map, on ground at least
/// `floor` above the water (a wader's is below it).
fn open_at(map: &Map, p: Vec2, floor: f32) -> bool {
    let edge = 1.0;
    let side = map.tiles as f32 - edge;
    p.x > edge
        && p.y > edge
        && p.x < side
        && p.y < side
        && map.height_at(p.x, p.y) > map.water_at(p.x, p.y) + floor
}

/// Whether an animal may step from `a` to `b` (a little way): `b` open to
/// its kind and the step not steeper than it will take.
fn passable(map: &Map, a: Vec2, b: Vec2, tr: &Traits) -> bool {
    open_at(map, b, tr.floor)
        && (map.height_at(b.x, b.y) - map.height_at(a.x, a.y)).abs() <= tr.steepest
}

pub(super) fn turn_toward(from: Vec2, to: Vec2, most: f32) -> Vec2 {
    if to.length_squared() < 1e-6 {
        return from;
    }
    let to = to.normalize();
    let angle = from.angle_to(to);
    Vec2::from_angle(angle.clamp(-most, most))
        .rotate(from)
        .normalize_or(to)
}

impl Herd {
    fn unit(&mut self) -> f32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn between(&mut self, range: (f32, f32)) -> f32 {
        range.0 + self.unit() * (range.1 - range.0)
    }

    fn centre(&self) -> Vec2 {
        self.animals.iter().map(|d| d.pos).sum::<Vec2>() / self.animals.len().max(1) as f32
    }

    /// A random spot within `reach` of `at`.
    fn near(&mut self, at: Vec2, reach: f32) -> Vec2 {
        let a = self.unit() * std::f32::consts::TAU;
        at + Vec2::new(a.cos(), a.sin()) * self.unit().sqrt() * reach
    }
}

impl Wildlife {
    /// Forget every herd (a new match).
    pub fn clear(&mut self) {
        self.herds.clear();
        self.empty.clear();
        self.seen.clear();
    }

    /// Draw no animals (the game is behind a menu, or the view is not the
    /// field's), keeping the herds as they are.
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The animals to draw, as of the last `advance`.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Move the herds on by `dt` seconds of effects time, near `focus` with
    /// the camera `distance` away. `clear(p)` says an animal may stand at `p`
    /// to graze (no building or tree there), `tame(p)` that a building is
    /// near, and `threats` are the soldiers the viewer sees.
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        map: &Map,
        clear: impl Fn(Vec2) -> bool,
        tame: impl Fn(Vec2) -> bool,
        threats: &[Vec2],
    ) {
        let dt = dt.min(0.25);
        let reach = reach(distance);
        let keep = reach + KEEP;
        let cells = |r: f32| {
            let lo = ((focus - Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            let hi = ((focus + Vec2::splat(r)) / CELL as f32).floor().as_ivec2();
            (lo.x..=hi.x).flat_map(move |x| (lo.y..=hi.y).map(move |y| (x, y)))
        };
        let centre_of = |c: (i32, i32)| (Vec2::new(c.0 as f32, c.1 as f32) + 0.5) * CELL as f32;
        self.herds
            .retain(|&(c, _), _| centre_of(c).distance(focus) <= keep + CELL as f32);
        self.empty
            .retain(|&(c, _)| centre_of(c).distance(focus) <= keep + CELL as f32);
        for c in cells(reach) {
            // Made by the same measure `retain` keeps by, so a herd made
            // here is not dropped and made again in the next breath.
            if centre_of(c).distance(focus) > reach {
                continue;
            }
            for kind in Species::ALL {
                let key = (c, kind);
                if self.herds.contains_key(&key) || self.empty.contains(&key) {
                    continue;
                }
                match Self::found(c, kind, map, &clear, &tame) {
                    Some(h) => {
                        self.herds.insert(key, h);
                    }
                    None => {
                        self.empty.insert(key);
                    }
                }
            }
        }
        if dt > 0.0 {
            for h in self.herds.values_mut() {
                step(h, dt, map, &clear, &tame, threats);
            }
        }
        self.seen.clear();
        for h in self.herds.values() {
            for d in &h.animals {
                self.seen.push(Seen {
                    kind: h.kind,
                    pos: d.pos,
                    facing: d.facing,
                    doing: d.doing,
                    phase: d.phase,
                    scale: d.scale,
                    lift: d.lift,
                });
            }
        }
    }

    /// The herd of `kind` in cell `c`, if it has one. Each kind has its own
    /// hash salts (the deer's are the first set, 1 to 5; the others are 100
    /// on, clear of the doves' 21 to 25 on the same cells), so a place keeps
    /// its herds of every kind.
    fn found(
        c: (i32, i32),
        kind: Species,
        map: &Map,
        clear: &impl Fn(Vec2) -> bool,
        tame: &impl Fn(Vec2) -> bool,
    ) -> Option<Herd> {
        let tr = kind.traits();
        let salt = |n: u32| n + 100 * kind.index() as u32;
        if hash(c, salt(1)) >= tr.chance {
            return None;
        }
        let home = Vec2::new(
            (c.0 as f32 + 0.15 + 0.7 * hash(c, salt(2))) * CELL as f32,
            (c.1 as f32 + 0.15 + 0.7 * hash(c, salt(3))) * CELL as f32,
        );
        let fits = |p: Vec2| suits_at(kind, map, p) && open_at(map, p, tr.floor) && clear(p);
        if !fits(home) || tame(home) {
            return None;
        }
        let mut herd = Herd {
            kind,
            home,
            pasture: home,
            animals: Vec::new(),
            rng: (hash(c, salt(4)) * 4_294_967_000.0) as u32 | 1,
            move_in: 0.0,
            bolt: 0.0,
            wary: 0.0,
            landing: None,
            runup: 0.0,
            flee: Vec2::X,
        };
        herd.move_in = herd.between(tr.move_every);
        let n = tr.size.0 + (hash(c, salt(5)) * (tr.size.1 + 1) as f32) as u32;
        for k in 0..n {
            let pos = herd.near(home, tr.scatter);
            let pos = if fits(pos) { pos } else { home };
            let a = herd.unit() * std::f32::consts::TAU;
            let doing = if herd.unit() < 0.7 {
                Doing::Graze
            } else {
                Doing::Stand
            };
            let left = herd.between(tr.graze) * herd.unit();
            let (phase, scale) = (
                k as f32 * 0.37 + herd.unit(),
                (tr.scale.0 + tr.scale.1 * herd.unit()) * if k == 0 { tr.lead } else { 1.0 },
            );
            herd.animals.push(Animal {
                pos,
                facing: Vec2::from_angle(a),
                doing,
                left,
                goal: pos,
                fresh: false,
                phase,
                scale,
                lift: 0.0,
            });
        }
        Some(herd)
    }
}

/// Move one herd on by `dt`.
fn step(
    h: &mut Herd,
    dt: f32,
    map: &Map,
    clear: &impl Fn(Vec2) -> bool,
    tame: &impl Fn(Vec2) -> bool,
    threats: &[Vec2],
) {
    let tr = h.kind.traits();
    if h.landing.is_some() {
        fly_on(h, dt, map);
        return;
    }
    let centre = h.centre();
    let nearest = threats.iter().copied().min_by(|a, b| {
        a.distance_squared(centre)
            .total_cmp(&b.distance_squared(centre))
    });
    let danger = nearest.map(|t| (t, t.distance(centre)));
    match danger {
        // A wary herd, standing alert, is startled again only by a soldier
        // well inside the spooking distance, so a herd that has bolted
        // for as long as it will does not start again at once.
        Some((from, d))
            if h.bolt <= 0.0
                && d < if h.wary > 0.0 {
                    tr.spook * 0.5
                } else {
                    tr.spook
                } =>
        {
            if tr.flies {
                take_off(h, from, map, clear, tame);
                return;
            }
            h.bolt = tr.bolt_most;
            h.wary = 0.0;
            for d in &mut h.animals {
                d.doing = Doing::Bolt;
            }
        }
        _ => {}
    }
    if h.bolt > 0.0 {
        let from = danger.map_or(centre, |(t, _)| t);
        h.bolt -= dt;
        let safe = danger.is_none_or(|(_, d)| d > tr.safe);
        if safe || h.bolt <= 0.0 {
            h.bolt = 0.0;
            h.wary = tr.calm;
            for d in &mut h.animals {
                d.doing = Doing::Stand;
                d.left = tr.calm;
            }
        } else {
            for i in 0..h.animals.len() {
                let jitter = (h.unit() - 0.5) * 0.4;
                let (bolt, d) = (h.bolt, &mut h.animals[i]);
                // A hare jinks: each swerves to and fro out of step with the
                // others.
                let jink = tr.zigzag * (bolt * 4.0 + d.phase * 7.0).sin();
                let away = (d.pos - from).normalize_or(Vec2::X);
                let want = Vec2::from_angle(jitter + jink).rotate(away);
                run(d, want, tr, true, dt, map);
            }
            return;
        }
    }
    if h.wary > 0.0 {
        // Standing alert; a soldier coming back starts the bolt again, above,
        // and the herd stays wary while one is within `safe`.
        h.wary = if danger.is_some_and(|(_, d)| d < tr.safe) {
            tr.calm
        } else {
            h.wary - dt
        };
        if h.wary <= 0.0 {
            h.pasture = h.home;
            for d in &mut h.animals {
                d.doing = Doing::Walk;
                d.fresh = true;
            }
        } else {
            return;
        }
    }
    h.move_in -= dt;
    if h.move_in <= 0.0 {
        h.move_in = h.between(tr.move_every);
        for _ in 0..4 {
            let p = h.near(h.home, tr.roam);
            if open_at(map, p, tr.floor) && clear(p) {
                h.pasture = p;
                break;
            }
        }
        for d in &mut h.animals {
            d.doing = Doing::Walk;
            d.fresh = true;
        }
    }
    for i in 0..h.animals.len() {
        let pick = h.unit();
        let spot = h.near(h.pasture, tr.scatter);
        let rest = h.between(tr.graze);
        let stand = h.between(tr.stand);
        let d = &mut h.animals[i];
        match d.doing {
            Doing::Walk => {
                if d.fresh {
                    // Just told to walk: choose where in the pasture.
                    d.goal = if open_at(map, spot, tr.floor) && clear(spot) {
                        spot
                    } else {
                        h.pasture
                    };
                    d.left = d.goal.distance(d.pos) / tr.walk * 2.0 + 3.0;
                    d.fresh = false;
                }
                d.left -= dt;
                let to = d.goal - d.pos;
                if to.length() < 0.4 || d.left <= 0.0 {
                    d.doing = Doing::Graze;
                    d.left = rest;
                } else {
                    run(d, to, tr, false, dt, map);
                    d.doing = Doing::Walk;
                }
            }
            Doing::Graze | Doing::Stand | Doing::Bolt | Doing::Fly => {
                d.left -= dt;
                if d.left <= 0.0 {
                    if d.doing == Doing::Graze && pick < 0.35 {
                        d.doing = Doing::Stand;
                        d.left = stand;
                    } else if pick < 0.5 {
                        d.doing = Doing::Walk;
                        d.fresh = true;
                    } else {
                        d.doing = Doing::Graze;
                        d.left = rest;
                    }
                }
            }
        }
    }
}

/// Move `d` along `want` at its kind's run (`fast`) or walk, turning towards
/// it, round ground too steep for its kind or wet by turning aside. Keeps
/// `doing` as it is.
fn run(d: &mut Animal, want: Vec2, tr: &Traits, fast: bool, dt: f32, map: &Map) {
    let (speed, turn) = if fast {
        (tr.run, tr.turn_run)
    } else {
        (tr.walk, tr.turn_walk)
    };
    let want = want.normalize_or(d.facing);
    d.facing = turn_toward(d.facing, want, turn * dt);
    // Walk only once nearly facing the way: an animal turns on the spot first.
    if d.facing.dot(want) < 0.5 {
        return;
    }
    let reach = speed * dt;
    for angle in [0.0f32, 0.5, -0.5, 1.0, -1.0, 1.6, -1.6] {
        let dir = Vec2::from_angle(angle).rotate(d.facing);
        let to = d.pos + dir * reach.max(STEP * 0.5);
        if passable(map, d.pos, to, tr) {
            d.pos += dir * reach;
            return;
        }
    }
}

/// A frightened flock of a flying kind: pick where it lands, 25 to 40 tiles
/// off and away from the soldier `from` (on its own kind of ground, clear of
/// buildings, else back home), and start the run-up.
fn take_off(
    h: &mut Herd,
    from: Vec2,
    map: &Map,
    clear: &impl Fn(Vec2) -> bool,
    tame: &impl Fn(Vec2) -> bool,
) {
    let tr = h.kind.traits();
    let centre = h.centre();
    let away = (centre - from).normalize_or(Vec2::X);
    let (kind, floor) = (h.kind, tr.floor);
    let fits = |p: Vec2| suits_at(kind, map, p) && open_at(map, p, floor) && clear(p);
    let mut landing = h.home;
    for _ in 0..16 {
        let turn = (h.unit() - 0.5) * 2.4;
        let far = h.between(LAND_FAR);
        let p = centre + Vec2::from_angle(turn).rotate(away) * far;
        if fits(p) && !tame(p) && p.distance(from) >= LAND_CLEAR {
            landing = p;
            break;
        }
    }
    h.landing = Some(landing);
    h.flee = away;
    h.runup = h.between((0.7, 1.1));
    // The longest the flight may last, as a guard against circling.
    h.bolt = 30.0;
    for i in 0..h.animals.len() {
        let spot = h.near(landing, tr.scatter);
        let d = &mut h.animals[i];
        d.goal = if fits(spot) { spot } else { landing };
        d.doing = Doing::Bolt;
    }
}

/// A flying flock after `take_off`: it runs up along the ground, then rises
/// and flies low to its landing, coming down over the last few tiles, and
/// stands there as a new home.
fn fly_on(h: &mut Herd, dt: f32, map: &Map) {
    let tr = h.kind.traits();
    let Some(landing) = h.landing else { return };
    h.bolt -= dt;
    if h.runup > 0.0 {
        h.runup -= dt;
        for i in 0..h.animals.len() {
            let jitter = (h.unit() - 0.5) * 0.3;
            let want = Vec2::from_angle(jitter).rotate(h.flee);
            run(&mut h.animals[i], want, tr, true, dt, map);
        }
        if h.runup <= 0.0 {
            for d in &mut h.animals {
                d.doing = Doing::Fly;
            }
        }
        return;
    }
    for i in 0..h.animals.len() {
        let rest = h.between(tr.stand);
        let (timeout, d) = (h.bolt <= 0.0, &mut h.animals[i]);
        if d.doing != Doing::Fly {
            continue;
        }
        let to = d.goal - d.pos;
        let dist = to.length();
        if dist < 0.5 || timeout {
            d.pos = d.goal;
            d.lift = 0.0;
            d.doing = Doing::Stand;
            d.left = rest;
            continue;
        }
        d.facing = turn_toward(d.facing, to, tr.turn_run * dt);
        let ease = (dist / GLIDE).min(1.0);
        d.pos += d.facing * FLY * (0.5 + 0.5 * ease) * dt;
        let want = CRUISE * ease;
        d.lift += (want - d.lift).clamp(-CLIMB * dt, CLIMB * dt);
    }
    if h.animals.iter().all(|d| d.doing != Doing::Fly) {
        h.landing = None;
        h.bolt = 0.0;
        h.wary = 0.0;
        h.home = landing;
        h.pasture = landing;
        h.move_in = h.between(tr.move_every);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small grown map whose every tile is `h`.
    fn land(h: Habitat) -> Map {
        let mut map = gc_map::FieldMap {
            choice: gc_map::GROWN + 1,
            tiles: 128,
            seed: 5,
            ring: 0,
        }
        .make();
        map.habitat.fill(h as u8);
        map
    }

    fn run_for(w: &mut Wildlife, map: &Map, seconds: f32, focus: Vec2, threats: &[Vec2]) {
        for _ in 0..(seconds * 20.0) as usize {
            w.advance(0.05, focus, 30.0, map, |_| true, |_| false, threats);
        }
    }

    #[test]
    fn cells_have_herds_where_the_land_suits_and_the_same_each_time() {
        let map = land(Habitat::Grassland);
        let (mut a, mut b) = (Wildlife::default(), Wildlife::default());
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut a, &map, 0.1, focus, &[]);
        run_for(&mut b, &map, 0.1, focus, &[]);
        assert!(!a.seen().is_empty());
        assert_eq!(a.seen().len(), b.seen().len());
        let shingle = land(Habitat::Beach);
        let mut c = Wildlife::default();
        run_for(&mut c, &shingle, 0.5, focus, &[]);
        assert!(c.seen().is_empty());
    }

    #[test]
    fn a_painted_map_is_judged_by_its_land() {
        let focus = Vec2::new(64.0, 64.0);
        let mut map = land(Habitat::Grassland);
        map.habitat.clear();
        let mut meadow = Wildlife::default();
        run_for(&mut meadow, &map, 0.1, focus, &[]);
        assert!(!meadow.seen().is_empty());
        map.land = vec![Land::Waste as u8; (map.tiles * map.tiles) as usize];
        let mut waste = Wildlife::default();
        run_for(&mut waste, &map, 0.1, focus, &[]);
        assert!(!waste.seen().is_empty());
        assert!(waste.seen().iter().all(|s| s.kind == Species::Jackal));
    }

    #[test]
    fn deer_graze_and_wander_near_home_on_dry_ground() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 90.0, focus, &[]);
        assert!(!w.seen().is_empty());
        for d in w.seen() {
            assert!(map.height_at(d.pos.x, d.pos.y) > map.water_at(d.pos.x, d.pos.y));
        }
        let kinds: HashSet<_> = w.seen().iter().map(|s| format!("{:?}", s.doing)).collect();
        assert!(!kinds.contains("Bolt"));
        for h in w.herds.values() {
            for d in &h.animals {
                let tr = h.kind.traits();
                assert!(d.pos.distance(h.home) < tr.roam + tr.scatter + 6.0);
            }
        }
    }

    #[test]
    fn a_soldier_near_sends_the_herd_running_away_and_it_settles_after() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 1.0, focus, &[]);
        let key = *w.herds.keys().find(|k| k.1 == Species::Deer).unwrap();
        let before: Vec<Vec2> = w.herds[&key].animals.iter().map(|d| d.pos).collect();
        let mid = w.herds[&key].centre();
        let threat = mid + Vec2::new(6.0, 0.0);
        run_for(&mut w, &map, 2.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.animals.iter().all(|d| d.doing == Doing::Bolt));
        assert!(herd.centre().distance(threat) > mid.distance(threat) + 3.0);
        assert!(herd.animals.iter().zip(&before).all(|(d, b)| d.pos != *b));
        // No threat any more: they stop, stand alert and go back to grazing.
        run_for(&mut w, &map, 40.0, focus, &[]);
        let herd = &w.herds[&key];
        assert!(herd.animals.iter().all(|d| d.doing != Doing::Bolt));
    }

    #[test]
    fn herds_far_from_the_camera_are_forgotten() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(30.0, 30.0), &[]);
        let near = w.herds.len();
        assert!(near > 0);
        run_for(&mut w, &map, 0.1, Vec2::new(100.0, 100.0), &[]);
        assert!(
            w.herds
                .keys()
                .all(|&((x, y), _)| (x * CELL - 100).abs() < 120 && (y * CELL - 100).abs() < 120)
        );
        assert!(w.herds.keys().all(|&((x, y), _)| x > 0 || y > 0));
    }

    #[test]
    fn no_herd_stands_near_a_building() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        for _ in 0..4 {
            w.advance(
                0.05,
                focus,
                30.0,
                &map,
                |_| true,
                |p| p.distance(focus) < 30.0,
                &[],
            );
        }
        assert!(w.seen().iter().all(|d| d.pos.distance(focus) > 20.0));
    }

    #[test]
    fn a_deer_turns_before_it_walks_and_never_steps_into_water() {
        let map = land(Habitat::Grassland);
        let start = (20..110)
            .flat_map(|x| (20..110).map(move |y| Vec2::new(x as f32 + 0.5, y as f32 + 0.5)))
            .find(|&p| open(&map, p))
            .expect("some dry ground");
        let tr = Species::Deer.traits();
        let mut d = Animal {
            pos: start,
            facing: Vec2::X,
            doing: Doing::Walk,
            left: 5.0,
            goal: Vec2::ZERO,
            fresh: false,
            phase: 0.0,
            scale: 1.0,
            lift: 0.0,
        };
        run(&mut d, Vec2::NEG_X, tr, false, 0.05, &map);
        assert_eq!(d.pos, start);
        assert!(d.facing.x < 1.0);
        let mut moved = false;
        for _ in 0..400 {
            let want = Vec2::new(-1.0, 0.3);
            run(&mut d, want, tr, true, 0.05, &map);
            moved |= d.pos != start;
            assert!(open(&map, d.pos), "{:?}", d.pos);
        }
        assert!(moved);
    }

    #[test]
    fn a_zoomed_out_camera_keeps_its_herds_instead_of_making_them_anew() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        let frame =
            |w: &mut Wildlife| w.advance(0.05, focus, 110.0, &map, |_| true, |_| false, &[]);
        frame(&mut w);
        let keys: HashSet<_> = w.herds.keys().copied().collect();
        let first: Vec<Vec2> = w.seen().iter().map(|s| s.pos).collect();
        assert!(!first.is_empty());
        for _ in 0..600 {
            frame(&mut w);
            assert_eq!(w.herds.keys().copied().collect::<HashSet<_>>(), keys);
        }
        let later: Vec<Vec2> = w.seen().iter().map(|s| s.pos).collect();
        assert_eq!(first.len(), later.len());
        assert!(first.iter().zip(&later).any(|(a, b)| a != b));
    }

    #[test]
    fn a_wary_herd_is_not_startled_again_by_a_soldier_that_is_only_nearish() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
        let mut herd = w
            .herds
            .values()
            .find(|h| h.kind == Species::Deer)
            .unwrap()
            .clone();
        herd.bolt = 0.0;
        let tr = herd.kind.traits();
        herd.wary = tr.calm;
        let at = herd.centre() + Vec2::new(tr.spook * 0.7, 0.0);
        step(&mut herd, 0.05, &map, &|_| true, &|_| false, &[at]);
        assert!(herd.bolt <= 0.0);
        assert_eq!(herd.wary, tr.calm, "a soldier within `safe` keeps it wary");
        let at = herd.centre() + Vec2::new(tr.spook * 0.3, 0.0);
        step(&mut herd, 0.05, &map, &|_| true, &|_| false, &[at]);
        assert!(herd.bolt > 0.0);
    }

    /// The kinds of animal on show after `seconds` on a map of `h`.
    fn kinds_on(h: Habitat) -> HashSet<Species> {
        let map = land(h);
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
        w.seen().iter().map(|s| s.kind).collect()
    }

    #[test]
    fn goats_live_on_the_highland_and_hares_on_open_grass() {
        for h in [Habitat::Scree, Habitat::BareRock, Habitat::HeatherMoor] {
            assert!(kinds_on(h).contains(&Species::Goat), "{h:?}");
        }
        for h in [Habitat::Grassland, Habitat::FlowerMeadow] {
            let kinds = kinds_on(h);
            assert!(kinds.contains(&Species::Hare), "{h:?}");
            assert!(!kinds.contains(&Species::Goat), "{h:?}");
        }
        for h in [Habitat::Scree, Habitat::BareRock] {
            let kinds = kinds_on(h);
            assert!(!kinds.contains(&Species::Hare), "{h:?}");
            assert!(!kinds.contains(&Species::Deer), "{h:?}");
        }
        for h in [Habitat::PeatBog, Habitat::OldGrowth, Habitat::CrackedFlat] {
            assert!(!kinds_on(h).contains(&Species::Hare), "{h:?}");
            assert!(!kinds_on(h).contains(&Species::Goat), "{h:?}");
        }
    }

    #[test]
    fn a_painted_map_gives_goats_its_highland_and_hares_its_meadow() {
        let mut map = land(Habitat::Grassland);
        map.habitat.clear();
        let n = (map.tiles * map.tiles) as usize;
        for (land, goats, hares) in [
            (Land::Highland, true, false),
            (Land::Meadow, false, true),
            (Land::Forest, false, false),
            (Land::Fen, false, false),
        ] {
            map.land = vec![land as u8; n];
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            let kinds: HashSet<_> = w.seen().iter().map(|s| s.kind).collect();
            assert_eq!(kinds.contains(&Species::Goat), goats, "{land:?}");
            assert_eq!(kinds.contains(&Species::Hare), hares, "{land:?}");
        }
        // A map with neither habitat nor land is all meadow: no goats.
        map.land.clear();
        let mut w = Wildlife::default();
        run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
        let kinds: HashSet<_> = w.seen().iter().map(|s| s.kind).collect();
        assert!(!kinds.contains(&Species::Goat) && kinds.contains(&Species::Hare));
    }

    #[test]
    fn a_hare_sits_tight_where_a_deer_would_bolt_and_then_dashes() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 1.0, focus, &[]);
        let key = *w.herds.keys().find(|k| k.1 == Species::Hare).unwrap();
        let mid = w.herds[&key].centre();
        // Ten tiles off: inside a deer's fear, outside a hare's.
        run_for(&mut w, &map, 1.0, focus, &[mid + Vec2::new(10.0, 0.0)]);
        assert!(w.herds[&key].animals.iter().all(|d| d.doing != Doing::Bolt));
        // Five tiles off: it dashes, faster than a deer's walk, and swerves.
        let start: Vec<Vec2> = w.herds[&key].animals.iter().map(|d| d.pos).collect();
        let threat = w.herds[&key].centre() + Vec2::new(5.0, 0.0);
        run_for(&mut w, &map, 1.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.animals.iter().all(|d| d.doing == Doing::Bolt));
        assert!(
            herd.animals
                .iter()
                .zip(&start)
                .all(|(d, s)| d.pos.distance(*s) > 3.0)
        );
    }

    #[test]
    fn goats_take_steeper_steps_than_deer_and_hares_no_steeper() {
        let (deer, goat, hare) = (
            Species::Deer.traits(),
            Species::Goat.traits(),
            Species::Hare.traits(),
        );
        assert!(goat.steepest > deer.steepest);
        assert_eq!(hare.steepest, deer.steepest);
        // The map's own cliffs: a goat may step where a deer may not.
        let map = land(Habitat::Scree);
        let mut counts = (0, 0);
        for x in 4..124 {
            for y in 4..124 {
                let a = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let b = a + Vec2::new(STEP * 0.5, 0.0);
                counts.0 += usize::from(passable(&map, a, b, deer));
                counts.1 += usize::from(passable(&map, a, b, goat));
            }
        }
        assert!(counts.1 >= counts.0, "{counts:?}");
    }

    #[test]
    fn every_kind_has_a_model_name_and_a_sane_herd() {
        for k in Species::ALL {
            assert_eq!(Species::ALL[k.index()], k);
            assert!(!k.model().is_empty());
            let t = k.traits();
            assert!(t.run > t.walk && t.safe > t.spook && t.chance > 0.0 && t.chance <= 1.0);
            assert!(t.size.0 >= 1 && t.scale.0 > 0.5);
        }
    }

    /// The herd of `kind` nearest the map's middle, after a moment on `map`.
    fn herd_of(w: &mut Wildlife, map: &Map, kind: Species) -> ((i32, i32), Species) {
        run_for(w, map, 1.0, Vec2::new(64.0, 64.0), &[]);
        *w.herds
            .keys()
            .filter(|k| k.1 == kind)
            .min_by_key(|k| (k.0.0 - 3).abs() + (k.0.1 - 3).abs())
            .expect("a herd")
    }

    const MARSH: [Habitat; 5] = [
        Habitat::ReedBed,
        Habitat::SedgeTussock,
        Habitat::PeatBog,
        Habitat::AlderCarr,
        Habitat::WetMeadow,
    ];

    #[test]
    fn each_new_kind_lives_only_on_its_own_habitats_and_lands() {
        let wood = [
            Habitat::OldGrowth,
            Habitat::FernHollow,
            Habitat::BirchFringe,
            Habitat::Glade,
        ];
        let waste = [
            Habitat::CrackedFlat,
            Habitat::BoneDrift,
            Habitat::RibField,
            Habitat::DyingGrass,
        ];
        for h in Habitat::ALL {
            let kinds = kinds_on(h);
            assert_eq!(kinds.contains(&Species::Heron), MARSH.contains(&h), "{h:?}");
            assert_eq!(kinds.contains(&Species::Boar), wood.contains(&h), "{h:?}");
            assert_eq!(
                kinds.contains(&Species::Jackal),
                waste.contains(&h),
                "{h:?}"
            );
        }
        let mut map = land(Habitat::Grassland);
        map.habitat.clear();
        let n = (map.tiles * map.tiles) as usize;
        for l in [
            Land::Meadow,
            Land::Forest,
            Land::Fen,
            Land::Highland,
            Land::Waste,
            Land::Valley,
        ] {
            map.land = vec![l as u8; n];
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            let kinds: HashSet<_> = w.seen().iter().map(|s| s.kind).collect();
            assert_eq!(kinds.contains(&Species::Heron), l == Land::Fen, "{l:?}");
            assert_eq!(kinds.contains(&Species::Boar), l == Land::Forest, "{l:?}");
            assert_eq!(kinds.contains(&Species::Jackal), l == Land::Waste, "{l:?}");
        }
    }

    #[test]
    fn herd_sizes_and_the_sounders_big_one() {
        let (mut sizes, mut sounders) = (HashSet::new(), 0);
        for h in [Habitat::OldGrowth, Habitat::FernHollow, Habitat::Glade] {
            let map = land(h);
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            for herd in w.herds.values().filter(|h| h.kind == Species::Boar) {
                let n = herd.animals.len();
                assert!((3..=6).contains(&n), "{n}");
                sizes.insert(n);
                let (lead, rest) = (herd.animals[0].scale, &herd.animals[1..]);
                assert!(rest.iter().all(|a| a.scale < lead * 0.85), "{lead}");
                sounders += 1;
                // A tight group.
                let scatter = herd.kind.traits().scatter;
                assert!(
                    herd.animals
                        .iter()
                        .all(|a| a.pos.distance(herd.home) <= scatter)
                );
            }
        }
        assert!(sounders > 2 && sizes.len() > 1, "{sounders} {sizes:?}");
        for h in [Habitat::CrackedFlat, Habitat::BoneDrift, Habitat::RibField] {
            let map = land(h);
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            let packs: Vec<usize> = (w.herds.values())
                .filter(|h| h.kind == Species::Jackal)
                .map(|h| h.animals.len())
                .collect();
            assert!(!packs.is_empty());
            assert!(packs.iter().all(|n| (2..=4).contains(n)), "{packs:?}");
        }
        for h in MARSH {
            let map = land(h);
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            let flocks: Vec<usize> = (w.herds.values())
                .filter(|h| h.kind == Species::Heron)
                .map(|h| h.animals.len())
                .collect();
            assert!(!flocks.is_empty(), "{h:?}");
            assert!(flocks.iter().all(|n| (1..=3).contains(n)), "{flocks:?}");
        }
    }

    #[test]
    fn herons_may_wade_the_shallows_but_never_deep_water_and_the_rest_stay_dry() {
        let map = land(Habitat::PeatBog);
        let (heron, deer) = (Species::Heron.traits(), Species::Deer.traits());
        assert!(heron.floor < 0.0 && heron.floor > -0.6 && deer.floor > 0.0);
        let tiles = map.tiles as i32;
        let (mut shallow, mut deep) = (None, None);
        for x in 2..tiles - 2 {
            for y in 2..tiles - 2 {
                let p = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                let above = map.height_at(p.x, p.y) - map.water_at(p.x, p.y);
                if above > heron.floor && above <= deer.floor {
                    shallow = Some(p);
                } else if above <= heron.floor {
                    deep = Some(p);
                }
            }
        }
        // Where the shallows are a heron may stand and a deer may not; in
        // deep water neither may.
        let shallow = shallow.expect("this map has shallows");
        assert!(open_at(&map, shallow, heron.floor) && !open(&map, shallow));
        let deep = deep.expect("this map has deep water");
        assert!(!open_at(&map, deep, heron.floor) && !open(&map, deep));
        // Herons left to themselves for a long time never stand in deep water.
        let mut w = Wildlife::default();
        let mut waded = false;
        for _ in 0..(240.0 * 20.0) as usize {
            w.advance(
                0.05,
                Vec2::new(64.0, 64.0),
                30.0,
                &map,
                |_| true,
                |_| false,
                &[],
            );
            for s in w.seen() {
                let above = map.height_at(s.pos.x, s.pos.y) - map.water_at(s.pos.x, s.pos.y);
                if s.kind == Species::Heron {
                    assert!(above > heron.floor - 0.05, "{:?} {above}", s.pos);
                    waded |= above <= deer.floor;
                } else {
                    assert!(above > SHORE - 0.05, "{:?} {above}", s.pos);
                }
            }
        }
        assert!(waded, "the herons stayed on dry land");
        // The other new kinds keep to dry ground, too.
        for (h, kind) in [
            (Habitat::OldGrowth, Species::Boar),
            (Habitat::CrackedFlat, Species::Jackal),
        ] {
            let map = land(h);
            let mut w = Wildlife::default();
            run_for(&mut w, &map, 60.0, Vec2::new(64.0, 64.0), &[]);
            let mine: Vec<_> = w.seen().iter().filter(|s| s.kind == kind).collect();
            assert!(!mine.is_empty());
            for s in mine {
                assert!(open(&map, s.pos), "{kind:?} at {:?}", s.pos);
            }
        }
    }

    #[test]
    fn a_frightened_heron_runs_up_flies_low_and_lands_far_off() {
        let map = land(Habitat::PeatBog);
        let mut w = Wildlife::default();
        let key = herd_of(&mut w, &map, Species::Heron);
        let focus = Vec2::new(64.0, 64.0);
        let start = w.herds[&key].centre();
        // Twenty tiles off: outside its fear, it stays.
        run_for(&mut w, &map, 1.0, focus, &[start + Vec2::new(20.0, 0.0)]);
        assert!(w.herds[&key].animals.iter().all(|d| d.lift == 0.0));
        assert!(w.herds[&key].landing.is_none());
        let start = w.herds[&key].centre();
        let threat = start + Vec2::new(8.0, 0.0);
        // The run-up: on the ground, running, heading away.
        run_for(&mut w, &map, 0.3, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.landing.is_some());
        assert!(
            herd.animals
                .iter()
                .all(|d| d.doing == Doing::Bolt && d.lift == 0.0)
        );
        // Then in the air, low.
        let mut highest: f32 = 0.0;
        for _ in 0..60 {
            run_for(&mut w, &map, 0.1, focus, &[threat]);
            for d in &w.herds[&key].animals {
                highest = highest.max(d.lift);
                assert!(d.lift <= CRUISE + 0.01);
            }
        }
        assert!(highest > 1.0, "{highest}");
        // And down again somewhere else, on suitable ground.
        run_for(&mut w, &map, 20.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.landing.is_none());
        assert!(
            herd.animals
                .iter()
                .all(|d| d.lift == 0.0 && !matches!(d.doing, Doing::Fly | Doing::Bolt))
        );
        let far = herd.centre().distance(start);
        assert!(far > 20.0, "{far}");
        assert!(herd.centre().distance(threat) > LAND_CLEAR - 5.0);
        let tr = herd.kind.traits();
        for d in &herd.animals {
            assert!(suits_at(Species::Heron, &map, d.pos));
            assert!(open_at(&map, d.pos, tr.floor));
        }
        // It lives there now: standing and stalking as before, not flying.
        run_for(&mut w, &map, 30.0, focus, &[]);
        assert!(w.herds[&key].animals.iter().all(|d| d.doing != Doing::Fly));
    }

    #[test]
    fn boar_are_less_skittish_than_deer_and_bolt_in_a_bunch() {
        let (boar, deer) = (Species::Boar.traits(), Species::Deer.traits());
        assert!(boar.spook < deer.spook);
        let map = land(Habitat::OldGrowth);
        let mut w = Wildlife::default();
        let key = herd_of(&mut w, &map, Species::Boar);
        let focus = Vec2::new(64.0, 64.0);
        let mid = w.herds[&key].centre();
        // Twelve tiles off: inside a deer's fear, outside a boar's.
        run_for(&mut w, &map, 1.0, focus, &[mid + Vec2::new(12.0, 0.0)]);
        assert!(w.herds[&key].animals.iter().all(|d| d.doing != Doing::Bolt));
        let mid = w.herds[&key].centre();
        run_for(&mut w, &map, 1.5, focus, &[mid + Vec2::new(6.0, 0.0)]);
        let herd = &w.herds[&key];
        assert!(herd.animals.iter().all(|d| d.doing == Doing::Bolt));
        let spread = (herd.animals.iter())
            .map(|d| d.pos.distance(herd.centre()))
            .fold(0.0, f32::max);
        assert!(spread < 5.0, "{spread}");
    }

    #[test]
    fn jackals_keep_further_off_than_goats_and_come_back_sooner() {
        let (jackal, goat) = (Species::Jackal.traits(), Species::Goat.traits());
        assert!(jackal.spook > goat.spook && jackal.safe > goat.safe);
        assert!(jackal.calm < goat.calm && jackal.bolt_most <= goat.bolt_most);
        let map = land(Habitat::BoneDrift);
        let mut w = Wildlife::default();
        let key = herd_of(&mut w, &map, Species::Jackal);
        let focus = Vec2::new(64.0, 64.0);
        let mid = w.herds[&key].centre();
        // Fifteen tiles off: inside a jackal's fear, outside a goat's.
        run_for(&mut w, &map, 0.5, focus, &[mid + Vec2::new(15.0, 0.0)]);
        assert!(w.herds[&key].animals.iter().all(|d| d.doing == Doing::Bolt));
        run_for(&mut w, &map, 20.0, focus, &[]);
        assert!(w.herds[&key].animals.iter().all(|d| d.doing != Doing::Bolt));
    }

    #[test]
    fn the_new_kinds_make_the_same_herds_every_time() {
        for h in [Habitat::ReedBed, Habitat::FernHollow, Habitat::RibField] {
            let map = land(h);
            let (mut a, mut b) = (Wildlife::default(), Wildlife::default());
            run_for(&mut a, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            run_for(&mut b, &map, 0.1, Vec2::new(30.0, 70.0), &[]);
            run_for(&mut b, &map, 0.1, Vec2::new(64.0, 64.0), &[]);
            assert!(!a.seen().is_empty(), "{h:?}");
            // The camera went away and came back: every herd is where it was.
            for (k, ha) in &a.herds {
                let hb = &b.herds[k];
                assert_eq!(
                    (ha.home, ha.animals.len()),
                    (hb.home, hb.animals.len()),
                    "{k:?}"
                );
            }
        }
    }
}
