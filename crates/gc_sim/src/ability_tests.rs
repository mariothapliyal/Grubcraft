//! Tests of hero abilities (see [`crate::abilities`]) in one small
//! Conquest world: a hero of any kit, hired at an altar, and regiments
//! put down round it.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::abilities::{CastTarget, HeroKit, Kit, rank};
use crate::conquest::ConquestUnit;
use crate::heroes::HeroGrowth;
use crate::morale::MoraleState;
use crate::nav::CELLS_PER_TILE;
use crate::scenario::{DRILL_UNITS, NORTH};
use crate::structures::{Armour, Refusal, Role, Trap, WallTwist};
use crate::{
    BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
    RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const MAP: u32 = 128;
const HALL: u16 = 0;
const ALTAR: u16 = 1;
/// 24 soldiers of 10 hit points.
const LINE: u16 = 0;
/// One model of 300 hit points, hired at the altar.
const HERO: u16 = 1;
/// What the hero summons: 8 of 5 hit points.
const SPAWN: u16 = 2;
/// A ranged line: 12 of 10 hit points, shooting 3 a shot.
const BOWS: u16 = 3;

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
    let spawn = UnitStats {
        models: 8,
        files: 4,
        hp_per_model: 5,
        ..base
    };
    let bows = UnitStats {
        models: 12,
        files: 6,
        range: Fx::from_int(20),
        missile_damage: 3,
        reload_ticks: 40,
        accuracy: 100,
        ..base
    };
    [base, hero, spawn, bows]
}

fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 9,
        capacities: Capacities { regiments: 24 },
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
        supply: 60,
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
        train_ticks: 20,
        trained_at: Some(ALTAR),
        tier: 1,
        hero: Some(HeroGrowth::default()),
        kit: Some(HeroKit {
            kit: Kit::Daimyo,
            summons: Some(SPAWN),
        }),
        ..ConquestUnit::default()
    };
    let mut start = [(0, 0); MAX_PARTIES];
    start[0] = (5000, 1000);
    start[1] = (5000, 1000);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[hall, altar],
        units: &[line, hero, line, line],
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

/// A hero of `kit` at `level`, standing at (64, 64).
fn hero(kit: Kit, level: u8) -> (World, RegimentHandle) {
    let mut w = world();
    for (building, tile) in [(HALL, (10, 10)), (ALTAR, (20, 10))] {
        let spawn = Command::SpawnStructure {
            building,
            tile,
            turn: 0,
        };
        assert!(order(&mut w, 0, spawn));
    }
    let altar = w
        .structures()
        .unwrap()
        .iter()
        .find(|v| v.tile == (20, 10))
        .unwrap()
        .handle;
    let hire = Command::Train {
        structure: altar,
        unit: HERO,
    };
    assert!(order(&mut w, 0, hire));
    seconds(&mut w, 2);
    let r = w.heroes().next().and_then(|h| h.regiment).expect("hired");
    let c = w.conquest.as_mut().unwrap();
    let h = c.heroes.get_mut(0);
    h.kit = Some(HeroKit {
        kit,
        summons: Some(SPAWN),
    });
    h.level = level;
    let i = w.regiments.resolve(r).unwrap();
    let to = FxVec2::from_ints(64, 64);
    let facing = w.regiments.facing[i];
    w.regiments.place(r, to, facing, 0).unwrap();
    w.regiments.target[i] = None;
    w.tick(&[]);
    (w, r)
}

fn cast(w: &mut World, hero: RegimentHandle, ability: u8, target: CastTarget) -> bool {
    order(
        w,
        0,
        Command::Cast {
            regiment: hero,
            ability,
            target,
        },
    )
}

fn models(w: &World, r: RegimentHandle) -> u16 {
    w.regiments.get(r).map_or(0, |v| v.models)
}

fn slot(w: &World, r: RegimentHandle) -> usize {
    w.regiments.resolve(r).unwrap()
}

#[test]
fn the_basic_ability_ranks_up_by_level_and_the_ultimate_opens_at_six() {
    let ranks: Vec<(u8, u8)> = (1..=10).map(|l| (rank(0, l), rank(1, l))).collect();
    assert_eq!(
        ranks,
        [
            (1, 0),
            (1, 0),
            (1, 0),
            (2, 0),
            (2, 0),
            (2, 1),
            (3, 1),
            (3, 1),
            (3, 1),
            (3, 1)
        ]
    );
    let (mut w, r) = hero(Kit::Daimyo, 5);
    assert_eq!(
        w.cast_check(0, r, 1, CastTarget::Hero),
        Err(Refusal::Locked)
    );
    assert!(!cast(&mut w, r, 1, CastTarget::Hero));
    // The other side can't cast with it, and the aim must fit.
    assert_eq!(
        w.cast_check(1, r, 0, CastTarget::Hero),
        Err(Refusal::Unknown)
    );
    let point = CastTarget::Point(FxVec2::from_ints(70, 64));
    assert_eq!(w.cast_check(0, r, 0, point), Err(Refusal::Unknown));
}

