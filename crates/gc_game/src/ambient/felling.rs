//! Clearing land as it looks (docs/34-ambient-life.md, docs/32): workers
//! cut at what they clear, and when the simulation says a tile is cleared
//! (`Event::Cleared`) its trees come down, tipping away from the cutters,
//! to lie a while before they are hauled off, and its bushes are torn up.
//! Each is the scenery model it was, drawn as an [`Extra`] tipped over about
//! its base (the scenery shader's `lean`).

use glam::{Vec2, Vec3};

use crate::effects;
use crate::scenery::{Extra, Stood};

/// A tree takes this long to go over, in seconds; it then lies this long
/// and takes this long to be carried off (shrinking out of sight).
const FALL_SECONDS: f32 = 1.7;
const LIE_SECONDS: f32 = 14.0;
const HAUL_SECONDS: f32 = 2.5;
/// A bush is torn up in this long.
const UPROOT_SECONDS: f32 = 0.6;
/// The trees and bushes coming down or lying at once; a new one pushes out
/// the oldest.
const MOST_DOWN: usize = 512;
/// A cutter not seen for this long (seconds) has stopped.
const CUTTER_SEEN: f32 = 0.6;
/// How far a tree's fall strays from straight away from its cutter, in
/// turns.
const STRAY: f32 = 0.08;
/// How much of a tree's height its crown is from its base when it lies.
const CROWN_REACH: f32 = 0.75;

/// A worker seen cutting a tile this frame.
#[derive(Clone, Copy, Debug)]
struct Cutter {
    tile: (i32, i32),
    from: Vec2,
    seen: f32,
}

/// A tree or bush on its way down, lying, or being taken away.
#[derive(Clone, Copy, Debug)]
struct Down {
    stood: Stood,
    /// Its height as a model, in tiles, before scaling.
    height: f32,
    /// The way it falls, in 256ths of a turn.
    turn: u8,
    /// When it began, on the felling's clock.
    born: f32,
    landed: bool,
}

impl Down {
    /// Seconds since it began.
    fn age(&self, clock: f32) -> f32 {
        clock - self.born
    }

    fn lasts(&self) -> f32 {
        if self.stood.tree {
            FALL_SECONDS + LIE_SECONDS + HAUL_SECONDS
        } else {
            UPROOT_SECONDS
        }
    }

    /// How tipped over it is (0 to 255), and how big, by its age.
    fn pose(&self, age: f32) -> (u8, f32) {
        if !self.stood.tree {
            let f = (age / UPROOT_SECONDS).clamp(0.0, 1.0);
            return ((f * 90.0) as u8, self.stood.scale * (1.0 - f * f));
        }
        let over = (age / FALL_SECONDS).clamp(0.0, 1.0);
        // Slow to start, then faster as it goes.
        let mut tip = over.powf(2.2);
        // A small bounce as the crown lands.
        let since = age - FALL_SECONDS;
        if (0.0..0.5).contains(&since) {
            tip -= 0.025 * (since * 12.0).sin().abs() * (1.0 - since * 2.0);
        }
        let haul = ((age - FALL_SECONDS - LIE_SECONDS) / HAUL_SECONDS).clamp(0.0, 1.0);
        (
            (tip.clamp(0.0, 1.0) * 255.0) as u8,
            self.stood.scale * (1.0 - haul),
        )
    }
}

/// The turn (256ths) of the direction `dir`, as the scenery table reads it.
fn turn_of(dir: Vec2) -> u8 {
    (dir.y.atan2(dir.x) / std::f32::consts::TAU * 256.0) as i32 as u8
}

#[derive(Default)]
pub struct Felling {
    clock: f32,
    cutters: Vec<Cutter>,
    down: Vec<Down>,
}

impl Felling {
    /// A worker at `from` is cutting at `tile` this frame.
    pub fn cutting(&mut self, tile: (i32, i32), from: Vec2) {
        let seen = self.clock;
        match self.cutters.iter_mut().find(|c| c.tile == tile) {
            Some(c) => (c.from, c.seen) = (from, seen),
            None => self.cutters.push(Cutter { tile, from, seen }),
        }
    }

    /// `tile` was cleared: what stood on it comes down. Call it with
    /// [`crate::scenery::Scenery::standing`], before the tile is felled.
    pub fn clear(
        &mut self,
        tile: (i32, i32),
        stood: impl Iterator<Item = Stood>,
        height: impl Fn(&Stood) -> f32,
    ) {
        let centre = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
        let away = self
            .cutters
            .iter()
            .find(|c| c.tile == tile)
            .map(|c| (centre - c.from).normalize_or_zero())
            .filter(|d| *d != Vec2::ZERO);
        for s in stood {
            // Straight away from the cutter, give or take; with no one to
            // push it, the way the tree was already turned.
            let turn = match away {
                Some(d) => {
                    let wobble = (f32::from(s.turn) / 256.0 - 0.5) * 2.0 * STRAY * 256.0;
                    turn_of(d).wrapping_add(wobble as i32 as u8)
                }
                None => s.turn,
            };
            if self.down.len() >= MOST_DOWN {
                self.down.remove(0);
            }
            let height = height(&s);
            // Not all at once: a few trees on a tile go over one after
            // another.
            let lag = f32::from(s.tint[3]) / 255.0 * 0.5;
            self.down.push(Down {
                stood: s,
                height,
                turn,
                born: self.clock + lag,
                landed: false,
            });
        }
    }

