//! The hero golden replay (`replays/conquest-heroes-1v1.gcr`): three
//! minutes of Conquest on Ribcage Pass between a scripted Ember Clans side
//! and a scripted Carrion Concord side, playing heroes and their abilities
//! (docs/43, docs/44). The host puts up an Ancestor Shrine and a Vault of
//! Names, and raises Yari for the Clans and Bone Pikes for the Concord
//! midway; each side hires its first hero (the Daimyo, the Usurer). Once
//! they are out, both heroes and their regiments march on each other, and
//! each second a hero casts what is ready: the Daimyo his Rally when foes
//! are near (`Cast` at the hero), the Usurer his Leech on the nearest foe
//! (`Cast` at a regiment, walking into reach first when need be).
//!
//! To record it again (only when the script or the data change what it
//! does; a simulation change that only moves the hashes is blessed
//! instead):
//!
//! ```text
//! GOLDEN=conquest-heroes-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_data::Faction;
use gc_fixed::{Angle, Fx, FxVec2};
use gc_replay::{Recorder, Replay};
use gc_sim::abilities::CastTarget;
use gc_sim::{
    Command, PlayerCommand, PlayerId, RegimentHandle, TICKS_PER_SECOND, UnitId, Victories, World,
};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::conquest_golden::{hero_of, nearest, own};
use crate::headless::{Headless, Real};

pub const NAME: &str = "conquest-heroes-1v1";
const SEED: u64 = 0x4e50;
const MINUTES: u32 = 3;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::CarrionConcord];
/// When the host puts up the hero buildings, the sides hire their heroes,
/// and the host raises their regiments.
const BUILD_AT: u32 = 2 * TICKS_PER_SECOND;
const HIRE_AT: u32 = 3 * TICKS_PER_SECOND;
const RAISE_AT: u32 = 45 * TICKS_PER_SECOND;
/// When the heroes and their regiments march on each other.
const MARCH_AT: u32 = 50 * TICKS_PER_SECOND;
/// Regiments each side raises.
const REGIMENTS: usize = 3;
/// What each side opens with, Grub and Marrow.
const STORES: (u32, u32) = (2000, 800);
/// How near a foe must be for the Daimyo to rally, in tiles.
const RALLY_TILES: i32 = 10;

/// The ids the script goes by.
struct Ids {
    shrine: u16,
    vault: u16,
    heroes: [UnitId; 2],
    troops: [UnitId; 2],
}

impl Ids {
    fn of(real: &Real) -> Ids {
        Ids {
            shrine: real.building("Ancestor Shrine"),
            vault: real.building("Vault of Names"),
            heroes: [real.unit("The Daimyo"), real.unit("The Usurer")],
            troops: [real.unit("Yari Ashigaru"), real.unit("Bone Pikes")],
        }
    }
}

struct Script {
    ids: Ids,
    starts: Vec<(i32, i32)>,
    middle: (i32, i32),
}

