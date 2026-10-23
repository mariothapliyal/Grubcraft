//! The battlefield's particle effects: where the game asks `gc_render`'s
//! recipes for dust, sparks and smoke. It only collects what to spawn; the
//! frame hands the list to the renderer (`Field::frame`). Nothing here
//! feeds back into the simulation, and effects of what the viewer cannot see
//! are never made, so the dust never shows an army the fog hides.

use gc_render::{DUST, Effects, HitStyle, Particle};
use glam::{Vec2, Vec3};

/// How often a soldier on the move kicks up dust, a puff a second at a
/// walk, more at a run.
const FOOTSTEP_RATE: f32 = 0.25;
const FOOTSTEP_RATE_PER_SPEED: f32 = 0.3;
/// Seconds (of effects time) between the demo's one-shot effects, and how
/// slowly the demo plays so a spark can be seen: a quarter speed.
const DEMO_EVERY: f32 = 1.5;
const DEMO_SPEED: f32 = 0.25;

pub struct Field {
    fx: Effects,
    demo: Option<Demo>,
}

/// `--fx-demo`: every effect, in a row at the camera's focus. With `still`
/// (`--fx-still`) time stands still and the whole gallery is set out afresh
/// every frame, each effect at several ages, for a picture.
struct Demo {
    clock: f32,
    next: f32,
    still: bool,
}

impl Field {
    pub fn new(demo: bool, still: bool) -> Field {
        Field {
            fx: Effects::default(),
            demo: (demo || still).then_some(Demo {
                clock: 0.0,
                next: 0.0,
                still,
            }),
        }
    }

    /// Whether the renderer should forget its particles before each frame's
    /// (the still gallery is spawned afresh every frame).
    pub fn still(&self) -> bool {
        self.demo.as_ref().is_some_and(|d| d.still)
    }

    /// Drop what was collected and not yet handed over.
    pub fn clear(&mut self) {
        self.fx.take();
    }

    /// A soldier at `pos` moved along `dir` at `speed` tiles a second for
    /// `dt` seconds of game time.
    pub fn footstep(&mut self, dt: f32, pos: Vec3, dir: Vec2, speed: f32) {
        if speed > 0.5
            && self
                .fx
                .chance(dt * (FOOTSTEP_RATE + FOOTSTEP_RATE_PER_SPEED * speed))
        {
            self.fx.footstep(pos, dir, speed, DUST);
        }
    }

    /// A volley's arrows came down at `pos`: a puff of dust and a few clods.
    pub fn arrows_landed(&mut self, pos: Vec3, arrows: u16) {
        let n = f32::from(arrows.clamp(1, 12));
        self.fx.dust_burst(pos, 0.5 + 0.08 * n, 2.0 + 0.5 * n, DUST);
        self.fx.debris(pos, [92, 74, 52], 1.5 + 0.3 * n, 0.5);
    }

    /// Two regiments met at `pos` with a charge: a roll of dust and a few
    /// clods thrown up.
    pub fn charge_hit(&mut self, pos: Vec3) {
        self.fx.dust_burst(pos, 2.2, 16.0, DUST);
        self.fx.debris(pos, [92, 74, 52], 8.0, 1.0);
    }

    /// How fast effects run against game time: 1, or a quarter for the
    /// demo, which is for looking at.
    pub fn time_scale(&self) -> f32 {
        match &self.demo {
            Some(d) if d.still => 0.0,
            Some(_) => DEMO_SPEED,
            None => 1.0,
        }
    }

    /// Everything collected this frame, ready for
    /// `Renderer::spawn_particles`. `dt` is the effects time since the last
    /// frame (the frame time, scaled by game speed and `time_scale`), `focus` the
    /// camera's focus (where the demo plays), `ground` the terrain height,
    /// and `density` how much to spawn (the Graphics preset's choice).
    pub fn frame(
        &mut self,
        dt: f32,
        density: f32,
        focus: Vec2,
        ground: impl Fn(Vec2) -> f32,
    ) -> Vec<Particle> {
        self.fx.set_density(density);
        if let Some(demo) = &mut self.demo {
            demo.clock += dt;
            let burst = demo.clock >= demo.next;
            if burst {
                demo.next = demo.clock + DEMO_EVERY;
            }
            if demo.still {
                play_still(&mut self.fx, focus, &ground);
            } else {
                play_demo(&mut self.fx, dt, burst, focus, &ground);
            }
        }
        self.fx.take()
    }
}

