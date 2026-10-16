//! Who won: one ruling, made by the simulation itself every tick, so the
//! game, the golden replays, the computer players' test harnesses and every
//! machine in a network match agree on it (review fix R5).
//!
//! A side wins by **holding the field**: it still has regiments fighting,
//! and none of its enemies' fighting regiments are in its sight, for
//! [`HOLD_TICKS`] in a row (looked at once a second). Allies share the hold. Once decided, the
//! verdict stands for the rest of the match; the battle itself goes on.
//!
//! Conquest is won otherwise (docs/06-game-modes.md, "Victory conditions"),
//! by whichever of these the lobby turned on ([`Victories`]):
//! - **Annihilation:** a player with no main building left (one going up
//!   counts) has [`EXILE_TICKS`] to lay one, or is out: their regiments and
//!   buildings leave the field. When everyone left is on one side, that
//!   side wins.
//! - **Titan's Heart:** from [`HEART_UNLOCK_TICKS`] (scaled by tempo), a
//!   side with regiments within [`HEART_REACH`] tiles of the Heart and no
//!   enemy there holds it; holding it for [`HEART_HOLD_TICKS`] in a row
//!   wins.

use gc_fixed::{Fx, FxVec2};

use crate::hash::StateHasher;
use crate::regiments::Regiments;
use crate::stances::Stances;
use crate::vision::Vision;
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND};

/// How long a side must hold the field to win.
pub const HOLD_TICKS: u32 = 10 * TICKS_PER_SECOND;
/// How long a Conquest player with no main building has to lay one.
pub const EXILE_TICKS: u32 = 90 * TICKS_PER_SECOND;
/// When the Titan's Heart can first be held, at Standard tempo: minute 20.
pub const HEART_UNLOCK_TICKS: u32 = 20 * 60 * TICKS_PER_SECOND;
/// How long the Heart must be held, uncontested, to win.
pub const HEART_HOLD_TICKS: u32 = 3 * 60 * TICKS_PER_SECOND;
/// How near the Heart (in tiles) a regiment holds or contests it.
pub const HEART_REACH: Fx = Fx::from_int(6);

/// How a match was won.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Win {
    /// A side held the field.
    #[default]
    Field,
    /// Every enemy is out of a Conquest match.
    Annihilation,
    /// A side held the Titan's Heart.
    Heart,
}

impl Win {
    pub const ALL: [Win; 3] = [Win::Field, Win::Annihilation, Win::Heart];
}

/// How a match was decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verdict {
    /// The party holding the field, the last standing or holding the
    /// Heart when it was decided (its allies won too).
    pub holder: PlayerId,
    /// The tick it was decided on, counted from the start of the match.
    pub tick: u32,
    pub by: Win,
}

/// Which victories a Conquest match is played for. With neither, it never
/// ends (the sandbox).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Victories {
    pub annihilation: bool,
    pub heart: bool,
}

/// The Titan's Heart as the game shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeartView {
    /// Its middle, in tiles.
    pub at: FxVec2,
    /// The battle tick it can first be held.
    pub unlock: u32,
    /// The party holding it and the tick it took it, while one does.
    pub held: Option<(PlayerId, u32)>,
}

/// Conquest's rulings (see the module docs).
#[derive(Clone, Debug, Default)]
pub(crate) struct Throne {
    victories: Victories,
    /// The Heart's middle and unlock tick, if the map has one and it counts.
    heart: Option<(FxVec2, u32)>,
    held: Option<(PlayerId, u32)>,
    /// Parties that have had a main building: only they can be put out.
    had_main: u8,
    /// Parties put out.
    out: u8,
    /// The tick each party lost its last main building, while it has none.
    exiled: [Option<u32>; MAX_PARTIES],
}

impl Throne {
    /// The rulings for a match played for `victories`, with the Heart at
    /// `heart` unlocking at `HEART_UNLOCK_TICKS` scaled by `percent`.
    pub(crate) fn new(victories: Victories, heart: Option<FxVec2>, percent: u32) -> Throne {
        let unlock = (u64::from(HEART_UNLOCK_TICKS) * u64::from(percent) / 100) as u32;
        Throne {
            victories,
            heart: heart.filter(|_| victories.heart).map(|at| (at, unlock)),
            ..Throne::default()
        }
    }

    /// Where the Heart is, if it counts.
    pub(crate) fn heart_at(&self) -> Option<FxVec2> {
        self.heart.map(|(at, _)| at)
    }

