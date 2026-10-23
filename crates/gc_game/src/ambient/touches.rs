//! The land's small living touches (docs/34-ambient-life.md, stage 2):
//! pollen drifting over meadows by day, fireflies over wet ground at dusk,
//! leaves coming off old woods, a whirlwind wandering the waste. None of it
//! is in the simulation. Each is made by trying random points on the ground
//! round the camera and spawning where the land's habitat says it belongs,
//! so a settled screen costs a few dozen probes a second and no state.

use gc_map::Map;
use gc_map::grown::Habitat;
use glam::{Vec2, Vec3};

use crate::effects;

/// How far from the camera's focus touches are made at the closest zoom and
/// per tile of camera distance, and the most, in tiles.
const REACH_NEAR: f32 = 14.0;
const REACH_PER_ZOOM: f32 = 0.9;
const REACH_MOST: f32 = 48.0;
/// Specks made a second per tile of fitting ground, at full density.
const POLLEN_RATE: f32 = 0.010;
const FIREFLY_RATE: f32 = 0.006;
const LEAF_RATE: f32 = 0.004;
/// The sun's elevation (its y, normalised) above which pollen drifts and
/// below which fireflies come out, each fading in over `DUSK_SPAN`.
const DUSK_AT: f32 = 0.30;
const DUSK_SPAN: f32 = 0.12;
/// A whirlwind: how often one starts in the waste, a second, how long it
/// lasts and how fast it wanders, in seconds and tiles a second.
const DEVIL_CHANCE: f32 = 0.02;
const DEVIL_SECONDS: f32 = 14.0;
const DEVIL_SPEED: f32 = 2.2;
/// Leaves: how high a tree sheds them.
const LEAF_HEIGHT: [f32; 2] = [2.0, 4.0];

/// Which touches a habitat has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Has {
    pollen: bool,
    firefly: bool,
    leaves: bool,
    waste: bool,
}

fn touches_of(h: Habitat) -> Has {
    use Habitat::*;
    Has {
        pollen: matches!(h, FlowerMeadow | Grassland | Glade | HeatherMoor),
        firefly: matches!(
            h,
            WetMeadow | SedgeTussock | AlderCarr | PeatBog | FernHollow | Glade
        ),
        leaves: matches!(h, OldGrowth | BirchFringe | TreeLine | FernHollow),
        waste: matches!(h, CrackedFlat | BoneDrift | DyingGrass),
    }
}

/// The leaves' colour by habitat: birches go gold, the rest stay green.
fn leaf_colour(h: Habitat) -> [u8; 3] {
    match h {
        Habitat::BirchFringe => [214, 178, 62],
        Habitat::TreeLine => [150, 150, 70],
        _ => [74, 112, 52],
    }
}

/// A whirlwind: where it is, which way it drifts, how long it has left.
#[derive(Clone, Copy, Debug)]
struct Devil {
    at: Vec2,
    dir: Vec2,
    left: f32,
}

#[derive(Default)]
pub struct Touches {
    rng: u32,
    devil: Option<Devil>,
}

