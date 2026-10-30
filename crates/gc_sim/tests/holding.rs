//! Holding until bloodied (docs/29, docs/05 "Morale"): regiments rout only
//! once they are down to their breaking point, below half strength, however
//! hard they are shocked. Each case prints the strength at which the
//! regiment wavered and broke; `cargo test -p gc_sim --test holding --
//! --nocapture` shows them.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::morale::{BREAK_CAP, BREAK_SPREAD, ELITE_BREAK, LEVY_BREAK, LINE_BREAK};
use gc_sim::scenario::{DRILL_UNITS, NORTH};
use gc_sim::{
    Capacities, Command, Ground, Mass, MatchSetup, MoraleState, PlayerCommand, RegimentHandle,
    Sky, UnitStats, World,
};

const SOUTH: Angle = Angle::QUARTER;
const WEST: Angle = Angle::HALF;
const YARI: u16 = 0;
const SAMURAI: u16 = 1;
const KATANA: u16 = 2;
const LEVY: u16 = 3;

/// Ember Clans stats from `data/rosters/ember_clans.ron` (turn rates
/// approximate), and a levy of the same men with morale 50.
fn units() -> [UnitStats; 4] {
    let yari = UnitStats {
        speed: Fx::from_int(3),
        models: 28,
        hp_per_model: 8,
        files: 8,
        melee_attack: 25,
        melee_defence: 45,
        melee_damage: 2,
        charge_bonus: 20,
        morale: 60,
        reach: true,
        ..DRILL_UNITS[0]
    };
    let samurai = UnitStats {
        speed: Fx::from_ratio(13, 2),
        turn_rate: DRILL_UNITS[1].turn_rate * 2,
        models: 8,
        hp_per_model: 30,
        files: 4,
        file_spacing: Fx::from_ratio(7, 5),
        rank_spacing: Fx::from_int(2),
        melee_attack: 45,
        melee_defence: 30,
        melee_damage: 6,
        charge_bonus: 80,
        morale: 70,
        mass: Mass::Heavy,
        ..DRILL_UNITS[0]
    };
    let katana = UnitStats {
        models: 16,
        hp_per_model: 14,
        melee_attack: 40,
        melee_defence: 40,
        melee_damage: 4,
        charge_bonus: 30,
        morale: 80,
        ..DRILL_UNITS[0]
    };
    let levy = UnitStats { morale: 50, ..yari };
    [yari, samurai, katana, levy]
}

fn world(units: &[UnitStats]) -> World {
    World::new(&MatchSetup {
        seed: 7,
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

/// Hit points left, in percent of full.
fn strength(w: &World, units: &[UnitStats], h: RegimentHandle) -> u32 {
    let v = w.regiments.get(h).unwrap();
    let full = u32::from(units[v.unit as usize].models) * u32::from(units[v.unit as usize].hp_per_model);
    let hp: u32 = w.regiments.model_hp(h).unwrap().iter().map(|&x| u32::from(x)).sum();
    hp * 100 / full
}

struct Outcome {
    break_at: u8,
    wavered: Option<u32>,
    routed: Option<u32>,
}

/// The `defender` stands facing north at (100, 100) and every attacker
/// goes for it; plays up to four minutes and reports the strengths at which
/// it wavered and routed.
fn fight(name: &str, defender: u16, attackers: &[(u16, (i32, i32), Angle)]) -> Outcome {
    let units = units();
    let mut w = world(&units);
    let d = spawn(&mut w, 0, defender, 100, 100, NORTH);
    for &(unit, (x, y), facing) in attackers {
        let a = spawn(&mut w, 1, unit, x, y, facing);
        w.tick(&[PlayerCommand {
            player: 1,
            command: Command::Attack {
                regiment: a,
                target: d,
            },
        }]);
    }
    let break_at = w.regiments.get(d).unwrap().break_at;
    let mut out = Outcome {
        break_at,
        wavered: None,
        routed: None,
    };
    for _ in 0..20 * 240 {
        w.tick(&[]);
        let Some(v) = w.regiments.get(d) else { break };
        let now = strength(&w, &units, d);
        if out.wavered.is_none() && v.morale == MoraleState::Wavering {
            out.wavered = Some(now);
        }
        if v.morale.broken() {
            out.routed = Some(now);
            break;
        }
    }
    println!(
        "{name}: breaking point {}%, wavered at {:?}%, routed at {:?}%",
        out.break_at, out.wavered, out.routed
    );
    out
}

fn holds(o: &Outcome) {
    if let Some(r) = o.routed {
        assert!(r <= u32::from(BREAK_CAP), "routed at {r}%, above half strength");
        assert!(r <= u32::from(o.break_at), "routed at {r}%, above its breaking point {}%", o.break_at);
    }
    if let Some(wv) = o.wavered {
        assert!(wv <= 60, "wavered at {wv}%");
    }
}

#[test]
fn heavy_shock_breaks_nobody_above_their_breaking_point() {
    let cases = [
        fight("Yari, cavalry charge in front", YARI, &[(SAMURAI, (100, 50), SOUTH)]),
        fight("Yari, cavalry charge in the flank", YARI, &[(SAMURAI, (150, 100), WEST)]),
        fight("Yari, cavalry charge in the rear", YARI, &[(SAMURAI, (100, 150), NORTH)]),
        fight("Yari against Yari", YARI, &[(YARI, (100, 80), SOUTH)]),
        fight("Yari against Katana", YARI, &[(KATANA, (100, 80), SOUTH)]),
        fight("Yari against two Yari", YARI, &[(YARI, (96, 80), SOUTH), (YARI, (104, 80), SOUTH)]),
        fight(
            "Yari held in front, cavalry in the rear",
            YARI,
            &[(YARI, (100, 80), SOUTH), (SAMURAI, (100, 160), NORTH)],
        ),
        fight(
            "Katana held in front, cavalry in the rear",
            KATANA,
            &[(YARI, (100, 80), SOUTH), (SAMURAI, (100, 160), NORTH)],
        ),
        fight(
            "Levy held in front, cavalry in the rear",
            LEVY,
            &[(YARI, (100, 80), SOUTH), (SAMURAI, (100, 160), NORTH)],
        ),
    ];
    for o in &cases {
        holds(o);
    }
    // Hammer and anvil still breaks them: shock works by killing men.
    assert!(cases[6].routed.is_some(), "spears caught front and rear still break");
}

#[test]
fn breaking_points_follow_the_grade_and_vary_a_little() {
    let units = units();
    let mut w = world(&units);
    let mut seen = std::collections::BTreeSet::new();
    for (unit, base) in [(LEVY, LEVY_BREAK), (YARI, LINE_BREAK), (KATANA, ELITE_BREAK)] {
        for k in 0..12 {
            let h = spawn(&mut w, 0, unit, 20 + 12 * k, 20 + 20 * i32::from(unit), NORTH);
            let at = w.regiments.get(h).unwrap().break_at;
            assert!(
                (base - BREAK_SPREAD..=base + BREAK_SPREAD).contains(&at),
                "unit {unit}: breaking point {at}%"
            );
            seen.insert(at);
        }
    }
    assert!(seen.len() > 6, "breaking points vary: {seen:?}");
}
