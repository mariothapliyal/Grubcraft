//! The Grubfolk's villages, towns and bazaar (docs/34-ambient-life.md,
//! stage 5a): until now these sites were only a clearing in the scenery.
//! Each gets its houses: huts and cottages round a green with a well, a
//! barn, haystacks and runs of wattle fence, and market stalls in a town or
//! bazaar. They are scenery models placed once from the map's own objects
//! (a hash of the site's tile, so a place always looks the same), picture
//! only: nothing blocks soldiers and nothing is stored in the simulation.
//!
//! A hamlet a battle is fought in is ruined: once enough of the fallen lie
//! among its houses, they are burnt out and smoulder a while, its fences
//! and stalls are knocked flat, its people hide indoors for good, and crows
//! come to roost on its roofs (`aftermath::Aftermath::roost`). A ruin is
//! the viewer's picture too: it follows the dead the viewer has seen.

use gc_map::Map;
use gc_map::def::Kind;
use glam::{Vec2, Vec3};

use crate::scenery::{Extra, Model, facing_of, turn_of};

/// The ground under a building may vary this much over its footprint (in
/// tiles), and a building must stand this far above the water.
const LEVEL: f32 = 0.45;
const DRY: f32 = 0.2;
/// Slack when two things' footprints meet, in tiles.
const TOUCH: f32 = 0.3;

/// Fallen soldiers lying among a hamlet's models that ruin it.
pub const RUIN_DEAD: usize = 12;
/// How long a ruined house smoulders, in seconds of effects time.
pub const SMOULDER: f32 = 180.0;
/// The tints of what a fire leaves: burnt timber and thatch, a scorched
/// well, a burnt haystack (the scenery shader doubles them, 128 as made).
const CHARRED: [u8; 3] = [50, 44, 40];
const SCORCHED: [u8; 3] = [86, 80, 76];
const ASH: [u8; 3] = [36, 33, 31];

/// What a site is: how big its hamlet is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Size {
    Village,
    Town,
    Bazaar,
}

/// The things a hamlet is made of, with the radius of the ground each
/// takes, in tiles.
fn radius(model: Model) -> f32 {
    match model {
        Model::GrubfolkHut => 1.9,
        Model::GrubfolkHouse => 2.4,
        Model::GrubfolkBarn => 3.0,
        Model::GrubfolkWell => 0.9,
        Model::Haystack => 1.1,
        Model::WattleFence => 1.0,
        Model::MarketStall => 1.3,
        _ => 1.0,
    }
}

/// Whether `model` is a house, hut or barn: a building with a door and a
/// roof.
fn dwelling(model: Model) -> bool {
    matches!(
        model,
        Model::GrubfolkHut | Model::GrubfolkHouse | Model::GrubfolkBarn
    )
}

/// A small random stream from a seed (xorshift).
#[derive(Clone, Debug)]
pub(super) struct Rng(u32);

impl Rng {
    /// A stream from any seed.
    pub(super) fn seeded(seed: u32) -> Rng {
        let mut r = Rng(seed.wrapping_mul(0x9e37_79b1) | 1);
        for _ in 0..4 {
            r.unit();
        }
        r
    }

    fn new(site: (i32, i32), size: Size) -> Rng {
        let mut x = (site.0 as u32)
            .wrapping_mul(0x9e37_79b1)
            .wrapping_add((site.1 as u32).wrapping_mul(0x85eb_ca6b))
            .wrapping_add(size as u32 * 0x27d4_eb2f)
            | 1;
        for _ in 0..4 {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
        }
        Rng(x)
    }

    pub(super) fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }
}

/// The ground the hamlet is laid out on.
struct Ground<'a> {
    map: &'a Map,
}

