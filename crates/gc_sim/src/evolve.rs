//! Brood evolutions (docs/03-factions.md, "Metamorphosis in the field"): a
//! tier-1 regiment cocoons where it stands and comes out as one of its two
//! tier-2 forms.
//!
//! - [`Command::Evolve`](crate::Command::Evolve) names a regiment and the
//!   evolution (a unit type whose [`ConquestUnit::evolves_from`] is the
//!   regiment's). It is refused unless its owner is at the evolution's
//!   tier, has a finished building of the type that unlocks it (the
//!   Chrysalis Chamber), has supply room for what the new form takes over
//!   the old and can pay the evolution's price for the soldiers it will
//!   have; and unless the regiment is its owner's, out of melee, not
//!   routing, off the walls, not summoned and not in a cocoon already.
//! - The price is the evolution's cost for the soldiers that come out: its
//!   cost times those soldiers over its full strength (a regiment at half
//!   strength pays half). The soldiers that come out are the regiment's
//!   share of the new form's full strength, rounded up: 24 of 32
//!   Skitterlings come out as 12 of 16 Spitters.
//! - The cocoon lasts the evolution's train time (15 s at Standard tempo;
//!   the tempo scales it as it does training). Meanwhile the regiment
//!   can't move, strike, shoot or rout, and doesn't reinforce: blows land
//!   on it as on any regiment, and if it is wiped out the cocoon is lost
//!   with it. Its supply is the new form's from the start.
//! - It comes out in place, the same regiment (its handle, its orders
//!   after, its experience and rank), as the new form with every soldier
//!   at full hit points and its nerve fresh.
//! - An evolved regiment reinforces as the form it grew from does, at the
//!   same points, each new soldier paying his share of both forms' costs.

use crate::conquest::{Conquest, ConquestUnit};
use crate::regiments::Regiments;
use crate::reinforce::Refill;
use crate::structures::Refusal;
use crate::{PlayerId, RegimentHandle, UnitId, UnitStats, morale};

/// What evolving a regiment would take and give: what a player is shown
/// before they cocoon it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Evolution {
    /// The soldiers that would come out.
    pub out: u16,
    /// Its price.
    pub grub: u32,
    pub marrow: u32,
    /// How long the cocoon would last, in ticks.
    pub ticks: u16,
}

impl Conquest {
    /// What `owner`'s evolving their regiment slot `r` into `into` would
    /// take and give, and why it can't, if it can't. Says nothing of the
    /// store.
    fn evolution(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        owner: PlayerId,
        r: usize,
        into: UnitId,
    ) -> Result<Evolution, Refusal> {
        let to = self.unit(into);
        let (Some(from), Some(at), Some(stats)) = (
            to.evolves_from,
            to.unlocked_at,
            units.get(usize::from(into)),
        ) else {
            return Err(Refusal::Unknown);
        };
        if regs.unit[r] != from || regs.owner[r] != owner {
            return Err(Refusal::Unknown);
        }
        if regs.engaged[r].is_some()
            || regs.morale_state[r].broken()
            || regs.on_wall[r]
            || regs.summoned_left[r] > 0
            || regs.cocooned(r)
            || regs.turncoat[r]
        {
            return Err(Refusal::Busy);
        }
        let st = &self.structures;
        if st.tier_of(owner) < to.tier {
            return Err(Refusal::Tier);
        }
        if !st
            .finished_of(owner)
            .any(|i| st.grew_from(st.kind_of(i), at))
        {
            return Err(Refusal::NoChamber);
        }
        let more = u32::from(to.supply).saturating_sub(u32::from(self.unit(from).supply));
        let used = self.supply_used(regs, owner);
        if more > 0 && used + more > st.supply_cap(owner) {
            return Err(Refusal::Supply);
        }
        let (models, full) = (regs.models[r], regs.full_models[r].max(1));
        let out = emerging(models, full, stats.models);
        let n = u32::from(stats.models.max(1));
        let ticks = st.tempo_ticks(u32::from(to.train_ticks)).max(1);
        Ok(Evolution {
            out,
            grub: u32::from(to.cost_grub) * u32::from(out) / n,
            marrow: u32::from(to.cost_marrow) * u32::from(out) / n,
            ticks: u16::try_from(ticks).unwrap_or(u16::MAX),
        })
    }

    /// What `owner`'s evolving their regiment `h` into `into` now would
    /// take and give, or why they can't (the store included).
    pub(crate) fn can_evolve(
        &self,
        regs: &Regiments,
        units: &[UnitStats],
        owner: PlayerId,
        h: RegimentHandle,
        into: UnitId,
    ) -> Result<Evolution, Refusal> {
        let r = regs.resolve(h).ok_or(Refusal::Unknown)?;
        let e = self.evolution(regs, units, owner, r, into)?;
        if !self.economy.can_afford(owner, e.grub, e.marrow) {
            return Err(Refusal::Price);
        }
        Ok(e)
    }

