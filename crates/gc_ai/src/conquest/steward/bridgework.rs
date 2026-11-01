//! Keeping its bridges standing (docs/39-water-engine.md, section 5): a
//! jam against one of its own bridges that grows past
//! [`CLEAR_JAM_ABOVE`] mass, or that has worn the tile under it below
//! [`CLEAR_JAM_BELOW`], gets a worker sent to clear it; a bridge tile hurt
//! below [`MEND_BELOW`] gets one sent to mend it.

use gc_sim::structures::{Role, StructureHandle};
use gc_sim::{Command, TICKS_PER_SECOND};
use glam::Vec2;

use super::{Look, Steward};
use crate::conquest::Sense;
use crate::orders::Hands as Clicks;

/// How often it looks at its bridges.
const LOOK_EVERY: u32 = 5 * TICKS_PER_SECOND;
/// A jam heavier than this is cleared (a natural one of 3 or less washes
/// free when a flood falls).
pub const CLEAR_JAM_ABOVE: u8 = 3;
/// A jam on a tile with less than this of its hit points left, percent, is
/// cleared however light.
pub const CLEAR_JAM_BELOW: u32 = 60;
/// A bridge tile with less than this of its hit points, percent, is mended.
pub const MEND_BELOW: u32 = 75;
/// How long a worker sent to a jam or a hurt tile is given to get there
/// before another is sent.
const SENT_FOR: u32 = 30 * TICKS_PER_SECOND;

/// What it is doing about its bridges.
#[derive(Clone, Debug, Default)]
pub(super) struct Bridgework {
    next_look: u32,
    /// Jams (by the tile they lie on) and tiles a worker was sent to, and
    /// when.
    sent: Vec<((i32, i32), u32)>,
    /// What it decided, for the journal.
    pub(super) notes: Vec<String>,
}

impl Steward {
    /// Send a worker to clear a dangerous jam on one of its bridges, else
    /// one to mend a hurt bridge tile.
    pub(super) fn tend_bridges(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        if s.now < self.bridgework.next_look {
            return;
        }
        self.bridgework.next_look = s.now + LOOK_EVERY;
        let reaction = s.reaction;
        (self.bridgework.sent).retain(|&(_, t)| s.now < t + reaction + SENT_FOR);
        let bridges: Vec<_> = (look.mine.iter())
            .filter(|m| m.stats.role == Role::Bridge && m.finished())
            .collect();
        if bridges.is_empty() || !clicks.free() {
            return;
        }
        let left = |deck: StructureHandle| {
            (bridges.iter())
                .find(|m| m.view.handle == deck)
                .map(|m| m.view.hp * 100 / m.view.max_hp.max(1))
        };
        let jam = (s.world.flood().rivers().jams())
            .filter(|j| j.owner == s.party && !j.burning)
            .filter(|j| {
                j.mass > CLEAR_JAM_ABOVE || left(j.deck).is_some_and(|l| l < CLEAR_JAM_BELOW)
            })
            .find(|j| !self.sent_to(j.at));
        if let Some(j) = jam {
            let at = Vec2::new(j.at.0 as f32 + 0.5, j.at.1 as f32 + 0.5);
            let Some(w) = self.builder(s, look, at) else {
                return;
            };
            let order = Command::Clear {
                regiment: w,
                tile: (j.at.0 as i16, j.at.1 as i16),
                size: (1, 1),
            };
            if clicks.give(s.now, order) {
                self.sent.push((w, s.now));
                self.bridgework.sent.push((j.at, s.now));
                self.bridgework.notes.push(format!(
                    "Debris jammed on our bridge ({} mass): sending a worker to clear it",
                    j.mass
                ));
            }
            return;
        }
        let hurt = (bridges.iter())
            .filter(|m| m.view.hp * 100 < MEND_BELOW * m.view.max_hp)
            .filter(|m| !self.is_busy(m.view.handle) && !self.sent_to(m.view.tile))
            .find(|m| {
                !(look.workers.iter())
                    .any(|w| s.world.building_target(w.handle) == Some(m.view.handle))
            });
        let Some(m) = hurt else {
            return;
        };
        let Some(w) = self.builder(s, look, m.at()) else {
            return;
        };
        let order = Command::Construct {
            regiment: w,
            structure: m.view.handle,
        };
        if clicks.give(s.now, order) {
            self.sent.push((w, s.now));
            self.bridgework.sent.push((m.view.tile, s.now));
            self.bridgework.notes.push(format!(
                "A bridge tile is down to {}%: sending a worker to mend it",
                m.view.hp * 100 / m.view.max_hp.max(1)
            ));
        }
    }

    /// Whether a worker was sent to the jam or bridge tile at `at` lately.
    fn sent_to(&self, at: (i32, i32)) -> bool {
        self.bridgework.sent.iter().any(|&(t, _)| t == at)
    }

    /// What it decided about its bridges since last asked, for the journal.
    pub fn take_notes(&mut self) -> Vec<String> {
        core::mem::take(&mut self.bridgework.notes)
    }
}
