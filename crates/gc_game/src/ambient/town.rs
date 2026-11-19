//! Town life (docs/34-ambient-life.md "Town life as built", docs/26
//! Towns): the touches that make a base look worked in beyond its smoke and
//! windows. A forge throws sparks and flares at its door; once a player's
//! town is walled its gates and towers hang lanterns at dusk; an ox cart
//! stands parked by every store; and where a store stands well away from
//! its hall a cart goes between them. A picture only: not in the
//! simulation, not saved, not sent, in nobody's way (soldiers walk through
//! the carts as they do through every other ambient thing).

use std::collections::HashMap;

use gc_map::Map;
use gc_sim::structures::Role;
use glam::{Vec2, Vec3};

use crate::effects::Field;
use crate::siege::{Look, SMOKES_BELOW, Style};

use super::bases::{MOST_DT, RESONANCE, WINDOW, hash, hash_of, lamp_fade};
use super::caravans::{Piece, Route, Seen, Train, near_segment};
use super::hamlets::route_clear;
use super::villagers::Eyes;

/// What a building is for in the life of a town, as the game reads it from
/// its type (`buildings::Table::town_part`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Part {
    #[default]
    None,
    /// A forge or a workshop: throws sparks.
    Forge,
    /// A store or drop-off for what workers carry (a granary, an expansion
    /// hall): a cart stands beside it.
    Store,
    /// A gate or gatehouse.
    Gate,
    /// A tower.
    Tower,
}

/// The glow of a forge's fire.
const FORGE_GLOW: [u8; 3] = [255, 142, 62];
/// Sparks and flares are drawn only from a camera closer than this many
/// tiles of distance; farther they are specks.
const FORGE_REACH: f32 = 55.0;
/// Bursts a second at the door, by day and added at full dusk, and the
/// sparks in each.
const BURSTS_DAY: f32 = 0.3;
const BURSTS_DUSK: f32 = 0.9;
const BURST_SPARKS: f32 = 5.0;
/// The forge's own glow at the door at dusk, and its size in tiles.
const FORGE_LAMP: f32 = 0.6;
const LAMP_SIZE: f32 = 0.15;
/// How far outside its door the forge's mouth is, in tiles.
const MOUTH_OUT: f32 = 0.25;
/// How far outside its posts a lantern hangs, in tiles.
const HANG: f32 = 0.12;
const LANTERN_SIZE: f32 = 0.13;

/// How far from its middle a building's wall is along unit `dir`.
fn reach_along(half: Vec2, dir: Vec2) -> f32 {
    let (x, y) = (dir.x.abs(), dir.y.abs());
    match (x > 1e-4, y > 1e-4) {
        (true, true) => (half.x / x).min(half.y / y),
        (true, false) => half.x / x,
        (false, true) => half.y / y,
        _ => 0.0,
    }
}

/// Where a forge's fire shows: at its door, a little to one side where the
/// anvil stands, low on the wall.
fn mouth(look: &Look) -> (Vec3, Vec2) {
    let half = (look.max - look.min) / 2.0;
    let facing = look.facing.normalize_or(Vec2::Y);
    let side = facing.perp();
    let out = reach_along(half, facing) + MOUTH_OUT;
    let along = (hash(look, 40) - 0.5) * reach_along(half, side) * 0.8;
    let at = look.centre() + facing * out + side * along;
    (Vec3::new(at.x, look.base + look.top * 0.22, at.y), facing)
}

/// Make this frame's touches of town life for the building `look`, which
/// the viewer can see: forge sparks and glow, and, with `walled` (the
/// viewer's own town is walled: an enemy's ring is not for them to know),
/// lanterns on its gates and towers at dusk. `dt` is the effects time since
/// the last frame, `dusk` how dark the light is (0 by day to 1) and
/// `distance` how far the camera is.
pub fn emit(look: &Look, walled: bool, dt: f32, dusk: f32, distance: f32, fx: &mut Field) {
    if !look.finished || look.hp < SMOKES_BELOW {
        return;
    }
    let dt = dt.min(MOST_DT);
    match look.part {
        Part::Forge => forge(look, dt, dusk, distance, fx),
        Part::Gate | Part::Tower if walled => lanterns(look, dt, dusk, distance, fx),
        _ => {}
    }
}

/// A forge's sparks, and its glow growing with the dusk. The Choir's Forge
/// of Facets throws light motes of its own colour instead.
fn forge(look: &Look, dt: f32, dusk: f32, distance: f32, fx: &mut Field) {
    if distance > FORGE_REACH {
        return;
    }
    let (at, facing) = mouth(look);
    let rate = BURSTS_DAY + BURSTS_DUSK * dusk;
    let colour = if look.stuff.style == Style::Crystal {
        fx.drift(at, RESONANCE, rate * 2.0 * dt);
        RESONANCE
    } else {
        fx.forge(
            at,
            Vec3::new(facing.x, 0.9, facing.y),
            rate,
            BURST_SPARKS,
            dt,
        );
        FORGE_GLOW
    };
    let level = FORGE_LAMP * dusk * lamp_fade(distance);
    if level > 0.0 {
        fx.lamp(at, colour, LAMP_SIZE, level, dt);
    }
}

