//! Morale and routing (docs/05-combat.md, "Morale").
//!
//! Armies lose because they break, not because every soldier dies. Each
//! regiment has a morale value, in points, that starts at its unit's
//! `morale` stat. Once a second (staggered by pool index, like melee blows)
//! it is checked:
//! - it falls with casualties taken since the last check, while the regiment
//!   is below half strength and fighting, when hit in the flank or rear,
//!   while pinned with nowhere to give ground (see
//!   [`crate::combat::Combat::push_lines`]), and when fighting while badly
//!   outnumbered nearby;
//! - it recovers when the regiment is out of the fight and losing nobody, up
//!   to the unit's stat scaled by how much of the regiment is left (half the
//!   stat for a regiment that has lost almost everyone), raised or lowered
//!   for the rest of the battle when its side's champion wins or loses
//!   (see [`crate::champions`]).
//!
//! Nerve alone can't break a regiment that still has most of its men
//! (docs/29, "Holding until bloodied"). Each regiment has a *breaking
//! point*, a share of its full hit points rolled when it is raised (see
//! [`breaking_point`]): above it, its morale holds at the edge of breaking
//! however hard it is pressed, and it neither wavers nor routs. Shock works
//! by killing men: a charge or a flank attack cuts it down to its breaking
//! point sooner, and with its nerve spent it breaks the moment it gets
//! there. Below its breaking point its losses cost twice the nerve.
//!
//! Below [`WAVER_BELOW`] points, and within [`WAVER_MARGIN`] of its
//! breaking point, a regiment is *wavering* and deals less damage. At zero
//! it *routs*: it breaks off, ignores orders and runs from
//! the enemy, a little slower than it marches. Its rout shakes friends
//! nearby and heartens the enemies who broke it. After [`RALLY_SECONDS`]
//! with no enemy near, it rallies, wavering. A regiment that routs again
//! within [`SHATTER_SECONDS`] is *shattered*: it runs, then stands where it
//! stops and never takes orders again (later it will recover at a friendly
//! structure). Regiments whose unit has a morale stat of 0 never rout.

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{BATTLE_PACE_PERCENT, Combat, Whose, placement};
use crate::regiments::Regiments;
use crate::vision::Vision;
use crate::{Formation, Stances, TICKS_PER_SECOND, UnitStats, generals};

