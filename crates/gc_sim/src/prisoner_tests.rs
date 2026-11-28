//! Tests of prisoners after battles (see [`crate::prisoners`]): a small
//! Conquest world with a hall for each of two players, and a Field
//! Battle.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::conquest::ConquestUnit;
use crate::events::Event;
use crate::morale::MoraleState;
use crate::nav::CELLS_PER_TILE;
use crate::prisoners::{MAIN_HOLDS, WORK_GRUB};
use crate::scenario::{DRILL_UNITS, NORTH};
use crate::structures::{Armour, Role, StructureHandle, Trap, WallTwist};
use crate::{
    BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
    RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const MAP: u32 = 128;
const HALL: u16 = 0;
/// 24 soldiers of 10 hit points, trained at the hall.
const LINE: u16 = 0;
/// The same, summoned: nothing trains it.
const SUMMONED: u16 = 1;

fn units() -> [UnitStats; 2] {
    [DRILL_UNITS[0], DRILL_UNITS[0]]
}

fn setup(units: &[UnitStats]) -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 24 },
        units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: crate::Ground::FLAT,
        champions: false,
        sky: crate::Sky::STILL,
        generals: false,
    })
}

fn conquest() -> (World, [StructureHandle; 2]) {
    let mut w = setup(&units());
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 3000,
        armour: Armour::Fortified,
        width: 4,
        depth: 4,
        supply: 60,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: Trap::None,
    };
    let line = ConquestUnit {
        supply: 2,
        cost_grub: 120,
        cost_marrow: 24,
        trained_at: Some(HALL),
        tier: 1,
        train_ticks: 100,
        ..ConquestUnit::default()
    };
    let summoned = ConquestUnit {
        trained_at: None,
        ..line
    };
    let mut start = [(0, 0); MAX_PARTIES];
    start[0] = (5000, 1000);
    start[1] = (5000, 1000);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall],
        units: &[line, summoned],
        structures: 16,
        start,
        ..ConquestSetup::default()
    }));
    let mut halls = Vec::new();
    for (p, tile) in [(0u8, (10i16, 10i16)), (1, (110, 110))] {
        let spawn = Command::SpawnStructure {
            building: HALL,
            tile,
            turn: 0,
        };
        assert!(order(&mut w, p, spawn));
        let s = w.structures().unwrap();
        let at = (i32::from(tile.0), i32::from(tile.1));
        halls.push(s.iter().find(|v| v.tile == at).unwrap().handle);
    }
    (w, [halls[0], halls[1]])
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    let command = Command::SpawnRegiment {
        unit,
        position: FxVec2::from_ints(x, y),
        facing: NORTH,
    };
    assert!(order(w, player, command));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

/// Regiment `h` breaks, down to `soldiers`, too spent to get away.
fn rout(w: &mut World, h: RegimentHandle, soldiers: u16) {
    // Broken long enough ago to be run down now.
    while w.tick <= crate::prisoners::RUN_SECONDS * TICKS_PER_SECOND {
        w.tick(&[]);
    }
    let i = w.regiments.resolve(h).unwrap();
    w.regiments.speed[i] = Fx::ZERO;
    w.regiments.pace[i] = Fx::ZERO;
    w.regiments.models[i] = soldiers;
    let base = i * crate::MAX_MODELS as usize;
    w.regiments.hp[base + usize::from(soldiers)..base + crate::MAX_MODELS as usize].fill(0);
    w.regiments.morale_state[i] = MoraleState::Routing;
    w.regiments.morale[i] = 0;
    w.regiments.last_rout[i] = 1;
}

/// Ticks until the next whole second has been played, and the events of
/// that tick.
fn to_next_second(w: &mut World) -> Vec<Event> {
    let mut seen = Vec::new();
    loop {
        let check = w.tick.is_multiple_of(TICKS_PER_SECOND);
        w.tick(&[]);
        seen.extend(w.events().iter().cloned());
        if check {
            return seen;
        }
    }
}

/// A routing regiment of player 1 touching one of player 0's, alone.
fn caught(unit: u16) -> (World, [StructureHandle; 2], RegimentHandle) {
    let (mut w, halls) = conquest();
    spawn(&mut w, 0, LINE, 64, 66);
    let r = spawn(&mut w, 1, unit, 64, 64);
    rout(&mut w, r, 10);
    (w, halls, r)
}

#[test]
fn a_broken_regiment_caught_alone_gives_itself_up_and_is_held_at_the_hall() {
    let (mut w, halls, r) = caught(LINE);
    let events = to_next_second(&mut w);
    assert!(w.regiments.resolve(r).is_none(), "still on the field");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Surrendered { regiment, owner: 1, captor: 0, soldiers: 10, .. } if *regiment == r
        )),
        "{events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::Losses { regiment, .. } if *regiment == r)),
        "told as killed"
    );
    assert_eq!(w.prisoners_taken(0), 10);
    let held = w.prisoners_held();
    assert_eq!(held.len(), 1);
    assert_eq!((held[0].captor, held[0].owner, held[0].unit), (0, 1, LINE));
    assert_eq!((held[0].soldiers, held[0].stockade), (10, halls[0]));
}