#[test]
fn rally_rallies_routers_near_and_lifts_everyone_then_cools_down() {
    let (mut w, r) = hero(Kit::Daimyo, 4);
    let near = spawn(&mut w, 0, LINE, 70, 64);
    let routed = spawn(&mut w, 0, LINE, 58, 64);
    let far = spawn(&mut w, 0, LINE, 100, 64);
    let ri = slot(&w, routed);
    w.regiments.morale_state[ri] = MoraleState::Routing;
    w.regiments.morale[ri] = 0;
    let ni = slot(&w, near);
    w.regiments.morale[ni] = 200;
    let fi = slot(&w, far);
    w.regiments.morale[fi] = 200;
    assert!(cast(&mut w, r, 0, CastTarget::Hero));
    // Rallied, and its 25 morale on top steady it.
    assert_eq!(w.regiments.morale_state[ri], MoraleState::Steady);
    // Rank 2: 25 morale.
    // (Both may have got a little back by themselves since.)
    assert!(w.regiments.morale[ni] >= 200 + 250);
    assert!(w.regiments.morale[fi] < 200 + 250);
    let h = *w.hero_of(r).unwrap();
    assert_eq!(h.cooling(0, w.tick_count()), 40 * TICKS_PER_SECOND - 1);
    assert_eq!(
        w.cast_check(0, r, 0, CastTarget::Hero),
        Err(Refusal::Cooling)
    );
}

#[test]
fn a_cast_out_of_reach_walks_the_hero_there_and_another_order_calls_it_off() {
    let (mut w, r) = hero(Kit::HollowBell, 1);
    let foe = spawn(&mut w, 1, LINE, 64, 84);
    assert!(cast(&mut w, r, 0, CastTarget::Regiment(foe)));
    assert!(w.hero_of(r).unwrap().pending.is_some(), "walking");
    let fi = slot(&w, foe);
    assert_eq!(w.regiments.stunned_left[fi], 0);
    seconds(&mut w, 8);
    // In reach of 10 tiles it tolled: 1.5 s at rank 1.
    let h = *w.hero_of(r).unwrap();
    assert!(h.pending.is_none());
    assert!(h.cooling(0, w.tick_count()) > 0, "cast");
    let from = w.regiments.get(r).unwrap().position;
    assert!((FxVec2::from_ints(64, 84) - from).length() <= Fx::from_int(10));

    // Another order calls a walking cast off.
    let (mut w, r) = hero(Kit::HollowBell, 1);
    let foe = spawn(&mut w, 1, LINE, 64, 100);
    assert!(cast(&mut w, r, 0, CastTarget::Regiment(foe)));
    let go = Command::MoveTo {
        regiment: r,
        target: FxVec2::from_ints(40, 64),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
        brigade: 0,
    };
    assert!(order(&mut w, 0, go));
    assert!(w.hero_of(r).unwrap().pending.is_none());
}

#[test]
fn a_stunned_regiment_stands_and_strikes_nothing_and_a_stunned_hero_cant_cast() {
    let (mut w, r) = hero(Kit::HollowBell, 7);
    let foe = spawn(&mut w, 1, LINE, 64, 70);
    assert!(cast(&mut w, r, 0, CastTarget::Regiment(foe)));
    let fi = slot(&w, foe);
    // Rank 3: 3.5 s.
    assert_eq!(w.regiments.stunned_left[fi], 70 - 1);
    let at = w.regiments.position[fi];
    let go = Command::MoveTo {
        regiment: foe,
        target: FxVec2::from_ints(64, 100),
        facing: None,
        files: 0,
        pace: Fx::ZERO,
        brigade: 0,
    };
    assert!(order(&mut w, 1, go));
    seconds(&mut w, 2);
    assert_eq!(w.regiments.position[fi], at, "held fast");
    seconds(&mut w, 3);
    assert_ne!(w.regiments.position[fi], at, "it marches on once free");
    let i = slot(&w, r);
    w.regiments.stun(i, 40);
    let peal = CastTarget::Hero;
    assert_eq!(w.cast_check(0, r, 1, peal), Err(Refusal::Stunned));
}

