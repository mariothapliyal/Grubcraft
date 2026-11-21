//! Hold, Skirmish, Run, Column and Wedge in real use (docs/05-combat.md,
//! "Hold, Skirmish and Run", "Real use"), on the real game data: the gate
//! army marching through its Gate in column against the same army in
//! line, Yumi archers skirmishing away from Bone Pikes and Gravewardens,
//! Yari running, a held Yari line driven back by Oni-Bound, Mounted
//! Samurai charging in a wedge, and two Generals giving every one of these
//! orders in a Conquest match (rule (c)).

use gc_sim::{
    Command, Formation, MoraleState, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World,
};

use super::arena::{
    Arena, Real, SOUTH, WEST, centre, gated_wall, grid, hp, move_to, of, order, soldiers, spawn,
};
use super::{Report, Scenario, seconds};
use crate::conquest;
use crate::headless::Headless;
use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::Victories;
use gc_sim::scenario::NORTH;
use gc_ui::lobby::Tempo;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "new-orders",
    about: "column through a Gate, skirmishers, a run, a held line, a wedge charge and Generals giving them all",
    budget_ms: 12_000,
    run,
};

const TILES: u32 = 64;
const WALL_ROW: i32 = 30;
const GATE_X: i32 = 30;
/// Who drives which line back.
const DRIVER: &str = "Oni-Bound";
const LINE: &str = "Yari Ashigaru";

fn formation(regiment: RegimentHandle, formation: Formation) -> PlayerCommand {
    order(
        0,
        Command::SetFormation {
            regiment,
            formation,
        },
    )
}

fn tiles(a: gc_fixed::FxVec2, b: gc_fixed::FxVec2) -> f32 {
    (a - b).length().raw() as f32 / gc_fixed::Fx::ONE.raw() as f32
}

fn run(real: &Real, report: &mut Report) {
    // Through the Gate: the gate scenario's 200 soldiers, in line and in
    // column.
    let line = through_gate(real, report, Formation::Line);
    let column = through_gate(real, report, Formation::Column);
    report.metric("200 soldiers through a Gate, in line", seconds(line));
    report.metric("in column", seconds(column));
    report.check(
        "the army in column is through and standing no later than in line",
        column.is_some() && line.is_none_or(|l| column.unwrap() <= l),
    );

    for (foe, name) in [
        ("Bone Pikes", "Bone Pikes, 2.75 tiles a second"),
        ("Gravewardens", "Gravewardens, 2.5"),
    ] {
        let standing = skirmish(real, report, foe, false);
        let skirmishing = skirmish(real, report, foe, true);
        report.metric(
            format!("Yumi caught by {name}, standing"),
            seconds(standing.caught),
        );
        report.metric("skirmishing", seconds(skirmishing.caught));
        report.metric(
            "hit points shot off in 40 s / lost in 20 s, standing",
            format!("{} / {}", standing.shot, standing.lost),
        );
        report.metric(
            "skirmishing",
            format!("{} / {}", skirmishing.shot, skirmishing.lost),
        );
        report.check(
            format!("skirmishing Yumi are caught by {foe} at least 4 s later and lose less"),
            standing.caught.is_some_and(|s| {
                skirmishing
                    .caught
                    .is_none_or(|k| k >= s + 4 * TICKS_PER_SECOND)
            }) && skirmishing.lost < standing.lost,
        );
    }

    let (walked, ran) = (sprint(real, report, false), sprint(real, report, true));
    report.metric("Yari over 6 s, walking (tiles)", format!("{walked:.1}"));
    report.metric("running", format!("{ran:.1}"));
    report.check("a run goes at least a third further", ran > walked * 1.33);

    let (free, held) = (driven(real, report, false), driven(real, report, true));
    report.metric(
        format!("{LINE} driven back by {DRIVER} before they break, free (tiles)"),
        format!("{free:.1}"),
    );
    report.metric("held", format!("{held:.1}"));
    report.check("a held line gives less ground than a free one", held < free);

    // A match goes its own way with any change to how battles play out, so
    // a few are played, until every kind of order has been given.
    let mut given = [0u32; 5];
    let all = |g: &[u32; 5]| g[..3].iter().all(|&n| n > 0) && g[3] + g[4] > 0;
    for seed in [104, 105, 106] {
        let more = ai_match(real, report, seed);
        for (g, m) in given.iter_mut().zip(more) {
            *g += m;
        }
        if all(&given) {
            break;
        }
    }
    let names = ["skirmish", "wedge", "column", "hold", "run"];
    for (n, g) in names.iter().zip(given) {
        report.metric(format!("Generals' {n} orders in a Conquest match"), g);
    }
    report.check(
        "two Generals in Conquest skirmish, wedge, march in column and hold or run",
        all(&given),
    );

    let (in_line, in_wedge) = (charge(real, report, false), charge(real, report, true));
    report.metric(
        "Katana hit points lost to a Mounted Samurai charge in 4 s, line",
        in_line,
    );
    report.metric("wedge", in_wedge);
    report.check("a wedge's charge hits harder", in_wedge > in_line);
}

