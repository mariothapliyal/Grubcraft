//! Hold, Skirmish, Run and the Column and Wedge formations
//! (docs/05-combat.md, "Hold, Skirmish and Run").

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    COLUMN_FILES, Capacities, Command, Formation, Mass, MatchSetup, PlayerCommand, RegimentHandle,
    RegimentView, UnitStats, World,
};

const SOUTH: Angle = Angle::QUARTER;
const EAST: Angle = Angle::ZERO;
/// Drill units: 0 infantry (24 in 8 files, 3 tiles a second), 2 archers
/// (range 25); here 1 is the drill's heavy infantry (2.5 tiles a second).
const FOOT: u16 = 0;
const ARCHERS: u16 = 2;
const HEAVY: u16 = 1;
/// Unit 3 here: riders that can form a wedge.
const RIDERS: u16 = 3;

/// The drill's units with morale off (nobody routs), unit 3 made into
/// charging cavalry.
fn units() -> [UnitStats; 4] {
    let mut units = DRILL_UNITS.map(|u| UnitStats { morale: 0, ..u });
    units[HEAVY as usize] = units[3];
    units[RIDERS as usize] = UnitStats {
        speed: Fx::from_int(6),
        models: 9,
        files: 9,
        charge_bonus: 60,
        mass: Mass::Heavy,
        range: Fx::ZERO,
        ..DRILL_UNITS[1]
    };
    units
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities::default(),
        units,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    })
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32, facing: Angle) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing,
        },
    ));
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
        .unwrap()
}

fn march(w: &mut World, player: u8, regiment: RegimentHandle, x: i32, y: i32) -> bool {
    order(
        w,
        player,
        Command::MoveTo {
            regiment,
            target: FxVec2::from_ints(x, y),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            brigade: 0,
        },
    )
}

fn attack(w: &mut World, player: u8, regiment: RegimentHandle, target: RegimentHandle) -> bool {
    order(w, player, Command::Attack { regiment, target })
}

fn view(w: &World, h: RegimentHandle) -> RegimentView {
    w.regiments.get(h).unwrap()
}

fn hp(w: &World, h: RegimentHandle) -> u32 {
    w.regiments
        .model_hp(h)
        .map_or(0, |hp| hp.iter().copied().map(u32::from).sum())
}

fn run(w: &mut World, ticks: u32) {
    for _ in 0..ticks {
        w.tick(&[]);
    }
}

fn tiles(a: FxVec2, b: FxVec2) -> f32 {
    (a - b).length().raw() as f32 / Fx::ONE.raw() as f32
}

/// How far a regiment marching east from (40, 100) gets in `ticks`, with
/// `before` given first.
fn marched(units: &[UnitStats], unit: u16, before: &[Command], ticks: u32) -> f32 {
    let mut w = world(units);
    let r = spawn(&mut w, 0, unit, 40, 100, EAST);
    for c in before {
        let mut c = *c;
        if let Command::SetFormation { regiment, .. } = &mut c {
            *regiment = r;
        }
        assert!(order(&mut w, 0, c), "{c:?} refused");
    }
    assert!(march(&mut w, 0, r, 200, 100));
    let start = view(&w, r).position;
    run(&mut w, ticks);
    tiles(view(&w, r).position, start)
}

#[test]
fn hold_halts_a_regiment_and_any_order_lifts_it() {
    let units = units();
    let mut w = world(&units);
    let r = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    assert!(march(&mut w, 0, r, 100, 60));
    run(&mut w, 20);
    assert!(order(
        &mut w,
        0,
        Command::Hold {
            regiment: r,
            on: true
        }
    ));
    let at = view(&w, r).position;
    run(&mut w, 60);
    assert!(view(&w, r).hold);
    assert!(
        tiles(view(&w, r).position, at) < 0.5,
        "a held regiment stands"
    );
    assert_eq!(
        w.regiments.feedback(r).unwrap().doing,
        gc_sim::Doing::StandingFast
    );
    // Not for someone else's regiment.
    assert!(!order(
        &mut w,
        1,
        Command::Hold {
            regiment: r,
            on: false
        }
    ));
    assert!(march(&mut w, 0, r, 100, 80));
    assert!(!view(&w, r).hold, "a move lifts it");
    assert!(order(
        &mut w,
        0,
        Command::Hold {
            regiment: r,
            on: true
        }
    ));
    assert!(order(
        &mut w,
        0,
        Command::Hold {
            regiment: r,
            on: false
        }
    ));
    assert!(!order(
        &mut w,
        0,
        Command::Hold {
            regiment: r,
            on: false
        }
    ));
}