/// Lanterns on a gate or a tower at dusk: one on each jamb of a gate's
/// passage, one on a tower.
fn lanterns(look: &Look, dt: f32, dusk: f32, distance: f32, fx: &mut Field) {
    let level = dusk * lamp_fade(distance);
    if level <= 0.0 {
        return;
    }
    let (spots, n) = lantern_spots(look);
    for &at in &spots[..n] {
        fx.lamp(at, WINDOW, LANTERN_SIZE, level, dt);
    }
}

/// Where the lanterns of a gate or tower hang, and how many: for a gate,
/// either side of the passage, which runs across its shorter side, hung on
/// the posts at the ends of the longer; for a tower, one on a side of its
/// wall picked from its handle. Just outside the walls.
pub fn lantern_spots(look: &Look) -> ([Vec3; 2], usize) {
    let size = look.max - look.min;
    let centre = look.centre();
    let y = look.base + look.top * 0.7;
    match look.part {
        Part::Gate => {
            // The way along the wall; the passage is across it.
            let along = if size.x >= size.y { Vec2::X } else { Vec2::Y };
            let across = along.perp();
            let end = reach_along(size / 2.0, along) + HANG;
            // The face the lanterns look out of is the same for both.
            let face = if hash(look, 50) < 0.5 { 1.0 } else { -1.0 };
            let front = reach_along(size / 2.0, across) + HANG;
            let at = |s: f32| {
                let p = centre + along * (s * (end - 2.0 * HANG)) + across * (face * front);
                Vec3::new(p.x, y, p.y)
            };
            ([at(-1.0), at(1.0)], 2)
        }
        Part::Tower => {
            let a = (hash(look, 51) * std::f32::consts::TAU).sin_cos();
            let dir = Vec2::new(a.1, a.0);
            let out = reach_along(size / 2.0, dir) + HANG;
            let p = centre + dir * out;
            let at = Vec3::new(p.x, look.base + look.top * 0.8, p.y);
            ([at, at], 1)
        }
        _ => ([Vec3::ZERO; 2], 0),
    }
}

/// A building as the town's carts see it: where it stands and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    /// A number that stays with the building (see [`super::id_of`]).
    pub id: u64,
    pub owner: u8,
    pub role: Role,
    pub part: Part,
    /// Whether it is done (an unfinished building is only in the way).
    pub finished: bool,
    /// Whether the viewer sees its ground now (not only that they have seen
    /// it once): carts are laid out for, and shown at, what they see.
    pub visible: bool,
    pub min: Vec2,
    pub max: Vec2,
}

impl Site {
    fn centre(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    fn half(&self) -> Vec2 {
        (self.max - self.min) / 2.0
    }

    /// A building a cart stands beside or goes to: a store the viewer sees.
    fn is_store(&self) -> bool {
        self.finished && self.visible && self.part == Part::Store
    }

    /// Whether `p` is within `margin` tiles of its walls.
    fn near(&self, p: Vec2, margin: f32) -> bool {
        p.x > self.min.x - margin
            && p.x < self.max.x + margin
            && p.y > self.min.y - margin
            && p.y < self.max.y + margin
    }
}

/// How far from a store's wall a parked cart's middle is, in tiles.
const PARK_OUT: f32 = 1.3;
/// The cart's body from its middle, in tiles: the tail, the ox's nose, and
/// the half width (`oxcart.py`).
const CART_TAIL: f32 = 1.5;
const CART_NOSE: f32 = 2.9;
const CART_HALF: f32 = 0.85;
/// How far a cart keeps clear of any building, in tiles.
const CART_CLEAR: f32 = 0.3;
/// How far a parked cart keeps from the supply cart's way, and how far
/// past each of its stops that way is taken to reach (the ox's nose).
const KEEP_FROM_ROUTE: f32 = 2.2;
const ROUTE_ENDS: f32 = 3.0;
/// Places tried for a parked cart, round the store.
const PARK_TRIES: u32 = 8;
/// A parked cart is laid out for stores this close to the camera's focus,
/// in tiles.
pub const PARK_REACH: f32 = 55.0;

/// A cart standing still.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Parked {
    pub pos: Vec2,
    pub facing: Vec2,
}

/// The points of a cart at `pos` facing `facing`: its tail, middle and nose
/// down the line, and the corners at the tail and middle either side.
fn body(pos: Vec2, facing: Vec2) -> [Vec2; 7] {
    let side = facing.perp() * CART_HALF;
    let (tail, nose) = (pos - facing * CART_TAIL, pos + facing * CART_NOSE);
    [
        pos,
        tail,
        nose,
        pos + side,
        pos - side,
        tail + side,
        tail - side,
    ]
}

/// The strip a supply cart on `route` takes up: its line, reaching past
/// both stops, for a parked cart to keep off.
fn route_strip(route: &Route) -> (Vec2, Vec2) {
    let way = (route.to - route.from).normalize_or(Vec2::X);
    (route.from - way * ROUTE_ENDS, route.to + way * ROUTE_ENDS)
}

