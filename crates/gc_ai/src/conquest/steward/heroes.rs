//! Heroes for the Steward (docs/43-heroes.md, docs/18-ai.md "Steward"):
//! once it has a building that trains soldiers and Grub to spare, it puts
//! up its faction's hero building by the hall; it hires a hero as soon as
//! it may (one a tier), in the order the roster lists them, and revives a
//! fallen one as soon as it can pay, after its buildings and tier have had
//! their turn to save. Heroes march with the army like any
//! regiment.

use gc_sim::Command;
use gc_sim::structures::Refusal;
use glam::Vec2;

use super::super::Sense;
use super::super::kit::KitBuilding;
use super::{Clicks, Look, Steward, trains_fighters};

/// Fighting regiments it has before it puts up its hero building.
const ARMY_FIRST: usize = 3;

impl Steward {
    /// The hero building to put up, if it is time: none yet, a building
    /// that trains soldiers standing and the start of an army. Short of
    /// its price, the Steward saves for it.
    pub(super) fn hero_hall<'k>(
        &self,
        s: &Sense<'k>,
        look: &Look,
    ) -> Option<(&'k KitBuilding, Vec2)> {
        let b = s.kit.hero_hall()?;
        if look.mine.iter().any(|m| m.view.building == b.id) {
            return None;
        }
        let trains =
            (look.mine.iter()).any(|m| m.finished() && trains_fighters(s, m.view.building));
        (trains && look.army.len() >= ARMY_FIRST).then(|| (b, self.inner_spot(look)))
    }

    /// Hire or revive the first hero it may, at a finished hero building
    /// with room in its queue; short of the price, it saves for it unless
    /// a building or the tier is saved for. The hero building itself is
    /// put up with the others ([`Steward::hero_hall`]); due and short, it
    /// saves for that, as for a hero.
    pub(super) fn hire_heroes(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(bank) = s.world.bank(s.party) else {
            return;
        };
        if let Some((b, _)) = self.hero_hall(s, look) {
            // Short of it, and with nothing else saved for, it saves for
            // the hero building.
            let price = (u32::from(b.stats.cost_grub), u32::from(b.stats.cost_marrow));
            if bank.grub < price.0 || bank.marrow < price.1 {
                self.ledger.saving = price;
            }
            return;
        }
        for m in (look.mine.iter()).filter(|m| m.finished() && !self.is_busy(m.view.handle)) {
            for hero in s.kit.trained_at(s.world, m.view.building) {
                if hero.unit.hero.is_none() {
                    continue;
                }
                match s.world.can_train(s.party, m.view.handle, hero.id) {
                    Ok(()) => {}
                    // Short of the price it waits, without holding back the
                    // buildings and the tier (which save after this).
                    Err(Refusal::Price) => return,
                    Err(_) => continue,
                }
                if bank.supply_used + u32::from(hero.unit.supply) > bank.supply_cap {
                    return;
                }
                if clicks.give(
                    s.now,
                    Command::Train {
                        structure: m.view.handle,
                        unit: hero.id,
                    },
                ) {
                    self.busy.push((m.view.handle, Some(hero.id), s.now));
                }
                return;
            }
        }
    }
}