#[test]
fn thousand_cuts_shares_its_blows_and_nothing_touches_the_ronin() {
    let (mut w, r) = hero(Kit::Ronin, 6);
    let a = spawn(&mut w, 1, LINE, 64, 67);
    let b = spawn(&mut w, 1, LINE, 64, 61);
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    let i = slot(&w, r);
    assert!(w.regiments.untouchable_left[i] > 0);
    w.regiments.take_damage(i, 100);
    assert_eq!(w.regiments.hp_left(i), 300, "untouchable");
    seconds(&mut w, 3);
    // 8 cuts of 45 shared: 180 hit points each, 18 soldiers of 10.
    let lost = (24 - models(&w, a), 24 - models(&w, b));
    assert!(lost.0 >= 17 && lost.1 >= 17, "{lost:?}");
    assert!(w.heroes().next().unwrap().regiment.is_some());
}

#[test]
fn iaijutsu_dash_blinks_and_cuts_the_line_it_crosses_but_not_through_walls() {
    let (mut w, r) = hero(Kit::Ronin, 1);
    let crossed = spawn(&mut w, 1, LINE, 69, 64);
    let aside = spawn(&mut w, 1, LINE, 69, 80);
    assert!(cast(
        &mut w,
        r,
        0,
        CastTarget::Point(FxVec2::from_ints(73, 64))
    ));
    let at = w.regiments.get(r).unwrap().position;
    assert!(
        (at - FxVec2::from_ints(73, 64)).length() < Fx::from_int(2),
        "{at:?}"
    );
    // Rank 1: 60, six soldiers.
    assert_eq!(models(&w, crossed), 24 - 6);
    assert_eq!(models(&w, aside), 24);
}

#[test]
fn spawn_brood_puts_regiments_down_beside_the_queen_that_vanish_and_feed_nobody() {
    let (mut w, r) = hero(Kit::Queen, 4);
    let before = w.regiments.iter().count();
    assert!(cast(&mut w, r, 0, CastTarget::Hero));
    let spawned: Vec<_> = (w.regiments.iter())
        .filter(|v| v.unit == SPAWN && v.owner == 0)
        .map(|v| (v.handle, v.position))
        .collect();
    assert_eq!(spawned.len(), 2, "rank 2");
    assert_eq!(w.regiments.iter().count(), before + 2);
    let at = w.regiments.get(r).unwrap().position;
    for &(_, p) in &spawned {
        assert!((p - at).length() <= Fx::from_int(crate::abilities::SUMMON_TILES));
    }
    // An enemy hero near them gains nothing when they die.
    let i = slot(&w, spawned[0].0);
    assert!(w.regiments.summoned_left[i] > 0);
    seconds(&mut w, 31);
    assert!(
        w.regiments.iter().all(|v| v.unit != SPAWN || v.owner != 0),
        "gone"
    );
}

#[test]
fn foreclosure_takes_a_third_of_a_regiment_below_half_strength_only() {
    let (mut w, r) = hero(Kit::Usurer, 6);
    let foe = spawn(&mut w, 1, LINE, 64, 70);
    // Refused at the order: too sturdy, and no cooldown spent.
    assert_eq!(
        w.cast_check(0, r, 1, CastTarget::Regiment(foe)),
        Err(Refusal::Sturdy)
    );
    assert!(!cast(&mut w, r, 1, CastTarget::Regiment(foe)));
    assert_eq!(models(&w, foe), 24);
    assert_eq!(w.hero_of(r).unwrap().cooling(1, w.tick_count()), 0);
    let fi = slot(&w, foe);
    w.regiments.take_damage(fi, 150);
    assert_eq!(models(&w, foe), 9);
    assert!(cast(&mut w, r, 1, CastTarget::Regiment(foe)));
    assert_eq!(models(&w, foe), 6);
}

#[test]
fn leech_debt_drains_a_regiment_into_the_usurer() {
    let (mut w, r) = hero(Kit::Usurer, 1);
    let foe = spawn(&mut w, 1, LINE, 64, 70);
    let i = slot(&w, r);
    w.regiments.take_damage(i, 100);
    assert!(cast(&mut w, r, 0, CastTarget::Regiment(foe)));
    seconds(&mut w, 3);
    // Three pulses of 15.
    assert_eq!(w.regiments.hp_left(slot(&w, foe)), 240 - 45);
    assert_eq!(w.regiments.hp_left(i), 200 + 45);
}

