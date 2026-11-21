//! Tests of the Brood's evolutions (see [`crate::evolve`]) and the
//! Concord's Tithe, contracts, refilling and crumbling (see
//! [`crate::tithe`]), in one small Conquest world.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::conquest::ConquestUnit;
use crate::nav::CELLS_PER_TILE;
use crate::powers::{Meter, PowerEffect, PowerStats, PowerTarget};
use crate::scenario::{DRILL_UNITS, NORTH};
use crate::structures::{Armour, Refusal, Role, TITHE, Trap, WallTwist};
use crate::{
    BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
    RegimentHandle, TICKS_PER_SECOND, UnitStats, Waiting, World,
};

const MAP: u32 = 128;
const HALL: u16 = 0;
const CHAMBER: u16 = 1;
const RELIQUARY: u16 = 2;
/// Drill unit 0: 24 soldiers of 10 hit points, trained at the hall for
/// 120 Grub, supply 2.
const GRUBS: u16 = 0;
/// Grubs evolved: 12 soldiers of 20 hit points, 60 Grub and 12 Marrow,
/// supply 3, 10 s in the cocoon.
const BEETLES: u16 = 1;
/// Undead, trained at the hall: 24 soldiers of 10 hit points.
const BONES: u16 = 2;
/// Raised or hired: 16 soldiers of 7 hit points.
const RAISED: u16 = 3;
const RAISE: u8 = 0;
const INDENTURE: u8 = 1;
const HIRE: u8 = 2;

fn units() -> [UnitStats; 4] {
    let base = DRILL_UNITS[0];
    [
        base,
        UnitStats {
            models: 12,
            files: 6,
            hp_per_model: 20,
            ..base
        },
        UnitStats { morale: 0, ..base },
        UnitStats {
            models: 16,
            files: 8,
            hp_per_model: 7,
            morale: 0,
            ..base
        },
    ]
}

