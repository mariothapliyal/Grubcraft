//! The faction-rules golden replay (`replays/conquest-evolve-tithe-1v1.gcr`):
//! four minutes of Conquest on Ribcage Pass between a scripted Brood side
//! and a scripted Concord side, playing the rules of docs/03 the shared
//! goldens otherwise leave out. The host puts up a Great Hive and a
//! Chrysalis Chamber for the Brood and a Reliquary midway for the Concord,
//! and raises a few regiments for each. The Brood cocoons two Skitterlings
//! by its hall, one into each of their forms (`Evolve`), and sends Chitin
//! Guard against the Concord's Bone Pikes and crossbows by the Reliquary;
//! the dead pay the Concord Tithe, and it raises the Bound at the fullest
//! grave.
//!
//! To record it again (only when the script or the data change what it
//! does; a simulation change that only moves the hashes is blessed
//! instead):
//!
//! ```text
//! GOLDEN=conquest-evolve-tithe-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_data::Faction;
use gc_fixed::{Angle, Fx, FxVec2};
use gc_replay::{Recorder, Replay};
use gc_sim::{
    Command, PlayerCommand, PlayerId, PowerEffect, PowerTarget, RegimentHandle, TICKS_PER_SECOND,
    UnitId, Victories, World,
};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::conquest_golden::nearest;
use crate::headless::{Headless, Real};

pub const NAME: &str = "conquest-evolve-tithe-1v1";
const SEED: u64 = 0x7174;
const MINUTES: u32 = 4;
const FACTIONS: [Faction; 2] = [Faction::Brood, Faction::CarrionConcord];
/// When the host puts up the buildings, raises the regiments, and the
/// Brood cocoons and attacks.
const BUILD_AT: u32 = 2 * TICKS_PER_SECOND;
const RAISE_AT: u32 = 3 * TICKS_PER_SECOND;
const COCOON_AT: u32 = 5 * TICKS_PER_SECOND;
const ATTACK_AT: u32 = 6 * TICKS_PER_SECOND;
/// The regiments of Chitin Guard the Brood sends, and of Bone Pikes the
/// Concord holds by its Reliquary with them.
const GUARDS: usize = 5;
const PIKES: usize = 4;
/// What the Brood opens with, Grub and Marrow.
const BROOD_STORES: (u32, u32) = (1500, 500);
/// The fewest corpses the Concord raises the Bound from.
const RAISE_FROM: u32 = 8;

/// The ids the script goes by.
struct Ids {
    great_hive: u16,
    chamber: u16,
    reliquary: u16,
    skitterlings: UnitId,
    chitin_guard: UnitId,
    pikes: UnitId,
    crossbows: UnitId,
    /// The Skitterlings' two forms.
    forms: [UnitId; 2],
    raise: u8,
}

impl Ids {
    fn of(real: &Real) -> Ids {
        let power = |name: &str| {
            (real.data.powers.iter())
                .position(|p| p.name() == name && p.built())
                .expect("the Concord's power") as u8
        };
        Ids {
            great_hive: real.building("Great Hive"),
            chamber: real.building("Chrysalis Chamber"),
            reliquary: real.building("Reliquary"),
            skitterlings: real.unit("Skitterlings"),
            chitin_guard: real.unit("Chitin Guard"),
            pikes: real.unit("Bone Pikes"),
            crossbows: real.unit("Crossbow Auditors"),
            forms: [real.unit("Spitters"), real.unit("Burrowers")],
            raise: power("Raise the Bound"),
        }
    }
}

/// The script's state.
struct Script {
    ids: Ids,
    starts: Vec<(i32, i32)>,
    /// Where the two sides meet: midway between the starts, on open
    /// ground.
    middle: (i32, i32),
    raised: bool,
}

impl Script {
    /// Open ground for a regiment near `at`, where a small building could
    /// go up, and 6 tiles or more from each of `taken`.
    fn open_near(&self, world: &World, at: (i32, i32), taken: &[(i32, i32)]) -> (i32, i32) {
        nearest(world, at, |x, y| {
            world.can_build(self.ids.chamber, (x - 1, y - 1), 0).is_ok()
                && taken
                    .iter()
                    .all(|&(tx, ty)| (tx - x).pow(2) + (ty - y).pow(2) >= 36)
        })
        .expect("open ground")
    }

