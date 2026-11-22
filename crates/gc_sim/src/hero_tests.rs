//! Tests of heroes (see [`crate::heroes`]): hiring, levels, leadership,
//! the fall and the revive, in one small Conquest world.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::conquest::ConquestUnit;
use crate::heroes::{HeroGrowth, LEVELS};
use crate::nav::CELLS_PER_TILE;
use crate::scenario::{DRILL_UNITS, NORTH};
use crate::structures::{Armour, Refusal, Role, StructureHandle, Trap, WallTwist};
use crate::{
    BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
    RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const MAP: u32 = 128;
const HALL: u16 = 0;
const ALTAR: u16 = 1;
/// 24 soldiers of 10 hit points, trained at the hall.
const LINE: u16 = 0;
/// One model of 300 hit points, hired at the altar.
const HERO: u16 = 1;
const OTHER_HERO: u16 = 2;
/// Undead, trained at the hall.
const BONES: u16 = 3;
const HIRE_TICKS: u16 = 40 * TICKS_PER_SECOND as u16;
const GROWTH: HeroGrowth = HeroGrowth {
    hp: 30,
    melee: 2,
    missile: 0,
};

fn units() -> [UnitStats; 4] {
    let base = DRILL_UNITS[0];
    let hero = UnitStats {
        models: 1,
        files: 1,
        hp_per_model: 300,
        melee_damage: 10,
        morale: 0,
        ..base
    };
    [base, hero, hero, UnitStats { morale: 0, ..base }]
}

fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 5,
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
        supply: 30,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: Trap::None,
    };
    let altar = BuildingStats {
        role: Role::Heroes,
        width: 3,
        depth: 3,
        supply: 0,
        ..hall
    };
    let line = ConquestUnit {
        supply: 2,
        cost_grub: 120,
        trained_at: Some(HALL),
        tier: 1,
        train_ticks: 100,
        ..ConquestUnit::default()
    };
    let hero = ConquestUnit {
        supply: 5,
        cost_grub: 200,
        cost_marrow: 50,
        train_ticks: HIRE_TICKS,
        trained_at: Some(ALTAR),
        tier: 1,
        hero: Some(GROWTH),
        ..ConquestUnit::default()
    };
    let bones = ConquestUnit {
        undead: true,
        ..line
    };
    let mut start = [(0, 0); MAX_PARTIES];
    start[0] = (5000, 1000);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, altar],
        units: &[line, hero, hero, bones],
        structures: 64,
        start,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, building: u16, tile: (i16, i16)) -> StructureHandle {
    let spawn = Command::SpawnStructure {
        building,
        tile,
        turn: 0,
    };
    assert!(order(w, 0, spawn));
    let at = (i32::from(tile.0), i32::from(tile.1));
    w.structures()
        .unwrap()
        .iter()
        .find(|v| v.tile == at)
        .unwrap()
        .handle
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

fn seconds(w: &mut World, s: u32) {
    for _ in 0..s * TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

fn hire(w: &mut World, altar: StructureHandle, unit: u16) -> bool {
    order(
        w,
        0,
        Command::Train {
            structure: altar,
            unit,
        },
    )
}

/// A base with a hall and an altar, and a hero of type [`HERO`] hired and
/// standing.
fn with_hero() -> (World, StructureHandle, RegimentHandle) {
    let mut w = world();
    put_up(&mut w, HALL, (20, 20));
    let altar = put_up(&mut w, ALTAR, (30, 20));
    assert!(hire(&mut w, altar, HERO));
    seconds(&mut w, 41);
    let r = w.heroes().next().and_then(|h| h.regiment).expect("hired");
    (w, altar, r)
}

fn hp(w: &World, r: RegimentHandle) -> u16 {
    let i = w.regiments.resolve(r).unwrap();
    w.regiments.hp[i * crate::MAX_MODELS as usize]
}

#[test]
fn a_hero_is_hired_once_and_one_a_tier() {
    let (mut w, altar, r) = with_hero();
    let hero = *w.hero_of(r).unwrap();
    assert_eq!((hero.level, hero.xp), (1, 0));
    assert_eq!(hp(&w, r), 300);
    let bank = w.bank(0).unwrap();
    assert_eq!((bank.grub, bank.marrow), (5000 - 200, 1000 - 50));
    assert_eq!(bank.supply_used, 5);
    assert_eq!(w.can_train(0, altar, HERO), Err(Refusal::HeroTaken));
    // Tier 1: one hero.
    assert_eq!(w.can_train(0, altar, OTHER_HERO), Err(Refusal::HeroLimit));
    assert!(!hire(&mut w, altar, OTHER_HERO));
}

#[test]
fn a_hero_in_training_counts_against_the_limit_and_comes_back_if_called_off() {
    let mut w = world();
    put_up(&mut w, HALL, (20, 20));
    let altar = put_up(&mut w, ALTAR, (30, 20));
    assert!(hire(&mut w, altar, HERO));
    assert_eq!(w.can_train(0, altar, HERO), Err(Refusal::Busy));
    assert_eq!(w.can_train(0, altar, OTHER_HERO), Err(Refusal::HeroLimit));
    assert!(order(&mut w, 0, Command::Untrain { structure: altar }));
    assert_eq!(w.bank(0).unwrap().grub, 5000);
    assert_eq!(w.can_train(0, altar, OTHER_HERO), Ok(()));
}

#[test]
fn enemies_dying_near_a_hero_level_it_up() {
    let (mut w, _, r) = with_hero();
    let at = w.regiments.get(r).unwrap().position;
    let foe = spawn(&mut w, 1, LINE, at.x.floor_int() + 10, at.y.floor_int());
    let far = spawn(&mut w, 1, LINE, at.x.floor_int() + 60, at.y.floor_int());
    let (foe_i, far_i) = (
        w.regiments.resolve(foe).unwrap(),
        w.regiments.resolve(far).unwrap(),
    );
    // Ten of 10 hit points near it, ten far away.
    w.regiments.take_damage(foe_i, 100);
    w.regiments.take_damage(far_i, 100);
    w.tick(&[]);
    assert_eq!(w.hero_of(r).unwrap().xp, 100);
    w.regiments.take_damage(foe_i, 100);
    w.tick(&[]);
    let hero = *w.hero_of(r).unwrap();
    assert_eq!((hero.xp, hero.level), (LEVELS[0], 2));
    // The level's hit points come whole, and its blows land harder.
    assert_eq!(hp(&w, r), 330);
    let i = w.regiments.resolve(r).unwrap();
    assert_eq!(w.regiments.melee_bonus[i], 20);
    assert_eq!(crate::veterancy::damage_percent(&w.regiments, i), 120);
}

#[test]
fn a_hero_leads_its_players_regiments_near_it() {
    let (mut w, _, r) = with_hero();
    let at = w.regiments.get(r).unwrap().position;
    let (x, y) = (at.x.floor_int(), at.y.floor_int());
    let near = spawn(&mut w, 0, LINE, x + 10, y);
    let bones = spawn(&mut w, 0, BONES, x - 10, y);
    let far = spawn(&mut w, 0, LINE, x + 40, y);
    let enemy = spawn(&mut w, 1, LINE, x, y + 10);
    w.tick(&[]);
    let led = |w: &World, h| w.regiments.led[w.regiments.resolve(h).unwrap()];
    assert!(led(&w, near) && led(&w, bones));
    assert!(!led(&w, far) && !led(&w, enemy));
    // A hero, with no morale and not undead, isn't driven on.
    assert!(!led(&w, r));
}

#[test]
fn a_fallen_hero_shakes_its_side_and_is_revived_at_its_level() {
    let (mut w, altar, r) = with_hero();
    let at = w.regiments.get(r).unwrap().position;
    let foe = spawn(&mut w, 1, LINE, at.x.floor_int() + 10, at.y.floor_int());
    let foe_i = w.regiments.resolve(foe).unwrap();
    w.regiments.take_damage(foe_i, 200);
    w.tick(&[]);
    assert_eq!(w.hero_of(r).unwrap().level, 2);
    let friend = spawn(&mut w, 0, LINE, at.x.floor_int() - 8, at.y.floor_int());
    w.tick(&[]);
    let fi = w.regiments.resolve(friend).unwrap();
    let before = w.regiments.morale[fi];
    let i = w.regiments.resolve(r).unwrap();
    w.regiments.take_damage(i, 1000);
    w.tick(&[]);
    let hero = *w.heroes().next().unwrap();
    assert!(hero.dead());
    assert!(w.regiments.morale[fi] < before, "its side lost heart");
    assert_eq!(w.bank(0).unwrap().supply_used, 2, "the dead take no supply");
    // Revived for its level's price.
    assert_eq!(w.price_of(0, HERO), Some((160, 40)));
    let grub = w.bank(0).unwrap().grub;
    assert!(hire(&mut w, altar, HERO));
    assert_eq!(w.bank(0).unwrap().grub, grub - 160);
    // 80% of the hire's 40 s.
    seconds(&mut w, 31);
    assert!(w.heroes().next().unwrap().dead());
    seconds(&mut w, 2);
    let hero = *w.heroes().next().unwrap();
    let back = hero.regiment.expect("revived");
    assert_eq!((hero.level, hero.xp), (2, LEVELS[0]));
    assert_eq!(hp(&w, back), 330);
}
