//! Point lights from what shines at night. Lit windows and fires report a
//! [`Glow`] each frame they are on (`gc_render::Effects::lamp` and
//! `flames`); this keeps each one lit for a short while after its last
//! report, so a fire that is only stoked every few frames does not blink,
//! picks the nearest ones to the camera and turns them into the renderer's
//! [`PointLight`]s. In daylight there are none, so the shaders pay nothing.
//! Presentation only: nothing here touches the simulation.

use gc_render::{Glow, MAX_LIGHTS, PointLight};
use glam::Vec2;

/// A glow not reported again for this many seconds of effects time goes out,
/// fading over the last of it.
const KEEP: f32 = 0.5;
/// Glows within this many tiles of each other are the one light.
const MERGE: f32 = 0.45;
/// The most glows tracked at once.
const MOST: usize = 512;
/// How strong a light of full strength is: HDR, where about 1.7 is sunlit
/// white paint. Ground and walls are dark and the light falls off with the
/// square of the way to its edge, so a lamp needs this much to show.
const BRIGHTNESS: f32 = 5.0;
/// No lights from a camera higher than this (they are a few pixels across).
const REACH: f32 = 140.0;
/// Below this share of darkness there are no lights at all.
const DARK_ENOUGH: f32 = 0.05;

struct Item {
    glow: Glow,
    /// Seconds since it was last reported.
    quiet: f32,
}

#[derive(Default)]
pub struct Lights {
    items: Vec<Item>,
}

impl Lights {
    /// Forget everything (a new match).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// Glows reported this frame.
    pub fn note(&mut self, glows: impl IntoIterator<Item = Glow>) {
        for glow in glows {
            let near = |i: &Item| {
                Vec2::new(i.glow.pos.x, i.glow.pos.z).distance(Vec2::new(glow.pos.x, glow.pos.z))
                    < MERGE
                    && (i.glow.pos.y - glow.pos.y).abs() < MERGE
            };
            let item = Item { glow, quiet: 0.0 };
            match self.items.iter().position(near) {
                Some(at) => self.items[at] = item,
                None if self.items.len() < MOST => self.items.push(item),
                None => {}
            }
        }
    }

    /// The lights to draw: `dt` seconds of effects time on, the camera at
    /// `focus` and `distance` high, and `darkness` from 0 (day) to 1.
    pub fn frame(&mut self, dt: f32, focus: Vec2, distance: f32, darkness: f32) -> Vec<PointLight> {
        for item in &mut self.items {
            item.quiet += dt.max(0.0);
        }
        self.items.retain(|i| i.quiet < KEEP);
        if darkness < DARK_ENOUGH || distance > REACH {
            return Vec::new();
        }
        let mut lit: Vec<(f32, &Item, f32)> = self
            .items
            .iter()
            .map(|i| {
                let fade = 1.0 - (i.quiet / KEEP).powi(2);
                let level = i.glow.strength * fade * darkness;
                let away = Vec2::new(i.glow.pos.x, i.glow.pos.z).distance_squared(focus);
                (away, i, level)
            })
            .filter(|&(_, _, level)| level > 0.02)
            .collect();
        lit.sort_by(|a, b| a.0.total_cmp(&b.0));
        lit.into_iter()
            .take(MAX_LIGHTS)
            .map(|(_, i, level)| {
                let [r, g, b] = i.glow.color.map(|c| (f32::from(c) / 255.0).powf(2.2));
                let k = level * BRIGHTNESS;
                PointLight {
                    pos: i.glow.pos,
                    radius: i.glow.radius,
                    color: [r * k, g * k, b * k],
                    seed: seed_of(i.glow.pos.x, i.glow.pos.z),
                }
            })
            .collect()
    }
}

/// A steady number for a place, so a light flickers in its own way.
fn seed_of(x: f32, z: f32) -> u8 {
    let h = (x * 73.0).floor() as i32 * 31 + (z * 57.0).floor() as i32 * 17;
    (h & 0xff) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    fn glow(x: f32, z: f32) -> Glow {
        Glow {
            pos: Vec3::new(x, 1.0, z),
            color: [255, 160, 70],
            radius: 3.0,
            strength: 1.0,
        }
    }

    #[test]
    fn there_are_no_lights_by_day() {
        let mut lights = Lights::default();
        lights.note([glow(5.0, 5.0)]);
        assert!(
            lights
                .frame(0.016, Vec2::new(5.0, 5.0), 40.0, 0.0)
                .is_empty()
        );
        assert_eq!(lights.frame(0.016, Vec2::new(5.0, 5.0), 40.0, 1.0).len(), 1);
    }

    #[test]
    fn a_light_stays_a_moment_after_its_last_report_then_fades_out() {
        let mut lights = Lights::default();
        lights.note([glow(5.0, 5.0)]);
        let at = Vec2::new(5.0, 5.0);
        let first = lights.frame(0.0, at, 40.0, 1.0);
        let later = lights.frame(0.3, at, 40.0, 1.0);
        assert_eq!(later.len(), 1);
        assert!(
            later[0].color[0] < first[0].color[0],
            "dimming as it goes quiet"
        );
        assert!(
            lights.frame(0.3, at, 40.0, 1.0).is_empty(),
            "gone after half a second"
        );
    }

    #[test]
    fn a_light_reported_again_is_one_light_not_two() {
        let mut lights = Lights::default();
        for _ in 0..10 {
            lights.note([glow(5.0, 5.0), glow(5.1, 5.0)]);
            lights.frame(0.05, Vec2::ZERO, 40.0, 1.0);
        }
        assert_eq!(lights.items.len(), 1);
        lights.note([glow(9.0, 5.0)]);
        assert_eq!(lights.items.len(), 2);
    }

    #[test]
    fn the_nearest_lights_are_the_ones_sent() {
        let mut lights = Lights::default();
        lights.note((0..100).map(|i| glow(i as f32 * 2.0, 0.0)));
        let sent = lights.frame(0.0, Vec2::new(100.0, 0.0), 40.0, 1.0);
        assert_eq!(sent.len(), MAX_LIGHTS);
        for l in &sent {
            assert!(
                (l.pos.x - 100.0).abs() <= MAX_LIGHTS as f32 + 1.0,
                "{}",
                l.pos.x
            );
        }
    }

    #[test]
    fn lights_are_colour_and_brightness_in_hdr_and_far_cameras_get_none() {
        let mut lights = Lights::default();
        lights.note([glow(5.0, 5.0)]);
        let sent = lights.frame(0.0, Vec2::new(5.0, 5.0), 40.0, 1.0);
        let c = sent[0].color;
        assert!(c[0] > c[1] && c[1] > c[2] && c[2] > 0.0, "{c:?}");
        assert!(
            c[0] > 1.0,
            "a lamp is brighter than sunlit paint's half: {c:?}"
        );
        assert!(lights.frame(0.0, Vec2::ZERO, 200.0, 1.0).is_empty());
    }

    #[test]
    fn a_new_match_starts_dark() {
        let mut lights = Lights::default();
        lights.note([glow(5.0, 5.0)]);
        lights.clear();
        assert!(lights.frame(0.0, Vec2::ZERO, 40.0, 1.0).is_empty());
    }
}
