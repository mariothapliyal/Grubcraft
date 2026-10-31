//! Bolt and Blast towers (docs/26-fortifications.md, stage 5): a bolt runs
//! down one file and hits big soldiers harder, a blast splashes the
//! regiments beside its target and never comes down close, and each
//! faction's tower does its own thing to what it hits.

use gc_fixed::{Fx, FxVec2};
use gc_sim::events::Event;
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::structures::{Armour, Quirk, Role, TowerAttack, TowerShot};
use gc_sim::towers::{
    ACID_POOL_TICKS, BLAST_MIN_RANGE, COLLECT_GRUB, DRAG_TILES, RAISE_AT, RAISED_TICKS,
};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, MatchSetup, PlayerCommand,
    RegimentHandle, Shooter, StructureHandle, TICKS_PER_SECOND, UnitStats, World,
};

/// Spears: 24 soldiers of 10 hit points, 8 files (3 ranks).
const SPEARS: u16 = 0;
/// A monster: one soldier of 300 hit points.
const MONSTER: u16 = 1;
/// What a Corpse Catapult raises.
const PIKES: u16 = 2;

/// Building types, one for each tower tried.
const BOLT: u16 = 0;
const BLAST: u16 = 1;
const COLLECTOR: u16 = 2;
const CATAPULT: u16 = 3;
const LANCE: u16 = 4;
const OBELISK: u16 = 5;
const HARPOON: u16 = 6;
const BLOATER: u16 = 7;
const HOROKU: u16 = 8;
/// A plain arrow tower and its level 2, with fire arrows.
const ARROWS: u16 = 9;
const ARROWS_2: u16 = 10;
/// A hall, for a side's tier.
const HALL: u16 = 11;

fn bolt(quirk: Quirk) -> TowerAttack {
    TowerAttack {
        range: Fx::from_int(34),
        damage: 200,
        reload_ticks: 5 * TICKS_PER_SECOND as u16,
        shots: 1,
        shot: TowerShot::Bolt,
        quirk,
    }
}

fn blast(quirk: Quirk) -> TowerAttack {
    TowerAttack {
        range: Fx::from_int(36),
        damage: 6,
        reload_ticks: 6 * TICKS_PER_SECOND as u16,
        shots: 6,
        shot: TowerShot::Blast,
        quirk,
    }
}

fn arrows(quirk: Quirk, range: i32) -> TowerAttack {
    TowerAttack {
        range: Fx::from_int(range),
        damage: 5,
        reload_ticks: 2 * TICKS_PER_SECOND as u16,
        shots: 2,
        shot: TowerShot::Arrows,
        quirk,
    }
}

fn tower(attack: TowerAttack) -> BuildingStats {
    BuildingStats {
        role: Role::Defence,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 600,
        hp: 1000,
        armour: Armour::Fortified,
        width: 2,
        depth: 2,
        supply: 0,
        flags: 0,
        attack: Some(attack),
        field: 0,
        tunes: None,
        links: 0,
        twist: gc_sim::structures::WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    }
}

fn units() -> [UnitStats; 3] {
    let spears = DRILL_UNITS[0];
    let monster = UnitStats {
        models: 1,
        hp_per_model: 300,
        files: 1,
        mass: gc_sim::Mass::Heavy,
        ..spears
    };
    let pikes = UnitStats {
        models: 19,
        hp_per_model: 7,
        files: 10,
        ..spears
    };
    [spears, monster, pikes]
}

