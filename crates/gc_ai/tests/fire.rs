//! The computer players and fire (docs/32-changing-land.md, "Woods that
//! burn"): a regiment caught in burning woods marches out of them.

use gc_ai::{Difficulty, Mind, MindConfig, Minds, Terrain};
use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, Command, Cover, Ground, MatchSetup, PlayerCommand, Sky, World};

const MAP: u32 = 64;
const AI: u8 = 0;
const FIRER: u8 = 1;

#[test]
fn a_mind_marches_its_troops_out_of_burning_woods() {
    let cover: Vec<u8> = (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if (16..48).contains(&x) && (16..48).contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect();
    let mut world = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: Sky::STILL,
        generals: false,
    });
    let at = |x: i32, y: i32| FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF);
    // The mind's spears deep in the wood; an enemy beside them sets it
    // alight (and burns with it).
    world.tick(&[
        PlayerCommand {
            player: AI,
            command: Command::SpawnRegiment {
                unit: 0,
                position: at(32, 32),
                facing: Angle::ZERO,
            },
        },
        PlayerCommand {
            player: FIRER,
            command: Command::SpawnRegiment {
                unit: 0,
                position: at(32, 40),
                facing: Angle::ZERO,
            },
        },
    ]);
    let handle = |w: &World, owner| {
        w.regiments
            .iter()
            .find(|r| r.owner == owner)
            .unwrap()
            .handle
    };
    let (spears, firer) = (handle(&world, AI), handle(&world, FIRER));
    let mut out = vec![PlayerCommand {
        player: FIRER,
        command: Command::Burn { regiment: firer },
    }];
    let mut minds = Minds::new(Terrain::new(&[], MAP).with_cover(cover.clone()));
    minds.add(Mind::new(MindConfig {
        party: AI,
        difficulty: Difficulty::General,
        personality: 0,
        seed: 3,
    }));
    let mut burned = false;
    let mut fled = false;
    let mut out_of_it = false;
    for _ in 0..60 * gc_sim::TICKS_PER_SECOND {
        world.tick(&out);
        out.clear();
        if world.regiments.get(spears).is_none() {
            break;
        };
        out_of_it |= fled && !world.in_fire(spears);
        burned |= world.in_fire(spears);
        minds.tick(&world, &mut out);
        fled |= burned
            && out.iter().any(
                |c| matches!(c.command, Command::MoveTo { regiment, .. } if regiment == spears),
            );
        out.retain(|c| c.player == AI);
    }
    assert!(burned, "the fire reached the spears");
    assert!(fled, "the mind sent them out of it");
    assert!(out_of_it, "and they got out alive");
}

/// A wood on the tiles 16..48 each way, open round it.
fn wood() -> Vec<u8> {
    (0..MAP * MAP)
        .map(|i| {
            let (x, y) = (i % MAP, i / MAP);
            if (16..48).contains(&x) && (16..48).contains(&y) {
                Cover::Wood as u8
            } else {
                Cover::Open as u8
            }
        })
        .collect()
}

/// The mind's spears just south of the wood and an enemy's in it, with
/// `more` spawned too; what the mind burns from second `from` to `to`:
/// whether it ordered a burn, and whether the woods caught.
fn burns(
    difficulty: Difficulty,
    spells: &'static [gc_sim::Spell],
    more: &[PlayerCommand],
    (from, to): (u32, u32),
) -> (bool, bool) {
    let cover = wood();
    let mut world = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 8 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: Ground {
            heights: &[],
            water_level: 0,
            cover: &cover,
            growth: &[],
            footing: &[],
            flow: &[],
        },
        champions: false,
        sky: Sky {
            spells,
            ..Sky::STILL
        },
        generals: false,
    });
    let mut spawn = vec![
        PlayerCommand {
            player: AI,
            command: Command::SpawnRegiment {
                unit: 0,
                position: at(32, 51),
                facing: Angle::ZERO,
            },
        },
        PlayerCommand {
            player: FIRER,
            command: Command::SpawnRegiment {
                unit: 0,
                position: at(32, 42),
                facing: Angle::ZERO,
            },
        },
    ];
    spawn.extend_from_slice(more);
    world.tick(&spawn);
    let mut minds = Minds::new(Terrain::new(&[], MAP).with_cover(cover));
    minds.add(Mind::new(MindConfig {
        party: AI,
        difficulty,
        personality: 0,
        seed: 3,
    }));
    let (mut ordered, mut caught) = (false, false);
    let mut out = Vec::new();
    for t in 0..to * gc_sim::TICKS_PER_SECOND {
        world.tick(&out);
        out.clear();
        minds.tick(&world, &mut out);
        if t >= from * gc_sim::TICKS_PER_SECOND {
            caught |= !world.fires().is_empty();
            ordered |= out
                .iter()
                .any(|c| matches!(c.command, Command::Burn { .. }));
        }
    }
    (ordered, caught)
}

fn at(x: i32, y: i32) -> FxVec2 {
    FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF)
}

#[test]
fn a_mind_sets_the_woods_alight_on_an_enemy_in_them() {
    assert_eq!(burns(Difficulty::General, &[], &[], (0, 5)), (true, true));
    // A Recruit doesn't think of it.
    assert!(!burns(Difficulty::Recruit, &[], &[], (0, 30)).0);
}

#[test]
fn not_on_its_own_troops_nor_in_the_rain_nor_into_the_wind() {
    // Its own regiment in the woods by the enemy (while it is still
    // there: the mind soon moves it).
    let own_in_woods = [PlayerCommand {
        player: AI,
        command: Command::SpawnRegiment {
            unit: 0,
            position: at(37, 44),
            facing: Angle::ZERO,
        },
    }];
    assert!(!burns(Difficulty::General, &[], &own_in_woods, (0, 3)).0);
    // Heavy rain from the start: after half a minute the ground is too
    // wet to catch, and it knows.
    const RAIN: &[gc_sim::Spell] = &[gc_sim::Spell {
        at: 0,
        weather: gc_sim::Weather {
            kind: gc_sim::WeatherKind::Rain,
            strength: 255,
            cloud: 255,
            wind: (0, 0),
        },
    }];
    let (ordered, caught) = burns(Difficulty::General, RAIN, &[], (40, 70));
    assert!(!caught, "ordered {ordered}");
    // A wind from the north blows the fire back on the burner.
    const NORTH_WIND: &[gc_sim::Spell] = &[gc_sim::Spell {
        at: 0,
        weather: gc_sim::Weather {
            kind: gc_sim::WeatherKind::Clear,
            strength: 0,
            cloud: 0,
            wind: (0, 20),
        },
    }];
    assert!(!burns(Difficulty::General, NORTH_WIND, &[], (0, 5)).0);
}