impl Ground<'_> {
    fn height(&self, p: Vec2) -> f32 {
        self.map.surface_at(p.x, p.y)
    }

    /// Whether something of `reach` radius may stand at `p`: dry, off the
    /// woods, and the ground under it level enough.
    fn suits(&self, p: Vec2, reach: f32) -> bool {
        let r = reach * 0.7;
        let mut low = f32::MAX;
        let mut high = f32::MIN;
        let mut dry = true;
        let side = self.map.tiles as i32;
        for d in [
            Vec2::ZERO,
            Vec2::X,
            Vec2::NEG_X,
            Vec2::Y,
            Vec2::NEG_Y,
            Vec2::ONE,
            Vec2::NEG_ONE,
            Vec2::new(1.0, -1.0),
            Vec2::new(-1.0, 1.0),
        ] {
            let q = p + d * r;
            let h = self.map.height_at(q.x, q.y);
            dry &= h > self.map.water_at(q.x, q.y) + DRY;
            low = low.min(h);
            high = high.max(h);
            let (tx, ty) = (q.x.floor() as i32, q.y.floor() as i32);
            if (0..side).contains(&tx)
                && (0..side).contains(&ty)
                && self.wooded((ty * side + tx) as usize)
            {
                return false;
            }
        }
        dry && high - low <= LEVEL
    }

    /// Whether a tile (row by row) is woods, thicket or not.
    fn wooded(&self, tile: usize) -> bool {
        self.map.cover.get(tile) == Some(&(gc_sim::Cover::Wood as u8)) || self.map.is_thicket(tile)
    }
}

/// How steep a track may be: the most the ground may climb between two
/// points `ROUTE_STEP` tiles apart.
const ROUTE_STEP: f32 = 0.75;
pub(super) const ROUTE_CLIMB: f32 = 0.12;

/// Whether a cart can go from `a` to `b` in a straight line over dry,
/// gently sloping ground that is not woods, and `ok` at every step.
pub(super) fn route_clear(map: &Map, a: Vec2, b: Vec2, ok: impl Fn(Vec2) -> bool) -> bool {
    let ground = Ground { map };
    let side = map.tiles as i32;
    let steps = (a.distance(b) / ROUTE_STEP).ceil().max(1.0) as usize;
    let mut last = map.height_at(a.x, a.y);
    for i in 0..=steps {
        let p = a.lerp(b, i as f32 / steps as f32);
        let h = map.height_at(p.x, p.y);
        let (tx, ty) = (p.x.floor() as i32, p.y.floor() as i32);
        if h <= map.water_at(p.x, p.y) + DRY
            || (h - last).abs() > ROUTE_CLIMB
            || !(0..side).contains(&tx)
            || !(0..side).contains(&ty)
            || !ok(p)
            || ground.wooded((ty * side + tx) as usize)
        {
            return false;
        }
        last = h;
    }
    true
}

/// What has been put down so far in one hamlet.
struct Hamlet<'a> {
    ground: Ground<'a>,
    centre: Vec2,
    rng: Rng,
    placed: Vec<(Vec2, f32)>,
    out: Vec<Extra>,
}

impl Hamlet<'_> {
    /// Stand `model` at `at`, its front toward `facing`, if the ground
    /// suits it and it keeps off what is there.
    fn put(&mut self, model: Model, at: Vec2, facing: Vec2) -> bool {
        let scale = 0.96 + 0.1 * self.rng.unit();
        let r = radius(model) * scale;
        if !self.ground.suits(at, r)
            || self
                .placed
                .iter()
                .any(|&(p, pr)| p.distance(at) < r + pr - TOUCH)
        {
            return false;
        }
        self.out.push(Extra {
            model,
            at: Vec3::new(at.x, self.ground.height(at), at.y),
            turn: turn_of(facing),
            scale,
            tint: [128; 3],
            phase: 0,
            lean: 0,
            fall: 0,
        });
        self.placed.push((at, r));
        true
    }

    /// A point `radius` out from the middle at `angle`.
    fn on(&self, radius: f32, angle: f32) -> Vec2 {
        self.centre + Vec2::from_angle(angle) * radius
    }

    /// A ring of `models` round the middle at `round`, their fronts
    /// toward it, spread evenly from `start` with a little give.
    fn ring(&mut self, models: &[Model], round: f32, start: f32) {
        let n = models.len();
        // The biggest first, so a barn is not squeezed out by its neighbours.
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&a, &b| radius(models[b]).total_cmp(&radius(models[a])));
        for k in order {
            let m = models[k];
            let jitter = (self.rng.unit() - 0.5) * 0.2;
            let angle = start + (k as f32 + jitter) * std::f32::consts::TAU / n as f32;
            let at = self.on(round, angle);
            let facing = (self.centre - at).normalize_or(Vec2::Y);
            self.put(m, at, facing);
        }
    }

    /// Haystacks and then fence runs out at `radius`, where the ring of
    /// buildings has left room.
    fn edge(&mut self, haystacks: usize, radius: f32) {
        let start = self.rng.unit() * std::f32::consts::TAU;
        let mut made = 0;
        for k in 0..16 {
            if made >= haystacks {
                break;
            }
            let angle = start + k as f32 * 0.83;
            let at = self.on(radius - 0.6, angle);
            if self.put(Model::Haystack, at, Vec2::from_angle(angle)) {
                made += 1;
            }
        }
        let n = (std::f32::consts::TAU * radius / 2.0) as usize;
        for k in 0..n {
            if self.rng.unit() < 0.55 {
                continue;
            }
            let angle = start + k as f32 * std::f32::consts::TAU / n as f32;
            let at = self.on(radius, angle);
            // Along the circle, the front facing out.
            self.put(Model::WattleFence, at, Vec2::from_angle(angle));
        }
    }
}