    /// `owner` evolves their regiment `h` into `into`: it pays and goes
    /// into its cocoon. False (nothing paid) if it can't.
    pub(crate) fn evolve(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        owner: PlayerId,
        h: RegimentHandle,
        into: UnitId,
    ) -> bool {
        let Some(r) = regs.resolve(h) else {
            return false;
        };
        let Ok(e) = self.evolution(regs, units, owner, r, into) else {
            return false;
        };
        if !self.economy.spend(owner, e.grub, e.marrow) {
            return false;
        }
        self.stop_work(r);
        regs.cocoon(r, into, e.ticks);
        true
    }

    /// `owner`'s supply in use, a cocoon's at its new form's and what is
    /// in training included.
    pub(crate) fn supply_used(&self, regs: &Regiments, owner: PlayerId) -> u32 {
        let mut used = self.economy.supply_in_use(&self.units, regs);
        self.production
            .in_training(&self.structures, &self.units, &mut used);
        used.get(usize::from(owner)).copied().unwrap_or(0)
    }

    /// The unit type a regiment of `unit` reinforces as, and the Grub and
    /// Marrow its full strength is worth: an evolution's are the form it
    /// grew from's trained-at and both forms' costs together.
    pub(crate) fn reinforces_as(&self, unit: UnitId) -> (ConquestUnit, u16, u16) {
        let u = self.unit(unit);
        match u.evolves_from.map(|from| self.unit(from)) {
            Some(from) => (
                from,
                from.cost_grub.saturating_add(u.cost_grub),
                from.cost_marrow.saturating_add(u.cost_marrow),
            ),
            None => (u, u.cost_grub, u.cost_marrow),
        }
    }
}

/// The soldiers that come out of a cocoon of `models` of a regiment of
/// `full` evolving into a form `new_full` strong: the same share of the
/// new form, rounded up, at least one and at most all of it.
pub(crate) fn emerging(models: u16, full: u16, new_full: u16) -> u16 {
    let (m, full, n) = (
        u32::from(models),
        u32::from(full.max(1)),
        u32::from(new_full),
    );
    (m * n).div_ceil(full).clamp(1, n.max(1)) as u16
}

impl Regiments {
    /// Slot `i` goes into a cocoon for `ticks`, to come out as `into`: it
    /// stops where it is.
    pub(crate) fn cocoon(&mut self, i: usize, into: UnitId, ticks: u16) {
        self.halt(i);
        self.end_run(i);
        self.braced[i] = false;
        self.hold[i] = false;
        self.skirmish[i] = false;
        self.backing[i] = 0;
        self.cocoon_left[i] = ticks;
        self.cocoon_into[i] = into;
        self.refill[i] = Refill::Idle;
    }

    /// Regiment `h`'s cocoon: ticks left and the unit type it comes out
    /// as, if it is in one.
    pub fn cocoon_of(&self, h: RegimentHandle) -> Option<(u16, UnitId)> {
        let i = self.resolve(h)?;
        (self.cocoon_left[i] > 0).then(|| (self.cocoon_left[i], self.cocoon_into[i]))
    }

    /// A tick of every cocoon goes by; each whose time is up opens, its
    /// regiment its new form (see [`Regiments::emerge`]).
    pub(crate) fn open_cocoons(&mut self, units: &[UnitStats]) {
        for i in 0..self.alive.len() {
            if !self.alive[i] || self.cocoon_left[i] == 0 {
                continue;
            }
            self.cocoon_left[i] -= 1;
            if self.cocoon_left[i] == 0
                && let Some(stats) = units.get(usize::from(self.cocoon_into[i]))
            {
                self.emerge(i, self.cocoon_into[i], stats);
            }
        }
    }

    /// Slot `i` comes out of its cocoon as `unit`: its share of the new
    /// form's soldiers (see [`emerging`]), each at full hit points, its
    /// nerve fresh, in the new form's ranks; its handle, place, facing and
    /// experience kept.
    fn emerge(&mut self, i: usize, unit: UnitId, stats: &UnitStats) {
        let models =
            emerging(self.models[i], self.full_models[i], stats.models).min(crate::MAX_MODELS);
        let base = i * crate::MAX_MODELS as usize;
        self.hp[base..][..crate::MAX_MODELS as usize].fill(0);
        self.hp[base..][..usize::from(models)].fill(stats.hp_per_model);
        // A general rides with the regiment he leads, whatever it becomes.
        if let Some(g) = self.general[i] {
            self.general[i] = Some(g.min(models as u8 - 1));
        }
        self.unit[i] = unit;
        self.models[i] = models;
        self.full_models[i] = stats.models;
        self.files[i] = stats.files.min(models).max(1);
        self.frontage[i] = stats.files;
        self.width_kept[i] = 0;
        self.morale[i] = morale::fresh(stats);
        self.break_at[i] = morale::breaking_point(stats, i as u32, self.generation[i]);
        self.retune_slot(i, stats);
    }
}