    pub(crate) fn heart(&self) -> Option<HeartView> {
        self.heart.map(|(at, unlock)| HeartView {
            at,
            unlock,
            held: self.held,
        })
    }

    /// The tick `party` lost its last main building, while it has none and
    /// isn't out yet.
    pub(crate) fn exiled(&self, party: PlayerId) -> Option<u32> {
        self.exiled.get(usize::from(party)).copied().flatten()
    }

    pub(crate) fn is_out(&self, party: PlayerId) -> bool {
        self.out & (1 << party) != 0
    }

    /// One look, at tick `now` (`battle` ticks into the battle): the parties
    /// with a main building (`mains`) and with regiments at the Heart
    /// (`at_heart`). Returns the parties put out now, and the verdict if
    /// this decides the match.
    pub(crate) fn update(
        &mut self,
        mains: u8,
        at_heart: u8,
        stances: &Stances,
        now: u32,
        battle: u32,
    ) -> (u8, Option<Verdict>) {
        let mut put_out = 0;
        if self.victories.annihilation {
            self.had_main |= mains;
            for p in 0..MAX_PARTIES {
                let bit = 1u8 << p;
                if self.had_main & !self.out & bit == 0 {
                    continue;
                }
                if mains & bit != 0 {
                    self.exiled[p] = None;
                } else {
                    let since = *self.exiled[p].get_or_insert(now);
                    if now - since >= EXILE_TICKS {
                        self.exiled[p] = None;
                        put_out |= bit;
                    }
                }
            }
            self.out |= put_out;
        }
        let at_heart = at_heart & !self.out;
        if let Some((_, unlock)) = self.heart {
            self.held = match (self.held, one_side(at_heart, stances)) {
                _ if battle < unlock => None,
                (Some((p, since)), Some(h)) if !stances.hostile(p, h) => Some((p, since)),
                (_, Some(h)) => Some((h, now)),
                (_, None) => None,
            };
            if let Some((holder, since)) = self.held
                && now - since >= HEART_HOLD_TICKS
            {
                return (
                    put_out,
                    Some(Verdict {
                        holder,
                        tick: now,
                        by: Win::Heart,
                    }),
                );
            }
        }
        // Once someone is out, the match ends when everyone left is friends.
        let left = self.had_main & !self.out;
        let verdict = (self.out != 0)
            .then(|| one_side(left, stances))
            .flatten()
            .map(|holder| Verdict {
                holder,
                tick: now,
                by: Win::Annihilation,
            });
        (put_out, verdict)
    }

    fn hash_into(&self, h: &mut StateHasher) {
        match self.held {
            Some((p, since)) => {
                h.u8(1);
                h.u8(p);
                h.u32(since);
            }
            None => h.u8(0),
        }
        h.u8(self.had_main);
        h.u8(self.out);
        for e in self.exiled {
            h.u32(e.map_or(u32::MAX, |t| t));
        }
    }
}