/// The gate scenario's army through its Gate in `f`: when every regiment
/// is through and standing (within 90 s).
fn through_gate(real: &Real, report: &mut Report, f: Formation) -> Option<u32> {
    let mut world = Arena::new(TILES).world(real, 11, 0);
    gated_wall(real, &mut world, TILES, WALL_ROW, GATE_X);
    let [yari, katana, yumi] = real.ember_line();
    let horse = real.unit("Mounted Samurai");
    let army = [
        yari, yari, yari, yari, katana, katana, katana, yumi, yumi, horse,
    ];
    let at = |y: i32, i: usize| grid((14, y), 5, (9, 6), i);
    let placed: Vec<_> = (army.iter().enumerate())
        .map(|(i, &u)| (u, at(42, i)))
        .collect();
    let army = spawn(&mut world, 0, &placed, WEST);
    let start = soldiers(&world, &army);
    let mut orders: Vec<_> = army.iter().map(|&r| formation(r, f)).collect();
    orders.extend((army.iter().enumerate()).map(|(i, &r)| move_to(0, r, at(12, i))));
    report.tick(&mut world, &orders);
    let mut done = None;
    for t in 1..=90 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        let through = of(&world, &army)
            .filter(|v| v.position.y.floor_int() < WALL_ROW)
            .count();
        if through == army.len() && of(&world, &army).all(|v| !v.moving) {
            done = Some(t);
            break;
        }
    }
    report.check(
        format!("no soldier lost at the Gate in {}", f.name()),
        soldiers(&world, &army) == start,
    );
    done
}

/// How Yumi fared against a melee foe.
struct Skirmish {
    /// When they were caught (within 40 s).
    caught: Option<u32>,
    /// Hit points they shot off the foe in 40 s, and lost in the first 20.
    shot: u32,
    lost: u32,
}

/// Yumi at (64, 40) on a 128-tile field facing north, `foe` 20 tiles off
/// attacking them. (Room behind them to step back into: a block doesn't
/// step back where it doesn't fit, so not over the map's edge.)
fn skirmish(real: &Real, report: &mut Report, foe: &str, on: bool) -> Skirmish {
    let mut world = Arena::new(2 * TILES).world(real, 12, 0);
    let yumi = spawn(
        &mut world,
        0,
        &[(real.ember_line()[2], centre(64, 40))],
        NORTH,
    )[0];
    let pikes = spawn(&mut world, 1, &[(real.unit(foe), centre(64, 20))], SOUTH)[0];
    let (full, ours) = (hp(&world, pikes), hp(&world, yumi));
    let mut orders = vec![order(
        1,
        Command::Attack {
            regiment: pikes,
            target: yumi,
        },
    )];
    if on {
        orders.push(order(
            0,
            Command::Skirmish {
                regiment: yumi,
                on: true,
            },
        ));
    }
    report.tick(&mut world, &orders);
    let (mut caught, mut lost) = (None, 0);
    for t in 1..=40 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        if caught.is_none() && world.regiments.get(yumi).is_some_and(|v| v.engaged) {
            caught = Some(t);
        }
        if t == 20 * TICKS_PER_SECOND {
            lost = ours - hp(&world, yumi);
        }
    }
    Skirmish {
        caught,
        shot: full - hp(&world, pikes),
        lost,
    }
}

