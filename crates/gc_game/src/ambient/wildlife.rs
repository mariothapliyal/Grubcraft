//! Wildlife, stage 4a and "more fauna" (docs/34-ambient-life.md): small
//! herds of deer graze the open land near the camera, mountain goats browse
//! the Highland's scree and rock, and hares sit about the open grass. They
//! walk to new pasture now and then, and bolt from soldiers the viewer sees,
//! each in its own way. They are a picture: not in the simulation, not saved,
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
/// The ground must be this far above the water.
const SHORE: f32 = 0.12;
/// How far a step is looked at when judging the slope.
const STEP: f32 = 0.4;

/// The kinds of wild animal: each has its own land, herd, speed and fear.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Species {
    Deer,
    Goat,
    Hare,
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
}

impl Species {
    pub const ALL: [Species; 3] = [Species::Deer, Species::Goat, Species::Hare];

    /// The model's name in the creature pack.
    pub fn model(self) -> &'static str {
        match self {
            Species::Deer => "Deer",
            Species::Goat => "Goat",
            Species::Hare => "Hare",
        }
    }

    pub fn index(self) -> usize {
        self as usize
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
        };
        match self {
            Species::Deer => &DEER,
            Species::Goat => &GOAT,
            Species::Hare => &HARE,
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
}

#[derive(Clone, Debug)]
struct Herd {
    kind: Species,
    home: Vec2,
    pasture: Vec2,
    deer: Vec<Animal>,
    rng: u32,
    /// Seconds until the herd moves to new pasture, and until a bolting
    /// herd stops (zero when it is not bolting).
    move_in: f32,
    bolt: f32,
    /// Seconds with no soldier near, while the herd is wary after a bolt.
    wary: f32,
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
}

#[derive(Default)]
pub struct Wildlife {
    herds: HashMap<((i32, i32), Species), Herd>,
    /// Cells looked at that hold no herd of a kind (the land is wrong or a spot was
    /// not found), so they are not looked at again every frame.
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

/// Whether deer live on this habitat: open land and light woods, not bog,
/// rock or the waste.
fn suits(h: Habitat) -> bool {
    use Habitat::*;
    matches!(
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
    )
}

/// Whether goats live on this habitat: the Highland's scree and rock, the
/// heather and the tree line where it climbs out of the woods.
fn goats_like(h: Habitat) -> bool {
    use Habitat::*;
    matches!(h, Scree | BareRock | HeatherMoor | TreeLine)
}

/// Whether hares live on this habitat: open grass and its edges.
fn hares_like(h: Habitat) -> bool {
    use Habitat::*;
    matches!(h, FlowerMeadow | Grassland | ScrubEdge | Glade | DyingGrass)
}

/// Whether a `kind` lives at `p`: by the habitat of a grown map, else by the
/// land of a painted one (a map with neither is open meadow).
pub(super) fn suits_for(kind: Species, map: &Map, p: Vec2) -> bool {
    if kind == Species::Deer {
        return suits_at(map, p);
    }
    let tiles = map.tiles as i32;
    let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
    if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
        return false;
    }
    let tile = y as usize * tiles as usize + x as usize;
    if let Some(&b) = map.habitat.get(tile) {
        let h = Habitat::from_u8(b);
        return match kind {
            Species::Goat => goats_like(h),
            _ => hares_like(h),
        };
    }
    map.land_at(tile).is_none_or(|l| match kind {
        Species::Goat => l == Land::Highland,
        _ => matches!(l, Land::Meadow | Land::Valley),
    })
}

/// Whether deer live at `p`: by the habitat of a grown map, else by the
/// land of a painted one (a map with neither is open meadow).
pub(super) fn suits_at(map: &Map, p: Vec2) -> bool {
    let tiles = map.tiles as i32;
    let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
    if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
        return false;
    }
    let tile = y as usize * tiles as usize + x as usize;
    if let Some(&b) = map.habitat.get(tile) {
        return suits(Habitat::from_u8(b));
    }
    map.land_at(tile)
        .is_none_or(|l| !matches!(l, Land::Fen | Land::Waste))
}

/// Whether an animal may stand at `p`: on the map, on dry ground.
pub(super) fn open(map: &Map, p: Vec2) -> bool {
    let edge = 1.0;
    let side = map.tiles as f32 - edge;
    p.x > edge
        && p.y > edge
        && p.x < side
        && p.y < side
        && map.height_at(p.x, p.y) > map.water_level() + SHORE
}