#[test]
fn kitsune_storm_burns_every_enemy_inside_each_second() {
    let (mut w, r) = hero(Kit::Onmyoji, 6);
    let inside = spawn(&mut w, 1, LINE, 64, 80);
    let outside = spawn(&mut w, 1, LINE, 100, 84);
    let friend = spawn(&mut w, 0, LINE, 64, 91);
    assert!(cast(
        &mut w,
        r,
        1,
        CastTarget::Point(FxVec2::from_ints(64, 84))
    ));
    seconds(&mut w, 11);
    // Ten seconds of 20.
    assert_eq!(models(&w, inside), 24 - 20);
    assert_eq!(models(&w, outside), 24);
    assert_eq!(models(&w, friend), 24);
    assert!(w.heroes().next().unwrap().regiment.is_some());
}

#[test]
fn split_beam_charges_three_volleys() {
    let (mut w, r) = hero(Kit::Refractor, 1);
    let i = slot(&w, r);
    assert!(cast(&mut w, r, 0, CastTarget::Hero));
    assert_eq!(
        (w.regiments.volleys_up[i], w.regiments.volley_bonus[i]),
        (3, 50)
    );
    assert_eq!(w.regiments.beam_percent(i), 150);
}

#[test]
fn a_market_crash_halves_every_enemys_grub_income() {
    let (mut w, r) = hero(Kit::Magnate, 6);
    assert_eq!(
        w.cast_check(0, r, 0, CastTarget::Hero),
        Err(Refusal::Unknown),
        "passive"
    );
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    let c = w.conquest.as_mut().unwrap();
    assert!(c.economy.crashed_left(1) > 0 && c.economy.crashed_left(0) == 0);
    let before = c.economy.store(1).0;
    let regs = &w.regiments;
    c.economy.earn_grub(1, 100, regs, &[]);
    assert_eq!(c.economy.store(1).0, before + 50);
}

#[test]
fn crescendo_inspires_friends_near_the_maestro() {
    let (mut w, r) = hero(Kit::Maestro, 7);
    let bows = spawn(&mut w, 0, BOWS, 64, 70);
    let far = spawn(&mut w, 0, BOWS, 64, 100);
    assert!(cast(&mut w, r, 0, CastTarget::Hero));
    let (bi, fi) = (slot(&w, bows), slot(&w, far));
    assert_eq!(w.regiments.attack_bonus(bi), 20);
    assert_eq!(w.regiments.inspired_reload(bi, 135), 100);
    assert_eq!(w.regiments.attack_bonus(fi), 0);
    seconds(&mut w, 11);
    assert_eq!(w.regiments.attack_bonus(bi), 0, "worn off");
}

#[test]
fn shoguns_banner_hearten_every_regiment_of_the_daimyos_player() {
    let (mut w, r) = hero(Kit::Daimyo, 6);
    let far = spawn(&mut w, 0, LINE, 120, 120);
    let foe = spawn(&mut w, 1, LINE, 100, 100);
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    let (fi, ei) = (slot(&w, far), slot(&w, foe));
    assert!(w.regiments.steadfast_left[fi] > 0 && w.regiments.march_left[fi] > 0);
    assert_eq!(w.regiments.march[fi], 20);
    assert_eq!(w.regiments.steadfast_left[ei], 0);
}

#[test]
fn maw_of_the_titan_culls_twelve_soldiers_and_bites_heroes_and_monsters() {
    let (mut w, r) = hero(Kit::Devourer, 6);
    let a = spawn(&mut w, 1, LINE, 64, 78);
    let b = spawn(&mut w, 1, LINE, 70, 78);
    let far = spawn(&mut w, 1, LINE, 100, 78);
    assert!(cast(
        &mut w,
        r,
        1,
        CastTarget::Point(FxVec2::from_ints(67, 78))
    ));
    assert_eq!(
        (models(&w, a), models(&w, b), models(&w, far)),
        (18, 18, 24)
    );
}

#[test]
fn confusion_cloud_stuns_and_disorders_the_enemies_inside() {
    let (mut w, r) = hero(Kit::Whisperer, 1);
    let foe = spawn(&mut w, 1, LINE, 64, 76);
    let friend = spawn(&mut w, 0, LINE, 70, 80);
    assert!(cast(
        &mut w,
        r,
        0,
        CastTarget::Point(FxVec2::from_ints(64, 78))
    ));
    let (fi, ai) = (slot(&w, foe), slot(&w, friend));
    assert!(w.regiments.stunned_left[fi] > 0 && w.regiments.disorder[fi] > 0);
    assert_eq!(w.regiments.stunned_left[ai], 0);
}