    /// Move on by `dt` seconds of game time: forget cutters who stopped,
    /// raise the dust of what landed, and drop what is gone. `ground` is
    /// the terrain height.
    pub fn advance(&mut self, dt: f32, fx: &mut effects::Field, ground: impl Fn(Vec2) -> f32) {
        self.clock += dt;
        let clock = self.clock;
        self.cutters.retain(|c| clock - c.seen < CUTTER_SEEN);
        for d in &mut self.down {
            let age = d.age(clock);
            if d.landed || age < 0.0 {
                continue;
            }
            let fell = if d.stood.tree {
                FALL_SECONDS
            } else {
                UPROOT_SECONDS * 0.5
            };
            if age < fell {
                continue;
            }
            d.landed = true;
            let facing = Vec2::from_angle(f32::from(d.turn) / 256.0 * std::f32::consts::TAU);
            if d.stood.tree {
                let tall = d.height * d.stood.scale;
                let at = d.stood.at + facing * (tall * CROWN_REACH);
                fx.tree_down(Vec3::new(at.x, ground(at), at.y), tall);
            } else {
                let at = d.stood.at;
                fx.bush_up(Vec3::new(at.x, ground(at), at.y));
            }
        }
        self.down.retain(|d| d.age(clock) < d.lasts());
    }

    /// The scenery to draw for what is down, this frame.
    pub fn extras<'a>(
        &'a self,
        ground: impl Fn(Vec2) -> f32 + 'a,
    ) -> impl Iterator<Item = Extra> + 'a {
        let clock = self.clock;
        self.down.iter().map(move |d| {
            // Not yet begun, it still stands (the scenery has given it up).
            let (lean, scale) = match d.age(clock) {
                age if age < 0.0 => (0, d.stood.scale),
                age => d.pose(age),
            };
            let [r, g, b, _] = d.stood.tint;
            Extra {
                model: d.stood.model,
                at: Vec3::new(d.stood.at.x, ground(d.stood.at), d.stood.at.y),
                turn: d.turn,
                scale,
                tint: [r, g, b],
                lean,
            }
        })
    }

    /// Forget everything (a new match).
    pub fn reset(&mut self) {
        *self = Felling::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenery::Model;

    fn tree(at: Vec2) -> Stood {
        Stood {
            model: Model::OakA,
            at,
            turn: 40,
            scale: 1.0,
            tint: [128, 128, 128, 0],
            tree: true,
        }
    }

    #[test]
    fn a_tree_tips_away_from_its_cutter_and_lies_flat() {
        let mut f = Felling::default();
        f.cutting((10, 10), Vec2::new(8.5, 10.5));
        f.clear((10, 10), [tree(Vec2::new(10.5, 10.5))].into_iter(), |_| 4.0);
        // Cut from the west, it goes over toward the east: turn near 0.
        let t = i32::from(f.down[0].turn as i8);
        assert!(t.abs() < 30, "turn {t}");
        let mut fx = effects::Field::new(None);
        let ground = |_: Vec2| 0.0;
        let tip = |f: &Felling| f.extras(ground).next().map(|e| (e.lean, e.scale));
        assert_eq!(tip(&f).unwrap().0, 0);
        f.advance(FALL_SECONDS * 0.5, &mut fx, ground);
        let mid = tip(&f).unwrap().0;
        assert!((1..128).contains(&mid), "{mid}");
        f.advance(FALL_SECONDS, &mut fx, ground);
        let (lean, scale) = tip(&f).unwrap();
        assert!(lean > 240 && scale == 1.0, "{lean} {scale}");
        // Hauled away in the end.
        f.advance(LIE_SECONDS + HAUL_SECONDS, &mut fx, ground);
        assert!(f.extras(ground).next().is_none());
        assert!(f.down.is_empty());
    }

    #[test]
    fn a_bush_is_torn_up_quickly() {
        let mut f = Felling::default();
        let mut bush = tree(Vec2::new(3.5, 3.5));
        bush.tree = false;
        f.clear((3, 3), [bush].into_iter(), |_| 1.0);
        let mut fx = effects::Field::new(None);
        f.advance(UPROOT_SECONDS * 2.0, &mut fx, |_| 0.0);
        assert!(f.down.is_empty());
    }

    #[test]
    fn the_oldest_make_room() {
        let mut f = Felling::default();
        let many = (0..MOST_DOWN + 20).map(|i| tree(Vec2::new(i as f32 * 0.01, 1.0)));
        f.clear((0, 1), many, |_| 3.0);
        assert_eq!(f.down.len(), MOST_DOWN);
    }
}
