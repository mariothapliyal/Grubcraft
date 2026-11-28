//! Buying back soldiers an enemy holds prisoner (docs/45-prisoners.md).
//!
//! A computer player ransoms a lot only when the regiment was a veteran
//! (rank 1 or more) or it is short of an army, when it holds the price
//! twice over and has supply to spare, and at most once every
//! [`RANSOM_GAP_TICKS`]: cheap levies taken from it are not worth buying
//! back, so a player can't farm it for Grub.

use gc_sim::{Command, TICKS_PER_SECOND};

use super::{Clicks, Steward};
use crate::conquest::Sense;

/// Ticks between two ransoms.
const RANSOM_GAP_TICKS: u32 = 60 * TICKS_PER_SECOND;

impl Steward {
    pub(super) fn ransom(&mut self, s: &Sense, clicks: &mut Clicks) {
        if s.now < self.next_ransom || !clicks.free() {
            return;
        }
        let Some(bank) = s.world.bank(s.party) else {
            return;
        };
        if bank.supply_used >= bank.supply_cap {
            return;
        }
        for h in (s.world.prisoners_held().iter()).filter(|h| h.owner == s.party) {
            if h.rank == 0 && !self.arming {
                continue;
            }
            let Some((grub, marrow, _)) = s.world.ransom_price(s.party, h.captor, h.unit) else {
                continue;
            };
            if bank.grub < 2 * grub || bank.marrow < 2 * marrow {
                continue;
            }
            let order = Command::Ransom {
                from: h.captor,
                unit: h.unit,
            };
            if clicks.give(s.now, order) {
                self.next_ransom = s.now + RANSOM_GAP_TICKS;
            }
            return;
        }
    }
}
