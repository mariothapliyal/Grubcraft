//! A player drops out of a network Conquest match mid-game and the host's
//! computer plays their side until they rejoin (docs/11-networking.md,
//! "Disconnects and rejoining"). Two Generals play Ribcage Pass; at six
//! minutes side 1's player drops: their side stands idle for half a minute
//! (Wait running out, say), then `gc_match::cover` seats the cover mind,
//! which must pick up the town and army where they stand and go on
//! gathering, building and training. When the player is back the mind is
//! taken out again and the side gets no more orders from it.

use std::time::Instant;

use gc_ai::{Difficulty, Minds};
use gc_data::Faction;
use gc_match::cover::{Change, Cover};
use gc_sim::{PlayerId, TICKS_PER_SECOND, Victories, World};

use super::arena::Real;
use super::{Report, Scenario};
use crate::conquest;
use crate::conquest_ai::temper;
use crate::headless::Headless;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "dropped-player",
    about: "the computer takes over a Conquest side whose player dropped, then gives it back",
    budget_ms: 15_000,
    run,
};

const SEED: u64 = 0xD0_0B5E;
/// Ribcage Pass, a one-on-one.
const MAP: usize = 0;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::EmberClans];
const AWAY: PlayerId = 1;

/// Side `p`'s buildings standing, regiments and soldiers.
fn standing(world: &World, p: PlayerId) -> (usize, usize, usize) {
    let buildings = (world.structures()).map_or(0, |s| s.iter().filter(|v| v.owner == p).count());
    let mine = || world.regiments.iter().filter(|r| r.owner == p);
    let soldiers = mine().map(|r| usize::from(r.models)).sum();
    (buildings, mine().count(), soldiers)
}

fn run(real: &Real, report: &mut Report) {
    let game = Headless::new(real, conquest::make_map(MAP, 0, SEED), &FACTIONS, SEED);
    let victories = Victories {
        annihilation: true,
        heart: false,
    };
    let mut world = game.world(&game.plan(gc_ui::lobby::Tempo::Standard, victories));
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    for i in 0..FACTIONS.len() {
        minds.add(game.mind(i, Difficulty::General, temper(SEED, i), None));
    }
    let mut cover = Cover::default();
    let mut changes = Vec::new();
    let mut cmds = Vec::new();
    // The slowest think of the tick the cover mind first looked (seating
    // it and reading the town it takes over).
    let mut first_look_ms = 0.0f64;
    // Play `seconds` with the places in `away` uncommanded; the away side's
    // orders.
    let mut play = |world: &mut World, minds: &mut Minds, away: &[PlayerId], seconds: u32| {
        let mut orders = 0;
        for _ in 0..seconds * TICKS_PER_SECOND {
            cmds.clear();
            let began = Instant::now();
            let roll = SEED ^ u64::from(world.tick_count());
            let new = cover.follow(away, world, minds, &mut cmds, |p| game.cover_mind(p, roll));
            let seated = new.iter().any(|c| matches!(c, Change::Took(_)));
            changes.extend(new);
            minds.tick(world, &mut cmds);
            if seated {
                first_look_ms = began.elapsed().as_secs_f64() * 1000.0;
            }
            orders += cmds.iter().filter(|c| c.player == AWAY).count();
            report.tick(world, &cmds);
            if world.verdict().is_some() {
                break;
            }
        }
        orders
    };

    play(&mut world, &mut minds, &[], 6 * 60);
    // The player drops: their own mind (standing in for them) stops, and
    // for half a minute nobody commands the side.
    minds.remove(AWAY);
    let idle = play(&mut world, &mut minds, &[], 30);
    let before = standing(&world, AWAY);
    let away_orders = play(&mut world, &mut minds, &[AWAY], 6 * 60);
    let after = standing(&world, AWAY);
    let clicks = (minds.get(AWAY))
        .and_then(|m| m.realm())
        .map_or(0, |r| r.clicks().count);
    // Back: the side is theirs again.
    let back = play(&mut world, &mut minds, &[], 10);

    report.metric(
        "side 1 at takeover (buildings, regiments, soldiers)",
        format!("{before:?}"),
    );
    report.metric("six minutes later", format!("{after:?}"));
    report.metric(
        "cover mind's orders / economy clicks",
        format!("{away_orders} / {clicks}"),
    );
    report.metric("first look after seating", format!("{first_look_ms:.1} ms"));
    report.check("nobody commands the side while it waits", idle == 0);
    report.check(
        "the computer took the side and gave it back",
        changes == [Change::Took(AWAY), Change::Gave(AWAY)],
    );
    report.check(
        "the cover mind gave orders and ran the economy",
        away_orders > 0 && clicks > 0,
    );
    report.check(
        "the side built or trained while covered",
        after.0 > before.0 || after.1 > before.1 || after.2 > before.2,
    );
    report.check("the side still stands", after.0 > 0);
    report.check("no orders for the side once it is given back", back == 0);
}
