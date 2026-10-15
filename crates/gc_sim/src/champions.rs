//! Champions before battle (docs/05-combat.md, "Champions before battle";
//! Mario's pick MB-4).
//!
//! In a match that allows it ([`crate::MatchSetup::champions`]), the first
//! time two hostile regiments come within [`CALL_TILES`] of each other
//! there is a [`CHANCE_PERCENT`] chance that each side sends a champion out
//! between the lines. Until heroes exist, a side's champion is the best man
//! of its best fighting regiment near the front (melee attack plus
//! defence). The two run to the point halfway between their regiments and
//! trade blows, one swing each in turn: a blow lands with the simulation's
//! chance to hit for their regiments' attack and defence, and a man it
//! lands on reels and loses his next swing. The [`WOUNDS`]th blow to land
//! kills.
//!
//! The whole fight is rolled when the challenge is called, from the match's
//! events stream, so every machine and every replay sees the same duel and
//! the presentation can play it out blow by blow. Nothing changes until the
//! last blow lands: then every regiment on the winner's side gains
//! [`HEART`] morale points, and keeps them as a higher ceiling for the rest
//! of the battle, while every regiment on the loser's side loses as many,
//! from its morale and its ceiling. The loser's regiment loses him. If
//! neither falls within [`MAX_BEATS`] swings they part and nothing changes;
//! if either regiment is drawn into melee, breaks or is destroyed first,
//! the duel is abandoned, also without effect. The armies are never held
//! back: a challenge is a moment in the battle, not a pause in it.

use gc_fixed::{Fx, FxVec2, Pcg32};

use crate::combat::hit_percent;
use crate::morale::{self, SCALE};
use crate::regiments::{Regiments, hash_handle};
use crate::{RegimentHandle, Stances, StateHasher, TICKS_PER_SECOND, UnitStats};

/// Hostile regiments this close (centre to centre, in tiles) call out
/// champions, if they are going to: far enough out that the duel is
/// usually over before the lines meet.
pub const CALL_TILES: i32 = 60;
/// Percent of matches that allow champions in which they come out.
pub const CHANCE_PERCENT: u32 = 50;
/// A side's champion comes from a regiment within this many tiles of its
/// regiment nearest the enemy.
const PICK_TILES: i32 = 25;
/// Champions run out at this many tiles per second, and take at least
/// [`MIN_WALK_TICKS`] and at most [`MAX_WALK_TICKS`] to meet.
const PACE_TILES: i64 = 5;
const MIN_WALK_TICKS: u32 = 2 * TICKS_PER_SECOND;
const MAX_WALK_TICKS: u32 = 8 * TICKS_PER_SECOND;
/// Ticks per beat: one champion swings each beat, in turn.
pub const BEAT_TICKS: u32 = 12;
/// A swing's blow lands this many ticks into its beat.
pub const BLOW_TICKS: u32 = 6;
/// Blows that must land to kill a champion.
pub const WOUNDS: u8 = 3;
/// Beats they fight before parting (at most 32: see [`Challenge::landed`]).
pub const MAX_BEATS: u8 = 30;
/// Morale points the winner's side gains and the loser's loses.
pub const HEART: i16 = 6;

/// Two champions' duel between the lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Challenge {
    /// The regiments the champions step out of, for their two sides.
    pub champions: [RegimentHandle; 2],
    /// Where they fight: halfway between their regiments when called.
    pub spot: FxVec2,
    /// Tick the challenge was called.
    pub called: u32,
    /// Tick they meet; the first beat starts then.
    pub meet: u32,
    /// Beats fought. Champion `beat % 2` swings on each beat, unless the
    /// blow of the beat before landed on him.
    pub beats: u8,
    /// Bit `b` is set when the swing of beat `b` landed.
    pub landed: u32,
    /// The champion (0 or 1) whose last blow kills the other, or `None` when
    /// they part with nobody down.
    pub winner: Option<u8>,
    /// Tick the duel was cut short, if it was.
    pub abandoned: Option<u32>,
}

