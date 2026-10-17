//! Chords and Resonance (docs/03-factions.md, the Choir's economy): the
//! buildings that link, the triangles three of them make, and the meter
//! those triangles charge.
//!
//! - Two of one owner's finished buildings that link
//!   ([`crate::structures::BuildingStats::links`], the Resonator and
//!   Siphon 14 tiles in the shipped rosters) link when their centres are no
//!   further apart than the shorter link of the two.
//! - Any three mutually linked make a [`Chord`]. A seep a chord covers and
//!   one of the chord owner's buildings tunes yields more: from
//!   [`MIN_HARMONY`] percent for a sliver of a triangle to [`MAX_HARMONY`]
//!   for the largest the links allow (equilateral). A seep takes only its
//!   best chord; chords don't stack.
//! - Each tick a player's Resonance charges by their chords, up to
//!   [`MAX_CHARGING_CHORDS`] of them: [`RESONANCE_TICKS`] chord-ticks a
//!   point, to [`MAX_RESONANCE`].
//!
//! Which buildings link and the chords they make are worked out again only
//! when the linking buildings change; neither is stored or hashed. The
//! Resonance meters are.

use gc_fixed::{Fx, FxVec2};

use crate::structures::{StructureHandle, Structures};
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// At most this many chords are kept, the first found (in slot order).
pub const MAX_CHORDS: usize = 128;
/// A seep in the smallest chord yields this many percent more.
pub const MIN_HARMONY: u32 = 10;
/// A seep in the largest chord the links allow yields this many percent
/// more.
pub const MAX_HARMONY: u32 = 60;
/// The Resonance meter's top.
pub const MAX_RESONANCE: u32 = 100;
/// Chord-ticks to a point of Resonance: a point every 3 seconds for one
/// chord.
pub const RESONANCE_TICKS: u32 = 3 * TICKS_PER_SECOND;
/// Chords past this many charge Resonance no faster.
pub const MAX_CHARGING_CHORDS: u32 = 3;

/// Three of one owner's buildings, mutually linked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    pub owner: PlayerId,
    pub corners: [StructureHandle; 3],
    /// The corners' centres.
    pub at: [FxVec2; 3],
    /// How many percent more a seep it covers yields.
    pub harmony: u8,
}

impl Structures {
    /// Find the chords this tick's linking buildings make.
    pub(crate) fn find_chords(&mut self) {
        self.chords.clear();
        self.chord_count = [0; MAX_PARTIES];
        let n = self.linkers.len();
        for a in 0..n {
            for b in a + 1..n {
                if !self.links_with(a, b) {
                    continue;
                }
                for c in b + 1..n {
                    if self.chords.len() >= MAX_CHORDS {
                        return;
                    }
                    if self.links_with(a, c) && self.links_with(b, c) {
                        self.add_chord([a, b, c]);
                    }
                }
            }
        }
    }

    /// The building slot of this tick's linker `k`.
    fn linker(&self, k: usize) -> usize {
        self.linkers[k].index as usize
    }

    /// Whether this tick's linkers `a` and `b` link: one owner, and
    /// centres within the shorter link of the two.
    fn links_with(&self, a: usize, b: usize) -> bool {
        let (i, j) = (self.linker(a), self.linker(b));
        let reach = self.stats_of(i).links.min(self.stats_of(j).links);
        let reach = u64::from(Fx::from_int(i32::from(reach)).raw().unsigned_abs());
        self.owner_of(i) == self.owner_of(j)
            && (self.centre(i) - self.centre(j)).length_sq_raw() <= reach * reach
    }

    /// Keep the chord linkers `k` make, unless its corners lie in a line.
    fn add_chord(&mut self, k: [usize; 3]) {
        let slots = k.map(|k| self.linker(k));
        let at = slots.map(|i| self.centre(i));
        let twice_area = cross(at[0], at[1], at[2]).unsigned_abs();
        if twice_area == 0 {
            return;
        }
        let reach = slots
            .iter()
            .map(|&i| self.stats_of(i).links)
            .min()
            .unwrap_or(0);
        let owner = self.owner_of(slots[0]);
        self.chords.push(Chord {
            owner,
            corners: k.map(|k| self.linkers[k]),
            at,
            harmony: harmony_of(twice_area, reach) as u8,
        });
        if let Some(count) = self.chord_count.get_mut(usize::from(owner)) {
            *count = count.saturating_add(1);
        }
    }