/// Morale is kept in tenths of a point.
pub const SCALE: i16 = 10;
/// Below this many points a regiment wavers, once it is near its breaking
/// point.
pub const WAVER_BELOW: i16 = 25;
/// Breaking points, in percent of full hit points, by the unit's morale
/// stat: workers and other non-fighters (stat up to [`WORKER_MORALE`])
/// can break at any strength; levies (under [`LINE_MORALE`]) at 45%, line
/// troops at 40%, elite troops ([`ELITE_MORALE`] and up) at 35%.
pub const WORKER_MORALE: u8 = 30;
pub const LINE_MORALE: u8 = 55;
pub const ELITE_MORALE: u8 = 70;
pub const LEVY_BREAK: u8 = 45;
pub const LINE_BREAK: u8 = 40;
pub const ELITE_BREAK: u8 = 35;
/// Each regiment's breaking point is rolled up to this many points either
/// way when it is raised.
pub const BREAK_SPREAD: u8 = 5;
/// Its side's general near it lowers its breaking point this much.
pub const LED_BREAK: u8 = 5;
/// No fighting regiment ever routs above this share of its hit points.
pub const BREAK_CAP: u8 = 50;
const _: () = assert!(LEVY_BREAK + BREAK_SPREAD <= BREAK_CAP);
/// A regiment wavers only within this many points above its breaking point.
pub const WAVER_MARGIN: u8 = 10;
/// Percent of its usual damage a wavering regiment deals.
pub const WAVERING_DAMAGE_PERCENT: u64 = 85;
/// Tenths of a point lost per percent of full strength lost since the last
/// check: losing a tenth of the regiment in one second costs 12 points.
pub const CASUALTY_TENTHS_PER_PERCENT: i32 = 9;
/// The steady drains below are at full pace; a fight scales them by
/// [`crate::combat::BATTLE_PACE_PERCENT`].
/// Points lost each second while below half strength and in melee.
pub const HALF_STRENGTH_DRAIN: i16 = 2;
/// Points lost in a second with hits on the flank, or on the rear.
pub const FLANK_SHOCK: i16 = 3;
const REAR_SHOCK: i16 = 6;
/// Points lost in a second spent pinned: driven back against a friend, an
/// enemy, a river bank or a cliff with nowhere to give ground.
const PINNED_SHOCK: i16 = 3;
/// Bits in `Regiments::shock`.
pub(crate) const SHOCK_FLANK: u8 = 1;
pub(crate) const SHOCK_REAR: u8 = 2;
pub(crate) const SHOCK_PINNED: u8 = 4;
/// Why a regiment's nerve is going, as bits in `Regiments::why` and
/// [`crate::RegimentView::why`], for the screen (docs/29, "Clear orders and
/// feedback"). The steady pressures are set afresh at each once-a-second
/// check; the sudden knocks stay until the regiment is out of the fight and
/// recovering, so a regiment that breaks keeps the reasons it broke for.
/// Hit in the flank.
pub const WHY_FLANK: u16 = 1 << 0;
/// Hit in the rear.
pub const WHY_REAR: u16 = 1 << 1;
/// Driven back with nowhere to give ground.
pub const WHY_PINNED: u16 = 1 << 2;
/// Fighting at two to one or worse.
pub const WHY_OUTNUMBERED: u16 = 1 << 3;
/// Lost many men in the last second.
pub const WHY_LOSSES: u16 = 1 << 4;
/// Down to its breaking point.
pub const WHY_BLOODIED: u16 = 1 << 5;
/// A friend nearby broke and ran.
pub const WHY_ALLY_BROKE: u16 = 1 << 6;
/// Its side's general fell.
pub const WHY_GENERAL: u16 = 1 << 7;
/// Ambushed, or caught by a hidden volley.
pub const WHY_AMBUSH: u16 = 1 << 8;
/// Burned.
pub const WHY_FIRE: u16 = 1 << 9;
/// Swept by a flood.
pub const WHY_WATER: u16 = 1 << 10;
/// Its side's champion lost a duel.
pub const WHY_CHAMPION: u16 = 1 << 11;
/// The steady pressures, recomputed at every check; the rest are knocks.
/// Flank and rear blows count as knocks: a regiment soon turns to face
/// them, but the nerve they cost stays spent.
const WHY_PRESSURE: u16 = WHY_PINNED | WHY_OUTNUMBERED | WHY_LOSSES | WHY_BLOODIED;
/// Losing this many percent of full strength in one second (and at least
/// two soldiers) counts as heavy losses for [`WHY_LOSSES`].
const HEAVY_LOSS_PERCENT: i32 = 8;
/// Enemies and friends within this many tiles count toward being outnumbered.
const LOCAL_TILES: i32 = 12;
/// Points lost each second in melee when nearby enemies outnumber nearby
/// friends two to one, or three to one.
pub const OUTNUMBERED_2: i16 = 2;
pub const OUTNUMBERED_3: i16 = 4;
/// Points regained each second when out of the fight and losing nobody.
const RECOVERY: i16 = 3;
/// A rout costs friends within this many tiles some points, and gives the
/// enemies fighting the routers some.
const ROUT_SHOCK_TILES: i32 = 16;
const ROUT_SHOCK: i16 = 8;
const VICTORY_BOOST: i16 = 5;
/// Routing regiments run at this percent of their marching speed, so
/// pursuers catch them.
const ROUT_PACE_PERCENT: i32 = 90;
/// How far ahead a routing regiment aims each second, in tiles.
const FLEE_TILES: i32 = 10;
/// Seconds with no enemy within `RALLY_CLEAR_TILES` before routers rally.
pub const RALLY_SECONDS: u32 = 10;
const RALLY_CLEAR_TILES: i32 = 20;
/// Routing again within this many seconds of the last rout shatters a regiment.
pub const SHATTER_SECONDS: u32 = 120;
/// Fleeing regiments stay this far inside the map edge, in tiles.
const EDGE_MARGIN: i32 = 2;