/// The lowest of the parties in `mask`, if there are any and none of them
/// is another's enemy.
fn one_side(mask: u8, stances: &Stances) -> Option<PlayerId> {
    let parties = || (0..MAX_PARTIES as PlayerId).filter(move |&p| mask & (1 << p) != 0);
    let first = parties().next()?;
    parties()
        .all(|p| parties().all(|q| !stances.hostile(p, q)))
        .then_some(first)
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Judge {
    /// The party holding the field and the tick it took it, while one does.
    held: Option<(PlayerId, u32)>,
    decided: Option<Verdict>,
    /// Conquest's rulings, which take the place of holding the field.
    throne: Option<Throne>,
}

impl Judge {
    pub(crate) fn verdict(&self) -> Option<Verdict> {
        self.decided
    }

    /// Play the match by Conquest's rulings from now on.
    pub(crate) fn conquest(&mut self, throne: Throne) {
        self.throne = Some(throne);
    }

    pub(crate) fn throne(&self) -> Option<&Throne> {
        self.throne.as_ref()
    }

    /// Whether Conquest's rulings want a look at tick `now`.
    pub(crate) fn conquest_due(&self, now: u32) -> bool {
        self.throne.is_some() && self.decided.is_none() && now.is_multiple_of(TICKS_PER_SECOND)
    }

    /// Conquest's look at tick `now` (see [`Throne::update`]); returns the
    /// parties put out.
    pub(crate) fn update_conquest(
        &mut self,
        mains: u8,
        at_heart: u8,
        stances: &Stances,
        now: u32,
        battle: u32,
    ) -> u8 {
        let Some(throne) = &mut self.throne else {
            return 0;
        };
        let (out, verdict) = throne.update(mains, at_heart, stances, now, battle);
        self.decided = self.decided.or(verdict);
        out
    }

    /// Look at the field after tick `now` has been played.
    pub(crate) fn update(
        &mut self,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
        now: u32,
    ) {
        // Once a second is plenty for a ruling that needs ten. Conquest
        // has its own rulings.
        if self.decided.is_some() || self.throne.is_some() || !now.is_multiple_of(TICKS_PER_SECOND)
        {
            return;
        }
        self.held = match (self.held, holding_the_field(regs, stances, vision)) {
            // Allies take turns being the first regiment standing.
            (Some((p, since)), Some(h)) if !stances.hostile(p, h) => Some((p, since)),
            (_, Some(h)) => Some((h, now)),
            (_, None) => None,
        };
        if let Some((holder, since)) = self.held
            && now - since >= HOLD_TICKS
        {
            self.decided = Some(Verdict {
                holder,
                tick: now,
                by: Win::Field,
            });
        }
    }

    /// Nobody holds the field while the armies deploy.
    pub(crate) fn deploying(&mut self) {
        self.held = None;
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        match self.held {
            Some((p, since)) => {
                h.u8(1);
                h.u8(p);
                h.u32(since);
            }
            None => h.u8(0),
        }
        match self.decided {
            Some(v) => {
                h.u8(1);
                h.u8(v.holder);
                h.u32(v.tick);
                h.u8(v.by as u8);
            }
            None => h.u8(0),
        }
        if let Some(t) = &self.throne {
            t.hash_into(h);
        }
    }
}

/// The party holding the field, if one is: it has regiments still fighting
/// (not broken, not wiped out) and sees none of its enemies' fighting
/// regiments. It must also have one not hidden from its enemies, so
/// survivors rallying in a wood don't drag a battle on; two armies out of
/// each other's sight are still facing each other. With no enemy left
/// standing at all, hiding doesn't matter. Among allies that qualify, the
/// lowest-numbered party is named.
pub(crate) fn holding_the_field(
    regs: &Regiments,
    stances: &Stances,
    vision: &Vision,
) -> Option<PlayerId> {
    // Parties with regiments fighting, and those with one in the open.
    let (mut standing, mut shown) = (0u8, 0u8);
    let fighting = |i: usize| regs.alive[i] && regs.models[i] > 0 && !regs.morale_state[i].broken();
    for i in (0..regs.alive.len()).filter(|&i| fighting(i)) {
        let bit = 1u8 << regs.owner[i];
        standing |= bit;
        if !vision.hidden(i, regs.owner[i]) {
            shown |= bit;
        }
    }
    (0..MAX_PARTIES as PlayerId).find(|&p| {
        if standing & (1 << p) == 0 {
            return false;
        }
        let foes = (0..MAX_PARTIES as PlayerId)
            .filter(|&q| standing & (1 << q) != 0 && stances.hostile(p, q))
            .fold(0u8, |m, q| m | 1 << q);
        if foes == 0 {
            return true;
        }
        shown & (1 << p) != 0
            && !(0..regs.alive.len()).any(|i| {
                fighting(i) && foes & (1 << regs.owner[i]) != 0 && vision.sees(p, i, regs.owner[i])
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(j: &Judge) -> u64 {
        let mut h = StateHasher::new();
        j.hash_into(&mut h);
        h.finish()
    }

    #[test]
    fn the_ruling_is_part_of_the_state() {
        let none = Judge::default();
        let held = Judge {
            held: Some((0, 5)),
            ..Judge::default()
        };
        let later = Judge {
            held: Some((0, 6)),
            ..Judge::default()
        };
        let won = Judge {
            held: Some((0, 5)),
            decided: Some(Verdict {
                holder: 0,
                tick: 205,
                by: Win::Field,
            }),
            ..Judge::default()
        };
        let hashes = [hash(&none), hash(&held), hash(&later), hash(&won)];
        for (i, a) in hashes.iter().enumerate() {
            assert!(!hashes[i + 1..].contains(a), "{hashes:x?}");
        }
    }
}