/// Whether a cart at `pos` facing `facing` stands on free ground clear of
/// every building of `sites` and of the supply cart's way `strip`.
fn fits(
    pos: Vec2,
    facing: Vec2,
    sites: &[Site],
    free: &impl Fn(Vec2) -> bool,
    strip: Option<(Vec2, Vec2)>,
) -> bool {
    body(pos, facing).iter().all(|&p| {
        free(p)
            && !sites.iter().any(|s| s.near(p, CART_CLEAR))
            && strip.is_none_or(|(a, b)| near_segment(p, a, b) >= KEEP_FROM_ROUTE)
    })
}

/// Where a cart stands beside the store `store`, hashed from its handle:
/// outside its footprint, along one of its sides with its ox's nose along
/// the wall, on `free` ground, clear of every other building of `sites`
/// and of the supply cart's way `strip`. `None` when nowhere round it fits.
pub fn parking(
    store: &Site,
    sites: &[Site],
    free: &impl Fn(Vec2) -> bool,
    strip: Option<(Vec2, Vec2)>,
) -> Option<Parked> {
    let (centre, half) = (store.centre(), store.half());
    let first = (hash_of(store.id, 60) * 4.0) as u32 % 4;
    for k in 0..PARK_TRIES {
        // The sides in turn from the one its handle picks, then again with
        // the other end of each side.
        let dir = [Vec2::Y, Vec2::X, -Vec2::Y, -Vec2::X][((first + k) % 4) as usize];
        let way = dir.perp();
        let slide = (hash_of(store.id, 61 + k) - 0.5) * reach_along(half, way) * 1.2;
        let turn = if hash_of(store.id, 70 + k) < 0.5 {
            1.0
        } else {
            -1.0
        };
        let facing = way * turn;
        let pos = centre + dir * (reach_along(half, dir) + PARK_OUT) + way * slide;
        if fits(pos, facing, sites, free, strip) {
            return Some(Parked { pos, facing });
        }
    }
    None
}

/// The stops of a supply cart's route between the hall `main` and the store
/// `store`: just outside each, on the line between their middles.
fn stops(main: &Site, store: &Site) -> (Vec2, Vec2) {
    let way = (store.centre() - main.centre()).normalize_or(Vec2::X);
    let out = |s: &Site, dir: Vec2| s.centre() + dir * (reach_along(s.half(), dir) + STOP_OUT);
    (out(main, way), out(store, -way))
}

/// How far a supply cart's stop is from the wall, in tiles: the cart's
/// middle, so its ox's nose stays outside.
const STOP_OUT: f32 = 3.2;
/// A cart's look-ahead for a building across its way reaches this far past
/// a stop, and is not heeded there: a building's own margin of ground
/// (`Scenery::occupied_at`) would else turn it back short of the stop.
const STOP_LOOK: f32 = 4.5;
/// Stores nearer than this to their hall, centre to centre, get no cart;
/// farther than `FAR_STORE` is too long a way for one.
pub const NEAR_STORE: f32 = 12.0;
const FAR_STORE: f32 = 60.0;
/// How far a route keeps clear of buildings, in tiles.
const ROUTE_CLEAR: f32 = 1.4;
/// Supply carts run for stores this close to the camera's focus, in tiles.
pub const RUN_REACH: f32 = 70.0;
/// Seconds between looks at whether a store's hall, route and parked cart
/// are as they were, and before a store with no route is tried again.
const RECHECK: f32 = 2.0;
const RETRY: f32 = 20.0;

/// The route for a supply cart between `main` and `store`, if they stand
/// at least [`NEAR_STORE`] and at most `FAR_STORE` apart and the straight
/// way between their stops is clear: dry, gently sloping, not woods
/// (`route_clear`), on `free` ground, and clear of every building of
/// `sites`.
pub(super) fn supply_route(
    map: &Map,
    main: &Site,
    store: &Site,
    sites: &[Site],
    free: &impl Fn(Vec2) -> bool,
) -> Option<Route> {
    let gap = main.centre().distance(store.centre());
    if !(NEAR_STORE..=FAR_STORE).contains(&gap) {
        return None;
    }
    let (from, to) = stops(main, store);
    // The stops must not have crossed over on a short way.
    if (to - from).dot(store.centre() - main.centre()) <= 0.0 {
        return None;
    }
    route_clear(map, from, to, |p| {
        free(p) && !sites.iter().any(|s| s.near(p, ROUTE_CLEAR))
    })
    .then_some(Route {
        from,
        to,
        cart_odds: 1.0,
    })
}

/// The nearest finished hall of `store`'s owner that the viewer sees.
fn hall_of<'a>(store: &Site, sites: &'a [Site]) -> Option<&'a Site> {
    sites
        .iter()
        .filter(|s| s.finished && s.visible && s.role == Role::Main && s.owner == store.owner)
        .min_by(|a, b| {
            let d = |s: &Site| s.centre().distance(store.centre());
            d(a).total_cmp(&d(b))
        })
}