/// Whether an animal may step from `a` to `b` (a little way): both open and
/// not steeper than `steepest`.
fn passable(map: &Map, a: Vec2, b: Vec2, steepest: f32) -> bool {
    open(map, b) && (map.height_at(b.x, b.y) - map.height_at(a.x, a.y)).abs() <= steepest
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
        self.deer.iter().map(|d| d.pos).sum::<Vec2>() / self.deer.len().max(1) as f32
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
    /// the camera `distance` away. `clear(p)` says a deer may stand at `p`
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
                step(h, dt, map, &clear, threats);
            }
        }
        self.seen.clear();
        for h in self.herds.values() {
            for d in &h.deer {
                self.seen.push(Seen {
                    kind: h.kind,
                    pos: d.pos,
                    facing: d.facing,
                    doing: d.doing,
                    phase: d.phase,
                    scale: d.scale,
                });
            }
        }
    }

    /// The herd of `kind` in cell `c`, if it has one. Each kind has its own
    /// hash salts (the deer's are the first set), so a place keeps its herds
    /// of every kind.
    fn found(
        c: (i32, i32),
        kind: Species,
        map: &Map,
        clear: &impl Fn(Vec2) -> bool,
        tame: &impl Fn(Vec2) -> bool,
    ) -> Option<Herd> {
        let tr = kind.traits();
        let salt = |n: u32| n + 16 * kind.index() as u32;
        if hash(c, salt(1)) >= tr.chance {
            return None;
        }
        let home = Vec2::new(
            (c.0 as f32 + 0.15 + 0.7 * hash(c, salt(2))) * CELL as f32,
            (c.1 as f32 + 0.15 + 0.7 * hash(c, salt(3))) * CELL as f32,
        );
        let fits = |p: Vec2| suits_for(kind, map, p) && open(map, p) && clear(p);
        if !fits(home) || tame(home) {
            return None;
        }
        let mut herd = Herd {
            kind,
            home,
            pasture: home,
            deer: Vec::new(),
            rng: (hash(c, salt(4)) * 4_294_967_000.0) as u32 | 1,
            move_in: 0.0,
            bolt: 0.0,
            wary: 0.0,
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
                tr.scale.0 + tr.scale.1 * herd.unit(),
            );
            herd.deer.push(Animal {
                pos,
                facing: Vec2::from_angle(a),
                doing,
                left,
                goal: pos,
                fresh: false,
                phase,
                scale,
            });
        }
        Some(herd)
    }
}