/// A world where player 0 (meter `meter`) has `grub` and player 1 is an
/// enemy, with a hamlet at (100, 100).
fn world(grub: u32, meter: Meter) -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities { regiments: 16 },
        units: &units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: crate::Ground::FLAT,
        champions: false,
        sky: crate::Sky::STILL,
        generals: false,
    });
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
        supply: 20,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: Trap::None,
    };
    let chamber = BuildingStats {
        role: Role::Tech,
        width: 2,
        depth: 2,
        supply: 0,
        ..hall
    };
    let reliquary = BuildingStats {
        role: Role::Economy,
        flags: TITHE,
        ..chamber
    };
    let trained = ConquestUnit {
        supply: 2,
        cost_grub: 120,
        trained_at: Some(HALL),
        tier: 1,
        ..ConquestUnit::default()
    };
    let evolved = ConquestUnit {
        supply: 3,
        cost_grub: 60,
        cost_marrow: 12,
        train_ticks: 10 * TICKS_PER_SECOND as u16,
        trained_at: None,
        evolves_from: Some(GRUBS),
        unlocked_at: Some(CHAMBER),
        ..trained
    };
    let undead = ConquestUnit {
        undead: true,
        ..trained
    };
    let raised = ConquestUnit {
        supply: 0,
        cost_grub: 0,
        trained_at: None,
        tier: 0,
        undead: true,
        ..ConquestUnit::default()
    };
    let power = |effect, duration_ticks| PowerStats {
        tier: 1,
        cost: 40,
        meter: Meter::Tithe,
        cooldown_ticks: 0,
        duration_ticks,
        effect,
    };
    let powers = [
        power(
            PowerEffect::Raise {
                unit: RAISED,
                tiles: 8,
            },
            90 * TICKS_PER_SECOND,
        ),
        power(
            PowerEffect::Raise {
                unit: RAISED,
                tiles: 8,
            },
            0,
        ),
        power(
            PowerEffect::Hire {
                unit: RAISED,
                tiles: 8,
            },
            120 * TICKS_PER_SECOND,
        ),
    ];
    let mut start = [(0, 0); MAX_PARTIES];
    start[0] = (grub, 1000);
    let mut meters = [Meter::NotBuilt; MAX_PARTIES];
    meters[0] = meter;
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, chamber, reliquary],
        units: &[trained, evolved, undead, raised],
        structures: 64,
        posts: &[FxVec2::from_ints(100, 100)],
        start,
        powers: &powers,
        meters,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, building: u16, tile: (i16, i16)) {
    let spawn = Command::SpawnStructure {
        building,
        tile,
        turn: 0,
    };
    assert!(order(w, 0, spawn));
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

/// Regiment `h` loses `n` soldiers of 10 hit points.
fn kill(w: &mut World, h: RegimentHandle, n: u32) {
    let i = w.regiments.resolve(h).unwrap();
    w.regiments.take_damage(i, n * 10);
}

fn models(w: &World, h: RegimentHandle) -> u16 {
    w.regiments.get(h).map_or(0, |r| r.models)
}

fn seconds(w: &mut World, s: u32) {
    for _ in 0..s * TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

fn tithe(w: &World) -> u32 {
    w.bank(0).unwrap().tithe
}

fn evolve(w: &mut World, h: RegimentHandle) -> bool {
    order(
        w,
        0,
        Command::Evolve {
            regiment: h,
            into: BEETLES,
        },
    )
}

#[test]
fn a_regiment_cocoons_then_comes_out_as_its_evolution() {
    let mut w = world(1000, Meter::NotBuilt);
    put_up(&mut w, HALL, (20, 20));
    let h = spawn(&mut w, 0, GRUBS, 60, 60);
    kill(&mut w, h, 6);
    assert_eq!(w.can_evolve(0, h, BEETLES), Err(Refusal::NoChamber));
    assert!(!evolve(&mut w, h));
    put_up(&mut w, CHAMBER, (30, 20));
    assert_eq!(
        w.can_evolve(0, h, GRUBS),
        Err(Refusal::Unknown),
        "not an evolution"
    );
    assert_eq!(w.can_evolve(0, h, BEETLES), Ok(()));
    let paid = w.bank(0).unwrap();
    assert!(evolve(&mut w, h));
    let bank = w.bank(0).unwrap();
    // 18 of 24 come out as 9 of 12: 60 * 9 / 12 and 12 * 9 / 12.
    assert_eq!((paid.grub - bank.grub, paid.marrow - bank.marrow), (45, 9));
    assert_eq!(bank.supply_used, 3, "its new form's supply from the start");
    let feedback = w.regiments.feedback(h).unwrap();
    assert_eq!(feedback.waiting, Some(Waiting::Cocooned));
    assert!(!evolve(&mut w, h), "already in a cocoon");
    // It can't be marched off.
    let at = w.regiments.get(h).unwrap().position;
    let go = Command::MoveTo {
        regiment: h,
        target: FxVec2::from_ints(90, 60),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
    };
    order(&mut w, 0, go);
    seconds(&mut w, 5);
    assert_eq!(
        w.regiments.get(h).unwrap().position,
        at,
        "a cocoon stays put"
    );
    assert_eq!(w.regiments.get(h).unwrap().unit, GRUBS);
    seconds(&mut w, 5);
    let r = w.regiments.get(h).expect("the same regiment");
    assert_eq!((r.unit, r.models), (BEETLES, 9));
    let i = w.regiments.resolve(h).unwrap();
    assert!(
        w.regiments.hp[i * crate::MAX_MODELS as usize..][..9]
            .iter()
            .all(|&hp| hp == 20)
    );
    assert_eq!(w.regiments.full_models[i], 12);
    assert!(w.cocoon_of(h).is_none());
    // And then it goes where it was sent.
    seconds(&mut w, 3);
    assert_ne!(w.regiments.get(h).unwrap().position, at);
}

#[test]
fn an_evolution_needs_its_price_and_supply() {
    let mut w = world(40, Meter::NotBuilt);
    put_up(&mut w, HALL, (20, 20));
    put_up(&mut w, CHAMBER, (30, 20));
    let h = spawn(&mut w, 0, GRUBS, 60, 60);
    assert_eq!(
        w.can_evolve(0, h, BEETLES),
        Err(Refusal::Price),
        "60 Grub for all 12"
    );
    let mut w = world(1000, Meter::NotBuilt);
    put_up(&mut w, HALL, (20, 20));
    put_up(&mut w, CHAMBER, (30, 20));
    let first = spawn(&mut w, 0, GRUBS, 60, 60);
    // Ten regiments of 2 fill the hall's 20.
    for k in 0..9 {
        spawn(&mut w, 0, GRUBS, 40 + 6 * k, 90);
    }
    assert_eq!(w.can_evolve(0, first, BEETLES), Err(Refusal::Supply));
}

#[test]
fn an_evolved_regiment_reinforces_paying_for_both_forms() {
    let mut w = world(1000, Meter::NotBuilt);
    put_up(&mut w, HALL, (20, 20));
    put_up(&mut w, CHAMBER, (40, 40));
    let h = spawn(&mut w, 0, GRUBS, 28, 22);
    assert!(evolve(&mut w, h));
    seconds(&mut w, 11);
    assert_eq!(models(&w, h), 12);
    let i = w.regiments.resolve(h).unwrap();
    w.regiments.take_damage(i, 2 * 20);
    assert!(w.can_reinforce(h));
    let before = w.bank(0).unwrap();
    seconds(&mut w, 10);
    assert_eq!(models(&w, h), 12, "refilled by its hall");
    let after = w.bank(0).unwrap();
    // Two of twelve of 180 Grub and 12 Marrow.
    assert_eq!(
        (before.grub - after.grub, before.marrow - after.marrow),
        (30, 2)
    );
}

#[test]
fn the_dead_near_a_reliquary_pay_its_owner_tithe_and_lie_in_graves() {
    let mut w = world(1000, Meter::Tithe);
    put_up(&mut w, HALL, (20, 20));
    put_up(&mut w, RELIQUARY, (60, 60));
    let near = spawn(&mut w, 1, GRUBS, 66, 61);
    let far = spawn(&mut w, 1, GRUBS, 100, 61);
    kill(&mut w, near, 5);
    kill(&mut w, far, 3);
    w.tick(&[]);
    assert_eq!(tithe(&w), 5, "only the dead within 12 tiles");
    let graves: Vec<_> = w.graves().collect();
    assert_eq!(graves.len(), 2);
    assert_eq!(w.corpses_near(FxVec2::from_ints(66, 61), 4), 5);
    // Rotted after three minutes.
    seconds(&mut w, 181);
    assert_eq!(w.graves().count(), 0);
    // No Tithe player, no graves.
    let mut w = world(1000, Meter::NotBuilt);
    put_up(&mut w, RELIQUARY, (60, 60));
    let h = spawn(&mut w, 1, GRUBS, 66, 61);
    kill(&mut w, h, 5);
    w.tick(&[]);
    assert_eq!((tithe(&w), w.graves().count()), (0, 0));
}

/// A world with 60 Tithe for player 0 and `dead` corpses at (70, 70).
fn tithed(dead: u32) -> World {
    let mut w = world(1000, Meter::Tithe);
    put_up(&mut w, HALL, (20, 20));
    put_up(&mut w, RELIQUARY, (66, 66));
    let fodder = spawn(&mut w, 1, GRUBS, 70, 70);
    kill(&mut w, fodder, 20);
    w.tick(&[]);
    let i = w.regiments.resolve(fodder).unwrap();
    w.regiments.despawn(w.regiments.handle(i));
    let c = w.conquest.as_mut().unwrap();
    c.tithe.gain(0, 40);
    // Only `dead` of them are left lying.
    let extra = 20 - dead as u16;
    c.tithe.dig(FxVec2::from_ints(70, 70), 8, extra);
    w
}

fn cast(w: &mut World, power: u8, x: i32, y: i32) -> bool {
    let target = PowerTarget::Point(FxVec2::from_ints(x, y));
    order(w, 0, Command::UsePower { power, target })
}

#[test]
fn raise_the_bound_raises_as_many_as_lie_there_and_they_go_after_their_time() {
    let mut w = tithed(3);
    assert_eq!(tithe(&w), 60);
    assert!(!cast(&mut w, RAISE, 72, 70), "three corpses are too few");
    assert_eq!(tithe(&w), 60, "nothing paid");
    let mut w = tithed(12);
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(cast(&mut w, RAISE, 72, 70));
    assert_eq!(tithe(&w), 20);
    let raised = (w.regiments.iter())
        .find(|r| !before.contains(&r.handle))
        .unwrap();
    assert_eq!((raised.unit, raised.models, raised.owner), (RAISED, 12, 0));
    assert_eq!(
        w.corpses_near(FxVec2::from_ints(70, 70), 8),
        0,
        "the corpses are used up"
    );
    assert!(w.regiments.summoned_left(raised.handle) > 0);
    seconds(&mut w, 91);
    assert!(w.regiments.get(raised.handle).is_none(), "gone after 90 s");
}

#[test]
fn an_indentured_regiment_stays_and_crumbles_away_from_its_keepers() {
    let mut w = tithed(20);
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(cast(&mut w, INDENTURE, 72, 70));
    let h = (w.regiments.iter())
        .find(|r| !before.contains(&r.handle))
        .unwrap()
        .handle;
    assert_eq!((models(&w, h), w.regiments.summoned_left(h)), (16, 0));
    assert_eq!(w.bank(0).unwrap().supply_used, 0, "no supply");
    // By its Reliquary it keeps; 42 tiles from the Reliquary and the hall
    // it crumbles a soldier a minute.
    seconds(&mut w, 30);
    assert_eq!(models(&w, h), 16);
    let i = w.regiments.resolve(h).unwrap();
    w.regiments.position[i] = FxVec2::from_ints(110, 110);
    seconds(&mut w, 121);
    assert_eq!(models(&w, h), 14);
}

#[test]
fn undead_by_a_reliquary_are_refilled_with_tithe() {
    let mut w = tithed(0);
    let h = spawn(&mut w, 0, BONES, 72, 68);
    kill(&mut w, h, 5);
    assert!(w.can_reinforce(h));
    assert!(w.refills_here(h));
    let grub = w.bank(0).unwrap().grub;
    // The five deaths paid 5 more.
    seconds(&mut w, 10);
    assert_eq!(models(&w, h), 22, "a soldier every 3 s");
    assert_eq!(tithe(&w), 65 - 3, "a Tithe each");
    assert_eq!(w.bank(0).unwrap().grub, grub, "no Grub");
    // Out of Tithe it waits.
    w.conquest.as_mut().unwrap().tithe.spend(0, 250);
    seconds(&mut w, 10);
    assert_eq!(models(&w, h), 22);
    let feedback = w.regiments.feedback(h).unwrap();
    assert_eq!(feedback.waiting, Some(Waiting::ShortToReinforce));
}

#[test]
fn militia_is_hired_from_a_hamlet_no_enemy_holds() {
    let mut w = tithed(0);
    assert!(!cast(&mut w, HIRE, 70, 70), "no hamlet within 8 tiles");
    let foe = spawn(&mut w, 1, GRUBS, 104, 100);
    assert!(!cast(&mut w, HIRE, 98, 98), "an enemy holds it");
    let i = w.regiments.resolve(foe).unwrap();
    w.regiments.despawn(w.regiments.handle(i));
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(cast(&mut w, HIRE, 98, 98));
    let hired = (w.regiments.iter())
        .find(|r| !before.contains(&r.handle))
        .unwrap();
    assert_eq!((hired.unit, hired.models), (RAISED, 16));
    let gap = (hired.position - FxVec2::from_ints(100, 100)).length();
    assert!(gap <= Fx::from_int(13), "beside the hamlet");
    assert_eq!(tithe(&w), 20);
}