#[test]
fn a_friend_near_or_a_summoned_regiment_does_not_give_up() {
    let (mut w, _, r) = caught(LINE);
    spawn(&mut w, 1, LINE, 70, 64);
    rout(&mut w, r, 10);
    to_next_second(&mut w);
    assert!(w.regiments.resolve(r).is_some(), "gave up beside a friend");

    let (mut w, _, r) = caught(SUMMONED);
    to_next_second(&mut w);
    assert!(
        w.regiments.resolve(r).is_some(),
        "a unit nothing trains gave up"
    );
    assert_eq!(w.prisoners_taken(0), 0);
}

#[test]
fn a_ransom_pays_the_captor_and_brings_the_lot_back_at_its_rank() {
    let (mut w, _, r) = caught(LINE);
    let i = w.regiments.resolve(r).unwrap();
    w.regiments.xp[i] = crate::veterancy::xp_for(2);
    to_next_second(&mut w);
    let (grub, marrow, lot) = w.ransom_price(1, 0, LINE).unwrap();
    // 10 of 24 soldiers of a 120 Grub, 24 Marrow regiment.
    assert_eq!((grub, marrow, lot), (50, 10, 10));
    let before = [w.bank(0).unwrap(), w.bank(1).unwrap()];
    let ours: Vec<_> = w
        .regiments
        .iter()
        .filter(|v| v.owner == 1)
        .map(|v| v.handle)
        .collect();
    assert!(
        !order(
            &mut w,
            1,
            Command::Ransom {
                from: 2,
                unit: LINE
            }
        ),
        "nobody held"
    );
    assert!(order(
        &mut w,
        1,
        Command::Ransom {
            from: 0,
            unit: LINE
        }
    ));
    let after = [w.bank(0).unwrap(), w.bank(1).unwrap()];
    assert_eq!(after[0].marrow, before[0].marrow + 10);
    assert_eq!(after[1].marrow, before[1].marrow - 10);
    assert!(after[0].grub >= before[0].grub + 50);
    assert!(after[1].grub + 50 <= before[1].grub + 1);
    let back = (w.regiments.iter())
        .find(|v| v.owner == 1 && !ours.contains(&v.handle))
        .expect("back on the field");
    assert_eq!((back.models, back.rank), (10, 2));
    assert!(w.prisoners_held().is_empty());
    assert!(
        !order(
            &mut w,
            1,
            Command::Ransom {
                from: 0,
                unit: LINE
            }
        ),
        "none left"
    );
}

#[test]
fn held_soldiers_work_and_go_free_when_the_hall_falls() {
    let (mut w, halls, _) = caught(LINE);
    to_next_second(&mut w);
    let before = w.bank(0).unwrap().grub;
    while !w.tick.is_multiple_of(60 * TICKS_PER_SECOND) {
        w.tick(&[]);
    }
    w.tick(&[]);
    assert!(
        w.bank(0).unwrap().grub >= before + 10 * WORK_GRUB - 1,
        "no work done: {before} -> {}",
        w.bank(0).unwrap().grub
    );
    let ours = w.regiments.iter().filter(|v| v.owner == 1).count();
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure {
            structure: halls[0]
        }
    ));
    to_next_second(&mut w);
    assert!(w.prisoners_held().is_empty());
    let freed: Vec<_> = (w.regiments.iter()).filter(|v| v.owner == 1).collect();
    assert_eq!(freed.len(), ours + 1, "nobody stood up");
    assert!(freed.iter().any(|v| v.models == 10));
}

#[test]
fn a_hall_holds_sixty_and_the_rest_are_let_go() {
    let (mut w, _) = conquest();
    spawn(&mut w, 0, LINE, 64, 66);
    for k in 0..3 {
        let r = spawn(&mut w, 1, LINE, 64, 64);
        rout(&mut w, r, 24);
        to_next_second(&mut w);
        assert!(w.regiments.resolve(r).is_none(), "regiment {k} still there");
    }
    let held: u32 = w
        .prisoners_held()
        .iter()
        .map(|h| u32::from(h.soldiers))
        .sum();
    assert_eq!(held, MAIN_HOLDS);
    assert_eq!(w.prisoners_taken(0), 72);
}

#[test]
fn field_battles_count_prisoners_and_hold_none() {
    let mut w = crate::test_util::world(&units());
    crate::test_util::spawn(&mut w, 0, LINE, 64, 66);
    let r = crate::test_util::spawn(&mut w, 1, LINE, 64, 64);
    rout(&mut w, r, 8);
    to_next_second(&mut w);
    assert!(w.regiments.resolve(r).is_none());
    assert_eq!(w.prisoners_taken(0), 8);
    assert!(w.prisoners_held().is_empty());
    assert!(!order(
        &mut w,
        1,
        Command::Ransom {
            from: 0,
            unit: LINE
        }
    ));
}