/// Move one herd on by `dt`.
fn step(h: &mut Herd, dt: f32, map: &Map, clear: &impl Fn(Vec2) -> bool, threats: &[Vec2]) {
    let tr = h.kind.traits();
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
        Some((_, d))
            if h.bolt <= 0.0
                && d < if h.wary > 0.0 {
                    tr.spook * 0.5
                } else {
                    tr.spook
                } =>
        {
            h.bolt = tr.bolt_most;
            h.wary = 0.0;
            for d in &mut h.deer {
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
            for d in &mut h.deer {
                d.doing = Doing::Stand;
                d.left = tr.calm;
            }
        } else {
            for i in 0..h.deer.len() {
                let jitter = (h.unit() - 0.5) * 0.4;
                let (bolt, d) = (h.bolt, &mut h.deer[i]);
                // A hare jinks: each swerves to and fro out of step with the
                // others.
                let jink = tr.zigzag * (bolt * 4.0 + d.phase * 7.0).sin();
                let away = (d.pos - from).normalize_or(Vec2::X);
                run(
                    d,
                    Vec2::from_angle(jitter + jink).rotate(away),
                    tr,
                    tr.run,
                    tr.turn_run,
                    dt,
                    map,
                );
            }
            return;
        }
    }
    if h.wary > 0.0 {
        // Standing alert; a soldier coming back starts the bolt again, above,
        // and the herd stays wary while one is within `SAFE`.
        h.wary = if danger.is_some_and(|(_, d)| d < tr.safe) {
            tr.calm
        } else {
            h.wary - dt
        };
        if h.wary <= 0.0 {
            h.pasture = h.home;
            for d in &mut h.deer {
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
            if open(map, p) && clear(p) {
                h.pasture = p;
                break;
            }
        }
        for d in &mut h.deer {
            d.doing = Doing::Walk;
            d.fresh = true;
        }
    }
    for i in 0..h.deer.len() {
        let pick = h.unit();
        let spot = h.near(h.pasture, tr.scatter);
        let rest = h.between(tr.graze);
        let stand = h.between(tr.stand);
        let d = &mut h.deer[i];
        match d.doing {
            Doing::Walk => {
                if d.fresh {
                    // Just told to walk: choose where in the pasture.
                    d.goal = if open(map, spot) && clear(spot) {
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
                    run(d, to, tr, tr.walk, tr.turn_walk, dt, map);
                    d.doing = Doing::Walk;
                }
            }
            Doing::Graze | Doing::Stand | Doing::Bolt => {
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

/// Move `d` along `want` at `speed`, turning towards it, round ground too
/// steep for its kind or wet by turning aside. Keeps `doing` as `Bolt` when
/// it is one.
fn run(d: &mut Animal, want: Vec2, tr: &Traits, speed: f32, turn: f32, dt: f32, map: &Map) {
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
        if passable(map, d.pos, to, tr.steepest) {
            d.pos += dir * reach;
            return;
        }
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
        let bog = land(Habitat::PeatBog);
        let mut c = Wildlife::default();
        run_for(&mut c, &bog, 0.5, focus, &[]);
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
        assert!(waste.seen().is_empty());
    }

    #[test]
    fn deer_graze_and_wander_near_home_on_dry_ground() {
        let map = land(Habitat::Grassland);
        let mut w = Wildlife::default();
        let focus = Vec2::new(64.0, 64.0);
        run_for(&mut w, &map, 90.0, focus, &[]);
        assert!(!w.seen().is_empty());
        for d in w.seen() {
            assert!(map.height_at(d.pos.x, d.pos.y) > map.water_level());
        }
        let kinds: HashSet<_> = w.seen().iter().map(|s| format!("{:?}", s.doing)).collect();
        assert!(!kinds.contains("Bolt"));
        for h in w.herds.values() {
            for d in &h.deer {
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
        let before: Vec<Vec2> = w
            .herds
            .values()
            .next()
            .unwrap()
            .deer
            .iter()
            .map(|d| d.pos)
            .collect();
        let key = *w.herds.keys().find(|k| k.1 == Species::Deer).unwrap();
        let mid = w.herds[&key].centre();
        let threat = mid + Vec2::new(6.0, 0.0);
        run_for(&mut w, &map, 2.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.deer.iter().all(|d| d.doing == Doing::Bolt));
        assert!(herd.centre().distance(threat) > mid.distance(threat) + 3.0);
        assert!(herd.deer.iter().zip(&before).all(|(d, b)| d.pos != *b));
        // No threat any more: they stop, stand alert and go back to grazing.
        run_for(&mut w, &map, 40.0, focus, &[]);
        let herd = &w.herds[&key];
        assert!(herd.deer.iter().all(|d| d.doing != Doing::Bolt));
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
        };
        run(&mut d, Vec2::NEG_X, tr, tr.walk, tr.turn_walk, 0.05, &map);
        assert_eq!(d.pos, start);
        assert!(d.facing.x < 1.0);
        let mut moved = false;
        for _ in 0..400 {
            run(
                &mut d,
                Vec2::new(-1.0, 0.3),
                tr,
                tr.run,
                tr.turn_run,
                0.05,
                &map,
            );
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
        step(&mut herd, 0.05, &map, &|_| true, &[at]);
        assert!(herd.bolt <= 0.0);
        assert_eq!(herd.wary, tr.calm, "a soldier within `safe` keeps it wary");
        let at = herd.centre() + Vec2::new(tr.spook * 0.3, 0.0);
        step(&mut herd, 0.05, &map, &|_| true, &[at]);
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
        assert!(w.herds[&key].deer.iter().all(|d| d.doing != Doing::Bolt));
        // Five tiles off: it dashes, faster than a deer's walk, and swerves.
        let start: Vec<Vec2> = w.herds[&key].deer.iter().map(|d| d.pos).collect();
        let threat = w.herds[&key].centre() + Vec2::new(5.0, 0.0);
        run_for(&mut w, &map, 1.0, focus, &[threat]);
        let herd = &w.herds[&key];
        assert!(herd.deer.iter().all(|d| d.doing == Doing::Bolt));
        assert!(
            herd.deer
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
                counts.0 += usize::from(passable(&map, a, b, deer.steepest));
                counts.1 += usize::from(passable(&map, a, b, goat.steepest));
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
}
