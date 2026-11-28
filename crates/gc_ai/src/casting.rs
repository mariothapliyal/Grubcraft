//! Heroes casting by simple rules (docs/44-hero-abilities.md), for every
//! computer player: the Steward's in Conquest and a Field Battle's mind.
//! A hero casts the first of its abilities (the ultimate, then the basic
//! one) that is ready and has a foe to go at: the nearest standing foe it
//! sees, within [`FIGHT_TILES`] for one cast on the hero, within reach for
//! one aimed (and, at a regiment, one the cast would be taken for:
//! Foreclosure waits for a foe below half strength). It never walks off to
//! cast.

use gc_sim::abilities::{Aim, CastTarget};
use gc_sim::{Command, PlayerId, TICKS_PER_SECOND, World};

use crate::perception::{Seen, to_fx_vec};

/// How near a foe must be for a hero to cast an ability on itself (a rally,
/// a summons, an aura), in tiles.
pub const FIGHT_TILES: f32 = 8.0;

/// How long after a cast it gives a hero no other, past its reaction time
/// (while the order is on its way, the hero looks ready still).
pub const CAST_AGAIN_TICKS: u32 = 3 * TICKS_PER_SECOND;

/// The cast `party`'s hero regiment `me` makes now at one of `foes` (the
/// standing hostile regiments it sees), if any.
pub fn cast_for(world: &World, party: PlayerId, me: &Seen, foes: &[&Seen]) -> Option<Command> {
    let hero = world.hero_of(me.handle).filter(|h| h.kit.is_some())?;
    // The nearest foe within `reach` that `fits` (one a cast aimed at it
    // would be taken for).
    let nearest = |reach: f32, fits: &dyn Fn(&Seen) -> bool| {
        (foes.iter())
            .map(|f| (f, f.pos.distance_squared(me.pos)))
            .filter(|&(f, d)| d <= reach * reach && fits(f))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(f, _)| *f)
    };
    for slot in [1, 0] {
        // Locked, cooling, passive, or not for this match: nothing to aim.
        if world.can_cast(party, me.handle, slot).is_err() {
            continue;
        }
        let Some(a) = hero.ability(slot) else {
            continue;
        };
        let reach = f32::from(a.range);
        let target = match a.aim {
            Aim::Hero => nearest(FIGHT_TILES, &|_| true).map(|_| CastTarget::Hero),
            Aim::Point => nearest(reach, &|_| true).map(|f| CastTarget::Point(to_fx_vec(f.pos))),
            // Only a foe the cast would be taken for (Foreclosure wants
            // one below half strength).
            Aim::Regiment => nearest(reach, &|f| {
                let t = CastTarget::Regiment(f.handle);
                world.cast_check(party, me.handle, slot, t).is_ok()
            })
            .map(|f| CastTarget::Regiment(f.handle)),
        };
        let Some(target) = target else {
            continue;
        };
        if world.cast_check(party, me.handle, slot, target).is_err() {
            continue;
        }
        return Some(Command::Cast {
            regiment: me.handle,
            ability: slot,
            target,
        });
    }
    None
}
