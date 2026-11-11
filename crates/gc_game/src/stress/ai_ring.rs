//! Two Generals play Conquest on Four Teeth: each must wall its town all
//! round on its own, leaning the wall on the cliffs and water that close
//! the town already, with a gate facing its enemy, and keep marching its
//! army out through that gate.

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::structures::GATE;
use gc_sim::{TICKS_PER_SECOND, Victories};

use super::arena::Real;
use super::{Report, Scenario};
use crate::conquest;
use crate::headless::Headless;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "ai-ring",
    about: "two Generals each wall their town all round, with a gate, in one of four matches",
    budget_ms: 60_000,
    run,
};

/// The matches played, one after another until each General has walled
/// its town in one of them. One match is not enough to go by: in about
/// three matches in ten a General is kept too poor by the other's raids to
/// pay for its wall inside the limit, and which matches those are changes
/// with any change to how battles play out.
const SEEDS: [u64; 4] = [104, 105, 106, 107];
/// Four Teeth.
const MAP: usize = 1;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::CarrionConcord];
const LIMIT_MINUTES: u32 = 30;

fn run(real: &Real, report: &mut Report) {
    let mut done = [false; FACTIONS.len()];
    let mut gated = true;
    let mut played = 0;
    for seed in SEEDS {
        let (walled, gates) = play(real, report, seed);
        played += 1;
        for p in 0..FACTIONS.len() {
            done[p] |= walled[p].is_some();
            gated &= walled[p].is_none() || gates[p] > 0;
        }
        if done.iter().all(|&d| d) {
            break;
        }
    }
    report.metric("matches played", played);
    report.check(
        format!(
            "each General walls its town all round within {LIMIT_MINUTES} minutes, in one of {} matches",
            SEEDS.len()
        ),
        done.iter().all(|&d| d),
    );
    report.check("a town walled all round has a gate", gated);
}

/// Play the match of `seed`, to the limit or until both towns are walled
/// all round: when each was, and how many gates each has at the end.
fn play(
    real: &Real,
    report: &mut Report,
    seed: u64,
) -> ([Option<u32>; FACTIONS.len()], [usize; FACTIONS.len()]) {
    let game = Headless::new(real, conquest::make_map(MAP, 0, seed), &FACTIONS, seed);
    // Nobody wins, so each has the whole time to wall its town whatever
    // the other does to it.
    let victories = Victories {
        annihilation: false,
        heart: false,
    };
    let mut world = game.world(&game.plan(gc_ui::lobby::Tempo::Standard, victories));
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    for p in 0..FACTIONS.len() {
        minds.add(game.mind(p, Difficulty::General, 7, None));
    }
    let mut walled = [None; FACTIONS.len()];
    let mut cmds = Vec::new();
    let end = LIMIT_MINUTES * 60 * TICKS_PER_SECOND;
    while world.verdict().is_none() && world.tick_count() < end {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        report.tick(&mut world, &cmds);
        let t = world.tick_count();
        if !t.is_multiple_of(TICKS_PER_SECOND) {
            continue;
        }
        for (p, at) in walled.iter_mut().enumerate() {
            if at.is_none() && world.walled_town(p as u8) {
                *at = Some(t);
            }
        }
        if walled.iter().all(Option::is_some) {
            break;
        }
    }
    let at = super::clock;
    let count = |p: u8, gates: bool| {
        (world.structures().unwrap().iter())
            .filter(|s| s.owner == p && (s.line.is_some() || s.gate.is_some()))
            .filter(|s| (real.table.stats[usize::from(s.building)].flags & GATE != 0) == gates)
            .count()
    };
    report.metric(
        format!("match {seed}, game time played"),
        at(Some(world.tick_count())),
    );
    for (p, faction) in FACTIONS.iter().enumerate() {
        report.metric(
            format!("match {seed}, {faction:?} town walled all round at"),
            at(walled[p]),
        );
        report.metric(
            format!("match {seed}, {faction:?} wall tiles and gates"),
            format!("{} and {}", count(p as u8, false), count(p as u8, true)),
        );
    }
    (walled, core::array::from_fn(|p| count(p as u8, true)))
}