    fn tick(&mut self, world: &World, out: &mut Vec<PlayerCommand>) {
        let t = world.tick_count();
        let ids = &self.ids;
        let order = |player: PlayerId, command| PlayerCommand { player, command };
        let toward = |from: (i32, i32), to: (i32, i32), tiles: i32| {
            let (dx, dy) = ((to.0 - from.0) as f32, (to.1 - from.1) as f32);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            (
                from.0 + (dx / len * tiles as f32) as i32,
                from.1 + (dy / len * tiles as f32) as i32,
            )
        };
        let spawn = |p: PlayerId, unit: UnitId, (x, y): (i32, i32)| {
            order(
                p,
                Command::SpawnRegiment {
                    unit,
                    position: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
                    facing: Angle(0),
                },
            )
        };
        let own = |p: PlayerId, unit: UnitId| -> Vec<RegimentHandle> {
            let mut h: Vec<RegimentHandle> = (world.regiments.iter())
                .filter(|r| r.owner == p && r.unit == unit)
                .map(|r| r.handle)
                .collect();
            h.sort_by_key(|h| h.index);
            h
        };
        if t == BUILD_AT {
            let hall = toward(self.starts[0], self.middle, 12);
            let at = |b: u16, near: (i32, i32)| {
                nearest(world, near, |x, y| world.can_build(b, (x, y), 0).is_ok())
                    .expect("room to build")
            };
            let hive = at(ids.great_hive, hall);
            out.push(order(
                0,
                Command::SpawnStructure {
                    building: ids.great_hive,
                    tile: (hive.0 as i16, hive.1 as i16),
                    turn: 0,
                },
            ));
            let chamber = at(ids.chamber, toward(self.starts[0], self.middle, 4));
            out.push(order(
                0,
                Command::SpawnStructure {
                    building: ids.chamber,
                    tile: (chamber.0 as i16, chamber.1 as i16),
                    turn: 0,
                },
            ));
            let relic = at(ids.reliquary, self.middle);
            out.push(order(
                1,
                Command::SpawnStructure {
                    building: ids.reliquary,
                    tile: (relic.0 as i16, relic.1 as i16),
                    turn: 0,
                },
            ));
        }
        if t == RAISE_AT {
            let mut taken = Vec::new();
            let mut put = |p: PlayerId, unit: UnitId, near: (i32, i32), out: &mut Vec<_>| {
                let at = self.open_near(world, near, &taken);
                taken.push(at);
                out.push(spawn(p, unit, at));
            };
            let brood_home = toward(self.starts[0], self.middle, 10);
            put(0, ids.skitterlings, brood_home, out);
            put(0, ids.skitterlings, brood_home, out);
            let front = toward(self.middle, self.starts[0], 10);
            for _ in 0..GUARDS {
                put(0, ids.chitin_guard, front, out);
            }
            let held = toward(self.middle, self.starts[1], 2);
            for _ in 0..PIKES {
                put(1, ids.pikes, held, out);
            }
            let behind = toward(self.middle, self.starts[1], 8);
            put(1, ids.crossbows, behind, out);
            put(1, ids.crossbows, behind, out);
        }
        if t == COCOON_AT {
            for (regiment, into) in own(0, ids.skitterlings).into_iter().zip(ids.forms) {
                if world.can_evolve(0, regiment, into).is_ok() {
                    out.push(order(0, Command::Evolve { regiment, into }));
                }
            }
        }
        if t >= ATTACK_AT && (t - ATTACK_AT).is_multiple_of(4 * TICKS_PER_SECOND) {
            let foes: Vec<_> = (world.regiments.iter())
                .filter(|r| r.owner == 1 && [ids.pikes, ids.crossbows].contains(&r.unit))
                .map(|r| (r.handle, r.position))
                .collect();
            for r in world
                .regiments
                .iter()
                .filter(|r| r.owner == 0 && r.unit == ids.chitin_guard && !r.morale.broken())
                .filter(|r| t == ATTACK_AT || !r.engaged)
            {
                let Some(&(target, at)) = foes
                    .iter()
                    .min_by_key(|(_, at)| (*at - r.position).length_sq_raw())
                else {
                    continue;
                };
                // Charged once; after that, those out of the fight close on
                // the nearest foe (a charge the foe's running away refuses).
                let command = if t == ATTACK_AT {
                    Command::Attack {
                        regiment: r.handle,
                        target,
                    }
                } else {
                    Command::MoveTo {
                        regiment: r.handle,
                        target: at,
                        facing: None,
                        files: 0,
                        pace: Fx::ZERO,
                    }
                };
                out.push(order(0, command));
            }
        }
        // The Concord raises the Bound, looking once a second, as soon as it
        // can pay and a grave holds enough.
        if t.is_multiple_of(TICKS_PER_SECOND) {
            let tiles = |p: u8| match world.powers()[usize::from(p)].effect {
                PowerEffect::Raise { tiles, .. } => tiles,
                _ => 0,
            };
            if !self.raised && world.can_use_power(1, ids.raise).is_ok() {
                let fullest = (world.graves().map(|(at, _)| at))
                    .map(|at| (at, world.corpses_near(at, tiles(ids.raise))))
                    .filter(|&(_, n)| n >= RAISE_FROM)
                    .max_by_key(|&(_, n)| n);
                if let Some((at, _)) = fullest {
                    out.push(order(
                        1,
                        Command::UsePower {
                            power: ids.raise,
                            target: PowerTarget::Point(at),
                        },
                    ));
                    self.raised = true;
                }
            }
        }
    }
}

