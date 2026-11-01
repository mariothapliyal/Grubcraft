//! Keeping its bridges standing (docs/39-water-engine.md, section 5): a
//! jam against one of its own bridges that grows past
//! [`CLEAR_JAM_ABOVE`] mass, or that has worn the tile under it below
//! [`CLEAR_JAM_BELOW`], gets a worker sent to clear it; a bridge tile hurt
//! below [`MEND_BELOW`] gets one sent to mend it.

use super::{Look, Steward};
use crate::conquest::Sense;
use crate::orders::Hands as Clicks;
use gc_sim::structures::{Role, StructureHandle};
use gc_sim::{Command, TICKS_PER_SECOND};

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
/// before another is sent, should the work not be under way by then.
const SENT_FOR: u32 = 30 * TICKS_PER_SECOND;
/// What it is doing about its bridges.
#[derive(Clone, Debug, Default)]
pub(super) struct Bridgework {
    next_look: u32,
    /// Bridge tiles (by deck) it sent a worker to, and when, for
    /// [`SENT_FOR`]. Kept apart from the steward's `busy`, which reads an
    /// order on a finished building as an upgrade.
    sent: Vec<(StructureHandle, u32)>,
}

impl Steward {
    /// Send a worker to clear a dangerous jam on one of its bridges, else
    /// one to mend a hurt bridge tile. What it decided, for the journal.
    pub(super) fn tend_bridges(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
    ) -> Option<String> {
        if s.now < self.bridgework.next_look || !clicks.free() {
            return None;
        }
        self.bridgework.next_look = s.now + LOOK_EVERY;
        let reaction = s.reaction;
        (self.bridgework.sent).retain(|&(_, t)| s.now < t + reaction + SENT_FOR);
        let bridges: Vec<_> = (look.mine.iter())
            .filter(|m| m.stats.role == Role::Bridge && m.finished())
            .collect();
        if bridges.is_empty() {
            return None;
        }
        let sent = |deck: StructureHandle| self.bridgework.sent.iter().any(|b| b.0 == deck);
        let left = |deck| {
            (bridges.iter())
                .find(|m| m.view.handle == deck)
                .map(|m| m.hp_percent())
        };
        let clearing = |(x, y): (i32, i32)| {
            (look.workers.iter()).any(|w| {
                s.world.clearing_of(w.handle).is_some_and(|c| {
                    let [ax, ay, aw, ad] = c.area;
                    (ax..ax + aw).contains(&x) && (ay..ay + ad).contains(&y)
                })
            })
        };
        let jam = (s.world.flood().rivers().jams())
            .filter(|j| j.owner == s.party && !j.burning)
            .filter(|j| {
                j.mass > CLEAR_JAM_ABOVE || left(j.deck).is_some_and(|l| l < CLEAR_JAM_BELOW)
            })
            .filter(|j| !sent(j.deck) && !clearing(j.at))
            // The worst first: the weakest tile, then the heaviest jam.
            .min_by_key(|j| (left(j.deck).unwrap_or(100), core::cmp::Reverse(j.mass)));
        let (deck, w, order, note) = if let Some(j) = jam {
            let w = self.builder_at(s, look, j.at)?;
            let order = Command::Clear {
                regiment: w,
                tile: (j.at.0 as i16, j.at.1 as i16),
                size: (1, 1),
            };
            let note = format!(
                "Debris jammed on our bridge ({} mass): sending a worker to clear it",
                j.mass
            );
            (j.deck, w, order, note)
        } else {
            // A worker used up to start a building (the Brood's Gnawers)
            // can't mend one.
            if s.kit.worker().is_none_or(|w| w.unit.consumed) {
                return None;
            }
            let m = (bridges.iter())
                .filter(|m| m.hp_percent() < MEND_BELOW)
                .filter(|m| !sent(m.view.handle) && !self.is_busy(m.view.handle))
                .filter(|m| {
                    !(look.workers.iter())
                        .any(|w| s.world.building_target(w.handle) == Some(m.view.handle))
                })
                .min_by_key(|m| m.hp_percent())?;
            let w = self.builder(s, look, m.at())?;
            let order = Command::Construct {
                regiment: w,
                structure: m.view.handle,
            };
            let note = format!(
                "A bridge tile is down to {}%: sending a worker to mend it",
                m.hp_percent()
            );
            (m.view.handle, w, order, note)
        };
        if !clicks.give(s.now, order) {
            return None;
        }
        self.sent.push((w, s.now));
        self.bridgework.sent.push((deck, s.now));
        Some(note)
    }
}
