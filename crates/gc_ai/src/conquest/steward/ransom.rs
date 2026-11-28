//! Buying back soldiers an enemy holds prisoner, and recruiting those it
//! holds (docs/45-prisoners.md).
//!
//! A computer player ransoms a lot only when the regiment was a veteran
//! (rank 1 or more) or it is short of an army, when it holds the price
//! twice over and has supply to spare, and at most once every
//! [`RANSOM_GAP_TICKS`]: cheap levies taken from it are not worth buying
//! back, so a player can't farm it for Grub. Short of an army, with
//! nothing to ransom, it recruits the prisoners it holds on the same
//! terms. It never puts prisoners to death: no quarter would cost it more
//! than it gains.

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
        let held = s.world.prisoners_held();
        for h in held.iter().filter(|h| h.owner == s.party) {
            // A ransom buys back the oldest lot of that unit from that
            // captor, so judge only that one.
            let first = held
                .iter()
                .find(|g| g.owner == h.owner && g.captor == h.captor && g.unit == h.unit);
            if !first.is_some_and(|g| std::ptr::eq(g, h))
                || (h.rank == 0 && !self.arming)
                || !s.world.can_take_back(s.party, h.unit)
            {
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
        // Short of an army, it takes the prisoners it holds into it.
        if !self.arming {
            return;
        }
        for h in held.iter().filter(|h| h.captor == s.party) {
            let first = held
                .iter()
                .find(|g| g.captor == h.captor && g.owner == h.owner && g.unit == h.unit);
            if !first.is_some_and(|g| std::ptr::eq(g, h)) || !s.world.room_for(s.party, h.unit) {
                continue;
            }
            let Some((grub, marrow, _)) = s.world.recruit_price(s.party, h.owner, h.unit) else {
                continue;
            };
            if bank.grub < 2 * grub || bank.marrow < 2 * marrow {
                continue;
            }
            let order = Command::Recruit {
                from: h.owner,
                unit: h.unit,
            };
            if clicks.give(s.now, order) {
                self.next_ransom = s.now + RANSOM_GAP_TICKS;
            }
            return;
        }
    }
}