/// What the town keeps for one store.
#[derive(Default)]
struct Store {
    /// The hall its cart goes to, the way, and the cart on it.
    hall: Option<u64>,
    route: Option<Route>,
    train: Option<Train>,
    /// The cart standing beside it.
    parked: Option<Parked>,
    /// Seconds until the hall, the route and the parked cart are looked at
    /// again, and until a store with no route is tried again.
    recheck: f32,
    retry: f32,
}

impl Store {
    /// Look again at the store's hall (its nearest may have gone, or a
    /// nearer one been finished), its route, and its parked cart (a building
    /// may have been raised where it stands, or on the way).
    fn look_again(
        &mut self,
        store: &Site,
        near: bool,
        sites: &[Site],
        map: &Map,
        free: &impl Fn(Vec2) -> bool,
    ) {
        self.recheck = RECHECK;
        self.retry -= RECHECK;
        let hall = hall_of(store, sites);
        if hall.map(|h| h.id) != self.hall {
            self.hall = hall.map(|h| h.id);
            self.route = None;
            self.train = None;
            self.retry = 0.0;
        }
        if let (None, Some(hall), true) = (&self.route, hall, self.retry <= 0.0) {
            self.route = supply_route(map, hall, store, sites, free);
            match &self.route {
                Some(route) => {
                    let seed = (hash_of(store.id, 90) * 65_535.0) as usize;
                    self.train = Some(Train::new(seed, route));
                }
                None => self.retry = RETRY,
            }
        }
        self.parked = if near {
            let strip = self.route.as_ref().map(route_strip);
            (self.parked)
                .filter(|p| fits(p.pos, p.facing, sites, free, strip))
                .or_else(|| parking(store, sites, free, strip))
        } else {
            None
        };
    }
}

/// The town's carts: those parked by stores, and those going between a
/// store and its hall.
#[derive(Default)]
pub struct Town {
    stores: HashMap<u64, Store>,
    /// The stores laid out this frame, to forget the rest.
    live: Vec<u64>,
    seen: Vec<Seen>,
    /// Room for the game's list of buildings, kept between frames.
    pub sites: Vec<Site>,
}

impl Town {
    /// Forget everything (a new match).
    pub fn clear(&mut self) {
        *self = Town::default();
    }

    /// Stop drawing (the menu is open, or nothing is being watched).
    pub fn hush(&mut self) {
        self.seen.clear();
    }

    /// The pieces to draw this frame, as caravans'.
    pub fn seen(&self) -> &[Seen] {
        &self.seen
    }

    /// Lay out and move the carts for the buildings `sites` (all the
    /// viewer is shown, near the camera) by `dt` seconds, and gather the ones
    /// within reach of the camera's `focus` for drawing. `free` says whether
    /// ground is dry, bare and open at a point; `occupied` whether a tile has
    /// a building or scenery on it (what turns a supply cart back).
    #[allow(clippy::too_many_arguments)]
    pub fn advance(
        &mut self,
        dt: f32,
        focus: Vec2,
        eyes: Eyes,
        map: &Map,
        sites: &[Site],
        free: impl Fn(Vec2) -> bool,
        occupied: impl Fn((i32, i32)) -> bool,
    ) {
        self.seen.clear();
        self.live.clear();
        for site in sites.iter().filter(|s| s.is_store()) {
            let far = site.centre().distance(focus);
            if far > RUN_REACH {
                continue;
            }
            self.live.push(site.id);
            let near = far <= PARK_REACH;
            let store = self.stores.entry(site.id).or_default();
            store.recheck -= dt;
            if store.recheck <= 0.0 {
                store.look_again(site, near, sites, map, &free);
            }
            if let Some(p) = store.parked.filter(|_| near) {
                self.seen.push(Seen {
                    piece: Piece::Cart,
                    pos: p.pos,
                    facing: p.facing,
                    moving: false,
                    rolling: false,
                    rate: 1.0,
                    phase: hash_of(site.id, 80) * 4.0,
                });
            }
            if let (Some(route), Some(train)) = (&store.route, &mut store.train) {
                // The look-ahead for a building across the way is not heeded
                // by the stops, where the buildings it would see are the
                // route's own two ends.
                let ends = [route.from, route.to];
                let across = |t: (i32, i32)| {
                    let c = Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
                    ends.iter().all(|e| c.distance(*e) > STOP_LOOK) && occupied(t)
                };
                train.step(dt.min(MOST_DT), route, eyes, &across);
                train.show(route, &mut self.seen);
            }
        }
        let live = &self.live;
        self.stores.retain(|id, _| live.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ambient::hamlets::tests::flat;
    use crate::siege::stuff;
    use gc_data::Faction;
    use gc_sim::structures::Armour;

    const QUIET: Eyes<'static> = Eyes {
        armies: &[],
        fights: &[],
    };

    /// A building's footprint and a tile's margin round it, as
    /// `Scenery::occupied_at` marks it.
    fn occupied_by(sites: &[Site]) -> impl Fn((i32, i32)) -> bool + '_ {
        move |t| {
            let c = Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
            sites.iter().any(|s| s.near(c, 1.0))
        }
    }

    fn open(_: Vec2) -> bool {
        true
    }

