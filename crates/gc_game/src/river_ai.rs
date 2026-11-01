//! The computer players keep their bridges standing (docs/39-water-engine.md,
//! section 5, stage 4d): on a grown river map, an enemy worker tips rafts
//! into the river above a bridge of theirs until a heavy jam builds, then
//! an enemy soldier hacks at a deck tile; they send workers to clear the
//! jam and to mend the tile.

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::{Command, PlayerCommand, TICKS_PER_SECOND, World};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::conquest_golden::{Kit, regiments_of};
use crate::headless::{Headless, Real};

/// A grown map with a river near the first start (as `river_golden`).
const SEED: u64 = 3;
const GROWN: usize = gc_map::SHIPPED.len() + 1 + gc_map::wilds::Wildness::ALL.len();
const SIDES: [Faction; 2] = [Faction::EmberClans, Faction::EmberClans];
/// The bridge, across the river near the first start, and where the
/// rafts go in upstream of it.
const BRIDGE_Y: i32 = 62;
const BRIDGE_X: std::ops::RangeInclusive<i32> = 47..=53;
const RAFT_AT: (i16, i16) = (52, 66);

#[test]
fn computer_players_clear_jams_off_their_bridges_and_mend_them() {
    let real = Real::load();
    let game = Headless::new(&real, conquest::make_map(GROWN, 0, SEED), &SIDES, SEED);
    let plan = game.plan(Tempo::Swift, gc_sim::Victories::BOTH);
    let mut world = game.world(&plan);
    let mut minds = game.minds();
    minds.add(game.mind(0, Difficulty::Veteran, 0, None));
    game.open(&mut world, World::tick);
    let worker = Kit::of(&real.data, &real.table, SIDES[0]).worker;
    let bridge = real.building("Timber Bridge");
    // The bridge, the mind's.
    let decks: Vec<_> = BRIDGE_X
        .map(|x| PlayerCommand {
            player: 0,
            command: Command::SpawnStructure {
                building: bridge,
                tile: (x as i16, BRIDGE_Y as i16),
                turn: 0,
            },
        })
        .collect();
    assert_eq!(world.tick(&decks).commands_applied as usize, decks.len());
    // The second side has no mind: one of its workers tips the rafts.
    let before = regiments_of(&world, 1, worker);
    let spawn = PlayerCommand {
        player: 1,
        command: Command::SpawnRegiment {
            unit: worker,
            position: gc_fixed::FxVec2::from_ints(54, 67),
            facing: gc_sim::scenario::NORTH,
        },
    };
    assert_eq!(
        world.tick(&[spawn]).commands_applied,
        1,
        "the tipper is put down"
    );
    let tipper = (regiments_of(&world, 1, worker).into_iter())
        .find(|h| !before.contains(h))
        .unwrap();
    let hp = |w: &World| -> Vec<u32> {
        let s = w.structures().unwrap();
        (BRIDGE_X)
            .filter_map(|x| s.iter().find(|v| v.tile == (x, BRIDGE_Y)).map(|v| v.hp))
            .collect()
    };
    let full: u32 = hp(&world).iter().sum();
    let mut cmds = Vec::new();
    let (mut most_jam, mut lowest) = (0u8, full);
    let mut hacker = None;
    let fighter = Kit::of(&real.data, &real.table, SIDES[1]).fighter;
    let mut fighters = Vec::new();
    let end = 4 * 60 * TICKS_PER_SECOND;
    while world.tick_count() < end {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        let t = world.tick_count();
        // A raft every 8 seconds for the first two minutes.
        if t.is_multiple_of(8 * TICKS_PER_SECOND)
            && t < 120 * TICKS_PER_SECOND
            && world.takes_raft(1, i32::from(RAFT_AT.0), i32::from(RAFT_AT.1))
        {
            cmds.push(PlayerCommand {
                player: 1,
                command: Command::Dump {
                    regiment: tipper,
                    tile: RAFT_AT,
                },
            });
        }
        // Then it hacks at a deck tile until it is hurt, and walks off.
        let deck = (world.structures().unwrap().iter())
            .find(|v| v.tile == (50, BRIDGE_Y))
            .map(|v| (v.handle, v.hp * 100 / v.max_hp));
        if t == 145 * TICKS_PER_SECOND {
            fighters = regiments_of(&world, 1, fighter);
            cmds.push(PlayerCommand {
                player: 1,
                command: Command::SpawnRegiment {
                    unit: fighter,
                    position: gc_fixed::FxVec2::from_ints(50, BRIDGE_Y - 2),
                    facing: gc_sim::scenario::NORTH,
                },
            });
        }
        if t == 150 * TICKS_PER_SECOND
            && let Some((structure, _)) = deck
        {
            hacker = regiments_of(&world, 1, fighter)
                .into_iter()
                .find(|h| !fighters.contains(h));
            cmds.push(PlayerCommand {
                player: 1,
                command: Command::AttackStructure {
                    regiment: hacker.unwrap(),
                    structure,
                },
            });
        }
        if deck.is_some_and(|d| d.1 < 60)
            && let Some(hacker) = hacker.take()
        {
            cmds.push(PlayerCommand {
                player: 1,
                command: Command::MoveTo {
                    regiment: hacker,
                    target: gc_fixed::FxVec2::from_ints(62, 72),
                    facing: None,
                    files: 0,
                    pace: gc_fixed::Fx::ZERO,
                },
            });
        }
        world.tick(&cmds);
        if t.is_multiple_of(TICKS_PER_SECOND) {
            let jams = world.flood().rivers().jams().filter(|j| j.owner == 0);
            most_jam = most_jam.max(jams.map(|j| j.mass).max().unwrap_or(0));
            lowest = lowest.min(hp(&world).iter().sum());
        }
    }
    let journal: Vec<_> = (minds.get(0).unwrap().journal().entries())
        .map(|e| e.text.clone())
        .collect();
    let said = |what: &str| journal.iter().any(|t| t.contains(what));
    assert!(most_jam > 3, "a jam built up: {most_jam}");
    assert!(lowest < full, "the bridge was hurt");
    assert!(
        said("sending a worker to clear it"),
        "it cleared the jam: {journal:?}"
    );
    assert!(
        said("sending a worker to mend it"),
        "it mended the bridge: {journal:?}"
    );
    let jam_left = (world.flood().rivers().jams())
        .filter(|j| j.owner == 0)
        .map(|j| j.mass)
        .max()
        .unwrap_or(0);
    assert!(jam_left <= 3, "no heavy jam left: {jam_left}");
    assert!(
        hp(&world).iter().sum::<u32>() > lowest,
        "and the bridge mended"
    );
}
