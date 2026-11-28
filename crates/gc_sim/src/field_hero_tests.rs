//! Tests of heroes in Field Battles (docs/43-heroes.md, H3): put on the
//! field at a level ([`Command::SpawnHero`]), levelling as they fight, the
//! side's first hero its general, and its champion, wounded rather than
//! killed when he loses the duel.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::abilities::{HeroKit, Kit};
use crate::conquest::ConquestUnit;
use crate::generals::PANIC;
use crate::heroes::{HeroGrowth, LEVELS, MAX_HEROES};
use crate::nav::CELLS_PER_TILE;
use crate::scenario::{DRILL_UNITS, NORTH};
use crate::test_util::{order, spawn};
use crate::{
    Capacities, Command, Ground, MAX_MODELS, MatchSetup, RegimentHandle, TICKS_PER_SECOND,
    UnitStats, World,
};

const MAP: u32 = 128;
/// 24 soldiers of 10 hit points.
const LINE: u16 = 0;
/// One model of 300 hit points, each a hero.
const HERO: u16 = 1;
const OTHER_HERO: u16 = 2;
const THIRD_HERO: u16 = 3;
const FOURTH_HERO: u16 = 4;
const GROWTH: HeroGrowth = HeroGrowth {
    hp: 30,
    melee: 2,
    missile: 0,
};

fn units() -> Vec<UnitStats> {
    let base = DRILL_UNITS[0];
    let hero = UnitStats {
        models: 1,
        files: 1,
        hp_per_model: 300,
        melee_damage: 10,
        melee_attack: 60,
        melee_defence: 40,
        morale: 0,
        ..base
    };
    vec![base, hero, hero, hero, hero]
}

fn roster() -> Vec<ConquestUnit> {
    let hero = ConquestUnit {
        tier: 1,
        hero: Some(GROWTH),
        ..ConquestUnit::default()
    };
    vec![
        ConquestUnit {
            tier: 1,
            ..ConquestUnit::default()
        },
        hero,
        hero,
        hero,
        hero,
    ]
}

/// A flat Field Battle with generals, given the hero table, `champions`
/// allowed or not, from `seed`.
fn field(seed: u64, champions: bool) -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed,
        capacities: Capacities { regiments: 16 },
        units: &units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground::FLAT,
        champions,
        sky: crate::Sky::STILL,
        generals: true,
    });
    assert!(w.field_roster(&roster()));
    w
}

fn put_hero(w: &mut World, player: u8, unit: u16, (x, y): (i32, i32), level: u8) -> bool {
    order(
        w,
        player,
        Command::SpawnHero {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
            level,
        },
    )
}

fn hero_regiment(w: &World, player: u8, unit: u16) -> RegimentHandle {
    (w.heroes())
        .find(|h| h.owner == player && h.unit == unit)
        .and_then(|h| h.regiment)
        .expect("on the field")
}

fn hp(w: &World, r: RegimentHandle) -> u16 {
    let i = w.regiments.resolve(r).unwrap();
    w.regiments.hp[i * MAX_MODELS as usize]
}

fn seconds(w: &mut World, s: u32) {
    for _ in 0..s * TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

#[test]
fn a_hero_takes_the_field_at_its_level_with_that_levels_experience() {
    let mut w = field(1, false);
    assert!(put_hero(&mut w, 0, HERO, (40, 40), 5));
    let h = *w.heroes().next().unwrap();
    assert_eq!((h.level, h.xp), (5, LEVELS[3]));
    let r = h.regiment.unwrap();
    assert_eq!(hp(&w, r), 300 + 4 * GROWTH.hp);
    assert_eq!(w.health_of(r), Some((420, 420)));
    let mut level_one = field(1, false);
    assert!(put_hero(&mut level_one, 0, HERO, (40, 40), 1));
    assert_eq!(level_one.heroes().next().unwrap().xp, 0);
}

#[test]
fn a_hero_is_placed_once_three_a_player_and_only_with_the_hero_table() {
    let mut w = field(1, false);
    assert!(put_hero(&mut w, 0, HERO, (30, 40), 1));
    assert!(!put_hero(&mut w, 0, HERO, (40, 40), 1), "each hero once");
    assert!(
        put_hero(&mut w, 1, HERO, (30, 90), 1),
        "the enemy has his own"
    );
    assert!(put_hero(&mut w, 0, OTHER_HERO, (40, 40), 2));
    assert!(put_hero(&mut w, 0, THIRD_HERO, (50, 40), 3));
    assert_eq!(MAX_HEROES, 3);
    assert!(
        !put_hero(&mut w, 0, FOURTH_HERO, (60, 40), 1),
        "three at most"
    );
    assert!(!put_hero(&mut w, 1, LINE, (60, 90), 1), "not a hero");
    assert!(!put_hero(&mut w, 1, OTHER_HERO, (60, 90), 0), "no level 0");
    assert!(
        !put_hero(&mut w, 1, OTHER_HERO, (60, 90), 11),
        "no level 11"
    );
    assert!(!put_hero(&mut w, 1, OTHER_HERO, (-5, 90), 1), "off the map");
    // Without the hero table there are no heroes.
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut bare = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: crate::Sky::STILL,
        generals: true,
    });
    assert!(!put_hero(&mut bare, 0, HERO, (40, 40), 1));
    // The table is given once, before the first tick.
    assert!(!w.field_roster(&roster()));
}

