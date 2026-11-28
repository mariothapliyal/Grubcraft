//! The prisoners golden replay (`replays/conquest-prisoners-1v1.gcr`): four
//! minutes of Conquest on Ribcage Pass between a scripted Ember Clans side
//! and a scripted Brood side (docs/45). The host raises a regiment of
//! Skitterlings for the Brood midway, alone, and Mounted Samurai and Yari
//! for the Clans a little way off; the Clans hunt the Skitterlings until
//! they give themselves up, and then the Brood buys them back (`Ransom`,
//! replay format 60). Twice more the host raises Skitterlings midway and
//! the Clans run them down: the second lot they take into their own army
//! (`Recruit`), the third they put to death (`Execute`, replay format 61).
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
const MINUTES: u32 = 6;
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
    /// A Clans house: the host puts up two, so the Clans have the supply
    /// to recruit.
    house: u16,
}

struct Script {
    ids: Ids,
    starts: Vec<(i32, i32)>,
    middle: (i32, i32),
    /// Where the script is: 0 hunting the first Skitterlings, 1 waiting
    /// to raise the second, 2 hunting them, 3 waiting to raise the third,
    /// 4 hunting them, 5 done.
    phase: u8,
    /// When the next Skitterlings are raised.
    raise_at: u32,
    /// Where the hunters are raised.
    hunters_from: (i32, i32),
}

/// How long after a ransom or a recruiting the next prey is raised.
const NEXT_PREY: u32 = 5 * TICKS_PER_SECOND;

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
            self.hunters_from = off;
            let mut taken = vec![prey];
            for unit in ids.hunters {
                let at = open(off, &taken);
                taken.push(at);
                out.push(order(0, spawn(unit, at)));
            }
            let home = self.starts[0];
            let mut built: Vec<(i32, i32)> = Vec::new();
            for _ in 0..2 {
                let (x, y) = nearest(world, (home.0 + 8, home.1 + 8), |x, y| {
                    world.can_build(ids.house, (x, y), 0).is_ok()
                        && (built.iter()).all(|&(bx, by)| (bx - x).abs().max((by - y).abs()) >= 5)
                })
                .expect("room for a house");
                built.push((x, y));
                out.push(order(
                    0,
                    Command::SpawnStructure {
                        building: ids.house,
                        tile: (x as i16, y as i16),
                        turn: 0,
                    },
                ));
            }
        }
        if matches!(self.phase, 1 | 3) && t == self.raise_at {
            let prey = open(self.middle, &[]);
            out.push(order(1, spawn(ids.prey, prey)));
            // The second lot wore the first hunters out: fresh ones for the
            // third.
            if self.phase == 3 {
                let mut taken = vec![prey];
                for unit in ids.hunters {
                    let at = open(self.hunters_from, &taken);
                    taken.push(at);
                    out.push(order(0, spawn(unit, at)));
                }
            }
            self.phase += 1;
        }
        // The hunters stay on the Skitterlings while they run, until they
        // give up (an attack-move: a hunter that has lost sight of them may
        // not order an attack). Ransomed ones, out at the Hive, are left be.
        let hunting = matches!(self.phase, 0 | 2 | 4);
        if hunting && t > RAISE_AT && t.is_multiple_of(2 * TICKS_PER_SECOND) {
            let mid = FxVec2::new(Fx::from_int(self.middle.0), Fx::from_int(self.middle.1));
            let prey = (world.regiments.iter())
                .filter(|r| r.owner == 1 && r.unit == ids.prey)
                .min_by_key(|r| (r.position - mid).length_sq_raw());
            if let Some(prey) = prey {
                for r in (world.regiments.iter())
                    .filter(|r| r.owner == 0 && ids.hunters.contains(&r.unit) && !r.morale.broken())
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
        let held = world.recruit_price(0, 1, ids.prey).is_some();
        match self.phase {
            // The Brood buy the first back.
            0 if held => {
                out.push(order(
                    1,
                    Command::Ransom {
                        from: 0,
                        unit: ids.prey,
                    },
                ));
                (self.phase, self.raise_at) = (1, t + NEXT_PREY);
            }
            // The Clans take the second into their army.
            2 if held => {
                out.push(order(
                    0,
                    Command::Recruit {
                        from: 1,
                        unit: ids.prey,
                    },
                ));
                (self.phase, self.raise_at) = (3, t + NEXT_PREY);
            }
            // And put the third to death.
            4 if held => {
                let Some(stockade) = (world.prisoners_held().iter())
                    .find(|h| h.captor == 0 && h.owner == 1)
                    .map(|h| h.stockade)
                else {
                    return;
                };
                out.push(order(0, Command::Execute { stockade }));
                self.phase = 5;
            }
            _ => {}
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
            house: real.building("Farmstead"),
        },
        starts,
        middle,
        phase: 0,
        raise_at: 0,
        hunters_from: (0, 0),
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
    assert_eq!(
        script.phase, 5,
        "the Skitterlings gave themselves up three times"
    );
    let mut replay = rec.finish();
    replay.note = format!(
        "{NAME}: Conquest recorded from gc_game's prisoners_golden.rs, seed {SEED:#x}, Ribcage \
         Pass; Ember Clans v Brood, scripted: Skitterlings run down alone give themselves up, \
         are ransomed, recruited and put to death, {MINUTES} minutes"
    );
    replay
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Skitterlings give themselves up three times, and are ransomed,
    /// recruited and put to death.
    #[test]
    fn the_skitterlings_give_up_and_are_ransomed_recruited_and_put_to_death() {
        use gc_sim::events::Event;
        let replay = record();
        let mut world = replay.new_world();
        let (mut gave_up, mut executed) = (0, false);
        for t in 0..replay.info().ticks {
            world.tick(replay.commands_at(t));
            for e in world.events() {
                match e {
                    Event::Surrendered { owner: 1, .. } => gave_up += 1,
                    Event::Executed { captor: 0, .. } => executed = true,
                    _ => {}
                }
            }
        }
        assert!(gave_up >= 3 && executed, "{gave_up} {executed}");
        assert!(world.prisoners_held().is_empty());
        assert!(world.no_quarter(1, 0));
        let skitterlings = Real::load().unit("Skitterlings");
        assert!(
            (world.regiments.iter()).any(|r| r.owner == 0 && r.unit == skitterlings),
            "the Clans field recruited Skitterlings"
        );
    }
}
