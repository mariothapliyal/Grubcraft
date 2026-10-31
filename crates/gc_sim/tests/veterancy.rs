//! Veterancy (docs/05 "Veterancy", docs/29 "Holding until bloodied"):
//! regiments earn ranks from the enemy soldiers they kill, in melee and
//! with volleys, and each rank lowers their breaking point.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::veterancy::RANK_BREAK;
use gc_sim::{
    Capacities, Command, Ground, MatchSetup, PlayerCommand, RegimentHandle, Sky, UnitStats, World,
};

const SOUTH: Angle = Angle::QUARTER;
const HERO: u16 = 0;
const SPEARS: u16 = 1;
const BOWS: u16 = 2;
const POST: u16 = 3;

fn units() -> [UnitStats; 4] {
    // A small, deadly band (224 hit points in all) that cuts down spearmen
    // of 8 hit points each.
    let hero = UnitStats {
        models: 16,
        hp_per_model: 14,
        files: 8,
        melee_attack: 90,
        melee_defence: 90,
        melee_damage: 8,
        morale: 80,
        ..DRILL_UNITS[0]
    };
    let spears = UnitStats {
        models: 28,
        hp_per_model: 8,
        files: 8,
        melee_attack: 10,
        melee_defence: 10,
        melee_damage: 1,
        // Never rout: they stand to be cut down.
        morale: 0,
        ..DRILL_UNITS[0]
    };
    // Archers of 64 hit points in all, shooting at a post that never routs.
    let bows = UnitStats {
        models: 16,
        hp_per_model: 4,
        files: 8,
        range: Fx::from_int(40),
        missile_damage: 3,
        reload_ticks: 40,
        accuracy: 80,
        morale: 50,
        ..DRILL_UNITS[0]
    };
    let post = UnitStats {
        models: 28,
        hp_per_model: 8,
        files: 8,
        speed: Fx::from_ratio(1, 100),
        morale: 0,
        ..DRILL_UNITS[0]
    };
    [hero, spears, bows, post]
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 3,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: Ground::FLAT,
        champions: false,
        sky: Sky::STILL,
        generals: false,
    })
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player,
        command: Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn attack(w: &mut World, player: u8, regiment: RegimentHandle, target: RegimentHandle) {
    w.tick(&[PlayerCommand {
        player,
        command: Command::Attack { regiment, target },
    }]);
}

#[test]
fn killing_earns_ranks_that_lower_the_breaking_point() {
    let units = units();
    let mut w = world(&units);
    let hero = spawn(&mut w, 0, HERO, 100, 100, NORTH);
    let start = w.regiments.get(hero).unwrap();
    assert_eq!(start.rank, 0);
    let fresh_break = start.break_at;
    // Two regiments of spearmen, each worth as much as the hero's band,
    // come at it one after another.
    let mut ranks = vec![0];
    for wave in 0..2 {
        let foe = spawn(&mut w, 1, SPEARS, 100, 85, SOUTH);
        attack(&mut w, 1, foe, hero);
        for _ in 0..20 * 120 {
            w.tick(&[]);
            if w.regiments.get(foe).is_none() {
                break;
            }
        }
        assert!(
            w.regiments.get(foe).is_none(),
            "wave {wave} still stands after two minutes"
        );
        let me = w.regiments.get(hero).expect("the hero's band fell");
        println!(
            "after wave {wave}: rank {}, breaking point {}%",
            me.rank, me.break_at
        );
        ranks.push(me.rank);
    }
    // Half its worth makes rank 1, its worth rank 2, twice rank 3.
    assert!(ranks.windows(2).all(|p| p[0] <= p[1]), "ranks {ranks:?}");
    assert_eq!(ranks[1], 2, "one band's worth killed: {ranks:?}");
    assert_eq!(*ranks.last().unwrap(), 3, "ranks {ranks:?}");
    let me = w.regiments.get(hero).unwrap();
    assert_eq!(me.break_at, fresh_break - 3 * RANK_BREAK);
}

#[test]
fn archers_earn_ranks_from_their_volleys() {
    let units = units();
    let mut w = world(&units);
    let bows = spawn(&mut w, 0, BOWS, 100, 100, NORTH);
    let post = spawn(&mut w, 1, POST, 100, 75, SOUTH);
    attack(&mut w, 0, bows, post);
    let mut first = None;
    for t in 0..20 * 120 {
        w.tick(&[]);
        let rank = w.regiments.get(bows).unwrap().rank;
        if rank > 0 && first.is_none() {
            first = Some(t);
        }
        if rank >= 2 {
            break;
        }
    }
    let rank = w.regiments.get(bows).unwrap().rank;
    let left = w.regiments.get(post).map_or(0, |r| r.models);
    println!("archers: rank {rank} after {first:?} ticks to the first, post down to {left}");
    // The archers' 64 hit points: 8 spearmen killed is their worth.
    assert!(rank >= 2, "rank {rank}, post down to {left} of 28");
    assert!(left <= 28 - 8, "rank {rank} with only {} killed", 28 - left);
}
