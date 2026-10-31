//! Debris riding the rivers, as drawn (docs/39-water-engine.md, section
//! 3): each piece the simulation floats is a bare trunk lying on the
//! water along the way it goes (a raft: three lashed side by side; a
//! fallen bridge span: two grey deck timbers), and one aground or jammed
//! sits low on the bed.

use gc_sim::rivers::debris::{Kind, Seen};
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
/// A raft's trunks, how far apart (tiles) and their timber: paler, cut.
const RAFT_TRUNKS: usize = 3;
const RAFT_GAP: f32 = 0.22;
const RAFT_TIMBER: [u8; 3] = [130, 110, 84];
/// A fallen span's deck timbers: two, weathered grey.
const WRECK_TIMBERS: usize = 2;
const WRECK_TIMBER: [u8; 3] = [112, 106, 100];

/// The river's surface at `at` (tiles), on a map drawn with water at
/// `level`: the river's own line over it, raised by the rivers' rise.
pub fn surface(t: &gc_sim::terrain::Terrain, level: f32, at: gc_fixed::FxVec2) -> f32 {
    let line = t.water_line() - i32::from(t.water_level()) + i32::from(t.river_rise(at));
    level + line as f32 / HEIGHT_UNITS as f32
}

/// The trunks to draw for `pieces`, floating at the river's `surface` at
/// a point (tiles, see [`surface`]), with the trunk model `tall` tiles
/// high; only those on tiles `seen`.
pub fn extras<'a>(
    pieces: impl Iterator<Item = Seen> + 'a,
    surface: impl Fn(gc_fixed::FxVec2) -> f32 + 'a,
    tall: f32,
    seen: impl Fn((i32, i32)) -> bool + 'a,
) -> impl Iterator<Item = Extra> + 'a {
    let scale = LOG_LENGTH / tall.max(0.1);
    pieces
        .filter(move |p| seen((p.at.x.floor_int(), p.at.y.floor_int())))
        .flat_map(move |p| {
            let at = crate::from_fx_vec(p.at);
            let way = Some(crate::from_fx_vec(p.way))
                .filter(|w| w.length_squared() > 0.01)
                .map_or(Vec2::X, Vec2::normalize);
            let y = surface(p.at) - if p.grounded { AGROUND } else { 0.0 };
            // Base upstream, so the trunk lies across where the piece is.
            let base = at - way * (LOG_LENGTH / 2.0);
            let turn = turn_of(way);
            // A raft is trunks lashed side by side, a wreck its deck timbers.
            let (trunks, tint) = match p.kind {
                Kind::Raft => (RAFT_TRUNKS, RAFT_TIMBER),
                Kind::Wreck => (WRECK_TIMBERS, WRECK_TIMBER),
                _ => (1, WET_BARK),
            };
            let beside = way.perp() * RAFT_GAP;
            (0..trunks).map(move |k| {
                let off = beside * (k as f32 - (trunks - 1) as f32 / 2.0);
                Extra {
                    model: LOG_MODEL,
                    at: Vec3::new(base.x + off.x, y, base.y + off.y),
                    turn,
                    scale,
                    tint,
                    phase: 0,
                    lean: 255,
                    fall: fall_of(turn, way),
                }
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_fixed::{Fx, FxVec2};

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
        let e: Vec<Extra> = extras([p].into_iter(), |_| 2.0, 7.0, |_| true).collect();
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
        let (afloat, aground) = (seen(at, FxVec2::ZERO, false), seen(at, FxVec2::ZERO, true));
        let y = |p| {
            extras([p].into_iter(), |_| 1.0, 5.0, |_| true)
                .next()
                .unwrap()
                .at
                .y
        };
        assert!(y(aground) < y(afloat));
        assert_eq!(
            extras([afloat].into_iter(), |_| 1.0, 5.0, |_| false).count(),
            0
        );
    }

    #[test]
    fn a_raft_is_three_trunks_side_by_side_across_its_way() {
        let raft = Seen {
            kind: Kind::Raft,
            ..seen(
                FxVec2::from_ints(5, 5),
                FxVec2::new(Fx::ONE, Fx::ZERO),
                false,
            )
        };
        let e: Vec<Extra> = extras([raft].into_iter(), |_| 1.0, 5.0, |_| true).collect();
        assert_eq!(e.len(), RAFT_TRUNKS);
        // Along x, so they stand apart in z, at one x.
        assert!(e.iter().all(|t| (t.at.x - e[0].at.x).abs() < 1e-4));
        assert!((e[2].at.z - e[0].at.z).abs() > RAFT_GAP);
    }
}