/// An open map 96 tiles across, in Conquest, with every tower tried here.
fn conquest() -> World {
    let tiles = 96;
    let side = tiles as usize * CELLS_PER_TILE as usize;
    let nav = vec![1u8; side * side];
    let units = units();
    let mut w = World::new(&MatchSetup {
        seed: 5,
        capacities: Capacities { regiments: 64 },
        units: &units,
        map_tiles: tiles,
        nav: &nav,
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let buildings = [
        tower(bolt(Quirk::None)),
        tower(blast(Quirk::None)),
        tower(bolt(Quirk::Collects)),
        tower(blast(Quirk::Raises(PIKES))),
        tower(bolt(Quirk::Pierces)),
        tower(blast(Quirk::Disorders)),
        tower(bolt(Quirk::Drags)),
        tower(blast(Quirk::Acid)),
        tower(blast(Quirk::Burns)),
        tower(arrows(Quirk::None, 30)),
        BuildingStats {
            upgrades_from: Some(ARROWS),
            cost_grub: 0,
            build_ticks: 200,
            hp: 1500,
            ..tower(arrows(Quirk::Kindles, 34))
        },
        BuildingStats {
            role: Role::Main,
            width: 4,
            depth: 4,
            attack: None,
            ..tower(arrows(Quirk::None, 30))
        },
    ];
    let kinds = [ConquestUnit::default(); 3];
    assert!(w.conquest(&ConquestSetup {
        buildings: &buildings,
        units: &kinds,
        structures: 32,
        ..ConquestSetup::default()
    }));
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn put_up(w: &mut World, player: u8, building: u16, x: i16, y: i16) -> StructureHandle {
    assert!(order(
        w,
        player,
        Command::SpawnStructure {
            building,
            tile: (x, y),
            turn: 0,
        }
    ));
    let s = w.structures().unwrap();
    s.iter()
        .find(|v| v.tile == (i32::from(x), i32::from(y)))
        .unwrap()
        .handle
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: NORTH,
        }
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn models(w: &World, h: RegimentHandle) -> u16 {
    w.regiments.get(h).map_or(0, |r| r.models)
}

/// Hit points regiment `h` has left (its soldiers' whole hit points).
fn hp(w: &World, h: RegimentHandle, each: u32) -> u32 {
    u32::from(models(w, h)) * each
}

/// Ticks until tower `tower` looses its first volley, at most `ticks`;
/// the regiments it loosed at in that tick.
fn first_volley(w: &mut World, tower: StructureHandle, ticks: u32) -> Vec<RegimentHandle> {
    for _ in 0..ticks {
        w.tick(&[]);
        let shot: Vec<_> = (w.events().iter())
            .filter_map(|e| match *e {
                Event::Loosed {
                    shooter: Shooter::Tower(t),
                    target,
                    ..
                } if t == tower => Some(target),
                _ => None,
            })
            .collect();
        if !shot.is_empty() {
            return shot;
        }
    }
    Vec::new()
}

/// Run until every volley in the air has come down.
fn land(w: &mut World) {
    for _ in 0..10 * TICKS_PER_SECOND {
        if w.volleys().next().is_none() {
            return;
        }
        w.tick(&[]);
    }
    panic!("a volley never came down");
}

#[test]
fn a_bolt_runs_down_one_file_and_no_further() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, BOLT, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![r]);
    land(&mut w);
    // 200 hit points of bolt, but three ranks of 10 deep, rolled up to a
    // quarter more: three or four soldiers, never the 20 the damage holds.
    let lost = 24 - models(&w, r);
    assert!((2..=4).contains(&lost), "lost {lost}");
}

#[test]
fn a_bolt_hits_a_monster_twice_as_hard_and_shoots_it_first() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, BOLT, 40, 40);
    // The spears are nearer, but the monster is big.
    let spears = spawn(&mut w, 0, SPEARS, 41, 55);
    let monster = spawn(&mut w, 0, MONSTER, 41, 65);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![monster]);
    land(&mut w);
    assert_eq!(models(&w, spears), 24);
    // Twice 200, less the volleys' pace (85%), rolled ±25%: 255 to 425. The
    // 300-point monster falls to this seed's roll, which a bolt not
    // doubled (212 at most) never could do.
    assert_eq!(models(&w, monster), 0, "one bolt didn't fell the monster");
}

#[test]
fn a_blast_never_comes_down_close_and_splashes_the_regiments_beside() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, BLAST, 40, 40);
    // Too near its walls: never shot.
    let near = spawn(&mut w, 0, SPEARS, 41, 42 + BLAST_MIN_RANGE - 1);
    assert!(first_volley(&mut w, t, 8 * TICKS_PER_SECOND).is_empty());
    assert_eq!(models(&w, near), 24);
    // Out at 20 tiles, two blocks shoulder to shoulder: one is shot, both
    // are hurt.
    let a = spawn(&mut w, 0, SPEARS, 37, 62);
    let b = spawn(&mut w, 0, SPEARS, 44, 62);
    let shot = first_volley(&mut w, t, 8 * TICKS_PER_SECOND);
    assert_eq!(shot.len(), 1);
    land(&mut w);
    let (la, lb) = (24 - models(&w, a), 24 - models(&w, b));
    let (aimed, beside) = if shot[0] == a { (la, lb) } else { (lb, la) };
    assert!(
        aimed > 0 && beside > 0,
        "aimed lost {aimed}, beside {beside}"
    );
    assert_eq!(models(&w, near), 24);
}

#[test]
fn a_collectors_bolt_is_paid_for_each_soldier_it_kills() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, COLLECTOR, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    let grub = |w: &World| w.bank(1).unwrap().grub;
    let before = grub(&w);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND).len(), 1);
    land(&mut w);
    let killed = u32::from(24 - models(&w, r));
    assert!(killed > 0);
    assert_eq!(grub(&w) - before, killed * COLLECT_GRUB);
}