/// The models of a hamlet of `size` laid round `centre`.
fn lay(map: &Map, centre: Vec2, site: (i32, i32), size: Size) -> Vec<Extra> {
    use Model::*;
    let mut h = Hamlet {
        ground: Ground { map },
        centre,
        rng: Rng::new(site, size),
        placed: Vec::new(),
        out: Vec::new(),
    };
    let start = h.rng.unit() * std::f32::consts::TAU;
    // The well, or the market's well, in the middle.
    h.put(GrubfolkWell, centre, Vec2::Y);
    match size {
        Size::Village => {
            h.ring(
                &[
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkBarn,
                ],
                5.8,
                start,
            );
            h.edge(2, 8.5);
        }
        Size::Town => {
            for k in 0..3 {
                let angle = start + k as f32 * std::f32::consts::TAU / 3.0 + 0.5;
                let at = h.on(2.9, angle);
                h.put(MarketStall, at, (centre - at).normalize_or(Vec2::Y));
            }
            h.ring(
                &[
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkBarn,
                ],
                6.8,
                start,
            );
            h.edge(2, 9.0);
        }
        Size::Bazaar => {
            h.ring(&[MarketStall; 7], 3.4, start);
            h.ring(
                &[
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                    GrubfolkHut,
                    GrubfolkHouse,
                ],
                7.4,
                start + 0.3,
            );
            h.edge(2, 9.0);
        }
    }
    h.out
}

/// Whether a building stands on the ground under `e`: any tile under the
/// middle of its footprint is occupied.
fn built_on(e: &Extra, occupied: &impl Fn((i32, i32)) -> bool) -> bool {
    let r = (radius(e.model) * e.scale * 0.8).max(0.5);
    let (x0, x1) = ((e.at.x - r).floor() as i32, (e.at.x + r).floor() as i32);
    let (y0, y1) = ((e.at.z - r).floor() as i32, (e.at.z + r).floor() as i32);
    (y0..=y1).any(|y| (x0..=x1).any(|x| occupied((x, y))))
}

/// What a villager does at a [`Node`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    /// A cottage's or barn's door: a place to go in.
    Door,
    /// The back of a market stall.
    Stall,
    /// Beside a haystack.
    Haystack,
    /// The well on a village green.
    Well,
}

/// A place in a hamlet where a villager may stand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Node {
    pub at: Vec2,
    pub spot: Spot,
    /// The way to face there: toward what the place is for.
    pub look: Vec2,
}

/// How far in front of a model's middle its door, or the place to stand
/// at it, is, in tiles.
fn out_of(model: Model) -> f32 {
    match model {
        Model::GrubfolkHut => 1.75,
        Model::GrubfolkHouse => 1.45,
        Model::GrubfolkBarn => 2.0,
        Model::MarketStall => 1.1,
        Model::Haystack => 1.4,
        _ => 1.5,
    }
}

/// The places to stand in a hamlet of `extras` round `centre`, in order of
/// their bearing from the middle: doors, the backs of stalls and the sides
/// of haystacks, and the well where a green has no stalls round it.
fn nodes_of(centre: Vec2, extras: &[Extra]) -> Vec<Node> {
    let stalls = extras.iter().any(|e| e.model == Model::MarketStall);
    let mut nodes: Vec<Node> = extras
        .iter()
        .filter_map(|e| {
            let at = Vec2::new(e.at.x, e.at.z);
            let front = facing_of(e.turn);
            let out = out_of(e.model) * e.scale;
            Some(match e.model {
                Model::GrubfolkHut | Model::GrubfolkHouse | Model::GrubfolkBarn => Node {
                    at: at + front * out,
                    spot: Spot::Door,
                    look: -front,
                },
                Model::MarketStall => Node {
                    at: at - front * out,
                    spot: Spot::Stall,
                    look: front,
                },
                // A haystack's front faces out from the middle.
                Model::Haystack => Node {
                    at: at - front * out,
                    spot: Spot::Haystack,
                    look: front,
                },
                Model::GrubfolkWell if !stalls => Node {
                    at: at + Vec2::Y * (out_of(e.model) + 0.2),
                    spot: Spot::Well,
                    look: -Vec2::Y,
                },
                _ => return None,
            })
        })
        .collect();
    nodes.sort_by(|a, b| {
        let bearing = |n: &Node| (n.at - centre).to_angle();
        bearing(a).total_cmp(&bearing(b))
    });
    nodes
}