/// A regiment's state of mind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MoraleState {
    Steady = 0,
    /// Shaken: deals 15% less damage.
    Wavering = 1,
    /// Running; ignores orders; rallies when the enemy is gone.
    Routing = 2,
    /// Broke twice: runs, then stands, and takes no more orders.
    Shattered = 3,
}

impl MoraleState {
    /// Running away (or past caring): no fighting, no orders.
    pub fn broken(self) -> bool {
        matches!(self, MoraleState::Routing | MoraleState::Shattered)
    }
}

/// Starting (and highest) morale for a regiment of this unit, in tenths.
pub(crate) fn fresh(stats: &UnitStats) -> i16 {
    i16::from(stats.morale) * SCALE
}

/// The breaking point of a unit with this morale stat before the roll, in
/// percent of full hit points (100: it can break at any strength).
pub fn grade_break(morale: u8) -> u8 {
    match morale {
        m if m <= WORKER_MORALE => 100,
        m if m < LINE_MORALE => LEVY_BREAK,
        m if m < ELITE_MORALE => LINE_BREAK,
        _ => ELITE_BREAK,
    }
}

/// The breaking point of a regiment of this unit raised in pool slot
/// `index` with `generation`, in percent of its full hit points: its grade's
/// (see [`LEVY_BREAK`]), moved up to [`BREAK_SPREAD`] either way by a roll
/// from its handle, so every machine and replay gives it the same one.
pub(crate) fn breaking_point(stats: &UnitStats, index: u32, generation: u16) -> u8 {
    let base = grade_break(stats.morale);
    if base >= 100 {
        return base;
    }
    let h = crate::fire::splitmix(u64::from(index) << 16 | u64::from(generation));
    let roll = (h % (2 * u64::from(BREAK_SPREAD) + 1)) as u8;
    base + roll - BREAK_SPREAD
}

/// Regiment `i`'s breaking point as it stands: its own, lowered while its
/// side's general is near it.
pub(crate) fn break_at_now(regs: &Regiments, i: usize) -> u8 {
    // Workers break at any strength, general or not.
    let base = regs.break_at[i];
    if base >= 100 || !regs.led[i] {
        base
    } else {
        base - LED_BREAK
    }
}

/// Regiment `i`'s hit points left, in percent of its full strength's.
fn strength_percent(regs: &Regiments, stats: &UnitStats, i: usize) -> u32 {
    let full = u32::from(regs.full_models[i].max(1)) * u32::from(stats.hp_per_model.max(1));
    let hp = &regs.hp[i * crate::formation::MAX_MODELS as usize..][..usize::from(regs.models[i])];
    hp.iter().map(|&h| u32::from(h)).sum::<u32>() * 100 / full
}

/// Check every regiment whose turn it is this tick.
pub(crate) fn update(
    regs: &mut Regiments,
    combat: &Combat,
    units: &[UnitStats],
    stances: &Stances,
    vision: &Vision,
    tick: u32,
    map_tiles: u32,
) {
    for i in 0..regs.alive.len() {
        // Half a second out of step with the melee blows.
        let turn = tick + i as u32 + TICKS_PER_SECOND / 2;
        if !regs.alive[i] || !turn.is_multiple_of(TICKS_PER_SECOND) {
            continue;
        }
        let stats = &units[regs.unit[i] as usize];
        if stats.morale != 0 {
            if regs.morale_state[i].broken() {
                flee(regs, combat, stances, vision, i, map_tiles);
            } else {
                check(regs, combat, stances, vision, stats, i, tick, map_tiles);
            }
        }
        regs.losses[i] = 0;
        regs.bled[i] = 0;
        regs.shock[i] = 0;
    }
}

