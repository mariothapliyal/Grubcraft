//! Heroes for the Steward (docs/43-heroes.md, docs/18-ai.md "Steward"):
//! once it is at the second tier, with a building that trains soldiers and
//! an army enough, it puts up its faction's hero building by the hall
//! (saving for it); it hires a hero as soon as it may (one a tier), in the
//! order the roster lists them, and revives a fallen one as soon as it can
//! pay, without holding back its buildings and tier to save for either.
//! Heroes march with the army like any regiment, and cast their two
//! abilities (docs/44-hero-abilities.md) by simple rules: the ultimate
//! first, when a foe is near enough (in reach, for one aimed at a place or
//! a regiment), never walking off to cast.

use gc_sim::abilities::{Aim, CastTarget};
use gc_sim::structures::Refusal;
use gc_sim::{Command, TICKS_PER_SECOND};
use glam::Vec2;

use super::super::Sense;
use super::super::kit::KitBuilding;
use super::{Clicks, Look, Steward, trains_fighters};
use crate::perception::{Seen, to_fx_vec};

/// Fighting regiments it has before it puts up its hero building.
const ARMY_FIRST: usize = 3;

/// How near a foe must be for a hero to cast an ability on itself (a rally,
/// a summons, an aura), in tiles.
const FIGHT_TILES: f32 = 8.0;

/// How long after a cast it gives a hero no other, past its reaction time
/// (while the order is on its way, the hero looks ready still).
const CAST_AGAIN_TICKS: u32 = 3 * TICKS_PER_SECOND;

impl Steward {
    /// The hero building to put up, if it is time: none yet, the second
    /// tier, a building that trains soldiers standing and an army enough. Short of
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
        // Not while short of an army, nor before the second tier: a hero
        // building early costs an army its first fight.
        let ready = trains && look.army.len() >= ARMY_FIRST && !self.arming && look.tier >= 2;
        ready.then(|| (b, self.inner_spot(look)))
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

    /// Each of its heroes casts the first of its abilities (the ultimate,
    /// then the basic one) that is ready and has a foe to go at: the
    /// nearest standing foe it sees, within [`FIGHT_TILES`] for one cast on
    /// the hero, within reach for one aimed (and, at a regiment, one the
    /// cast would be taken for: Foreclosure waits for a foe below half
    /// strength).
    pub(super) fn cast_abilities(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let reaction = s.reaction;
        self.cast
            .retain(|&(_, t)| s.now < t + reaction + CAST_AGAIN_TICKS);
        let foes: Vec<&Seen> = (s.view.regiments.iter())
            .filter(|r| s.view.hostile(s.party, r.owner) && !r.broken())
            .collect();
        for me in look.army.iter().filter(|r| !r.broken()) {
            let Some(hero) = s.world.hero_of(me.handle).filter(|h| h.kit.is_some()) else {
                continue;
            };
            if self.cast.iter().any(|&(h, _)| h == me.handle) {
                continue;
            }
            // The nearest foe within `reach` that `fits` (one a cast
            // aimed at it would be taken for).
            let nearest = |reach: f32, fits: &dyn Fn(&Seen) -> bool| {
                (foes.iter())
                    .map(|f| (f, f.pos.distance_squared(me.pos)))
                    .filter(|&(f, d)| d <= reach * reach && fits(f))
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(f, _)| *f)
            };
            for slot in [1, 0] {
                // Locked, cooling or passive: nothing to aim.
                if s.world.can_cast(s.party, me.handle, slot).is_err() {
                    continue;
                }
                let Some(a) = hero.ability(slot) else {
                    continue;
                };
                let reach = f32::from(a.range);
                let target = match a.aim {
                    Aim::Hero => nearest(FIGHT_TILES, &|_| true).map(|_| CastTarget::Hero),
                    Aim::Point => {
                        nearest(reach, &|_| true).map(|f| CastTarget::Point(to_fx_vec(f.pos)))
                    }
                    // Only a foe the cast would be taken for (Foreclosure
                    // wants one below half strength).
                    Aim::Regiment => nearest(reach, &|f| {
                        let t = CastTarget::Regiment(f.handle);
                        s.world.cast_check(s.party, me.handle, slot, t).is_ok()
                    })
                    .map(|f| CastTarget::Regiment(f.handle)),
                };
                let Some(target) = target else {
                    continue;
                };
                if s.world
                    .cast_check(s.party, me.handle, slot, target)
                    .is_err()
                {
                    continue;
                }
                let cast = Command::Cast {
                    regiment: me.handle,
                    ability: slot,
                    target,
                };
                if clicks.give(s.now, cast) {
                    self.cast.push((me.handle, s.now));
                }
                break;
            }
        }
    }
}