impl Challenge {
    /// Whether champion `beat % 2` swings on `beat`: he does unless the blow
    /// of the beat before landed on him.
    pub fn swings(&self, beat: u8) -> bool {
        beat < self.beats && (beat == 0 || self.landed & (1 << (beat - 1)) == 0)
    }

    /// Whether the swing of `beat` landed.
    pub fn lands(&self, beat: u8) -> bool {
        beat < self.beats && self.landed & (1 << beat) != 0
    }

    /// Tick the blow of `beat` lands (or would have).
    pub fn blow_tick(&self, beat: u8) -> u32 {
        self.meet + beat as u32 * BEAT_TICKS + BLOW_TICKS
    }

    /// Tick the duel is decided: the killing blow lands, or the last beat is
    /// over and they part.
    pub fn ends(&self) -> u32 {
        match self.winner {
            Some(_) => self.blow_tick(self.beats - 1),
            None => self.meet + self.beats as u32 * BEAT_TICKS,
        }
    }

    /// Whether the duel is still being fought at `tick`.
    pub fn fighting(&self, tick: u32) -> bool {
        self.abandoned.is_none() && tick < self.ends()
    }
}

/// The match's champions: whether it allows them, and the one challenge
/// it may have.
pub(crate) struct Champions {
    allowed: bool,
    /// The chance has been rolled (once a battle), whether or not they came
    /// out.
    rolled: bool,
    rng: Pcg32,
    challenge: Option<Challenge>,
}

impl Champions {
    pub(crate) fn new(allowed: bool, rng: Pcg32) -> Champions {
        Champions {
            allowed,
            rolled: false,
            rng,
            challenge: None,
        }
    }

    pub(crate) fn challenge(&self) -> Option<&Challenge> {
        self.challenge.as_ref()
    }

    /// Look for armies drawing near once a second until the chance is
    /// rolled; then follow the duel and settle it. Runs at the end of the
    /// tick, after morale.
    pub(crate) fn update(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        tick: u32,
    ) {
        if !self.allowed {
            return;
        }
        if !self.rolled {
            if tick.is_multiple_of(TICKS_PER_SECOND) {
                self.look(regs, units, stances, tick);
            }
            return;
        }
        let Some(c) = &mut self.challenge else {
            return;
        };
        if c.abandoned.is_some() || tick > c.ends() {
            return;
        }
        let ready = |h: RegimentHandle| {
            regs.resolve(h)
                .is_some_and(|i| regs.engaged[i].is_none() && !regs.morale_state[i].broken())
        };
        if !c.champions.iter().all(|&h| ready(h)) {
            c.abandoned = Some(tick);
        } else if tick == c.ends()
            && let Some(w) = c.winner
        {
            let (won, lost) = (c.champions[w as usize], c.champions[1 - w as usize]);
            settle(regs, stances, won, lost);
        }
    }