/// A light, weak line held by a heavy one: how far it is driven back in
/// 20 seconds, held or not.
fn driven_back(held: bool) -> f32 {
    let mut units = units();
    units[FOOT as usize].mass = Mass::Light;
    units[FOOT as usize].melee_attack = 10;
    units[FOOT as usize].hp_per_model = 60;
    let heavy = UnitStats {
        mass: Mass::Heavy,
        melee_attack: 60,
        hp_per_model: 60,
        files: 8,
        models: 24,
        ..DRILL_UNITS[3]
    };
    units[1] = UnitStats { morale: 0, ..heavy };
    let mut w = world(&units);
    let line = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    let foe = spawn(&mut w, 1, 1, 100, 92, SOUTH);
    if held {
        assert!(order(
            &mut w,
            0,
            Command::Hold {
                regiment: line,
                on: true
            }
        ));
    }
    assert!(attack(&mut w, 1, foe, line));
    let at = view(&w, line).position;
    run(&mut w, 20 * 20);
    assert!(w.regiments.get(line).is_some());
    tiles(view(&w, line).position, at)
}

#[test]
fn a_held_line_gives_ground_grudgingly() {
    let (free, held) = (driven_back(false), driven_back(true));
    assert!(free > 1.0, "the weak line is driven back ({free} tiles)");
    assert!(held < free * 0.75, "held {held} tiles, free {free}");
}

/// Archers (3.25 tiles a second) at (100, 100) facing north, infantry of
/// type `unit` 20 tiles off coming at them; returns how long until they
/// are caught (None: not in 20 s), how far the archers got and the hit
/// points the infantry lost.
fn archers_charged(unit: u16, skirmish: bool) -> (Option<u32>, f32, u32) {
    let units = units();
    let mut w = world(&units);
    let archers = spawn(&mut w, 0, ARCHERS, 100, 100, NORTH);
    let foot = spawn(&mut w, 1, unit, 100, 80, SOUTH);
    if skirmish {
        assert!(order(
            &mut w,
            0,
            Command::Skirmish {
                regiment: archers,
                on: true
            }
        ));
        assert!(view(&w, archers).skirmish);
    }
    let full = hp(&w, foot);
    assert!(attack(&mut w, 1, foot, archers));
    let mut caught = None;
    for t in 0..20 * 20 {
        w.tick(&[]);
        if caught.is_none() && view(&w, archers).engaged {
            caught = Some(t);
        }
    }
    let moved = tiles(view(&w, archers).position, FxVec2::from_ints(100, 100));
    (caught, moved, full - hp(&w, foot))
}

#[test]
fn skirmishers_step_back_from_melee_and_shoot_again() {
    // Heavy infantry, 2.5 tiles a second: skirmishers keep away all the
    // time, shooting between steps.
    let (caught, _, _) = archers_charged(HEAVY, false);
    assert!(caught.is_some(), "standing archers are caught");
    let (caught, moved, shot) = archers_charged(HEAVY, true);
    assert_eq!(caught, None, "skirmishers stay out of reach");
    assert!(moved > 10.0, "they stepped back ({moved} tiles)");
    assert!(shot > 0, "and shot between steps");
    // Infantry nearly as fast (3 tiles a second) catch them in the end,
    // but later.
    let (standing, _, _) = archers_charged(FOOT, false);
    let (skirmishing, _, _) = archers_charged(FOOT, true);
    let late = skirmishing.unwrap_or(u32::MAX);
    assert!(
        late > standing.unwrap() + 5 * 20,
        "caught at {late}, standing at {standing:?}"
    );
}

#[test]
fn only_regiments_that_shoot_skirmish() {
    let units = units();
    let mut w = world(&units);
    let foot = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    assert!(!order(
        &mut w,
        0,
        Command::Skirmish {
            regiment: foot,
            on: true
        }
    ));
}

#[test]
fn a_held_skirmisher_stands() {
    let units = units();
    let mut w = world(&units);
    let archers = spawn(&mut w, 0, ARCHERS, 100, 100, NORTH);
    let foot = spawn(&mut w, 1, FOOT, 100, 84, SOUTH);
    assert!(order(
        &mut w,
        0,
        Command::Skirmish {
            regiment: archers,
            on: true
        }
    ));
    assert!(order(
        &mut w,
        0,
        Command::Hold {
            regiment: archers,
            on: true
        }
    ));
    assert!(attack(&mut w, 1, foot, archers));
    run(&mut w, 10 * 20);
    assert!(view(&w, archers).engaged, "held, it stands and fights");
}

#[test]
fn a_run_goes_half_again_as_fast_then_winds_the_regiment() {
    let units = units();
    let walked = marched(&units, FOOT, &[], 5 * 20);
    let mut w = world(&units);
    let r = spawn(&mut w, 0, FOOT, 40, 100, EAST);
    // Nowhere to go: no running.
    assert!(!order(&mut w, 0, Command::Run { regiment: r }));
    assert!(march(&mut w, 0, r, 200, 100));
    assert!(order(&mut w, 0, Command::Run { regiment: r }));
    assert!(
        !order(&mut w, 0, Command::Run { regiment: r }),
        "running already"
    );
    let start = view(&w, r).position;
    run(&mut w, 5 * 20);
    let ran = tiles(view(&w, r).position, start);
    assert!(view(&w, r).running);
    assert!(
        ran > walked * 1.35 && ran < walked * 1.6,
        "ran {ran}, walked {walked}"
    );
    run(&mut w, 4 * 20);
    let v = view(&w, r);
    assert!(!v.running && v.winded, "out of breath after 8 s");
    assert!(!order(&mut w, 0, Command::Run { regiment: r }), "winded");
    let at = v.position;
    run(&mut w, 5 * 20);
    let tired = tiles(view(&w, r).position, at);
    assert!(tired < walked * 0.8, "winded {tired}, walked {walked}");
    run(&mut w, 6 * 20);
    assert!(!view(&w, r).winded, "it gets its breath back");
}