    fn look(faction: Faction, part: Part, min: Vec2, max: Vec2) -> Look {
        Look {
            handle: gc_sim::StructureHandle {
                index: 7,
                generation: 1,
            },
            min,
            max,
            base: 0.0,
            top: 3.0,
            model: None,
            lift: 0.0,
            facing: Vec2::Y,
            owner: 0,
            colour: [200, 0, 0],
            stuff: stuff(Some(faction), Armour::Structure),
            role: Role::Production,
            finished: true,
            hp: 1.0,
            part,
        }
    }

    fn forge_look() -> Look {
        look(
            Faction::EmberClans,
            Part::Forge,
            Vec2::new(10.0, 10.0),
            Vec2::new(13.0, 13.0),
        )
    }

    /// Particles made over `seconds` of frames.
    fn made(
        l: &Look,
        walled: bool,
        dusk: f32,
        distance: f32,
        seconds: f32,
    ) -> Vec<gc_render::Particle> {
        let mut fx = Field::new(None);
        let mut out = Vec::new();
        for _ in 0..(seconds * 30.0) as usize {
            emit(l, walled, 1.0 / 30.0, dusk, distance, &mut fx);
            out.extend(fx.frame(1.0 / 30.0, 1.0, Vec2::ZERO, 200.0, |_| 0.0));
        }
        out
    }

    #[test]
    fn a_finished_forge_throws_sparks_more_at_dusk() {
        let f = forge_look();
        let day = made(&f, false, 0.0, 20.0, 20.0).len();
        let dusk = made(&f, false, 1.0, 20.0, 20.0).len();
        assert!(day > 10, "{day}");
        assert!(dusk > day, "{dusk} {day}");
    }

    #[test]
    fn sparks_come_from_the_door_side_low_on_the_wall() {
        let f = forge_look();
        let (at, _) = mouth(&f);
        // Facing +y, the door is on the far (max y) wall, outside it.
        assert!(at.z > f.max.y && at.x > f.min.x && at.x < f.max.x, "{at:?}");
        assert!(at.y < f.top * 0.5);
    }

    #[test]
    fn the_forge_is_lit_at_dusk_and_dark_by_day() {
        let f = forge_look();
        let mut fx = Field::new(None);
        for _ in 0..30 {
            emit(&f, false, 1.0 / 30.0, 1.0, 20.0, &mut fx);
        }
        assert!(!fx.take_glows().is_empty());
        let mut fx = Field::new(None);
        for _ in 0..30 {
            emit(&f, false, 1.0 / 30.0, 0.0, 20.0, &mut fx);
        }
        // A flare of a burst may glow by day, but a steady lamp would be 30.
        assert!(fx.take_glows().len() < 15);
    }

    #[test]
    fn unfinished_hurt_and_far_forges_make_nothing() {
        let mut f = forge_look();
        f.finished = false;
        assert!(made(&f, false, 1.0, 20.0, 5.0).is_empty());
        let mut f = forge_look();
        f.hp = 0.3;
        assert!(made(&f, false, 1.0, 20.0, 5.0).is_empty());
        assert!(made(&forge_look(), false, 1.0, FORGE_REACH + 5.0, 5.0).is_empty());
        let mut none = forge_look();
        none.part = Part::None;
        assert!(made(&none, false, 1.0, 20.0, 5.0).is_empty());
    }

    #[test]
    fn the_choirs_forge_glints_in_its_own_light() {
        let mut f = forge_look();
        f.stuff = stuff(Some(Faction::Choir), Armour::Structure);
        assert!(made(&f, false, 1.0, 20.0, 10.0).len() > 5);
    }

    fn gate() -> Look {
        look(
            Faction::EmberClans,
            Part::Gate,
            Vec2::new(10.0, 10.0),
            Vec2::new(14.0, 11.0),
        )
    }

    fn tower() -> Look {
        look(
            Faction::EmberClans,
            Part::Tower,
            Vec2::new(10.0, 10.0),
            Vec2::new(12.0, 12.0),
        )
    }

    #[test]
    fn a_gate_has_a_lantern_each_side_of_its_passage_and_a_tower_one() {
        let g = gate();
        let (spots, n) = lantern_spots(&g);
        assert_eq!(n, 2);
        // The passage runs across the gate (along y here), so the two sit at
        // either end of its length, on one face and outside the walls.
        assert!(
            spots[0].x < g.min.x + 1.0 && spots[1].x > g.max.x - 1.0,
            "{spots:?}"
        );
        assert_eq!(spots[0].z, spots[1].z);
        assert!(spots.iter().all(|p| p.z < g.min.y || p.z > g.max.y));
        let t = tower();
        let (one, n) = lantern_spots(&t);
        assert_eq!(n, 1);
        let p = one[0];
        let inside = p.x > t.min.x && p.x < t.max.x && p.z > t.min.y && p.z < t.max.y;
        assert!(!inside, "{p:?}");
    }

