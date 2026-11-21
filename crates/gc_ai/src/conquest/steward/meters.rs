//! The faction rules the Steward plays besides its economy: the Brood's
//! evolutions and the Concord's Reliquaries and contracts
//! (docs/18-ai.md, "Steward").
//!
//! - **Evolutions.** From tier 2 it grows a Chrysalis Chamber once it has
//!   two regiments that could evolve, then cocoons each such regiment
//!   standing out of the fight near home or the rally point (no enemy
//!   regiment within [`SAFE`] tiles), into the form its army is shortest
//!   of (the one with long weapons against riders or monsters).
//! - **Reliquaries.** One by its hall once it has somewhere to train
//!   soldiers, one by the rally point, and one towards the building its
//!   army marches on, so its dead are refilled and don't crumble on the
//!   way, and the deaths there pay Tithe.
//! - **Contracts.** Raise the Bound at the fullest grave within
//!   [`FIGHT_NEAR`] of one of its regiments locked in a fight; Indenture
//!   at a grave within its keepers' reach with no enemy near; Hire
//!   Militia from the hamlet nearest home while an enemy army is at its
//!   buildings.

use gc_sim::tithe::{KEEP_TILES, MIN_RAISED};
use gc_sim::{Command, Meter, PowerEffect, PowerTarget, UnitId};
use glam::Vec2;

use super::super::Sense;
use super::super::kit::KitBuilding;
use super::super::spymaster;
use super::{Clicks, Look, Steward, class_slot};
use crate::perception::{to_fx_vec, v2};

/// How far from its regiments an enemy keeps them from cocooning, and
/// keeps an Indenture from being cast, in tiles.
const SAFE: f32 = 25.0;
/// How near home or the rally point a regiment cocoons, in tiles.
const HOME: f32 = 30.0;
/// How near one of its regiments in a fight a grave is raised, in tiles.
const FIGHT_NEAR: f32 = 15.0;
/// The fewest corpses it raises the Bound from, and indentures.
const RAISE_FROM: u32 = 8;
const INDENTURE_FROM: u32 = 12;
/// How near its buildings an enemy army makes it hire militia, in tiles.
const AT_HOME: f32 = 25.0;
/// How far from home a hamlet it hires from may be, in tiles.
const HAMLET_REACH: u8 = 40;
/// How far back from the building its army marches on the forward
/// Reliquary goes, in tiles, and how near one already standing makes
/// another needless.
const FORWARD_BACK: f32 = 22.0;
const RELIQUARY_SPACING: f32 = 20.0;

/// The evolution for a regiment whose evolutions are `options`: with
/// riders or monsters about, one with long weapons; else the one of the
/// class its army is shortest of against `mix`, its army being
/// `by_class`.
pub(super) fn pick_evolution(
    options: &[(UnitId, usize, bool)],
    heavy_foes: bool,
    mix: [f32; 3],
    by_class: [usize; 3],
) -> Option<UnitId> {
    if heavy_foes && let Some(&(id, ..)) = options.iter().find(|o| o.2) {
        return Some(id);
    }
    let total = by_class.iter().sum::<usize>().max(1) as f32;
    let deficit = |slot: usize| mix[slot] - by_class[slot] as f32 / total;
    options
        .iter()
        .max_by(|a, b| deficit(a.1).total_cmp(&deficit(b.1)).then(b.0.cmp(&a.0)))
        .map(|o| o.0)
}