    /// Once two hostile regiments are within [`CALL_TILES`], roll whether
    /// champions come out, and if so pick them and roll their duel.
    fn look(&mut self, regs: &mut Regiments, units: &[UnitStats], stances: &Stances, tick: u32) {
        let reach = Fx::from_int(CALL_TILES).raw() as u64;
        let mut nearest: Option<(u64, usize, usize)> = None;
        for a in 0..regs.alive.len() {
            if !regs.alive[a] || regs.morale_state[a].broken() {
                continue;
            }
            for b in a + 1..regs.alive.len() {
                if !regs.alive[b]
                    || regs.morale_state[b].broken()
                    || !stances.hostile(regs.owner[a], regs.owner[b])
                {
                    continue;
                }
                let d = (regs.position[a] - regs.position[b]).length_sq_raw();
                if d <= reach * reach && nearest.is_none_or(|(best, _, _)| d < best) {
                    nearest = Some((d, a, b));
                }
            }
        }
        let Some((_, a, b)) = nearest else {
            return;
        };
        self.rolled = true;
        if self.rng.below(100) >= CHANCE_PERCENT {
            return;
        }
        let (Some(x), Some(y)) = (pick(regs, units, stances, a), pick(regs, units, stances, b))
        else {
            return;
        };
        let spot = FxVec2::new(
            Fx::from_raw(midpoint(regs.position[x].x.raw(), regs.position[y].x.raw())),
            Fx::from_raw(midpoint(regs.position[x].y.raw(), regs.position[y].y.raw())),
        );
        // Ticks for the champion further from the spot to run there.
        let far = (regs.position[x] - spot).length().raw() as i64;
        let walk = (far * TICKS_PER_SECOND as i64 / (PACE_TILES << 16)) as u32;
        let stats = |i: usize| &units[regs.unit[i] as usize];
        let fighters = [stats(x), stats(y)];
        let (beats, landed, winner) = duel(fighters, &mut self.rng);
        let handle = |i: usize| RegimentHandle {
            index: i as u32,
            generation: regs.generation[i],
        };
        self.challenge = Some(Challenge {
            champions: [handle(x), handle(y)],
            spot,
            called: tick,
            meet: tick + walk.clamp(MIN_WALK_TICKS, MAX_WALK_TICKS),
            beats,
            landed,
            winner,
            abandoned: None,
        });
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        // Only in matches that allow champions, so other worlds hash as
        // they did before champions existed.
        if !self.allowed {
            return;
        }
        h.u8(self.rolled as u8);
        let (state, inc) = self.rng.raw_state();
        h.u64(state);
        h.u64(inc);
        match &self.challenge {
            Some(c) => {
                h.u8(1);
                for &r in &c.champions {
                    hash_handle(h, Some(r));
                }
                h.i32(c.spot.x.raw());
                h.i32(c.spot.y.raw());
                h.u32(c.called);
                h.u32(c.meet);
                h.u8(c.beats);
                h.u32(c.landed);
                h.u8(c.winner.map_or(u8::MAX, |w| w));
                h.u32(c.abandoned.map_or(u32::MAX, |t| t));
            }
            None => h.u8(0),
        }
    }
}

fn midpoint(a: i32, b: i32) -> i32 {
    ((a as i64 + b as i64) / 2) as i32
}

/// The regiment that sends out the champion for the side of regiment `a`:
/// the best fighter (melee attack plus defence, then the nearest to `a`)
/// among `a` and its allies within [`PICK_TILES`] that are ready to fight,
/// can spare a man, and fight in melee at all.
fn pick(regs: &Regiments, units: &[UnitStats], stances: &Stances, a: usize) -> Option<usize> {
    let near = Fx::from_int(PICK_TILES).raw() as u64;
    let mut best: Option<(i32, u64, usize)> = None;
    for i in 0..regs.alive.len() {
        let stats = &units[regs.unit[i] as usize];
        if !regs.alive[i]
            || stances.hostile(regs.owner[a], regs.owner[i])
            || regs.morale_state[i].broken()
            || regs.engaged[i].is_some()
            || regs.models[i] < 2
            || stats.melee_attack == 0
        {
            continue;
        }
        let d = (regs.position[i] - regs.position[a]).length_sq_raw();
        if d > near * near {
            continue;
        }
        let score = stats.melee_attack as i32 + stats.melee_defence as i32;
        if best.is_none_or(|(s, bd, _)| score > s || score == s && d < bd) {
            best = Some((score, d, i));
        }
    }
    best.map(|(_, _, i)| i)
}

/// Roll a duel between champions of `fighters`: (beats fought, the beats
/// whose blows landed, the winner if anyone fell).
fn duel(fighters: [&UnitStats; 2], rng: &mut Pcg32) -> (u8, u32, Option<u8>) {
    let mut wounds = [0u8; 2];
    let mut landed = 0u32;
    for beat in 0..MAX_BEATS {
        let s = (beat % 2) as usize;
        if beat > 0 && landed & (1 << (beat - 1)) != 0 {
            // Still reeling from the last blow: he loses this swing.
            continue;
        }
        let (me, them) = (fighters[s], fighters[1 - s]);
        // He stands alone: no formation helps his guard.
        let chance = hit_percent(me.melee_attack as i32, them.melee_defence as i32);
        if rng.below(100) < chance as u32 {
            landed |= 1 << beat;
            wounds[1 - s] += 1;
            if wounds[1 - s] == WOUNDS {
                return (beat + 1, landed, Some(s as u8));
            }
        }
    }
    (MAX_BEATS, landed, None)
}