/// How far Yari get marching west for 6 s, running or not.
fn sprint(real: &Real, report: &mut Report, running: bool) -> f32 {
    let mut world = Arena::new(TILES).world(real, 13, 0);
    let yari = spawn(
        &mut world,
        0,
        &[(real.ember_line()[0], centre(56, 32))],
        WEST,
    )[0];
    let mut orders = vec![move_to(0, yari, centre(4, 32))];
    if running {
        orders.push(order(0, Command::Run { regiment: yari }));
    }
    let start = world.regiments.get(yari).unwrap().position;
    report.tick(&mut world, &orders);
    for _ in 0..6 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
    }
    tiles(world.regiments.get(yari).unwrap().position, start)
}

/// Yari attacked by Oni-Bound for up to 30 s, held or not: ground given
/// before they break (once they rout they run, held or not).
fn driven(real: &Real, report: &mut Report, held: bool) -> f32 {
    let mut world = Arena::new(TILES).world(real, 14, 0);
    let line = spawn(&mut world, 0, &[(real.unit(LINE), centre(32, 36))], NORTH)[0];
    let foe = spawn(&mut world, 1, &[(real.unit(DRIVER), centre(32, 26))], SOUTH)[0];
    let mut orders = vec![order(
        1,
        Command::Attack {
            regiment: foe,
            target: line,
        },
    )];
    if held {
        orders.push(order(
            0,
            Command::Hold {
                regiment: line,
                on: true,
            },
        ));
    }
    let start = world.regiments.get(line).unwrap().position;
    report.tick(&mut world, &orders);
    let mut given = 0.0;
    for _ in 0..30 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        match world.regiments.get(line) {
            Some(v) if v.morale != MoraleState::Routing => given = tiles(v.position, start),
            _ => break,
        }
    }
    given
}

/// Mounted Samurai charge Katana Samurai from 30 tiles, in line or in a
/// wedge: Katana felled in the 4 s after they meet.
fn charge(real: &Real, report: &mut Report, wedge: bool) -> u32 {
    let mut world = Arena::new(TILES).world(real, 15, 0);
    let katana = spawn(
        &mut world,
        1,
        &[(real.ember_line()[1], centre(32, 44))],
        NORTH,
    )[0];
    let horse = spawn(
        &mut world,
        0,
        &[(real.unit("Mounted Samurai"), centre(32, 12))],
        SOUTH,
    )[0];
    let mut orders = vec![order(
        0,
        Command::Attack {
            regiment: horse,
            target: katana,
        },
    )];
    if wedge {
        orders.insert(0, formation(horse, Formation::Wedge));
    }
    report.tick(&mut world, &orders);
    let met = |w: &World| w.regiments.get(horse).is_some_and(|v| v.engaged);
    for _ in 0..20 * TICKS_PER_SECOND {
        if met(&world) {
            break;
        }
        report.tick(&mut world, &[]);
    }
    let before = hp(&world, katana);
    for _ in 0..4 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
    }
    before - hp(&world, katana)
}

/// Two Generals, Ember Clans against Carrion Concord (both with bows and
/// horse), play Conquest match `seed` on Four Teeth for up to 40 minutes:
/// how many of each new order they give (skirmish, wedge, column, hold,
/// run), stopping once every kind has been given.
fn ai_match(real: &Real, report: &mut Report, seed: u64) -> [u32; 5] {
    let factions = [Faction::EmberClans, Faction::CarrionConcord];
    let game = Headless::new(real, conquest::make_map(1, 0, seed), &factions, seed);
    let mut world = game.world(&game.plan(Tempo::Standard, Victories::default()));
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    for p in 0..factions.len() {
        minds.add(game.mind(p, Difficulty::General, 7, None));
    }
    let mut given = [0u32; 5];
    let mut cmds = Vec::new();
    while world.verdict().is_none() && world.tick_count() < 40 * 60 * TICKS_PER_SECOND {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        for c in &cmds {
            let k = match c.command {
                Command::Skirmish { on: true, .. } => 0,
                Command::SetFormation {
                    formation: Formation::Wedge,
                    ..
                } => 1,
                Command::SetFormation {
                    formation: Formation::Column,
                    ..
                } => 2,
                Command::Hold { on: true, .. } => 3,
                Command::Run { .. } => 4,
                _ => continue,
            };
            given[k] += 1;
        }
        report.tick(&mut world, &cmds);
        if given[..3].iter().all(|&g| g > 0) && given[3] + given[4] > 0 {
            break;
        }
    }
    report.metric(
        format!("Conquest match {seed} played"),
        super::clock(Some(world.tick_count())),
    );
    given
}
