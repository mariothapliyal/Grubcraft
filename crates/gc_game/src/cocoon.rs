//! A Brood regiment in its cocoon (docs/03-factions.md, "Metamorphosis in
//! the field"), as the field draws it: each soldier is swapped for a pale,
//! glossy sac that swells and sinks slowly, a wide low box with a narrower
//! one on top, so a regiment reads at once as a field of cocoons.
//!
//! Only a picture: the simulation knows the cocoon (`World::cocoon_of`),
//! and nothing here can change a match or a replay.

use std::f32::consts::TAU;

use gc_render::BoxInstance;
use glam::Vec2;

/// The sac's silk, with a little of the team's colour worked in.
const SILK: [u8; 3] = [228, 232, 205];
/// How much of the colour is the team's, out of 255.
const TEAM_SHARE: u16 = 40;
/// How fast a sac swells and sinks, in radians a second, and how far
/// (a share of its size).
const PULSE_RATE: f32 = 2.4;
const PULSE_DEPTH: f32 = 0.07;
/// The sac's two parts: the fat belly and the narrower crown above it, as
/// shares of the height and the width.
const BELLY: (f32, f32) = (0.58, 1.0);
const CROWN: (f32, f32) = (0.5, 0.62);

/// The silk of a cocoon for a team of colour `team`.
pub fn silk(team: [u8; 3]) -> [u8; 3] {
    std::array::from_fn(|i| {
        ((u16::from(team[i]) * TEAM_SHARE + u16::from(SILK[i]) * (255 - TEAM_SHARE)) / 255) as u8
    })
}

/// The soldier at `at` facing `facing` (`lift` off the ground, `phase` his
/// number in [0, 1) so the sacs don't breathe in step) as a sac `width`
/// wide and `height` tall at `time` seconds, `highlight` the selection's
/// glow in the colour's alpha.
pub fn sac(
    (at, facing): (Vec2, Vec2),
    lift: f32,
    phase: f32,
    time: f32,
    (width, height): (f32, f32),
    team: [u8; 3],
    highlight: u8,
) -> [BoxInstance; 2] {
    let pulse = 1.0 + PULSE_DEPTH * (time * PULSE_RATE + phase * TAU).sin();
    let [r, g, b] = silk(team);
    let part = |(h, w): (f32, f32), base: f32| BoxInstance {
        pos: at.to_array(),
        facing: facing.to_array(),
        size: [width * w * pulse, height * h, width * w * pulse * 0.9],
        color: [r, g, b, highlight],
        lift: lift + base,
    };
    [part(BELLY, 0.0), part(CROWN, height * BELLY.0 * 0.85)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(phase: f32, time: f32) -> [BoxInstance; 2] {
        sac(
            (Vec2::new(5.0, 7.0), Vec2::X),
            0.0,
            phase,
            time,
            (0.5, 0.7),
            [200, 40, 40],
            0,
        )
    }

    #[test]
    fn a_sac_is_pale_with_a_trace_of_its_team() {
        let [r, g, b] = silk([200, 40, 40]);
        assert!(r > 200 && g > 190 && b > 170, "pale: {r} {g} {b}");
        assert!(r > g, "the team's red shows through");
        assert_eq!(silk(SILK), SILK);
    }

    #[test]
    fn a_sac_stands_on_the_ground_with_a_narrower_crown_over_its_belly() {
        let [belly, crown] = at(0.0, 0.0);
        assert_eq!(belly.pos, [5.0, 7.0]);
        assert_eq!(crown.pos, belly.pos);
        assert_eq!(belly.lift, 0.0);
        assert!(crown.lift > 0.0 && crown.lift < belly.size[1]);
        assert!(crown.size[0] < belly.size[0] && crown.size[2] < belly.size[2]);
        assert_eq!(belly.color, crown.color);
        // The top is no higher than the soldier it replaces.
        assert!(crown.lift + crown.size[1] <= 0.7 * 1.1);
    }

    #[test]
    fn sacs_swell_and_sink_out_of_step_within_bounds() {
        let widths: Vec<f32> = (0..40)
            .map(|k| at(0.0, k as f32 * 0.1)[0].size[0])
            .collect();
        let (lo, hi) = widths
            .iter()
            .fold((f32::MAX, f32::MIN), |(lo, hi), &w| (lo.min(w), hi.max(w)));
        assert!(hi > lo, "it breathes");
        assert!(lo >= 0.5 * (1.0 - PULSE_DEPTH) - 1e-4 && hi <= 0.5 * (1.0 + PULSE_DEPTH) + 1e-4);
        assert_ne!(at(0.0, 0.3)[0].size, at(0.5, 0.3)[0].size, "not in step");
    }

    #[test]
    fn the_selection_glow_rides_in_the_colour_alpha() {
        let lit = sac((Vec2::ZERO, Vec2::X), 0.0, 0.0, 0.0, (0.5, 0.7), [0; 3], 70);
        assert!(lit.iter().all(|b| b.color[3] == 70));
    }
}