/// The once-a-second check for a regiment that is still in the fight.
#[allow(clippy::too_many_arguments)]
fn check(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    vision: &Vision,
    stats: &UnitStats,
    i: usize,
    tick: u32,
    map_tiles: u32,
) {
    let full = i32::from(regs.full_models[i].max(1));
    // Casualties already come at the battle's pace (see
    // `combat::BATTLE_PACE_PERCENT`); the steady drains in a fight are scaled
    // to match, so regiments break from pressure as much as from losses.
    let mut drain = 0;
    let mut why = regs.why[i] & !WHY_PRESSURE;
    if regs.shock[i] & SHOCK_REAR != 0 {
        drain += i32::from(REAR_SHOCK * SCALE);
        why |= WHY_REAR;
    } else if regs.shock[i] & SHOCK_FLANK != 0 {
        drain += i32::from(FLANK_SHOCK * SCALE);
        why |= WHY_FLANK;
    }
    if regs.shock[i] & SHOCK_PINNED != 0 {
        drain += i32::from(PINNED_SHOCK * SCALE);
        why |= WHY_PINNED;
    }
    if regs.losses[i] >= 2 && i32::from(regs.losses[i]) * 100 >= full * HEAVY_LOSS_PERCENT {
        why |= WHY_LOSSES;
    }
    // How far it is from breaking: its general near it steadies it.
    let strength = strength_percent(regs, stats, i);
    let break_at = u32::from(break_at_now(regs, i));
    let bloodied = strength <= break_at;
    if bloodied {
        why |= WHY_BLOODIED;
    }
    // Wounds shake it as much as deaths: hit points lost, counted in men,
    // twice over once it is down to its breaking point.
    let bled =
        i32::from(regs.bled[i]) * 100 * CASUALTY_TENTHS_PER_PERCENT * if bloodied { 2 } else { 1 };
    let mut tenths = -(bled / (full * i32::from(stats.hp_per_model.max(1))));
    if regs.engaged[i].is_some() {
        if i32::from(regs.models[i]) * 2 < full {
            drain += i32::from(HALF_STRENGTH_DRAIN * SCALE);
        }
        let (friends, enemies) = local_strength(regs, combat, stances, vision, i);
        if enemies >= friends.max(1) * 3 {
            drain += i32::from(OUTNUMBERED_3 * SCALE);
            why |= WHY_OUTNUMBERED;
        } else if enemies >= friends.max(1) * 2 {
            drain += i32::from(OUTNUMBERED_2 * SCALE);
            why |= WHY_OUTNUMBERED;
        }
    } else if regs.bled[i] == 0 {
        tenths += i32::from(RECOVERY * SCALE);
    }
    // Its side's general near it steadies it (see [`crate::generals`]).
    if regs.led[i] {
        drain -= i32::from(generals::LEAD * SCALE);
    }
    tenths -= drain * BATTLE_PACE_PERCENT as i32 / 100;
    // A battered regiment never gets all its nerve back: the cap falls from
    // the unit's stat at full strength to half of it with nobody left. A
    // champion's win or loss raises or lowers it for the rest of the battle.
    let cap = i32::from(fresh(stats)) * (full + i32::from(regs.models[i])) / (2 * full)
        + i32::from(regs.party_morale[regs.owner[i] as usize])
        + if tick < regs.heartened_until[i] {
            i32::from(generals::SPEECH * SCALE)
        } else {
            0
        };
    let morale = (i32::from(regs.morale[i]) + tenths).clamp(0, cap.max(1)) as i16;
    // Above its breaking point, Steadfast, or summoned (see
    // [`crate::powers`]), it holds at the edge of breaking.
    let morale = if !bloodied || regs.cant_rout(i) {
        morale.max(1)
    } else {
        morale
    };
    regs.morale[i] = morale;
    // Old knocks fade once it is out of the fight and steady again.
    let steady = morale >= WAVER_BELOW * SCALE;
    regs.why[i] = if steady && regs.engaged[i].is_none() {
        why & WHY_PRESSURE
    } else {
        why
    };
    if morale > 0 {
        regs.morale_state[i] =
            if morale < WAVER_BELOW * SCALE && strength <= break_at + u32::from(WAVER_MARGIN) {
                MoraleState::Wavering
            } else {
                MoraleState::Steady
            };
    } else {
        rout(regs, combat, stances, vision, i, tick, map_tiles);
    }
}

