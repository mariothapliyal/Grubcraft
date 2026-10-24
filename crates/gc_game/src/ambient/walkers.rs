//! Busy bases, stage 3b (docs/34-ambient-life.md): a worker from each
//! finished workplace now and then walks to another building of the same
//! owner, stands at its door a while, and walks home. They are a picture:
//! not selectable, not in the simulation, in nobody's way, and they cost no
//! supply. They follow the buildings the viewer sees and the effects clock,
//! and walk straight between doors, so a trip that would cross a building,
//! water or a steep slope is never taken.

use std::collections::HashMap;

use glam::Vec2;

/// How far from the camera's focus walkers are kept, in tiles, how far past
/// a wall their door stands, and how far above the water the ground must
/// be for them to walk on it.
pub const REACH: f32 = 60.0;
pub const DOOR: f32 = 0.5;
pub const SHORE: f32 = 0.05;
/// How fast a walker goes, in tiles a second, and how far from his own door
/// he will go.
const SPEED: f32 = 1.1;
const FAR: f32 = 16.0;
/// How long he stands at a door before setting off again, in seconds: the
/// least, and how much more at most.
const STAY: (f32, f32) = (4.0, 9.0);
/// How far a walker's path keeps clear of walls, in tiles.
const CLEAR: f32 = 0.25;
/// A walker is forgotten when his home has been out of view this long.
const FORGET: f32 = 3.0;
/// Walkers kept at once, at most.
pub const MOST: usize = 160;
/// How far apart the ground is looked at along a path, in tiles, and the
/// steepest climb over one such step a walker will take.
const STEP: f32 = 0.4;
const STEEPEST: f32 = 0.3;

/// A building a walker can start from or go to, as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Home {
    /// A number that stays with the building (its handle's index and
    /// generation).
    pub id: u32,
    pub owner: u8,
    /// Whether a walker lives here (a hall or a workplace).
    pub lives: bool,
    /// Whether a walker may go here. A wall, a tower or a building going up
    /// is only in the way.
    pub visit: bool,
    /// The model his walker is drawn with, as the game numbers models.
    pub worker: usize,
    /// The middle of the footprint and half its sides.
    pub centre: Vec2,
    pub half: Vec2,
    /// Where its door stands, just outside the wall.
    pub door: Vec2,
}

impl Home {
    fn blocks(&self, p: Vec2) -> bool {
        let d = (p - self.centre).abs();
        d.x < self.half.x + CLEAR && d.y < self.half.y + CLEAR
    }
}

/// A walker as drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    pub pos: Vec2,
    pub facing: Vec2,
    pub walking: bool,
    /// Keeps walkers' steps out of time, 0 to 1.
    pub phase: f32,
    pub owner: u8,
    pub worker: usize,
}

#[derive(Clone, Copy, Debug)]
struct Walker {
    pos: Vec2,
    facing: Vec2,
    owner: u8,
    worker: usize,
    /// Where he is going, if on a trip.
    to: Option<Vec2>,
    /// Whether the trip in hand goes out (else home).
    outbound: bool,
    /// Whether he waits at a door away from home.
    away: bool,
    /// Seconds he stands still before the next trip.
    wait: f32,
    phase: f32,
    unseen: f32,
}

#[derive(Default)]
pub struct Walkers {
    walkers: HashMap<u32, Walker>,
    rng: u32,
}

impl Walkers {
    fn unit(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x6c07_8965;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    fn stay(&mut self) -> f32 {
        STAY.0 + self.unit() * STAY.1
    }

    /// Forget everyone (a new match).
    pub fn clear(&mut self) {
        self.walkers.clear();
    }

    /// Everyone to draw.
    pub fn seen(&self) -> impl Iterator<Item = Seen> + '_ {
        self.walkers.values().map(|w| Seen {
            pos: w.pos,
            facing: w.facing,
            walking: w.to.is_some(),
            phase: w.phase,
            owner: w.owner,
            worker: w.worker,
        })
    }

    /// Whether the straight way from `a` to `b` is clear: dry flat ground,
    /// not through any building.
    fn clear_way(
        a: Vec2,
        b: Vec2,
        homes: &[Home],
        walkable: &impl Fn(Vec2) -> bool,
        climb: &impl Fn(Vec2, Vec2) -> f32,
    ) -> bool {
        let steps = ((b - a).length() / STEP).ceil().max(1.0) as u32;
        let mut last = a;
        for k in 1..=steps {
            let p = a.lerp(b, k as f32 / steps as f32);
            if !walkable(p) || climb(last, p) > STEEPEST || homes.iter().any(|h| h.blocks(p)) {
                return false;
            }
            last = p;
        }
        true
    }

    /// A door for a walker from `home` to go to: another building of his
    /// owner within [`FAR`], by a clear way.
    fn pick(
        &mut self,
        home: &Home,
        homes: &[Home],
        walkable: &impl Fn(Vec2) -> bool,
        climb: &impl Fn(Vec2, Vec2) -> f32,
    ) -> Option<Vec2> {
        let near: Vec<&Home> = homes
            .iter()
            .filter(|h| h.visit && h.id != home.id && h.owner == home.owner)
            .filter(|h| h.door.distance(home.door) <= FAR)
            .collect();
        if near.is_empty() {
            return None;
        }
        // Start from a random one so the same neighbour is not always it.
        let first = (self.unit() * near.len() as f32) as usize;
        (0..near.len())
            .map(|k| near[(first + k) % near.len()])
            .map(|h| h.door)
            .find(|&d| Self::clear_way(home.door, d, homes, walkable, climb))
    }