impl Touches {
    fn unit(&mut self) -> f32 {
        if self.rng == 0 {
            self.rng = 0x2545_f491;
        }
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// A stochastic count: `n` rounded down, or up with the remainder's
    /// chance.
    fn count(&mut self, n: f32) -> u32 {
        n.floor() as u32 + u32::from(self.unit() < n.fract())
    }

    /// A random point within `reach` tiles of `focus`.
    fn near(&mut self, focus: Vec2, reach: f32) -> Vec2 {
        let a = self.unit() * std::f32::consts::TAU;
        let r = self.unit().sqrt() * reach;
        focus + Vec2::new(a.cos(), a.sin()) * r
    }

    /// Make this frame's touches. `dt` is the effects time since the last
    /// frame (zero while paused), `focus` the camera's focus and `distance`
    /// how far back it is, `sun` the look's sun elevation (its y,
    /// normalised), `density` the Graphics preset's share (zero makes nothing; the recipes do the thinning), `visible` whether
    /// the viewer can see a tile.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        dt: f32,
        focus: Vec2,
        distance: f32,
        sun: f32,
        density: f32,
        map: &Map,
        visible: impl Fn((i32, i32)) -> bool,
        fx: &mut effects::Field,
    ) {
        if dt <= 0.0 || density <= 0.0 || map.habitat.is_empty() {
            return;
        }
        let reach = (REACH_NEAR + distance * REACH_PER_ZOOM).min(REACH_MOST);
        let area = std::f32::consts::PI * reach * reach;
        let day = ((sun - DUSK_AT) / DUSK_SPAN + 0.5).clamp(0.0, 1.0);
        let dusk = ((DUSK_AT - sun) / DUSK_SPAN + 0.5).clamp(0.0, 1.0);
        let tiles = map.tiles as i32;
        let habitat_at = |p: Vec2| {
            let (x, y) = (p.x.floor() as i32, p.y.floor() as i32);
            if !(0..tiles).contains(&x) || !(0..tiles).contains(&y) {
                return None;
            }
            let h = Habitat::from_u8(*map.habitat.get(y as usize * tiles as usize + x as usize)?);
            visible((x, y)).then_some(h)
        };
        // Spawn on the ground (not under water), a little above it.
        let ground = |p: Vec2| map.height_at(p.x, p.y).max(map.water_level());
        let wet_ok = |p: Vec2| map.height_at(p.x, p.y) >= map.water_level();
        for (rate, want) in [
            (POLLEN_RATE * day, 0u8),
            (FIREFLY_RATE * dusk, 1),
            (LEAF_RATE, 2),
        ] {
            // The recipes thin themselves by the density, so trials are not.
            let trials = self.count(rate * area * dt);
            for _ in 0..trials {
                let p = self.near(focus, reach);
                let Some(h) = habitat_at(p) else { continue };
                let has = touches_of(h);
                let at = |up: f32| Vec3::new(p.x, ground(p) + up, p.y);
                match want {
                    0 if has.pollen && wet_ok(p) => {
                        let up = 0.2 + self.unit() * 0.9;
                        fx.pollen(at(up));
                    }
                    1 if has.firefly => {
                        let up = 0.3 + self.unit() * 1.1;
                        fx.firefly(at(up));
                    }
                    2 if has.leaves => {
                        let up = LEAF_HEIGHT[0] + self.unit() * (LEAF_HEIGHT[1] - LEAF_HEIGHT[0]);
                        fx.leaf_fall(at(up), leaf_colour(h));
                    }
                    _ => {}
                }
            }
        }
        self.whirlwind(dt, focus, reach, &habitat_at, &ground, fx);
    }

    /// The whirlwind: one at a time, started in the waste near the camera,
    /// wandering a while and throwing up dust.
    fn whirlwind(
        &mut self,
        dt: f32,
        focus: Vec2,
        reach: f32,
        habitat_at: &impl Fn(Vec2) -> Option<Habitat>,
        ground: &impl Fn(Vec2) -> f32,
        fx: &mut effects::Field,
    ) {
        if self.devil.is_none() && self.unit() < DEVIL_CHANCE * dt {
            let p = self.near(focus, reach);
            if habitat_at(p).is_some_and(|h| touches_of(h).waste) {
                let a = self.unit() * std::f32::consts::TAU;
                self.devil = Some(Devil {
                    at: p,
                    dir: Vec2::new(a.cos(), a.sin()),
                    left: DEVIL_SECONDS,
                });
            }
        }
        let Some(mut d) = self.devil else { return };
        d.at += d.dir * DEVIL_SPEED * dt;
        d.left -= dt;
        // It dies out leaving the waste, going off the map or the camera,
        // or when its time is up.
        let alive = d.left > 0.0
            && d.at.distance(focus) < reach * 1.5
            && habitat_at(d.at).is_some_and(|h| touches_of(h).waste);
        self.devil = alive.then_some(d);
        if alive {
            fx.dust_devil(dt, Vec3::new(d.at.x, ground(d.at), d.at.y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn habitats_have_their_own_touches() {
        assert!(touches_of(Habitat::FlowerMeadow).pollen);
        assert!(touches_of(Habitat::PeatBog).firefly);
        assert!(!touches_of(Habitat::PeatBog).pollen);
        assert!(touches_of(Habitat::OldGrowth).leaves);
        assert!(touches_of(Habitat::CrackedFlat).waste);
        // Rock and scree are bare.
        for h in [Habitat::BareRock, Habitat::Scree] {
            assert_eq!(touches_of(h), Has::default());
        }
    }

    #[test]
    fn counts_average_out_to_the_rate() {
        let mut t = Touches::default();
        let total: u32 = (0..10_000).map(|_| t.count(0.3)).sum();
        assert!((2700..3300).contains(&total), "{total}");
    }

    #[test]
    fn points_stay_within_reach() {
        let mut t = Touches::default();
        let focus = Vec2::new(40.0, 40.0);
        for _ in 0..1000 {
            assert!(t.near(focus, 10.0).distance(focus) <= 10.001);
        }
    }
}