#[test]
fn a_sides_first_hero_is_its_general_at_his_own_hit_points() {
    let mut w = field(1, false);
    let line = spawn(&mut w, 0, LINE, 40, 50);
    assert!(put_hero(&mut w, 0, HERO, (40, 44), 3));
    assert!(put_hero(&mut w, 0, OTHER_HERO, (48, 44), 1));
    let first = hero_regiment(&w, 0, HERO);
    let before = hp(&w, first);
    seconds(&mut w, 2);
    let g = *w.generals().next().expect("a general");
    assert!(g.hero);
    assert_eq!(g.regiment, first, "the first hero put down");
    assert_eq!(hp(&w, first), before, "no general's armour on a hero");
    let i = w.regiments.resolve(line).unwrap();
    assert!(w.regiments.led[i], "he leads");
    // A side without heroes rides with its best regiment, as before.
    spawn(&mut w, 1, LINE, 40, 100);
    seconds(&mut w, 2);
    let theirs = w.generals().find(|g| g.party == 1).expect("theirs");
    assert!(!theirs.hero);
}

#[test]
fn a_hero_generals_fall_shakes_his_side_once() {
    let mut w = field(1, false);
    let line = spawn(&mut w, 0, LINE, 40, 50);
    assert!(put_hero(&mut w, 0, HERO, (40, 44), 1));
    seconds(&mut w, 2);
    let hero = hero_regiment(&w, 0, HERO);
    assert!(w.generals().next().unwrap().hero);
    let i = w.regiments.resolve(line).unwrap();
    let before = w.regiments.morale[i];
    assert!(order(&mut w, 0, Command::RemoveRegiment { regiment: hero }));
    let after = w.regiments.morale[i];
    let lost = (before - after) / crate::morale::SCALE;
    // His leadership lifts a point a second or so on top; the panic counts
    // once, not again for the hero's own fall.
    assert!(
        (PANIC - 2..=PANIC + 1).contains(&lost),
        "lost {lost} morale points"
    );
    assert!(w.generals().next().unwrap().fell.is_some());
    assert!(w.heroes().next().unwrap().dead());
}

#[test]
fn a_hero_levels_as_his_army_kills_in_a_field_battle() {
    let mut w = field(1, false);
    assert!(put_hero(&mut w, 0, HERO, (40, 40), 1));
    let hero = hero_regiment(&w, 0, HERO);
    let foe = spawn(&mut w, 1, LINE, 40, 46);
    order(
        &mut w,
        0,
        Command::Attack {
            regiment: hero,
            target: foe,
        },
    );
    seconds(&mut w, 30);
    let h = *w.heroes().next().unwrap();
    assert!(h.xp > 0, "the dead pay their experience: {h:?}");
}

#[test]
fn a_hero_general_is_his_sides_champion_and_lives_if_he_loses() {
    let (mut called, mut lost) = (0, 0);
    for seed in 0..60 {
        let mut w = field(seed, true);
        spawn(&mut w, 0, LINE, 60, 30);
        assert!(put_hero(&mut w, 0, HERO, (60, 24), 1));
        spawn(&mut w, 1, LINE, 60, 80);
        let hero = hero_regiment(&w, 0, HERO);
        let Some(challenge) = (0..40 * TICKS_PER_SECOND).find_map(|_| {
            w.tick(&[]);
            w.challenge().copied()
        }) else {
            continue;
        };
        called += 1;
        assert!(
            challenge.champions.contains(&hero),
            "seed {seed}: {challenge:?}"
        );
        while w.tick_count() <= challenge.ends() + 1 {
            w.tick(&[]);
        }
        let ours = challenge.champions.iter().position(|&c| c == hero).unwrap();
        let ended = w.challenge().copied().unwrap();
        if ended.abandoned.is_none() && ended.winner == Some(1 - ours as u8) {
            lost += 1;
            let left = hp(&w, hero);
            assert!(left > 0 && left < 300, "seed {seed}: wounded, {left} left");
        }
    }
    assert!(called > 5, "{called} challenges");
    assert!(lost > 0, "the hero never lost a duel in {called}");
}

/// A hero's ability that needs an economy or the Tithe's graves is refused
/// in a Field Battle; the rest work.
#[test]
fn abilities_that_need_conquest_are_refused_in_a_field_battle() {
    use crate::abilities::needs_conquest;
    use crate::structures::Refusal;
    assert!(needs_conquest(Kit::Magnate, 1) && needs_conquest(Kit::Undertaker, 0));
    assert!(needs_conquest(Kit::Maestro, 1) && !needs_conquest(Kit::Maestro, 0));
    let mut roster = roster();
    roster[HERO as usize].kit = Some(HeroKit {
        kit: Kit::Maestro,
        summons: None,
    });
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities { regiments: 8 },
        units: &units,
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: crate::Sky::STILL,
        generals: true,
    });
    assert!(w.field_roster(&roster));
    assert!(put_hero(&mut w, 0, HERO, (40, 40), 10));
    let hero = hero_regiment(&w, 0, HERO);
    assert_eq!(w.can_cast(0, hero, 0), Ok(()));
    assert_eq!(w.can_cast(0, hero, 1), Err(Refusal::NoBuildings));
}