/// The champion of regiment `won` has killed the champion of `lost`: his
/// side takes heart and theirs loses it, and `lost` loses its man.
fn settle(regs: &mut Regiments, stances: &Stances, won: RegimentHandle, lost: RegimentHandle) {
    let (Some(w), Some(l)) = (regs.resolve(won), regs.resolve(lost)) else {
        return;
    };
    let (winners, losers) = (regs.owner[w], regs.owner[l]);
    let points = HEART * SCALE;
    for i in 0..regs.alive.len() {
        if !regs.alive[i] {
            continue;
        }
        let change = if !stances.hostile(winners, regs.owner[i]) {
            points
        } else if !stances.hostile(losers, regs.owner[i]) {
            -points
        } else {
            continue;
        };
        regs.morale_bonus[i] = regs.morale_bonus[i].saturating_add(change);
        if !regs.morale_state[i].broken() && regs.morale[i] > 0 {
            // A gain may go above the ceiling until the regiment's next
            // check, which uses the new one.
            regs.morale[i] = regs.morale[i].saturating_add(change).max(1);
            regs.morale_state[i] = morale::steady_or_wavering(regs.morale[i]);
        }
    }
    let last = regs.models[l] as usize - 1;
    let hp = regs.hp[l * crate::formation::MAX_MODELS as usize + last];
    regs.take_damage(l, hp as u32);
    if regs.models[l] == 0 {
        regs.despawn(lost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fighter(attack: u8, defence: u8) -> UnitStats {
        UnitStats {
            melee_attack: attack,
            melee_defence: defence,
            ..crate::scenario::DRILL_UNITS[0]
        }
    }

    #[test]
    fn a_reeling_man_loses_his_next_swing_and_the_third_blow_kills() {
        let (a, b) = (fighter(60, 30), fighter(30, 30));
        for seed in 0..200 {
            let mut rng = Pcg32::new(seed, 3);
            let (beats, landed, winner) = duel([&a, &b], &mut rng);
            let c = Challenge {
                champions: [RegimentHandle {
                    index: 0,
                    generation: 0,
                }; 2],
                spot: FxVec2::ZERO,
                called: 0,
                meet: 0,
                beats,
                landed,
                winner,
                abandoned: None,
            };
            let mut wounds = [0; 2];
            for beat in 0..beats {
                if c.lands(beat) {
                    assert!(c.swings(beat), "seed {seed}: a lost swing landed");
                    wounds[1 - (beat % 2) as usize] += 1;
                }
                if beat > 0 && c.lands(beat - 1) {
                    assert!(!c.swings(beat), "seed {seed}: a reeling man swung");
                }
            }
            match winner {
                Some(w) => {
                    assert_eq!(wounds[1 - w as usize], WOUNDS, "seed {seed}");
                    assert!(c.lands(beats - 1) && (beats - 1) % 2 == w);
                }
                None => {
                    assert_eq!(beats, MAX_BEATS);
                    assert!(wounds.iter().all(|&w| w < WOUNDS));
                }
            }
        }
    }

    #[test]
    fn the_better_fighter_usually_wins() {
        let (strong, weak) = (fighter(50, 50), fighter(25, 25));
        let mut wins = [0; 3];
        for seed in 0..1000 {
            let mut rng = Pcg32::new(seed, 3);
            // Half the time the weak man swings first.
            let (first, second) = if seed % 2 == 0 {
                (&strong, &weak)
            } else {
                (&weak, &strong)
            };
            let winner = duel([first, second], &mut rng).2;
            let strong_won = winner.map(|w| (w as u64 == seed % 2) as usize);
            wins[strong_won.unwrap_or(2)] += 1;
        }
        // wins[1]: the strong man; wins[0]: the weak; wins[2]: they parted.
        assert!(wins[1] > 850, "{wins:?}");
        assert!(wins[0] > 10, "the weak man never won: {wins:?}");
    }
}