    /// Move the walkers on by `dt` seconds of effects time. `homes` are the
    /// buildings the viewer sees now; `walkable` says whether the ground is
    /// dry and not too rough at a point, and `climb` how much it rises from
    /// one point to the next.
    pub fn advance(
        &mut self,
        dt: f32,
        homes: &[Home],
        walkable: impl Fn(Vec2) -> bool,
        climb: impl Fn(Vec2, Vec2) -> f32,
    ) {
        let dt = dt.min(0.25);
        let index: HashMap<u32, usize> = homes.iter().enumerate().map(|(i, h)| (h.id, i)).collect();
        self.walkers.retain(|id, w| {
            if index.contains_key(id) {
                w.unseen = 0.0;
            } else {
                w.unseen += dt;
            }
            w.unseen < FORGET
        });
        for h in homes.iter().filter(|h| h.lives) {
            if self.walkers.len() >= MOST {
                break;
            }
            if !self.walkers.contains_key(&h.id) {
                let (wait, phase) = (self.stay(), self.unit());
                self.walkers.insert(
                    h.id,
                    Walker {
                        pos: h.door,
                        facing: (h.door - h.centre).normalize_or(Vec2::Y),
                        owner: h.owner,
                        worker: h.worker,
                        to: None,
                        outbound: false,
                        away: false,
                        wait,
                        phase,
                        unseen: 0.0,
                    },
                );
            }
        }
        let ids: Vec<u32> = self.walkers.keys().copied().collect();
        for id in ids {
            let Some(&hi) = index.get(&id) else { continue };
            let home = homes[hi];
            let mut w = self.walkers[&id];
            match w.to {
                None => {
                    w.wait -= dt;
                    if w.wait <= 0.0 {
                        if w.away {
                            w.to = Some(home.door);
                            w.outbound = false;
                        } else if let Some(d) = self.pick(&home, homes, &walkable, &climb) {
                            w.to = Some(d);
                            w.outbound = true;
                        } else {
                            w.wait = STAY.0;
                        }
                    }
                }
                Some(to) => {
                    let way = to - w.pos;
                    let step = SPEED * dt;
                    if way.length() <= step {
                        w.pos = to;
                        w.to = None;
                        w.away = w.outbound;
                        w.wait = self.stay();
                        // Standing at a door he faces the building.
                    } else {
                        w.facing = way.normalize_or(w.facing);
                        w.pos += w.facing * step;
                    }
                }
            }
            self.walkers.insert(id, w);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(id: u32, x: f32, lives: bool) -> Home {
        Home {
            id,
            owner: 0,
            lives,
            visit: !lives,
            worker: 0,
            centre: Vec2::new(x, 10.0),
            half: Vec2::new(1.0, 1.0),
            door: Vec2::new(x, 11.6),
        }
    }

    fn flat() -> (impl Fn(Vec2) -> bool, impl Fn(Vec2, Vec2) -> f32) {
        (|_| true, |_, _| 0.0)
    }

    fn run(w: &mut Walkers, homes: &[Home], seconds: f32) {
        let (ok, climb) = flat();
        for _ in 0..(seconds * 20.0) as usize {
            w.advance(0.05, homes, &ok, &climb);
        }
    }

    #[test]
    fn a_workplace_gets_one_walker_who_goes_out_and_comes_home() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        assert_eq!(w.seen().count(), 1);
        let mut went = false;
        let mut came_home = false;
        for _ in 0..(120.0 * 20.0) as usize {
            run(&mut w, &homes, 0.05);
            let s = w.seen().next().unwrap();
            went |= s.pos.distance(homes[1].door) < 0.1;
            came_home |= went && s.pos.distance(homes[0].door) < 0.1;
        }
        assert!(went && came_home);
    }

    #[test]
    fn he_does_not_walk_through_a_building() {
        let mut wall = home(3, 13.0, false);
        wall.half = Vec2::new(1.0, 3.0);
        wall.door = Vec2::new(13.0, 14.0);
        let homes = [home(1, 10.0, true), home(2, 16.0, false), wall];
        let mut w = Walkers::default();
        // The building between them is a valid place to go but the far
        // one is shut off; he never ends up on the far side.
        for _ in 0..(200.0 * 20.0) as usize {
            run(&mut w, &homes, 0.05);
            let s = w.seen().next().unwrap();
            assert!(!wall.blocks(s.pos), "{:?}", s.pos);
        }
    }

    #[test]
    fn no_trip_over_water_or_a_cliff() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        for _ in 0..(100.0 * 20.0) as usize {
            w.advance(0.05, &homes, |_| false, |_, _| 0.0);
            assert!(!w.seen().next().unwrap().walking);
        }
        for _ in 0..(100.0 * 20.0) as usize {
            w.advance(0.05, &homes, |_| true, |_, _| 5.0);
            assert!(!w.seen().next().unwrap().walking);
        }
    }

    #[test]
    fn walkers_are_forgotten_when_their_home_is_not_seen() {
        let homes = [home(1, 10.0, true), home(2, 16.0, false)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        run(&mut w, &[], FORGET + 1.0);
        assert_eq!(w.seen().count(), 0);
    }

    #[test]
    fn a_lone_building_has_nowhere_to_go() {
        let homes = [home(1, 10.0, true)];
        let mut w = Walkers::default();
        run(&mut w, &homes, 60.0);
        assert!(!w.seen().next().unwrap().walking);
    }

    #[test]
    fn at_most_so_many_walk() {
        let homes: Vec<Home> = (0..400).map(|i| home(i, i as f32 * 3.0, true)).collect();
        let mut w = Walkers::default();
        run(&mut w, &homes, 1.0);
        assert_eq!(w.seen().count(), MOST);
    }
}