/// The demo: twelve stations 1.3 tiles apart along x. Fires, smoke and
/// motes run all the time; the rest go off together every few seconds.
fn play_demo(fx: &mut Effects, dt: f32, burst: bool, focus: Vec2, ground: &impl Fn(Vec2) -> f32) {
    let at = |i: usize, dy: f32| {
        let p = focus + Vec2::new((i as f32 - 5.5) * 1.3, 0.0);
        Vec3::new(p.x, ground(p) + dy, p.y)
    };
    // Running fires.
    fx.smoke(at(2, 0.0), 1.0, 0.85, dt);
    fx.flames(at(2, 0.0), 1.0, dt);
    fx.embers(at(2, 0.3), 0.5, 6.0 * dt);
    fx.smoke(at(11, 0.0), 1.6, 0.3, dt);
    fx.motes(at(10, 0.0), 0.8, [150, 235, 235], 6.0 * dt);
    fx.embers(at(3, 0.0), 0.7, 14.0 * dt);
    if !burst {
        return;
    }
    // Three rows, the nearer part-way through its life, so a still shows
    // each effect early, middle and late.
    for (row, age) in [(-1.6, 0.9), (0.0, 0.35), (1.6, 0.1)] {
        let mark = fx.mark();
        one_shots(fx, |i, dy| at(i, dy) + Vec3::new(0.0, 0.0, row));
        fx.age_since(mark, age);
    }
}

/// The effects that go off once, at stations 0 and 1 and 4 to 9.
fn one_shots(fx: &mut Effects, at: impl Fn(usize, f32) -> Vec3) {
    // A column of soldiers marching past: eight steps along a line.
    for k in 0..8 {
        let mut p = at(0, 0.0);
        p.x += k as f32 * 0.2 - 0.7;
        fx.footstep(p, Vec2::X, 3.0, DUST);
    }
    fx.dust_burst(at(1, 0.0), 1.4, 14.0, DUST);
    let (chest, dir) = (0.7, Vec3::new(1.0, 0.3, 0.0));
    fx.hit(at(4, chest), dir, HitStyle::Flesh, true, 1.0);
    fx.hit(at(5, chest), dir, HitStyle::Ichor, false, 1.0);
    fx.hit(at(6, chest), dir, HitStyle::Crystal, false, 1.0);
    fx.hit(at(7, chest), dir, HitStyle::Ghost, false, 1.0);
    fx.hit(at(8, chest), dir, HitStyle::Ember, true, 1.0);
    fx.debris(at(9, 0.2), [110, 104, 96], 14.0, 1.0);
}

/// The still gallery: the one-shots at four ages in four rows, and the
/// running effects as a stream already flowing, behind them.
fn play_still(fx: &mut Effects, focus: Vec2, ground: &impl Fn(Vec2) -> f32) {
    let at = |i: usize, row: f32, dy: f32| {
        let p = focus + Vec2::new((i as f32 - 5.5) * 1.3, row);
        Vec3::new(p.x, ground(p) + dy, p.y)
    };
    for (row, age) in [(-1.2, 0.12), (0.0, 0.35), (1.2, 0.8), (2.4, 1.5)] {
        let mark = fx.mark();
        one_shots(fx, |i, dy| at(i, row, dy));
        fx.age_since(mark, age);
    }
    // Steady streams: a few seconds of each, aged one step apart.
    for k in 0..40 {
        let mark = fx.mark();
        let step = 0.1;
        fx.smoke(at(2, -3.0, 0.0), 1.0, 0.85, step);
        fx.flames(at(2, -3.0, 0.0), 1.0, step);
        fx.embers(at(3, -3.0, 0.0), 0.7, 14.0 * step);
        fx.motes(at(10, -3.0, 0.0), 0.8, [150, 235, 235], 8.0 * step);
        fx.smoke(at(7, -3.0, 0.0), 1.6, 0.3, step);
        fx.age_since(mark, k as f32 * step);
    }
}
