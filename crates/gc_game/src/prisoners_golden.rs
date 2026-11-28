//! The prisoners golden replay (`replays/conquest-prisoners-1v1.gcr`): two
//! minutes of Conquest on Ribcage Pass between a scripted Ember Clans side
//! and a scripted Brood side (docs/45). The host raises a regiment of
//! Skitterlings for the Brood midway, alone, and Mounted Samurai and Yari
//! for the Clans a little way off; the Clans hunt the Skitterlings until
//! they give themselves up, and then the Brood buys them back (`Ransom`,
//! replay format 60).
//!
//! To record it again (only when the script or the data change what it
//! does; a simulation change that only moves the hashes is blessed
//! instead):
//!
//! ```text
//! GOLDEN=conquest-prisoners-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_data::Faction;
use gc_fixed::{Angle, Fx, FxVec2};
use gc_replay::{Recorder, Replay};
use gc_sim::{Command, PlayerCommand, PlayerId, TICKS_PER_SECOND, UnitId, Victories, World};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::conquest_golden::nearest;
use crate::headless::{Headless, Real};

pub const NAME: &str = "conquest-prisoners-1v1";
const SEED: u64 = 0x4e51;
const MINUTES: u32 = 2;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::Brood];
/// When the host raises the regiments.
const RAISE_AT: u32 = 2 * TICKS_PER_SECOND;
/// What each side opens with, Grub and Marrow.
const STORES: (u32, u32) = (2000, 800);

struct Ids {
    prey: UnitId,
    hunters: [UnitId; 2],
    /// Any building, to find open ground with.
    plot: u16,
}

struct Script {
    ids: Ids,
    starts: Vec<(i32, i32)>,
    middle: (i32, i32),
    ransomed: bool,
}

impl Script {
    fn tick(&mut self, world: &World, out: &mut Vec<PlayerCommand>) {
        let t = world.tick_count();
        let ids = &self.ids;
        let order = |player: PlayerId, command| PlayerCommand { player, command };
        let spawn = |unit, (x, y): (i32, i32)| Command::SpawnRegiment {
            unit,
            position: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
            facing: Angle(0),
        };
        let open = |near: (i32, i32), taken: &[(i32, i32)]| {
            nearest(world, near, |x, y| {
                world.can_build(ids.plot, (x - 1, y - 1), 0).is_ok()
                    && (taken.iter()).all(|&(tx, ty)| (tx - x).pow(2) + (ty - y).pow(2) >= 36)
            })
            .expect("open ground")
        };
        if t == RAISE_AT {
            let prey = open(self.middle, &[]);
            out.push(order(1, spawn(ids.prey, prey)));
            let (dx, dy) = (
                self.starts[0].0 - self.middle.0,
                self.starts[0].1 - self.middle.1,
            );
            let len = ((dx * dx + dy * dy) as f32).sqrt().max(1.0);
            let off = (
                self.middle.0 + (dx as f32 / len * 12.0) as i32,
                self.middle.1 + (dy as f32 / len * 12.0) as i32,
            );
            let mut taken = vec![prey];
            for unit in ids.hunters {
                let at = open(off, &taken);
                taken.push(at);
                out.push(order(0, spawn(unit, at)));
            }
        }
        // The hunters stay on the Skitterlings while they run, until they are
        // bought back (an attack-move: a hunter that has lost sight of them
        // may not order an attack).
        if !self.ransomed && t > RAISE_AT && t.is_multiple_of(2 * TICKS_PER_SECOND) {
            let prey = (world.regiments.iter()).find(|r| r.owner == 1 && r.unit == ids.prey);
            if let Some(prey) = prey {
                for r in (world.regiments.iter())
                    .filter(|r| r.owner == 0 && ids.hunters.contains(&r.unit))
                {
                    out.push(order(
                        0,
                        Command::AttackMove {
                            regiment: r.handle,
                            target: prey.position,
                            facing: None,
                            files: 0,
                            brigade: 0,
                            pace: Fx::ZERO,
                        },
                    ));
                }
            }
        }
        if !self.ransomed && world.ransom_price(1, 0, ids.prey).is_some() {
            self.ransomed = true;
            out.push(order(
                1,
                Command::Ransom {
                    from: 0,
                    unit: ids.prey,
                },
            ));
        }
    }
}

/// Play the script through a recorder.
pub fn record() -> Replay {
    let real = Real::load();
    let game = Headless::new(&real, conquest::make_map(0, 0, 0), &FACTIONS, SEED);
    let mut plan = game.plan(Tempo::Swift, Victories::BOTH);
    plan.start[0] = STORES;
    plan.start[1] = STORES;
    let mut world = game.world(&plan);
    let mut rec = Recorder::new(&game.setup(), &world, String::new()).conquest(plan);
    game.open(&mut world, |w, orders| rec.tick(w, orders));
    let starts = game.map.starts();
    let middle = (
        (starts[0].0 + starts[1].0) / 2,
        (starts[0].1 + starts[1].1) / 2,
    );
    let mut script = Script {
        ids: Ids {
            prey: real.unit("Skitterlings"),
            hunters: [real.unit("Mounted Samurai"), real.unit("Yari Ashigaru")],
            plot: real.building("Ancestor Shrine"),
        },
        starts,
        middle,
        ransomed: false,
    };
    let mut out = Vec::new();
    while world.tick_count() < MINUTES * 60 * TICKS_PER_SECOND {
        out.clear();
        script.tick(&world, &mut out);
        let stats = rec.tick(&mut world, &out);
        assert_eq!(
            stats.commands_rejected,
            0,
            "at tick {}: {out:?}",
            world.tick_count() - 1
        );
    }
    assert!(script.ransomed, "the Skitterlings gave themselves up");
    let mut replay = rec.finish();
    replay.note = format!(
        "{NAME}: Conquest recorded from gc_game's prisoners_golden.rs, seed {SEED:#x}, Ribcage \
         Pass; Ember Clans v Brood, scripted: Skitterlings run down alone give themselves up and \
         are ransomed, {MINUTES} minutes"
    );
    replay
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Skitterlings give themselves up, and come back ransomed.
    #[test]
    fn the_skitterlings_give_up_and_are_ransomed() {
        let replay = record();
        let mut world = replay.new_world();
        let (mut gave_up, mut ransomed) = (false, false);
        for t in 0..replay.info().ticks {
            let commands = replay.commands_at(t);
            ransomed |= commands
                .iter()
                .any(|c| matches!(c.command, Command::Ransom { .. }));
            world.tick(commands);
            gave_up |= (world.events().iter())
                .any(|e| matches!(e, gc_sim::events::Event::Surrendered { owner: 1, .. }));
        }
        assert!(gave_up && ransomed, "{gave_up} {ransomed}");
        assert!(world.prisoners_held().is_empty());
    }
}