    /// How many percent more a seep at `point` that `owner` tunes yields:
    /// its best chord of theirs that covers it, or 0.
    pub(crate) fn harmony_at(&self, owner: PlayerId, point: FxVec2) -> u32 {
        (self.chords.iter())
            .filter(|c| c.owner == owner && covers(c.at, point))
            .map(|c| u32::from(c.harmony))
            .max()
            .unwrap_or(0)
    }

    /// Charge each player's Resonance by their chords.
    pub(crate) fn charge_resonance(&mut self) {
        let full = MAX_RESONANCE * RESONANCE_TICKS;
        for p in 0..MAX_PARTIES {
            let chords = u32::from(self.chord_count[p]).min(MAX_CHARGING_CHORDS);
            self.resonance[p] = (self.resonance[p] + chords).min(full);
        }
    }

    /// Player `p`'s Resonance, in points.
    pub fn resonance(&self, p: PlayerId) -> u32 {
        (self.resonance.get(usize::from(p))).map_or(0, |&r| r / RESONANCE_TICKS)
    }

    /// This tick's chords (the first [`MAX_CHORDS`]).
    pub fn chords(&self) -> &[Chord] {
        &self.chords
    }
}

/// Twice the signed area of triangle `a b c`, in raw units squared.
fn cross(a: FxVec2, b: FxVec2, c: FxVec2) -> i128 {
    let (bx, by) = (i128::from((b.x - a.x).raw()), i128::from((b.y - a.y).raw()));
    let (cx, cy) = (i128::from((c.x - a.x).raw()), i128::from((c.y - a.y).raw()));
    bx * cy - by * cx
}

/// Whether `point` lies in triangle `t`, its edges included.
fn covers(t: [FxVec2; 3], point: FxVec2) -> bool {
    let d = [
        cross(t[0], t[1], point),
        cross(t[1], t[2], point),
        cross(t[2], t[0], point),
    ];
    d.iter().all(|&v| v >= 0) || d.iter().all(|&v| v <= 0)
}

/// The harmony of a chord of twice the area `twice_area` (raw units
/// squared) whose shortest link is `reach` tiles: [`MIN_HARMONY`] up to
/// [`MAX_HARMONY`] at the area of an equilateral triangle of side `reach`.
fn harmony_of(twice_area: u128, reach: u8) -> u32 {
    let one = u128::from(Fx::ONE.raw().unsigned_abs());
    // Twice an equilateral triangle's area is √3/2 of its side squared.
    let side = u128::from(reach) * one;
    let most = (side * side * 866 / 1000).max(1);
    let share = twice_area.min(most) * u128::from(MAX_HARMONY - MIN_HARMONY) / most;
    MIN_HARMONY + share as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32) -> FxVec2 {
        FxVec2::new(Fx::from_int(x), Fx::from_int(y))
    }

    #[test]
    fn a_point_on_an_edge_or_inside_is_covered_and_one_outside_is_not() {
        let t = [at(0, 0), at(10, 0), at(0, 10)];
        assert!(covers(t, at(2, 2)));
        assert!(covers(t, at(5, 0)));
        assert!(!covers(t, at(6, 6)));
        let back = [t[0], t[2], t[1]];
        assert!(covers(back, at(2, 2)));
    }

    #[test]
    fn harmony_grows_with_area_up_to_the_largest_triangle() {
        let one = u128::from(Fx::ONE.raw().unsigned_abs());
        let unit = one * one;
        assert_eq!(harmony_of(unit, 14), MIN_HARMONY);
        // An equilateral triangle of side 14: twice its area is about 169.7.
        assert_eq!(harmony_of(170 * unit, 14), MAX_HARMONY);
        assert_eq!(harmony_of(1000 * unit, 14), MAX_HARMONY);
        let half = harmony_of(85 * unit, 14);
        assert!((34..=36).contains(&half), "{half}");
    }
}