/// The hamlets' scenery, kept for the match and handed to the scenery pass
/// near the camera.
#[derive(Default)]
pub struct Hamlets {
    /// Every model of every hamlet, grouped by hamlet: the middle of its
    /// site and the models laid there.
    sites: Vec<(Vec2, Vec<Extra>)>,
    /// What size each site is, in the same order.
    sizes: Vec<Size>,
    /// How far from its middle each site's models reach, in tiles (0 for a
    /// site left bare).
    reaches: Vec<f32>,
    /// Seconds since each site was ruined, if it has been.
    ruined: Vec<Option<f32>>,
}

/// How far from its middle a hamlet's models reach, in tiles.
pub(super) const SPREAD: f32 = 12.0;

impl Hamlets {
    /// Lay out the hamlets of `map`'s Villages, Towns and Bazaar.
    pub fn new(map: &Map) -> Hamlets {
        let (sites, sizes): (Vec<(Vec2, Vec<Extra>)>, Vec<Size>) = map
            .objects
            .iter()
            .filter_map(|o| {
                let size = match o.kind {
                    Kind::Village => Size::Village,
                    Kind::Town => Size::Town,
                    Kind::Bazaar => Size::Bazaar,
                    _ => return None,
                };
                let centre = Vec2::new(o.at.0 as f32 + 0.5, o.at.1 as f32 + 0.5);
                Some(((centre, lay(map, centre, o.at, size)), size))
            })
            .unzip();
        let reaches = (sites.iter())
            .map(|(centre, extras)| {
                (extras.iter())
                    .map(|e| {
                        Vec2::new(e.at.x, e.at.z).distance(*centre) + radius(e.model) * e.scale
                    })
                    .fold(0.0, f32::max)
            })
            .collect();
        Hamlets {
            ruined: vec![None; sizes.len()],
            reaches,
            sites,
            sizes,
        }
    }

    /// `dt` seconds on: the ruins smoulder down.
    pub fn age(&mut self, dt: f32) {
        for r in self.ruined.iter_mut().flatten() {
            *r += dt;
        }
    }