    #[test]
    fn lanterns_only_in_a_walled_town_at_dusk_and_fade_with_distance() {
        for l in [gate(), tower()] {
            let glows = |walled, dusk, distance| {
                let mut fx = Field::new(None);
                for _ in 0..30 {
                    emit(&l, walled, 1.0 / 30.0, dusk, distance, &mut fx);
                }
                fx.take_glows().len()
            };
            assert!(glows(true, 1.0, 20.0) > 0);
            assert_eq!(glows(false, 1.0, 20.0), 0, "an open town");
            assert_eq!(glows(true, 0.0, 20.0), 0, "by day");
            assert_eq!(glows(true, 1.0, 200.0), 0, "far off");
            let strongest = |distance| {
                let mut fx = Field::new(None);
                emit(&l, true, 1.0 / 30.0, 1.0, distance, &mut fx);
                (fx.take_glows().iter()).fold(0.0f32, |m, g| m.max(g.strength))
            };
            assert!(strongest(20.0) > strongest(65.0));
            let mut unfinished = l;
            unfinished.finished = false;
            assert_eq!(made(&unfinished, true, 1.0, 20.0, 5.0).len(), 0);
        }
    }

    fn site(id: u64, owner: u8, role: Role, part: Part, min: (f32, f32), size: (f32, f32)) -> Site {
        Site {
            id,
            owner,
            role,
            part,
            finished: true,
            visible: true,
            min: Vec2::new(min.0, min.1),
            max: Vec2::new(min.0 + size.0, min.1 + size.1),
        }
    }

    fn store(id: u64, at: (f32, f32)) -> Site {
        site(id, 0, Role::Economy, Part::Store, at, (3.0, 3.0))
    }

    fn hall(at: (f32, f32)) -> Site {
        site(1, 0, Role::Main, Part::None, at, (5.0, 5.0))
    }

    #[test]
    fn a_parked_cart_stands_outside_the_store_and_is_the_same_each_time() {
        for id in 2..40 {
            let s = store(id, (50.0, 50.0));
            let p = parking(&s, &[s], &open, None).expect("room on every side");
            assert_eq!(parking(&s, &[s], &open, None), Some(p));
            for q in body(p.pos, p.facing) {
                assert!(!s.near(q, 0.0), "{id}: {q:?} is on the store");
            }
            // And close: its middle within a few tiles of the wall.
            assert!(s.centre().distance(p.pos) < 1.5 + PARK_OUT + 2.5);
        }
        // Different buildings get different places.
        let spots: Vec<_> = (2..12)
            .map(|id| {
                let s = store(id, (50.0, 50.0));
                parking(&s, &[s], &open, None).unwrap().pos
            })
            .collect();
        assert!(spots.iter().any(|&p| p != spots[0]));
    }

    #[test]
    fn a_parked_cart_keeps_off_other_buildings_and_out_of_water() {
        let s = store(2, (50.0, 50.0));
        let first = parking(&s, &[s], &open, None).unwrap();
        // A building where it would stand: it goes elsewhere.
        let block = site(
            3,
            0,
            Role::Housing,
            Part::None,
            (first.pos.x - 1.0, first.pos.y - 1.0),
            (2.0, 2.0),
        );
        let sites = [s, block];
        let second = parking(&s, &sites, &open, None).unwrap();
        assert_ne!(second, first);
        assert!(fits(second.pos, second.facing, &sites, &open, None));
        // Ringed by buildings on every side: none.
        let ring: Vec<Site> = std::iter::once(s)
            .chain((0..4).map(|i| {
                let (dx, dy) = [(0.0, -4.0), (0.0, 4.0), (-4.0, 0.0), (4.0, 0.0)][i];
                site(
                    10 + i as u64,
                    0,
                    Role::Housing,
                    Part::None,
                    (50.0 + dx - 1.0, 50.0 + dy - 1.0),
                    (5.0, 5.0),
                )
            }))
            .collect();
        assert_eq!(parking(&s, &ring, &open, None), None);
        // Water everywhere: none; water on one side only: the other.
        assert_eq!(parking(&s, &[s], &|_| false, None), None);
        let wet = |p: Vec2| p.y < 50.0;
        let dry = parking(&s, &[s], &wet, None).unwrap();
        assert!(body(dry.pos, dry.facing).iter().all(|&p| wet(p)));
    }

    #[test]
    fn a_parked_cart_keeps_off_the_supply_carts_way() {
        let (map, _) = flat();
        let main = hall((20.0, 48.0));
        for id in 2..40 {
            let far = store(id, (50.0, 50.0));
            let sites = [main, far];
            let route = supply_route(&map, &main, &far, &sites, &open).expect("clear");
            let strip = route_strip(&route);
            let p = parking(&far, &sites, &open, Some(strip)).expect("room on another side");
            for q in body(p.pos, p.facing) {
                assert!(near_segment(q, strip.0, strip.1) >= KEEP_FROM_ROUTE, "{id}");
            }
        }
        // Without the strip, some handle would park right on the way.
        let on_the_way = (2..40).any(|id| {
            let far = store(id, (50.0, 50.0));
            let route = supply_route(&map, &main, &far, &[main, far], &open).unwrap();
            let strip = route_strip(&route);
            let p = parking(&far, &[main, far], &open, None).unwrap();
            body(p.pos, p.facing)
                .iter()
                .any(|&q| near_segment(q, strip.0, strip.1) < KEEP_FROM_ROUTE)
        });
        assert!(on_the_way);
    }

