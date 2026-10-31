//! The river golden replay (`replays/conquest-river-1v1.gcr`): three
//! minutes of Conquest on grown land with a river, two scripted Clans
//! sides putting things into it (docs/39-water-engine.md, section 3). Each
//! side's first worker fells the bank woods nearest its main building into
//! the river (a Fell order), and its second tips a raft into the running
//! river nearest it (a Dump order); what they put in rides the current.
//!
//! To record it again (only when the script or the data change what it
//! does; a simulation change that only moves the hashes is blessed
//! instead):
//!
//! ```text
//! GOLDEN=conquest-river-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_data::Faction;
use gc_replay::{Recorder, Replay};
use gc_sim::{Command, PlayerCommand, PlayerId, TICKS_PER_SECOND, Victories, World};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::conquest_golden::{Kit, nearest, regiments_of};
use crate::headless::{Headless, Real};

pub const NAME: &str = "conquest-river-1v1";
/// The seed of the world and of its map (grown land, as wild as the seed
/// picks, at the smallest Conquest size, with a river near both starts).
/// One seed for both, as a lobby match has, so watching the replay makes
/// the same map again.
const SEED: u64 = 3;
const MINUTES: u32 = 3;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::EmberClans];
/// When the workers are sent: the fellers, then the raft.
const FELL_AT: u32 = 2 * TICKS_PER_SECOND;
const DUMP_AT: u32 = 5 * TICKS_PER_SECOND;
/// The side of the area each feller clears.
const FELL_SIDE: i32 = 3;

/// The map: grown land at the smallest Conquest size.
fn map() -> gc_map::Map {
    conquest::make_map(GROWN, 0, SEED)
}

/// The lobby's choice for grown land.
const GROWN: usize = gc_map::SHIPPED.len() + 1 + gc_map::wilds::Wildness::ALL.len();

/// Side `p`'s orders this tick.
fn script(
    world: &World,
    starts: &[(i32, i32)],
    worker: gc_sim::UnitId,
    p: PlayerId,
    out: &mut Vec<PlayerCommand>,
) {
    let t = world.tick_count();
    if t != FELL_AT && t != DUMP_AT {
        return;
    }
    let start = starts[usize::from(p)];
    let mut crew = regiments_of(world, p, worker);
    crew.sort_by_key(|h| h.index);
    let order = |command| PlayerCommand { player: p, command };
    if t == FELL_AT
        && let Some(&regiment) = crew.first()
        && let Some((x, y)) = nearest(world, start, |x, y| {
            world.fells_into(x, y) && world.overgrown([x, y, 1, 1])
        })
    {
        let half = FELL_SIDE / 2;
        out.push(order(Command::Fell {
            regiment,
            tile: ((x - half) as i16, (y - half) as i16),
            size: (FELL_SIDE as u8, FELL_SIDE as u8),
        }));
    }
    if t == DUMP_AT
        && let Some(&regiment) = crew.get(1)
        && let Some((x, y)) = nearest(world, start, |x, y| world.takes_raft(p, x, y))
    {
        out.push(order(Command::Dump {
            regiment,
            tile: (x as i16, y as i16),
        }));
    }
}

/// Play the script through a recorder.
pub fn record() -> Replay {
    let real = Real::load();
    let game = Headless::new(&real, map(), &FACTIONS, SEED);
    let plan = game.plan(Tempo::Swift, Victories::BOTH);
    let mut world = game.world(&plan);
    let mut rec = Recorder::new(&game.setup(), &world, String::new()).conquest(plan);
    game.open(&mut world, |w, orders| rec.tick(w, orders));
    let starts = game.map.starts();
    let worker = Kit::of(&real.data, &real.table, FACTIONS[0]).worker;
    let mut out = Vec::new();
    while world.tick_count() < MINUTES * 60 * TICKS_PER_SECOND {
        out.clear();
        for p in 0..2 {
            script(&world, &starts, worker, p, &mut out);
        }
        let stats = rec.tick(&mut world, &out);
        assert_eq!(
            stats.commands_rejected,
            0,
            "at tick {}: {out:?}",
            world.tick_count() - 1
        );
    }
    let mut replay = rec.finish();
    replay.header.map = Some(GROWN as u8);
    replay.note = format!(
        "{NAME}: Conquest recorded from gc_game's river_golden.rs, seed {SEED}, grown land; Clans v Clans felling and dumping into the river, scripted, \
         {MINUTES} minutes"
    );
    replay
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both sides fell logs and tip rafts into the river, and they ride it.
    #[test]
    fn both_sides_put_logs_and_rafts_in_the_river() {
        let replay = record();
        let mut world = replay.new_world();
        let (mut felled, mut raft) = ([0; 2], false);
        for t in 0..replay.info().ticks {
            world.tick(replay.commands_at(t));
            let debris = world.flood().rivers().debris();
            for p in 0..2u8 {
                felled[usize::from(p)] = felled[usize::from(p)].max(debris.owned(p));
            }
            raft |= debris
                .pieces(world.terrain().growth_side() as i32)
                .any(|s| s.kind == gc_sim::rivers::debris::Kind::Raft);
        }
        assert!(
            felled.iter().all(|&n| n > 0),
            "each side put pieces in: {felled:?}"
        );
        assert!(raft, "a raft went in");
    }
}