    /// The hamlets with houses that are still standing.
    fn standing(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.sites.len()).filter(|&i| self.ruined[i].is_none() && self.reaches[i] > 0.0)
    }

    /// Whether any hamlet is left to ruin.
    pub fn any_standing(&self) -> bool {
        self.standing().next().is_some()
    }

    /// Whether `p` is among the models of a hamlet still standing.
    pub fn in_standing(&self, p: Vec2) -> bool {
        self.standing()
            .any(|i| p.distance_squared(self.sites[i].0) <= self.reaches[i] * self.reaches[i])
    }

    /// Ruin each standing hamlet with [`RUIN_DEAD`] of the fallen `lying`
    /// among its models, and give the sites newly ruined.
    pub fn ruin_where(&mut self, lying: &[Vec2]) -> Vec<usize> {
        let ruin: Vec<usize> = (self.standing())
            .filter(|&i| {
                let (centre, reach) = (self.sites[i].0, self.reaches[i]);
                (lying.iter())
                    .filter(|p| p.distance_squared(centre) <= reach * reach)
                    .nth(RUIN_DEAD - 1)
                    .is_some()
            })
            .collect();
        for &i in &ruin {
            self.ruin(i);
        }
        ruin
    }

    /// Burn out site `i`: its houses and barn charred (left standing, for
    /// the crows), its haystacks burnt, its fences and stalls knocked flat
    /// a different way each.
    fn ruin(&mut self, i: usize) {
        self.ruined[i] = Some(0.0);
        let centre = self.sites[i].0;
        let mut rng = Rng::new((centre.x as i32, centre.y as i32), self.sizes[i]);
        // Not the stream the hamlet was laid out with.
        rng.unit();
        for e in &mut self.sites[i].1 {
            let (tint, lean) = match e.model {
                Model::GrubfolkWell => (SCORCHED, 0),
                Model::Haystack => (ASH, 0),
                Model::WattleFence => (CHARRED, 150 + (rng.unit() * 90.0) as u8),
                Model::MarketStall => (CHARRED, 200 + (rng.unit() * 55.0) as u8),
                _ => (CHARRED, 0),
            };
            e.tint = tint;
            e.lean = lean;
            e.fall = (rng.unit() * 255.0) as u8;
        }
    }

    /// Whether site `i` has been ruined.
    pub fn ruined(&self, i: usize) -> bool {
        self.ruined[i].is_some()
    }

    /// The burnt houses of site `i` that are still smouldering, not built
    /// over (`occupied`): each one's place on the ground, its radius, and how
    /// far through its smouldering it is (0 to 1).
    pub fn smouldering<'a>(
        &'a self,
        i: usize,
        occupied: impl Fn((i32, i32)) -> bool + 'a,
    ) -> impl Iterator<Item = (Vec3, f32, f32)> + 'a {
        let left = self.ruined[i].map_or(1.0, |r| r / SMOULDER);
        let houses = (left < 1.0).then(|| self.houses(i, occupied));
        (houses.into_iter().flatten()).map(move |e| (e.at, radius(e.model) * e.scale, left))
    }

    /// The roofs of site `i`'s houses and barn that are not built over
    /// (`occupied`), for crows to sit on: where each top is, and how high
    /// over the ground there, with `height` the height of a model.
    pub fn roofs(
        &self,
        i: usize,
        height: impl Fn(Model) -> f32,
        ground: impl Fn(Vec2) -> f32,
        occupied: impl Fn((i32, i32)) -> bool,
    ) -> Vec<(Vec2, f32)> {
        self.houses(i, occupied)
            .map(|e| {
                let at = Vec2::new(e.at.x, e.at.z);
                (at, e.at.y + height(e.model) * e.scale - ground(at))
            })
            .filter(|&(_, up)| up > 0.5)
            .collect()
    }

    /// Site `i`'s houses, huts and barn that are not built over.
    fn houses<'a>(
        &'a self,
        i: usize,
        occupied: impl Fn((i32, i32)) -> bool + 'a,
    ) -> impl Iterator<Item = &'a Extra> + 'a {
        (self.sites[i].1.iter()).filter(move |e| dwelling(e.model) && !built_on(e, &occupied))
    }

    /// The models of the hamlets within `reach` tiles of `focus`, but not
    /// those whose ground a player has built on: `occupied` says whether a
    /// tile has a building (or its margin) on it.
    pub fn near<'a>(
        &'a self,
        focus: Vec2,
        reach: f32,
        occupied: impl Fn((i32, i32)) -> bool + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        self.sites
            .iter()
            .filter(move |(c, _)| c.distance(focus) <= reach + SPREAD)
            .flat_map(|(_, extras)| extras.iter().copied())
            .filter(move |e| !built_on(e, &occupied))
    }

    /// The middle of the first site, if there is one.
    pub fn first(&self) -> Option<Vec2> {
        self.sites.first().map(|(c, _)| *c)
    }

    /// How many sites have been laid out.
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// The middle of site `i`.
    pub fn centre(&self, i: usize) -> Vec2 {
        self.sites[i].0
    }

    /// How far from its middle site `i`'s models reach, in tiles; nothing
    /// if the site was left bare.
    pub fn reach(&self, i: usize) -> f32 {
        self.reaches[i]
    }

    /// How big site `i` is.
    pub(super) fn size(&self, i: usize) -> Size {
        self.sizes[i]
    }

    /// The places to stand at site `i`, by bearing from its middle, leaving out
    /// the models a player has built over.
    pub fn nodes(&self, i: usize, occupied: impl Fn((i32, i32)) -> bool) -> Vec<Node> {
        let (centre, extras) = &self.sites[i];
        let standing: Vec<Extra> = extras
            .iter()
            .copied()
            .filter(|e| !built_on(e, &occupied))
            .collect();
        nodes_of(*centre, &standing)
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use gc_map::Placed;

    /// Every shipped Conquest map and a Wilds land.
    fn maps() -> Vec<Map> {
        let tiles = gc_ui::lobby::MapSize::Medium.conquest_tiles();
        (0..=gc_map::SHIPPED.len() + 1)
            .map(|choice| crate::conquest::make_map(choice, tiles, 7))
            .collect()
    }

    /// The sites of a test map: the kind and the tile.
    pub(in crate::ambient) type Sites = Vec<(Kind, (i32, i32))>;

    /// A level, bare, dry map with one site of each size, far apart.
    pub(in crate::ambient) fn flat() -> (Map, Sites) {
        let mut map = crate::conquest::make_map(0, 0, 7);
        let side = map.tiles as i32;
        let top = map.max_height();
        let sample = ((map.water_level() + 2.0) / top * f32::from(u16::MAX)) as u16;
        map.samples.fill(sample);
        map.cover.fill(0);
        map.thicket.clear();
        let sites = vec![
            (Kind::Village, (side / 4, side / 4)),
            (Kind::Town, (side / 2, side / 2)),
            (Kind::Bazaar, (side * 3 / 4, side * 3 / 4)),
        ];
        map.objects = sites
            .iter()
            .map(|&(kind, at)| Placed {
                kind,
                at,
                owner: None,
                label: String::new(),
            })
            .collect();
        (map, sites)
    }

    fn count(extras: &[Extra], model: Model) -> usize {
        extras.iter().filter(|e| e.model == model).count()
    }

    fn site(h: &Hamlets, at: (i32, i32)) -> &[Extra] {
        let centre = Vec2::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5);
        &h.sites.iter().find(|(c, _)| *c == centre).unwrap().1
    }

    #[test]
    fn every_village_gets_houses_on_dry_ground_and_the_same_each_time() {
        let (mut sites, mut with_houses) = (0, 0);
        for map in maps() {
            let (a, b) = (Hamlets::new(&map), Hamlets::new(&map));
            let wanted = map
                .objects
                .iter()
                .filter(|o| matches!(o.kind, Kind::Village | Kind::Town | Kind::Bazaar))
                .count();
            assert_eq!(a.len(), wanted);
            sites += wanted;
            for ((ca, ea), (_, eb)) in a.sites.iter().zip(&b.sites) {
                assert_eq!(ea, eb);
                with_houses += usize::from(
                    ea.iter()
                        .any(|e| matches!(e.model, Model::GrubfolkHut | Model::GrubfolkHouse)),
                );
                for e in ea {
                    assert!(e.at.y > map.water_level());
                    assert!(Vec2::new(e.at.x, e.at.z).distance(*ca) < SPREAD);
                }
            }
        }
        assert!(sites > 0, "no test map has a village");
        assert!(with_houses > 0);
    }

    #[test]
    fn no_house_stands_in_a_wood() {
        for map in maps() {
            let side = map.tiles as i32;
            for (_, extras) in &Hamlets::new(&map).sites {
                for e in extras {
                    let tile = e.at.z.floor() as i32 * side + e.at.x.floor() as i32;
                    let wood = map.cover[tile as usize] == gc_sim::Cover::Wood as u8
                        || map.is_thicket(tile as usize);
                    assert!(!wood, "{e:?} stands in a wood");
                }
            }
        }
    }

    #[test]
    fn nothing_in_a_hamlet_overlaps_its_neighbours() {
        for map in maps() {
            for (_, extras) in &Hamlets::new(&map).sites {
                for (i, a) in extras.iter().enumerate() {
                    for b in &extras[i + 1..] {
                        let d = Vec2::new(a.at.x - b.at.x, a.at.z - b.at.z).length();
                        let (ra, rb) = (radius(a.model) * a.scale, radius(b.model) * b.scale);
                        assert!(d >= ra + rb - TOUCH - 1e-3, "{a:?} {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn level_dry_ground_gives_each_size_its_whole_hamlet() {
        let (map, sites) = flat();
        let h = Hamlets::new(&map);
        for (kind, at) in sites {
            let e = site(&h, at);
            let cottages = count(e, Model::GrubfolkHut) + count(e, Model::GrubfolkHouse);
            let (cottages_want, stalls, barns) = match kind {
                Kind::Village => (4, 0, 1),
                Kind::Town => (5, 3, 1),
                _ => (5, 7, 0),
            };
            assert_eq!(cottages, cottages_want, "{kind:?}");
            assert_eq!(count(e, Model::MarketStall), stalls, "{kind:?}");
            assert_eq!(count(e, Model::GrubfolkBarn), barns, "{kind:?}");
            assert_eq!(count(e, Model::GrubfolkWell), 1, "{kind:?}");
            assert!(count(e, Model::Haystack) > 0, "{kind:?}");
            assert!(count(e, Model::WattleFence) > 0, "{kind:?}");
        }
    }

    #[test]
    fn a_site_in_a_wood_or_under_water_gets_nothing() {
        let (mut map, _) = flat();
        map.cover.fill(gc_sim::Cover::Wood as u8);
        assert!(
            Hamlets::new(&map)
                .sites
                .iter()
                .all(|(_, extras)| extras.is_empty())
        );
        let (mut map, _) = flat();
        map.samples.fill(0);
        assert!(
            Hamlets::new(&map)
                .sites
                .iter()
                .all(|(_, extras)| extras.is_empty())
        );
    }

    #[test]
    fn only_hamlets_near_the_camera_and_off_players_buildings_are_handed_over() {
        let (map, sites) = flat();
        let h = Hamlets::new(&map);
        let none = |_| false;
        let all = h.near(Vec2::ZERO, 1.0e6, none).count();
        assert!(all > 0);
        assert_eq!(h.near(Vec2::splat(-1.0e6), 100.0, none).count(), 0);
        // A building on the village green hides what stands on it, not the
        // far side of the village or other hamlets.
        let (_, at) = sites[0];
        let green = |t: (i32, i32)| t == at;
        let left = h.near(Vec2::ZERO, 1.0e6, green).count();
        assert!(left < all && left > all - site(&h, at).len());
        assert_eq!(h.near(Vec2::ZERO, 1.0e6, |_| true).count(), 0);
    }

    #[test]
    fn enough_dead_among_a_hamlet_ruin_it_and_only_it() {
        let (map, sites) = flat();
        let mut h = Hamlets::new(&map);
        let centre = |k: usize| Vec2::new(sites[k].1.0 as f32 + 0.5, sites[k].1.1 as f32 + 0.5);
        let fresh = site(&h, sites[0].1).to_vec();
        assert!(h.any_standing());
        assert!(h.in_standing(centre(0)) && !h.in_standing(centre(0) + Vec2::new(40.0, 0.0)));
        // A few dead, or many far off, ruin nothing.
        let few = vec![centre(0) + Vec2::new(2.0, 0.0); RUIN_DEAD - 1];
        let far = vec![centre(0) + Vec2::new(40.0, 0.0); RUIN_DEAD * 3];
        assert!(h.ruin_where(&few).is_empty() && h.ruin_where(&far).is_empty());
        let mut lying = few.clone();
        lying.push(centre(0) + Vec2::new(-3.0, 1.0));
        assert_eq!(h.ruin_where(&lying), vec![0]);
        assert!(h.ruined(0) && !h.ruined(1) && !h.ruined(2));
        // Ruined once only.
        assert!(h.ruin_where(&lying).is_empty());
        // The houses are charred and still stand; the fences are down; the
        // models are where they were.
        let ruin = site(&h, sites[0].1);
        assert_eq!(ruin.len(), fresh.len());
        for (e, was) in ruin.iter().zip(&fresh) {
            assert_eq!((e.model, e.at, e.scale), (was.model, was.at, was.scale));
            assert!(e.tint.iter().all(|&c| c < 100), "{:?} not burnt", e.model);
            match e.model {
                Model::WattleFence | Model::MarketStall => assert!(e.lean >= 150),
                _ => assert_eq!(e.lean, 0),
            }
        }
        // A roof for every house and the barn, at its model's height.
        let roofs = h.roofs(0, |_| 2.0, |_| map.water_level() + 2.0, |_| false);
        let houses = fresh.iter().filter(|e| dwelling(e.model)).count();
        assert_eq!(roofs.len(), houses);
        assert!(roofs.iter().all(|&(_, up)| (up - 2.0).abs() < 0.2));
        assert!(
            h.roofs(0, |_| 2.0, |_| 0.0, |_| true).is_empty(),
            "built over"
        );
        // They smoulder a while, then the smoke is done.
        assert_eq!(h.smouldering(0, |_| false).count(), houses);
        assert_eq!(h.smouldering(1, |_| false).count(), 0);
        assert!(!h.in_standing(centre(0)) && h.in_standing(centre(1)));
        h.age(SMOULDER);
        assert_eq!(h.smouldering(0, |_| false).count(), 0);
    }
}