    #[test]
    fn a_store_far_from_its_hall_with_a_clear_way_gets_a_route() {
        let (map, _) = flat();
        let main = hall((40.0, 40.0));
        let far = store(2, (70.0, 40.0));
        let route = supply_route(&map, &main, &far, &[main, far], &open).expect("clear");
        // The stops are outside both buildings, on the line between them.
        assert!(route.from.x > main.max.x + 3.0 && route.to.x < far.min.x - 3.0);
        assert!((route.from.y - 42.5).abs() < 0.5 && (route.to.y - 41.5).abs() < 1.6);
        // Nearer than twelve tiles: none.
        let near = store(3, (50.0, 40.0));
        assert_eq!(supply_route(&map, &main, &near, &[main, near], &open), None);
        // Too far: none.
        let beyond = store(4, (40.0 + FAR_STORE + 10.0, 40.0));
        assert_eq!(
            supply_route(&map, &main, &beyond, &[main, beyond], &open),
            None
        );
    }

    #[test]
    fn a_building_water_or_a_slope_across_the_way_means_no_route() {
        let (map, _) = flat();
        let main = hall((40.0, 40.0));
        let far = store(2, (70.0, 40.0));
        let across = site(3, 0, Role::Housing, Part::None, (55.0, 38.0), (3.0, 6.0));
        assert_eq!(
            supply_route(&map, &main, &far, &[main, far, across], &open),
            None
        );
        let creek = |p: Vec2| !(55.0..57.0).contains(&p.x);
        assert_eq!(supply_route(&map, &main, &far, &[main, far], &creek), None);
        // Off to one side it is clear again.
        let aside = site(3, 0, Role::Housing, Part::None, (55.0, 50.0), (3.0, 6.0));
        assert!(supply_route(&map, &main, &far, &[main, far, aside], &open).is_some());
        // Ground the map says is steep.
        let mut hilly = map.clone();
        for (i, h) in hilly.samples.iter_mut().enumerate() {
            *h = h.saturating_add(if i % 2 == 0 { 6000 } else { 0 });
        }
        assert_eq!(supply_route(&hilly, &main, &far, &[main, far], &open), None);
    }

    fn town_with_cart() -> (Town, Map, Vec<Site>) {
        let (map, _) = flat();
        let sites = vec![hall((40.0, 40.0)), store(2, (70.0, 40.0))];
        (Town::default(), map, sites)
    }

    fn step(t: &mut Town, map: &Map, sites: &[Site], seconds: f32, eyes: Eyes) {
        let occupied = occupied_by(sites);
        for _ in 0..(seconds * 10.0).round() as usize {
            t.advance(
                0.1,
                Vec2::new(55.0, 41.0),
                eyes,
                map,
                sites,
                open,
                &occupied,
            );
        }
    }

    fn carts(t: &Town) -> Vec<Seen> {
        (t.seen().iter().copied())
            .filter(|s| s.piece == Piece::Cart)
            .collect()
    }

    #[test]
    fn a_store_gets_a_parked_cart_and_a_far_one_a_cart_that_reaches_both_stops_and_unloads() {
        let (mut t, map, sites) = town_with_cart();
        step(&mut t, &map, &sites, 1.0, QUIET);
        // Parked and running: two carts, one standing and one on the road
        // or waiting at a stop.
        assert_eq!(carts(&t).len(), 2);
        assert!(carts(&t).iter().any(|c| !c.moving && !c.rolling));
        // With buildings that mark a margin of ground round them, as the
        // game's do, the runner still goes the whole way between the stops
        // and unloads there (a long wait, not the short pause for a
        // building across the way).
        let run = t.stores[&2].route.unwrap();
        let (mut far, mut near) = (0.0f32, f32::MAX);
        let (mut unloaded_far, mut unloaded_near) = (false, false);
        for _ in 0..(run.len() / super::super::caravans::CART_PACE * 4.0 + 60.0) as usize * 10 {
            step(&mut t, &map, &sites, 0.1, QUIET);
            let train = t.stores[&2].train.as_ref().unwrap();
            let (along, wait) = (train.along, train.unload);
            far = far.max(along);
            near = near.min(along);
            unloaded_far |= along > run.len() - 0.5 && wait > 7.0;
            unloaded_near |= along < 0.5 && wait > 7.0;
        }
        assert!(far > run.len() - 0.5 && near < 0.5, "{near} {far}");
        assert!(unloaded_far && unloaded_near);
    }