impl Script {
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
        if t == BUILD_AT {
            for (p, building) in [(0, ids.shrine), (1, ids.vault)] {
                let near = toward(self.starts[usize::from(p)], self.middle, 10);
                let (x, y) = nearest(world, near, |x, y| {
                    world.can_build(building, (x, y), 0).is_ok()
                })
                .expect("room to build");
                out.push(order(
                    p,
                    Command::SpawnStructure {
                        building,
                        tile: (x as i16, y as i16),
                        turn: 0,
                    },
                ));
            }
        }
        if t == HIRE_AT {
            for (p, building) in [(0, ids.shrine), (1, ids.vault)] {
                let structure = own(world, p, Some(building))
                    .first()
                    .expect("hero building")
                    .handle;
                let unit = ids.heroes[usize::from(p)];
                out.push(order(p, Command::Train { structure, unit }));
            }
        }
        // Once the heroes are out, so the regiments don't take the supply
        // the heroes wait on.
        if t == RAISE_AT {
            let mut taken: Vec<(i32, i32)> = Vec::new();
            for p in [0u8, 1] {
                let front = toward(self.middle, self.starts[usize::from(p)], 14);
                for _ in 0..REGIMENTS {
                    let (x, y) = nearest(world, front, |x, y| {
                        world.can_build(ids.shrine, (x - 1, y - 1), 0).is_ok()
                            && taken
                                .iter()
                                .all(|&(tx, ty)| (tx - x).pow(2) + (ty - y).pow(2) >= 36)
                    })
                    .expect("open ground");
                    taken.push((x, y));
                    out.push(order(
                        p,
                        Command::SpawnRegiment {
                            unit: ids.troops[usize::from(p)],
                            position: FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
                            facing: Angle(0),
                        },
                    ));
                }
            }
        }
        if t == MARCH_AT {
            let at = FxVec2::new(Fx::from_int(self.middle.0), Fx::from_int(self.middle.1));
            let marching = |u: UnitId| ids.heroes.contains(&u) || ids.troops.contains(&u);
            for r in world
                .regiments
                .iter()
                .filter(|r| r.owner < 2 && marching(r.unit))
            {
                out.push(order(
                    r.owner,
                    Command::AttackMove {
                        regiment: r.handle,
                        target: at,
                        facing: None,
                        files: 0,
                        brigade: 0,
                        pace: Fx::ZERO,
                    },
                ));
            }
        }
        if t > MARCH_AT && t.is_multiple_of(TICKS_PER_SECOND) {
            let foe_near = |p: PlayerId, at: FxVec2, tiles: i32| {
                (world.regiments.iter())
                    .filter(|r| r.owner == 1 - p && !r.morale.broken())
                    .map(|r| (r.handle, (r.position - at).length_sq_raw()))
                    .min_by_key(|&(_, d)| d)
                    .filter(|&(_, d)| {
                        d <= (FxVec2::new(Fx::from_int(tiles), Fx::ZERO)).length_sq_raw()
                    })
                    .map(|(h, _)| h)
            };
            if let Some(daimyo) = hero_of(world, 0)
                && let Some(at) = world.regiments.position(daimyo)
                && foe_near(0, at, RALLY_TILES).is_some()
                && world.cast_check(0, daimyo, 0, CastTarget::Hero).is_ok()
            {
                out.push(cast(0, daimyo, CastTarget::Hero));
            }
            if let Some(usurer) = hero_of(world, 1)
                && let Some(at) = world.regiments.position(usurer)
                && let Some(foe) = foe_near(1, at, 30)
                && world
                    .cast_check(1, usurer, 0, CastTarget::Regiment(foe))
                    .is_ok()
                && world.hero_of(usurer).is_some_and(|h| h.pending.is_none())
            {
                out.push(cast(1, usurer, CastTarget::Regiment(foe)));
            }
        }
    }
}

fn cast(player: PlayerId, regiment: RegimentHandle, target: CastTarget) -> PlayerCommand {
    PlayerCommand {
        player,
        command: Command::Cast {
            regiment,
            ability: 0,
            target,
        },
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
        ids: Ids::of(&real),
        starts,
        middle,
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
        "{NAME}: Conquest recorded from gc_game's hero_golden.rs, seed {SEED:#x}, Ribcage Pass; \
         Ember Clans v Carrion Concord, scripted: the Daimyo and the Usurer hired, Rally and Leech \
         cast, {MINUTES} minutes"
    );
    replay
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both heroes are hired, and both cast their abilities.
    #[test]
    fn the_heroes_are_hired_and_cast() {
        let replay = record();
        let mut world = replay.new_world();
        let mut went_off = [0u32; 2];
        for t in 0..replay.info().ticks {
            world.tick(replay.commands_at(t));
            for e in world.events() {
                if let gc_sim::events::Event::Cast { hero, .. } = *e
                    && let Some(p) = world.regiments.owner(hero)
                {
                    went_off[usize::from(p)] += 1;
                }
            }
        }
        assert!(went_off[0] >= 1, "the Daimyo rallied: {went_off:?}");
        assert!(went_off[1] >= 1, "the Usurer leeched: {went_off:?}");
    }
}
