//! The orders of docs/05-combat.md, "Hold, Skirmish and Run", as a computer
//! player gives them (rule (c): an order isn't done until the AI uses it).
//!
//! Once a second, after its other orders, a mind of Veteran level and up
//! ([`crate::difficulty::Knobs::stances`]) looks at each of its regiments
//! and gives at most one of these, each costing an order as a person's
//! would:
//! - **Column** for a regiment marching outside any battle (in Conquest,
//!   between fights) with no enemy within [`COLUMN_FAR`] tiles; back to its
//!   battle formation once one comes within [`COLUMN_NEAR`] or a battle's
//!   Captain takes it (not when it stops, so a march made of many short
//!   moves isn't an order at every halt).
//! - **Wedge** for cavalry that can form one, out of a fight, while it
//!   strikes (a hammer, ambusher or hunter, or outside any battle): its
//!   charges hit harder. Given any other part (screen, reserve,
//!   recovering) it stands in line, which defends better.
//! - **Run** for a hammer that has committed to its strike, a hunter after
//!   routers, both with [`RUN_FROM`] to [`RUN_UPTO`] tiles still to go, and
//!   a wavering regiment pulling back with an enemy within [`RUN_FROM`].
//! - **Hold** for the line (anchor and wings) standing to receive the
//!   enemy once it comes within [`HOLD_WITHIN`]: it gives ground slowly and
//!   doesn't chase. The Captain's next move or attack lifts it.
//! - **Brace** for spears or pikes standing with charging cavalry riding
//!   at their front within [`BRACE_WITHIN`] tiles, or marching with it
//!   within [`BRACE_HALT`] (inside about 45 degrees of straight on, the arc
//!   braced points stop a charge in); the
//!   points come up again once none is within [`BRACE_LIFT`].
//! - **Skirmish** for every regiment that shoots and marches at least
//!   [`SKIRMISH_SPEED`] (not slow engines): it steps back from melee on
//!   its own, quicker than the Captain could tell it to.

use gc_sim::{Command, Formation, PlayerId, RegimentHandle};

use crate::captain::{Captain, Posture, Role};
use crate::perception::{Perception, Seen};

/// No enemy this near (tiles, centre to centre): march in column.
pub const COLUMN_FAR: f32 = 50.0;
/// An enemy this near: form up out of column.
pub const COLUMN_NEAR: f32 = 35.0;
/// Tiles to the target between which a hammer or hunter runs.
pub const RUN_FROM: f32 = 8.0;
pub const RUN_UPTO: f32 = 40.0;
/// The line stands fast once an enemy is this near.
pub const HOLD_WITHIN: f32 = 30.0;
/// Shooters slower than this (tiles a second) don't skirmish.
pub const SKIRMISH_SPEED: f32 = 2.5;
/// Spears brace once charging cavalry comes at their front this near
/// (tiles between the blocks), and lift their points again once none is
/// within [`BRACE_LIFT`].
pub const BRACE_WITHIN: f32 = 30.0;
pub const BRACE_LIFT: f32 = 45.0;
/// Spears on the march stop and brace only for cavalry this near.
pub const BRACE_HALT: f32 = 12.0;
/// How squarely a foe must be ahead of the spears, and coming at them, to
/// be met braced: the cosine of the angle off straight on (about 45
/// degrees, the arc braced points stop a charge in).
const BRACE_ARC_COS: f32 = 0.7;

/// The order `r` should be given now, if any. `part` is what the battle's
/// Captain has it doing (see [`Captain::part_of`]).
pub fn wanted(
    view: &Perception,
    party: PlayerId,
    r: &Seen,
    captain: Option<&Captain>,
) -> Option<Command> {
    if r.broken() || r.engaged {
        return None;
    }
    let regiment = r.handle;
    let unit = view.unit(r);
    let nearest = view
        .regiments
        .iter()
        .filter(|f| view.hostile(party, f.owner) && !f.broken())
        .map(|f| f.pos.distance(r.pos))
        .fold(f32::INFINITY, f32::min);
    let part = captain.and_then(|c| c.part_of(regiment));
    let role = part.map(|(role, ..)| role);
    // Horse that strikes rides in a wedge; horse that screens, recovers or
    // waits stands in line, which defends better.
    let strikes = matches!(
        role,
        None | Some(Role::Hammer | Role::Ambusher | Role::Hunter)
    );
    let battle = if unit.stats.can_wedge() && strikes {
        Formation::Wedge
    } else {
        Formation::Line
    };
    let formation = |formation| Command::SetFormation {
        regiment,
        formation,
    };
    // Into column on a march outside any battle (the Captain forms its own
    // line), out of it as the enemy comes near or a battle takes it.
    match r.formation {
        Formation::Line | Formation::Wedge
            if part.is_none() && r.moving && nearest > COLUMN_FAR =>
        {
            return Some(formation(Formation::Column));
        }
        Formation::Column if part.is_some() || nearest < COLUMN_NEAR => {
            return Some(formation(battle));
        }
        f @ (Formation::Line | Formation::Wedge) if f != battle => {
            return Some(formation(battle));
        }
        _ => {}
    }
    // At the run, where it pays to be there first.
    let gap_to = |t: Option<RegimentHandle>| {
        t.and_then(|t| view.get(t)).map_or(f32::INFINITY, |t| {
            t.pos.distance(r.pos) - t.radius() - r.radius()
        })
    };
    let may_run = r.moving
        && !r.running
        && !r.winded
        && r.formation != Formation::ShieldWall
        && r.formation != Formation::Column;
    let runs = match part {
        Some((Role::Hammer | Role::Ambusher, target, true)) | Some((Role::Hunter, target, _)) => {
            (RUN_FROM..=RUN_UPTO).contains(&gap_to(target))
        }
        Some((Role::Recover, ..)) => nearest < RUN_FROM + r.radius() * 2.0,
        _ => false,
    };
    if may_run && runs {
        return Some(Command::Run { regiment });
    }
    // Spears set their points against cavalry coming at their front, and
    // lift them once it has gone.
    if unit.stats.reach {
        let riding_at_us = |within: f32| {
            view.regiments.iter().any(|f| {
                let to_us = (r.pos - f.pos).normalize_or_zero();
                view.hostile(party, f.owner)
                    && !f.broken()
                    && !f.engaged
                    && view.unit(f).stats.can_wedge()
                    && f.pos.distance(r.pos) - f.radius() - r.radius() < within
                    && r.facing.dot(-to_us) > BRACE_ARC_COS
                    && f.facing.dot(to_us) > BRACE_ARC_COS
            })
        };
        let within = if r.moving { BRACE_HALT } else { BRACE_WITHIN };
        if !r.braced && riding_at_us(within) {
            return Some(Command::Brace { regiment, on: true });
        }
        if r.braced && !riding_at_us(BRACE_LIFT) {
            return Some(Command::Brace {
                regiment,
                on: false,
            });
        }
    }
    // The line stands fast to receive the enemy.
    if matches!(role, Some(Role::Anchor | Role::Wing))
        && captain.is_some_and(|c| c.posture == Posture::Hold)
        && !r.moving
        && !r.hold
        && nearest < HOLD_WITHIN
    {
        return Some(Command::Hold { regiment, on: true });
    }
    // Shooters keep away from melee.
    if unit.stats.can_skirmish() && unit.speed >= SKIRMISH_SPEED && !r.skirmish {
        return Some(Command::Skirmish { regiment, on: true });
    }
    None
}