/// Soldiers still fighting near regiment `i`: (its side's, the enemy's).
/// Enemies its side can't see don't count.
fn local_strength(
    regs: &Regiments,
    combat: &Combat,
    stances: &Stances,
    vision: &Vision,
    i: usize,
) -> (u32, u32) {
    let at = placement!(regs, stances);
    let (p, owner, local) = (regs.position[i], regs.owner[i], Fx::from_int(LOCAL_TILES));
    let (mut friends, mut enemies) = (0u32, 0u32);
    combat.each_near(&at, p, local, Whose::Friends(owner), |j| {
        if !regs.morale_state[j].broken() {
            friends += u32::from(regs.models[j]);
        }
    });
    combat.each_near(&at, p, local, Whose::Enemies(owner), |j| {
        if !regs.morale_state[j].broken() && vision.sees(owner, j, regs.owner[j]) {
            enemies += u32::from(regs.models[j]);
        }
    });
    (friends, enemies)
}

/// Regiment `i` gains `points` of morale at once (a champion's win), or
/// loses them if negative (an ambush), down to no less than a tenth of a
/// point: its own next check decides whether it breaks, and a gain may go
/// above its ceiling until that check caps it. Routers and regiments that
/// never rout are left alone.
pub(crate) fn shift(regs: &mut Regiments, i: usize, points: i16) {
    adjust(
        &mut regs.morale[i],
        &mut regs.morale_state[i],
        &mut regs.why[i],
        points,
        0,
    );
}

/// Regiment `i` loses `points` of morale at once, as [`shift`] does, and
/// the screen is told why: one of the `WHY_` bits.
pub(crate) fn shake(regs: &mut Regiments, i: usize, points: i16, why: u16) {
    adjust(
        &mut regs.morale[i],
        &mut regs.morale_state[i],
        &mut regs.why[i],
        -points,
        why,
    );
}

fn adjust(morale: &mut i16, state: &mut MoraleState, reasons: &mut u16, points: i16, why: u16) {
    if state.broken() || *morale <= 0 {
        return;
    }
    *morale = morale.saturating_add(points * SCALE).max(1);
    if points < 0 {
        *reasons |= why;
    }
    // Heartened past wavering, it steadies at once; whether a knock sets it
    // wavering depends on its strength, which its own next check weighs.
    if *morale >= WAVER_BELOW * SCALE {
        *state = MoraleState::Steady;
    }
}

/// Every friend of regiment `i` within `tiles` of it (not `i` itself) that
/// isn't running loses `points` morale, down to 1, booked under `why`.
pub(crate) fn shake_friends(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    i: usize,
    tiles: i32,
    points: i16,
    why: u16,
) {
    let (p, owner) = (regs.position[i], regs.owner[i]);
    let at = placement!(regs, stances);
    let morale = &mut regs.morale;
    let state = &mut regs.morale_state;
    let reasons = &mut regs.why;
    combat.each_near(&at, p, Fx::from_int(tiles), Whose::Friends(owner), |j| {
        if j != i {
            adjust(&mut morale[j], &mut state[j], &mut reasons[j], -points, why);
        }
    });
}

