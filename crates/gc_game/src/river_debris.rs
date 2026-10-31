//! Debris riding the rivers, as drawn (docs/39-water-engine.md, section
//! 3): each piece the simulation floats is a bare trunk lying on the
//! water along the way it goes, and one aground sits low on the bed.

use gc_sim::rivers::debris::Seen;
use gc_sim::terrain::HEIGHT_UNITS;
use glam::{Vec2, Vec3};

use crate::scenery::{Extra, Model, fall_of, turn_of};

/// The trunk a log is drawn with.
pub const LOG_MODEL: Model = Model::Snag;
/// How long a log is drawn, in tiles.
pub const LOG_LENGTH: f32 = 1.4;
/// How much lower a piece aground sits than one afloat, in tiles.
const AGROUND: f32 = 0.08;
/// Wet bark: darker than the trees on the bank (the shader doubles a
/// tint, so 128 is as made).
const WET_BARK: [u8; 3] = [92, 84, 76];

/// The trunks to draw for `pieces`, the still water drawn at `water`
/// (tiles) and the rivers raising it by `rise` (height units, see
/// `Terrain::river_rise`), with the trunk model `tall` tiles high; only
/// those on tiles `seen`.
pub fn extras<'a>(
    pieces: impl Iterator<Item = Seen> + 'a,
    rise: impl Fn(gc_fixed::FxVec2) -> u16 + 'a,
    water: f32,
    tall: f32,
    seen: impl Fn((i32, i32)) -> bool + 'a,
) -> impl Iterator<Item = Extra> + 'a {
    let scale = LOG_LENGTH / tall.max(0.1);
    pieces.filter_map(move |p| {
        let tile = (p.at.x.floor_int(), p.at.y.floor_int());
        if !seen(tile) {
            return None;
        }
        let at = to_vec(p.at);
        let way = Some(to_vec(p.way))
            .filter(|w| w.length_squared() > 0.01)
            .map_or(Vec2::X, Vec2::normalize);
        let rise = f32::from(rise(p.at)) / HEIGHT_UNITS as f32;
        let y = water + rise - if p.grounded { AGROUND } else { 0.0 };
        // Base upstream, so the trunk lies across where the piece is.
        let base = at - way * (LOG_LENGTH / 2.0);
        let turn = turn_of(way);
        Some(Extra {
            model: LOG_MODEL,
            at: Vec3::new(base.x, y, base.y),
            turn,
            scale,
            tint: WET_BARK,
            phase: 0,
            lean: 255,
            fall: fall_of(turn, way),
        })
    })
}

fn to_vec(v: gc_fixed::FxVec2) -> Vec2 {
    Vec2::new(v.x.raw() as f32, v.y.raw() as f32) / 65536.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::{Fx, FxVec2};
    use gc_sim::rivers::debris::Kind;

    fn seen(at: FxVec2, way: FxVec2, grounded: bool) -> Seen {
        Seen {
            kind: Kind::Log,
            at,
            way,
            grounded,
        }
    }

    #[test]
    fn a_log_lies_flat_along_the_way_it_goes_centred_on_its_place() {
        let at = FxVec2::new(Fx::from_int(10), Fx::from_int(4));
        let p = seen(at, FxVec2::new(Fx::ONE, Fx::ZERO), false);
        let e: Vec<Extra> = extras([p].into_iter(), |_| 0, 2.0, 7.0, |_| true).collect();
        assert_eq!(e.len(), 1);
        let e = e[0];
        assert_eq!(e.lean, 255);
        assert!((e.at.x - (10.0 - LOG_LENGTH / 2.0)).abs() < 1e-4);
        assert!((e.at.z - 4.0).abs() < 1e-4 && (e.at.y - 2.0).abs() < 1e-4);
        assert!((e.scale * 7.0 - LOG_LENGTH).abs() < 1e-4);
    }

    #[test]
    fn one_aground_sits_lower_and_unseen_ones_are_not_drawn() {
        let at = FxVec2::from_ints(3, 3);
        let (afloat, aground) = (
            seen(at, FxVec2::ZERO, false),
            seen(at, FxVec2::ZERO, true),
        );
        let y = |p| extras([p].into_iter(), |_| 0, 1.0, 5.0, |_| true).next().unwrap().at.y;
        assert!(y(aground) < y(afloat));
        assert_eq!(extras([afloat].into_iter(), |_| 0, 1.0, 5.0, |_| false).count(), 0);
    }
}