#[test]
fn a_run_ends_on_arrival() {
    let units = units();
    let mut w = world(&units);
    let r = spawn(&mut w, 0, FOOT, 100, 100, EAST);
    assert!(march(&mut w, 0, r, 106, 100));
    assert!(order(&mut w, 0, Command::Run { regiment: r }));
    run(&mut w, 4 * 20);
    let v = view(&w, r);
    assert!(!v.running && v.winded);
}

#[test]
fn a_column_is_narrow_and_marches_faster() {
    let units = units();
    let mut w = world(&units);
    let r = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    let set = |w: &mut World, f| {
        order(
            w,
            0,
            Command::SetFormation {
                regiment: r,
                formation: f,
            },
        )
    };
    assert!(set(&mut w, Formation::Column));
    assert_eq!(view(&w, r).files, COLUMN_FILES);
    // A wider frontage asked for stays a column's.
    assert!(order(
        &mut w,
        0,
        Command::MoveTo {
            regiment: r,
            target: FxVec2::from_ints(100, 90),
            facing: None,
            files: 12,
            pace: Fx::ZERO,
            brigade: 0,
        }
    ));
    assert_eq!(view(&w, r).frontage, COLUMN_FILES);
    // Out of column it stands as wide as it was last asked to.
    assert!(set(&mut w, Formation::Line));
    assert_eq!(view(&w, r).frontage, 12);
    let column = Command::SetFormation {
        regiment: r,
        formation: Formation::Column,
    };
    let (line, col) = (
        marched(&units, FOOT, &[], 4 * 20),
        marched(&units, FOOT, &[column], 4 * 20),
    );
    assert!(col > line * 1.15, "column {col}, line {line}");
    assert!(!Formation::Column.can_charge());
    // A column is at its quick march already: it doesn't run as well.
    assert!(set(&mut w, Formation::Column));
    assert!(view(&w, r).moving);
    assert!(!order(&mut w, 0, Command::Run { regiment: r }));
}

#[test]
fn only_charging_cavalry_forms_a_wedge() {
    let units = units();
    let mut w = world(&units);
    let foot = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    let riders = spawn(&mut w, 0, RIDERS, 120, 100, NORTH);
    let wedge = |regiment| Command::SetFormation {
        regiment,
        formation: Formation::Wedge,
    };
    assert!(!order(&mut w, 0, wedge(foot)));
    assert!(order(&mut w, 0, wedge(riders)));
    assert_eq!(view(&w, riders).files, 3, "nine riders, three deep");
    assert!(order(
        &mut w,
        0,
        Command::SetFormation {
            regiment: riders,
            formation: Formation::Line,
        }
    ));
    assert_eq!(view(&w, riders).files, 9);
}

/// From a column straight into a wedge and out again, riders form the
/// line they stood in before the column, not one as narrow as it.
#[test]
fn a_column_into_a_wedge_still_widens_to_its_old_line() {
    let units = units();
    let mut w = world(&units);
    let riders = spawn(&mut w, 0, RIDERS, 120, 100, NORTH);
    let line = view(&w, riders).frontage;
    let set = |w: &mut World, formation| {
        order(
            w,
            0,
            Command::SetFormation {
                regiment: riders,
                formation,
            },
        )
    };
    assert!(set(&mut w, Formation::Column));
    let column = view(&w, riders).frontage;
    assert!(column < line, "column {column}, line {line}");
    assert!(set(&mut w, Formation::Wedge));
    assert!(set(&mut w, Formation::Line));
    assert_eq!(view(&w, riders).frontage, line);
}

/// Riders charging a line of infantry from 30 tiles: hit points the
/// infantry lose in the 3 seconds after they meet.
fn charge(wedge: bool) -> u32 {
    let units = units();
    let mut w = world(&units);
    let foot = spawn(&mut w, 0, FOOT, 100, 100, NORTH);
    let riders = spawn(&mut w, 1, RIDERS, 100, 70, SOUTH);
    if wedge {
        assert!(order(
            &mut w,
            1,
            Command::SetFormation {
                regiment: riders,
                formation: Formation::Wedge,
            }
        ));
    }
    assert!(attack(&mut w, 1, riders, foot));
    for _ in 0..20 * 20 {
        w.tick(&[]);
        if view(&w, riders).engaged {
            break;
        }
    }
    assert!(view(&w, riders).engaged);
    let before = hp(&w, foot);
    run(&mut w, 3 * 20);
    before - hp(&w, foot)
}

#[test]
fn a_wedge_charges_home_harder() {
    let (line, wedge) = (charge(false), charge(true));
    assert!(line > 0);
    assert!(wedge > line, "wedge {wedge}, line {line}");
}