#[test]
fn a_corpse_catapult_raises_the_dead_it_makes_for_a_while() {
    let mut w = conquest();
    put_up(&mut w, 1, CATAPULT, 40, 40);
    let targets: Vec<_> = (0..3)
        .map(|k| spawn(&mut w, 0, SPEARS, 30 + 12 * k, 64))
        .collect();
    let mut raised = None;
    for _ in 0..90 * TICKS_PER_SECOND {
        w.tick(&[]);
        raised = w.regiments.iter().find(|v| v.owner == 1 && v.unit == PIKES);
        if raised.is_some() {
            break;
        }
    }
    let raised = raised.expect("no dead rose");
    assert_eq!(raised.models, u16::from(RAISE_AT));
    let killed: u16 = targets.iter().map(|&r| 24 - models(&w, r)).sum();
    assert!(killed >= u16::from(RAISE_AT), "{killed} killed");
    // It crumbles away.
    for _ in 0..u32::from(RAISED_TICKS) + TICKS_PER_SECOND {
        w.tick(&[]);
    }
    assert!(w.regiments.get(raised.handle).is_none());
}

#[test]
fn a_prism_lance_runs_through_the_regiments_in_line_behind() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, LANCE, 40, 40);
    // Three in a column due south, and one off to the side.
    let line: Vec<_> = [52, 60, 68]
        .iter()
        .map(|&y| spawn(&mut w, 0, SPEARS, 41, y))
        .collect();
    let aside = spawn(&mut w, 0, SPEARS, 66, 60);
    let shot = first_volley(&mut w, t, 2 * TICKS_PER_SECOND);
    assert_eq!(shot, line);
    land(&mut w);
    assert!(line.iter().all(|&r| models(&w, r) < 24));
    assert_eq!(models(&w, aside), 24);
}

#[test]
fn a_chord_obelisk_throws_what_it_hits_into_disorder() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, OBELISK, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![r]);
    land(&mut w);
    assert!(w.regiments.get(r).unwrap().disorder > 0);
}

#[test]
fn a_harpoon_maw_drags_what_it_hits_toward_it() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, HARPOON, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    let y = |w: &World| w.regiments.get(r).unwrap().position.y;
    let before = y(&w);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![r]);
    land(&mut w);
    let pulled = before - y(&w);
    assert!(
        pulled > Fx::from_int(DRAG_TILES - 1) && pulled <= Fx::from_int(DRAG_TILES) + Fx::ONE,
        "pulled {pulled:?}"
    );
}

#[test]
fn an_acid_bloater_leaves_a_pool_that_hurts_and_dries_up() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, BLOATER, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![r]);
    land(&mut w);
    let pools = w.structures().unwrap().pools();
    assert_eq!(pools.len(), 1);
    assert!(pools[0].acid && pools[0].owner == 1);
    // Standing in it, the spears bleed though no volley is in the air.
    let after_hit = hp(&w, r, 10);
    for _ in 0..3 * TICKS_PER_SECOND {
        w.tick(&[]);
        if w.volleys().next().is_some() {
            break;
        }
    }
    assert!(hp(&w, r, 10) < after_hit);
    for _ in 0..u32::from(ACID_POOL_TICKS) {
        w.tick(&[]);
    }
    assert!(
        w.structures().unwrap().pools().len() <= 1,
        "the first pool never dried up"
    );
}

#[test]
fn a_horoku_towers_fire_pots_leave_burning_ground() {
    let mut w = conquest();
    let t = put_up(&mut w, 1, HOROKU, 40, 40);
    let r = spawn(&mut w, 0, SPEARS, 41, 61);
    assert_eq!(first_volley(&mut w, t, 2 * TICKS_PER_SECOND), vec![r]);
    land(&mut w);
    let pools = w.structures().unwrap().pools();
    assert_eq!(pools.len(), 1);
    assert!(!pools[0].acid);
}

#[test]
fn a_tower_goes_up_to_level_2_and_shoots_further() {
    let mut w = conquest();
    put_up(&mut w, 1, HALL, 30, 30);
    let t = put_up(&mut w, 1, ARROWS, 40, 40);
    // Out of level 1's reach, within level 2's.
    let r = spawn(&mut w, 0, SPEARS, 41, 73);
    assert!(first_volley(&mut w, t, 3 * TICKS_PER_SECOND).is_empty());
    assert_eq!(w.can_upgrade(1, t).map(|_| ()), Ok(()));
    assert!(order(&mut w, 1, Command::Upgrade { structure: t }));
    let kind = |w: &World| w.structures().unwrap().get(t).unwrap().building;
    for _ in 0..400 {
        if kind(&w) == ARROWS_2 {
            break;
        }
        w.tick(&[]);
    }
    assert_eq!(kind(&w), ARROWS_2, "the upgrade finishes");
    assert_eq!(first_volley(&mut w, t, 3 * TICKS_PER_SECOND), vec![r]);
}