    #[test]
    fn a_fight_near_stops_the_supply_cart() {
        let (mut t, map, sites) = town_with_cart();
        step(&mut t, &map, &sites, 1.0, QUIET);
        // Put it on the road, not waiting at an end.
        let s = t.stores.get_mut(&2).unwrap();
        let train = s.train.as_mut().unwrap();
        (train.along, train.unload) = (s.route.unwrap().len() / 2.0, 0.0);
        let at = |t: &Town| {
            let s = &t.stores[&2];
            s.route.unwrap().at(s.train.as_ref().unwrap().along)
        };
        let fight = [at(&t) + Vec2::new(10.0, 0.0)];
        let eyes = Eyes {
            armies: &[],
            fights: &fight,
        };
        let along = |t: &Town| t.stores[&2].train.as_ref().unwrap().along;
        let before = along(&t);
        step(&mut t, &map, &sites, 10.0, eyes);
        assert_eq!(along(&t), before);
        // Quiet again: it goes on.
        step(&mut t, &map, &sites, 15.0, QUIET);
        assert_ne!(along(&t), before);
    }

    #[test]
    fn the_supply_cart_does_not_drive_through_the_parked_one() {
        let (map, _) = flat();
        for id in 2..30 {
            let sites = vec![hall((20.0, 48.0)), store(id, (50.0, 50.0))];
            let mut t = Town::default();
            step(&mut t, &map, &sites, 2.5, QUIET);
            let s = &t.stores[&id];
            let (route, parked) = (s.route.unwrap(), s.parked.expect("a place"));
            let strip = route_strip(&route);
            for q in body(parked.pos, parked.facing) {
                assert!(near_segment(q, strip.0, strip.1) >= KEEP_FROM_ROUTE);
            }
        }
    }

    #[test]
    fn carts_are_laid_only_near_the_camera_and_forgotten_with_their_store() {
        let (mut t, map, mut sites) = town_with_cart();
        t.advance(
            0.1,
            Vec2::new(500.0, 500.0),
            QUIET,
            &map,
            &sites,
            open,
            |_| false,
        );
        assert!(t.seen().is_empty() && t.stores.is_empty());
        step(&mut t, &map, &sites, 1.0, QUIET);
        assert!(!t.seen().is_empty());
        sites.pop();
        step(&mut t, &map, &sites, 0.2, QUIET);
        assert!(t.seen().is_empty() && t.stores.is_empty());
        // An unfinished store has none yet.
        let mut sites = town_with_cart().2;
        sites[1].finished = false;
        step(&mut t, &map, &sites, 1.0, QUIET);
        assert!(t.seen().is_empty());
    }

    #[test]
    fn a_store_the_viewer_does_not_see_now_gets_no_cart() {
        let (mut t, map, mut sites) = town_with_cart();
        sites[1].visible = false;
        step(&mut t, &map, &sites, 3.0, QUIET);
        assert!(t.seen().is_empty() && t.stores.is_empty());
        // A hall out of sight is no hall to go to: the parked cart only.
        let (mut t, map, mut sites) = town_with_cart();
        sites[0].visible = false;
        step(&mut t, &map, &sites, 3.0, QUIET);
        assert_eq!(carts(&t).len(), 1);
        assert!(t.stores[&2].route.is_none());
    }

    #[test]
    fn the_cart_follows_its_hall_gone_or_a_nearer_one_finished() {
        let (mut t, map, mut sites) = town_with_cart();
        step(&mut t, &map, &sites, 3.0, QUIET);
        assert_eq!(t.stores[&2].hall, Some(1));
        // A nearer hall: the route is made again, to it.
        let mut nearer = hall((50.0, 56.0));
        nearer.id = 5;
        sites.push(nearer);
        step(&mut t, &map, &sites, 3.0, QUIET);
        assert_eq!(t.stores[&2].hall, Some(5));
        let route = t.stores[&2].route.expect("a way to the nearer hall");
        assert!(route.from.y > 50.0, "{route:?}");
        // No hall: no cart on the road.
        sites.retain(|s| s.role != Role::Main);
        step(&mut t, &map, &sites, 3.0, QUIET);
        assert!(t.stores[&2].route.is_none() && t.stores[&2].train.is_none());
        assert_eq!(carts(&t).len(), 1);
    }

    #[test]
    fn a_store_near_its_hall_has_a_parked_cart_but_no_runner() {
        let (map, _) = flat();
        let sites = vec![hall((40.0, 40.0)), store(2, (50.0, 40.0))];
        let mut t = Town::default();
        step(&mut t, &map, &sites, 1.0, QUIET);
        assert_eq!(carts(&t).len(), 1);
        assert!(t.stores[&2].route.is_none());
    }

    #[test]
    fn a_parked_cart_moves_off_a_building_put_up_on_it() {
        let (map, _) = flat();
        let s = store(2, (50.0, 40.0));
        let mut sites = vec![hall((20.0, 40.0)), s];
        let mut t = Town::default();
        step(&mut t, &map, &sites, 0.5, QUIET);
        let was = carts(&t)[0].pos;
        sites.push(site(
            9,
            0,
            Role::Housing,
            Part::None,
            (was.x - 1.0, was.y - 1.0),
            (2.0, 2.0),
        ));
        // It is looked at again every couple of seconds.
        step(&mut t, &map, &sites, RECHECK + 0.5, QUIET);
        let now = carts(&t)[0].pos;
        assert_ne!(now, was);
        assert!(!sites[2].near(now, 0.0));
    }
}