impl Steward {
    /// The Chrysalis Chamber to grow, if it is time: tier 2, none yet,
    /// two regiments that could evolve and Grub to spare.
    pub(super) fn chamber<'k>(
        &self,
        s: &Sense<'k>,
        look: &Look,
    ) -> Option<(&'k KitBuilding, Vec2)> {
        let b = s.kit.chamber()?;
        if look.tier < 2 || look.mine.iter().any(|m| m.view.building == b.id) {
            return None;
        }
        let can_evolve = (look.army.iter())
            .filter(|r| s.kit.evolutions(r.unit).next().is_some())
            .count();
        let bank = s.world.bank(s.party)?;
        (can_evolve >= 2 && bank.grub >= u32::from(b.stats.cost_grub) + 100)
            .then(|| (b, self.inner_spot(look)))
    }

    /// A Reliquary to put up, if one is wanted: by the hall once it has a
    /// building that trains soldiers, by the rally point, and back from
    /// `front`, the building its army marches on.
    pub(super) fn reliquary<'k>(
        &self,
        s: &Sense<'k>,
        look: &Look,
        rally: Option<Vec2>,
        front: Option<Vec2>,
    ) -> Option<(&'k KitBuilding, Vec2)> {
        let b = s.kit.reliquary()?;
        let home = self.home?;
        let reliquaries: Vec<Vec2> = (look.mine.iter())
            .filter(|m| m.view.building == b.id)
            .map(super::Mine::at)
            .collect();
        let bare = |at: Vec2| {
            reliquaries
                .iter()
                .all(|r| r.distance(at) > RELIQUARY_SPACING)
        };
        let producing = look
            .mine
            .iter()
            .any(|m| super::trains_fighters(s, m.view.building));
        if !producing {
            return None;
        }
        if bare(home) {
            return Some((b, self.inner_spot(look)));
        }
        if let Some(at) = rally.filter(|&at| bare(at)) {
            return Some((b, at));
        }
        let front = front?;
        let back = (home - front).normalize_or(Vec2::ZERO) * FORWARD_BACK;
        let at = front + back;
        (bare(at) && at.distance(home) > KEEP_TILES as f32).then_some((b, at))
    }

    /// Cocoon every regiment that can evolve, out of danger near home or
    /// the rally point, into the form its army is shortest of.
    pub(super) fn evolve(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        rally: Option<Vec2>,
    ) {
        let Some(chamber) = s.kit.chamber() else {
            return;
        };
        let grown = (look.mine.iter()).any(|m| m.view.building == chamber.id && m.finished());
        if look.tier < 2 || !grown {
            return;
        }
        let near_home = |p: Vec2| {
            self.home.is_some_and(|h| h.distance(p) <= HOME)
                || rally.is_some_and(|at| at.distance(p) <= HOME)
        };
        let mut idle = (look.army.iter())
            .filter(|r| !r.engaged && !r.moving && near_home(r.pos))
            .filter(|r| s.kit.evolutions(r.unit).next().is_some())
            .peekable();
        if idle.peek().is_none() {
            return;
        }
        let mut by_class = [0usize; 3];
        for r in &look.army {
            if let Some(info) = s.view.units.get(usize::from(r.unit)) {
                by_class[class_slot(info.class)] += 1;
            }
        }
        let mix = self.mix(s);
        let heavy = spymaster::heavy_foes(s);
        let foes: Vec<Vec2> = spymaster::enemy_army(s).map(|r| r.pos).collect();
        for r in idle {
            if foes.iter().any(|f| f.distance(r.pos) <= SAFE) {
                continue;
            }
            let options: Vec<(UnitId, usize, bool)> = s
                .kit
                .evolutions(r.unit)
                .filter(|u| s.world.can_evolve(s.party, r.handle, u.id).is_ok())
                .filter_map(|u| {
                    let info = s.view.units.get(usize::from(u.id))?;
                    Some((u.id, class_slot(info.class), info.stats.reach))
                })
                .collect();
            let Some(into) = pick_evolution(&options, heavy, mix, by_class) else {
                continue;
            };
            if !clicks.give(
                s.now,
                Command::Evolve {
                    regiment: r.handle,
                    into,
                },
            ) {
                return;
            }
            if let Some(info) = s.view.units.get(usize::from(into)) {
                by_class[class_slot(info.class)] += 1;
            }
        }
    }

    /// Use the Concord's contracts: raise the Bound by a fight, indenture
    /// at a safe grave, hire militia while its buildings are attacked.
    pub(super) fn contracts(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look) {
        let Some(bank) = s.world.bank(s.party) else {
            return;
        };
        if bank.tithe == 0 {
            return;
        }
        let powers = s.world.powers();
        let ready = |p: usize| {
            powers
                .get(p)
                .is_some_and(|stats| stats.meter == Meter::Tithe)
                && s.world.can_use_power(s.party, p as u8).is_ok()
        };
        let find = |want: fn(&PowerEffect, u32) -> bool| {
            (0..powers.len())
                .find(|&p| want(&powers[p].effect, powers[p].duration_ticks) && ready(p))
        };
        let raise_bound = find(|e, d| matches!(e, PowerEffect::Raise { .. }) && d > 0);
        let indenture = find(|e, d| matches!(e, PowerEffect::Raise { .. }) && d == 0);
        let hire = find(|e, _| matches!(e, PowerEffect::Hire { .. }));
        if raise_bound.is_none() && indenture.is_none() && hire.is_none() {
            return;
        }
        let cast = |clicks: &mut Clicks, power: usize, at: Vec2| {
            let target = PowerTarget::Point(to_fx_vec(at));
            clicks.give(
                s.now,
                Command::UsePower {
                    power: power as u8,
                    target,
                },
            )
        };
        let tiles = |p: usize| powers[p].effect.reach().unwrap_or(0);
        let graves: Vec<Vec2> = if raise_bound.is_some() || indenture.is_some() {
            s.world.graves().map(|(at, _)| v2(at)).collect()
        } else {
            Vec::new()
        };
        // The grave with the most corpses within reach of it, as a raising
        // there would find them, of those `ok` takes.
        let fullest = |p: usize, ok: &dyn Fn(Vec2) -> bool, least: u32| {
            (graves.iter().copied())
                .filter(|&at| ok(at))
                .map(|at| (at, s.world.corpses_near(to_fx_vec(at), tiles(p))))
                .filter(|&(_, n)| n >= least.max(u32::from(MIN_RAISED)))
                .max_by_key(|&(_, n)| n)
                .map(|(at, _)| at)
        };
        let foes: Vec<Vec2> = spymaster::enemy_army(s).map(|r| r.pos).collect();
        if let Some(p) = raise_bound {
            let fighting: Vec<Vec2> = (look.army.iter())
                .filter(|r| r.engaged)
                .map(|r| r.pos)
                .collect();
            let by_fight = |at: Vec2| fighting.iter().any(|f| f.distance(at) <= FIGHT_NEAR);
            if let Some(at) = fullest(p, &by_fight, RAISE_FROM)
                && cast(clicks, p, at)
            {
                return;
            }
        }
        if let Some(p) = indenture {
            let kept = |at: Vec2| {
                s.world.keeps_at(s.party, to_fx_vec(at))
                    && foes.iter().all(|f| f.distance(at) > SAFE)
            };
            if let Some(at) = fullest(p, &kept, INDENTURE_FROM)
                && cast(clicks, p, at)
            {
                return;
            }
        }
        if let (Some(p), Some(home)) = (hire, self.home) {
            // The nearest hamlet no enemy holds, as the hiring goes by.
            let attacked = foes.iter().any(|f| f.distance(home) <= AT_HOME);
            if attacked
                && let Some(at) = s.world.hamlet_near(s.party, to_fx_vec(home), HAMLET_REACH)
            {
                cast(clicks, p, v2(at));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_evolves_into_long_weapons_against_riders_else_what_it_lacks() {
        let options = [(5, 0, true), (6, 2, false)];
        assert_eq!(
            pick_evolution(&options, true, [0.5, 0.25, 0.25], [9, 0, 0]),
            Some(5)
        );
        // Short of archers: the ranged form.
        assert_eq!(
            pick_evolution(&options, false, [0.5, 0.25, 0.25], [9, 0, 0]),
            Some(6)
        );
        // Short of line: the line form.
        assert_eq!(
            pick_evolution(&options, false, [0.5, 0.25, 0.25], [0, 0, 9]),
            Some(5)
        );
        assert_eq!(
            pick_evolution(&[], false, [0.5, 0.25, 0.25], [0, 0, 0]),
            None
        );
    }
}