/// Play the script through a recorder.
pub fn record() -> Replay {
    let real = Real::load();
    let game = Headless::new(&real, conquest::make_map(0, 0, 0), &FACTIONS, SEED);
    let mut plan = game.plan(Tempo::Swift, Victories::BOTH);
    // The script gathers nothing: the Brood opens with enough to pay for
    // its two cocoons.
    plan.start[0] = BROOD_STORES;
    let mut world = game.world(&plan);
    let mut rec = Recorder::new(&game.setup(), &world, String::new()).conquest(plan);
    game.open(&mut world, |w, orders| rec.tick(w, orders));
    let starts = game.map.starts();
    let middle = (
        (starts[0].0 + starts[1].0) / 2,
        (starts[0].1 + starts[1].1) / 2,
    );
    let mut script = Script {
        ids: Ids::of(&real),
        starts,
        middle,
        raised: false,
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
    let mut replay = rec.finish();
    replay.note = format!(
        "{NAME}: Conquest recorded from gc_game's faction_golden.rs, seed {SEED:#x}, Ribcage Pass; \
         Brood v Carrion Concord, scripted: cocoons, a Reliquary's Tithe, Raise the Bound, \
         {MINUTES} minutes"
    );
    replay
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both Skitterlings come out evolved, the Concord earns Tithe, raises
    /// the Bound.
    #[test]
    fn the_brood_evolves_and_the_concord_raises_the_bound() {
        let replay = record();
        let real = Real::load();
        let ids = Ids::of(&real);
        let mut world = replay.new_world();
        let (mut evolved, mut tithe, mut casts) = (0, 0, 0);
        for t in 0..replay.info().ticks {
            let orders = replay.commands_at(t);
            casts += (orders.iter())
                .filter(|o| matches!(o.command, Command::UsePower { .. }))
                .count();
            world.tick(orders);
            let forms = (world.regiments.iter())
                .filter(|r| r.owner == 0 && ids.forms.contains(&r.unit))
                .count();
            evolved = evolved.max(forms);
            tithe = tithe.max(world.bank(1).map_or(0, |b| b.tithe));
        }
        assert_eq!(evolved, 2, "both Skitterlings came out evolved");
        assert!(tithe > 0, "the dead paid Tithe");
        assert_eq!(casts, 1, "the Bound raised");
    }
}