/// Regiment `i` breaks and runs.
#[allow(clippy::too_many_arguments)]
fn rout(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    vision: &Vision,
    i: usize,
    tick: u32,
    map_tiles: u32,
) {
    let again = regs.last_rout[i] != 0
        && tick + 1 - regs.last_rout[i] <= SHATTER_SECONDS * TICKS_PER_SECOND;
    regs.morale_state[i] = if again {
        MoraleState::Shattered
    } else {
        MoraleState::Routing
    };
    regs.morale[i] = 0;
    regs.last_rout[i] = tick + 1;
    regs.rout_clock[i] = 0;
    regs.engaged[i] = None;
    regs.attack_target[i] = None;
    regs.target_facing[i] = None;
    regs.charge[i] = 0;
    // Running men keep no ranks.
    regs.formation[i] = Formation::Line;
    regs.pace[i] = Fx::from_raw(regs.speed[i].raw() * ROUT_PACE_PERCENT / 100);
    // A march is for regiments that take orders.
    regs.march_left[i] = 0;

    // Friends nearby are shaken; the enemies who broke them take heart. The
    // friends' own checks decide whether they break in turn.
    let me = regs.handle(i);
    shake_friends(
        regs,
        combat,
        stances,
        i,
        ROUT_SHOCK_TILES,
        ROUT_SHOCK,
        WHY_ALLY_BROKE,
    );
    let (p, owner, near) = (
        regs.position[i],
        regs.owner[i],
        Fx::from_int(ROUT_SHOCK_TILES),
    );
    let at = placement!(regs, stances);
    let morale = &mut regs.morale;
    let state = &mut regs.morale_state;
    let reasons = &mut regs.why;
    let engaged = &regs.engaged;
    combat.each_near(&at, p, near, Whose::Enemies(owner), |j| {
        if engaged[j] == Some(me) {
            adjust(
                &mut morale[j],
                &mut state[j],
                &mut reasons[j],
                VICTORY_BOOST,
                0,
            );
        }
    });
    flee(regs, combat, stances, vision, i, map_tiles);
}

/// Steer a routing regiment away from the enemy, and rally (or, shattered,
/// stand) once the enemy has been gone long enough. Enemies its side can't
/// see don't count.
fn flee(
    regs: &mut Regiments,
    combat: &Combat,
    stances: &Stances,
    vision: &Vision,
    i: usize,
    map_tiles: u32,
) {
    let p = regs.position[i];
    let owner = regs.owner[i];
    let at = placement!(regs, stances);
    let mut away = (0i64, 0i64);
    let mut near = false;
    combat.each_near(
        &at,
        p,
        Fx::from_int(RALLY_CLEAR_TILES),
        Whose::Enemies(owner),
        |j| {
            if !vision.sees(owner, j, regs.owner[j]) {
                return;
            }
            near = true;
            let d = p - at.position[j];
            // Closer enemies push harder: weight by 1 / distance.
            let len = i64::from(d.length().raw().max(1));
            away.0 += i64::from(d.x.raw()) * 65_536 / len;
            away.1 += i64::from(d.y.raw()) * 65_536 / len;
        },
    );
    if near {
        regs.rout_clock[i] = 0;
    } else {
        regs.rout_clock[i] = regs.rout_clock[i].saturating_add(TICKS_PER_SECOND as u16);
        if u32::from(regs.rout_clock[i]) >= RALLY_SECONDS * TICKS_PER_SECOND {
            regs.target[i] = None;
            if regs.morale_state[i] == MoraleState::Routing {
                // Rally: wavering, with a little heart back.
                regs.morale_state[i] = MoraleState::Wavering;
                regs.morale[i] = WAVER_BELOW * SCALE / 2;
                regs.why[i] = 0;
                regs.pace[i] = regs.speed[i];
            }
            return;
        }
    }
    let dir = if away != (0, 0) {
        let v = FxVec2::new(
            Fx::from_raw(away.0.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
            Fx::from_raw(away.1.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32),
        );
        Angle::of_vec(v).to_vec()
    } else if let Some(t) = regs.target[i] {
        // Nobody close: keep running the way it was going.
        let d = t - p;
        if d == FxVec2::ZERO {
            return;
        }
        Angle::of_vec(d).to_vec()
    } else {
        regs.facing[i].to_vec()
    };
    let mut target = p + dir * Fx::from_int(FLEE_TILES);
    if map_tiles > 0 {
        let lo = Fx::from_int(EDGE_MARGIN);
        let hi = Fx::from_int(map_tiles as i32 - EDGE_MARGIN);
        target = FxVec2::new(target.x.clamp(lo, hi), target.y.clamp(lo, hi));
    }
    regs.target[i] = (target != p).then_some(target);
    regs.target_facing[i] = None;
}