#[test]
fn puppet_army_stuns_only_the_routing_enemies_near() {
    let (mut w, r) = hero(Kit::Whisperer, 6);
    let running = spawn(&mut w, 1, LINE, 64, 76);
    let standing = spawn(&mut w, 1, LINE, 76, 64);
    let ri = slot(&w, running);
    w.regiments.morale_state[ri] = MoraleState::Routing;
    w.regiments.morale[ri] = 0;
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    assert!(w.regiments.stunned_left[ri] > 0);
    assert_eq!(w.regiments.stunned_left[slot(&w, standing)], 0);
}

#[test]
fn burrow_charge_erupts_among_the_enemies_where_it_lands() {
    let (mut w, r) = hero(Kit::Devourer, 1);
    let foe = spawn(&mut w, 1, LINE, 64, 78);
    assert!(cast(
        &mut w,
        r,
        0,
        CastTarget::Point(FxVec2::from_ints(64, 76))
    ));
    let at = w.regiments.get(r).unwrap().position;
    assert!(
        (at - FxVec2::from_ints(64, 76)).length() < Fx::from_int(3),
        "{at:?}"
    );
    // Rank 1: 60, six soldiers; and in disorder.
    assert_eq!(models(&w, foe), 18);
    assert!(w.regiments.disorder[slot(&w, foe)] > 0);
}

#[test]
fn deafening_peal_shakes_every_enemy_near() {
    let (mut w, r) = hero(Kit::HollowBell, 6);
    let near = spawn(&mut w, 1, LINE, 64, 74);
    let far = spawn(&mut w, 1, LINE, 64, 100);
    seconds(&mut w, 1);
    let (ni, fi) = (slot(&w, near), slot(&w, far));
    let (a, b) = (w.regiments.morale[ni], w.regiments.morale[fi]);
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    assert!(w.regiments.morale[ni] <= a - 300 + 30);
    assert!(w.regiments.morale[fi] >= b - 30);
}

#[test]
fn swarm_tide_inspires_the_queens_regiments_and_hatches_four() {
    let (mut w, r) = hero(Kit::Queen, 6);
    let line = spawn(&mut w, 0, LINE, 70, 64);
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    let li = slot(&w, line);
    assert_eq!(w.regiments.attack_bonus(li), 20);
    assert!(w.regiments.is_unshaken(li));
    let hatched = w.regiments.iter().filter(|v| v.unit == SPAWN).count();
    assert_eq!(hatched, 4);
}

#[test]
fn prismatic_lance_shares_its_damage_down_the_line() {
    let (mut w, r) = hero(Kit::Refractor, 6);
    let a = spawn(&mut w, 1, LINE, 64, 74);
    let b = spawn(&mut w, 1, LINE, 64, 94);
    let aside = spawn(&mut w, 1, LINE, 80, 80);
    assert!(cast(
        &mut w,
        r,
        1,
        CastTarget::Point(FxVec2::from_ints(64, 70))
    ));
    // 250 shared by two: 125 each, 12 soldiers and a wound.
    assert_eq!(
        (models(&w, a), models(&w, b), models(&w, aside)),
        (12, 12, 24)
    );
}

#[test]
fn symphony_and_compound_interest_work_the_economy() {
    let (mut w, r) = hero(Kit::Maestro, 6);
    assert!(cast(&mut w, r, 1, CastTarget::Hero));
    assert!(w.conquest.as_ref().unwrap().structures.in_symphony(0));
    let (mut w, _) = hero(Kit::Magnate, 7);
    let before = w.conquest.as_ref().unwrap().economy.store(0).0;
    seconds(&mut w, 61);
    // Rank 3: 3% of the bank, at most 45 a minute.
    let after = w.conquest.as_ref().unwrap().economy.store(0).0;
    assert_eq!(after, before + 45);
}

#[test]
fn raise_dead_needs_corpses_and_raises_as_many_as_lie_near_up_to_its_rank() {
    let (mut w, r) = hero(Kit::Undertaker, 1);
    // No corpses: refused, told, no cooldown.
    assert!(cast(&mut w, r, 0, CastTarget::Hero));
    assert!(w.events().iter().any(|e| matches!(
        e,
        crate::events::Event::CastRefused {
            why: Refusal::NoCorpses,
            ..
        }
    )));
    assert_eq!(w.hero_of(r).unwrap().cooling(0, w.tick_count()), 0);
}
