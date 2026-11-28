//! Charges by weight and speed in real use (docs/05-combat.md, "Charges
//! by weight", "Real use"), on the real game data: Mounted Samurai riding
//! into the front of braced Bone Pikes and of unbraced ones, into Katana
//! Samurai from the front, the flank and the rear, into Bone Pikes locked
//! with Katana from the side, up a hill and through woods, with the
//! casualties on both sides printed; and two Generals bracing their spears
//! and pulling their horse out to charge again in a Conquest match.

use gc_fixed::{Angle, FxVec2};
use gc_sim::{Command, TICKS_PER_SECOND};

use super::arena::{Arena, Real, SOUTH, WEST, centre, hp, order, spawn};
use super::{Report, Scenario};
use crate::conquest;
use crate::headless::Headless;
use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::Victories;
use gc_sim::scenario::NORTH;
use gc_ui::lobby::Tempo;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "charges",
    about: "horse into braced and unbraced pikes, front, flank and rear, uphill, through woods, and Generals bracing and charging again",
    // Up to four Conquest matches of up to 40 minutes, most of it: three
    // full ones since heroes (docs/43) take about 21 s.
    budget_ms: 30_000,
    run,
};

const TILES: u32 = 64;
const HORSE: &str = "Mounted Samurai";
const PIKES: &str = "Bone Pikes";
const KATANA: &str = "Katana Samurai";

/// What one charge did, over the 4 s after the horse met its target.
#[derive(Clone, Copy)]
struct Hit {
    /// The charge bonus it hit home with, in percent of the unit's own.
    bonus: u8,
    /// Hit points the target lost, and the horse lost.
    dealt: u32,
    taken: u32,
    /// The horse's disorder when it met them, in ticks.
    disorder: u8,
    /// How long the charge drove the target back, in ticks.
    ploughed: u8,
}

fn secs(ticks: u8) -> f32 {
    f32::from(ticks) / TICKS_PER_SECOND as f32
}

impl Hit {
    fn line(&self) -> String {
        format!(
            "{} dealt / {} taken, bonus {}, disorder {:.1} s, ploughed {:.2} s",
            self.dealt,
            self.taken,
            self.bonus,
            secs(self.disorder),
            secs(self.ploughed),
        )
    }
}

/// Where the target stands and which way it faces, and what lies on the
/// way in.
struct Field {
    /// The target and its facing.
    target: (&'static str, Angle),
    /// Whether the target braces before the horse sets off.
    braced: bool,
    /// Where the horse starts and which way it faces: 30 tiles north of
    /// the target, facing south, unless given.
    from: Option<(FxVec2, Angle)>,
    /// Ground: a hill under the target, woods on the way.
    hill: bool,
    woods: bool,
    /// Katana Samurai locked with the target from the north first.
    locked: bool,
}

const FRONT: Field = Field {
    target: (KATANA, NORTH),
    braced: false,
    from: None,
    hill: false,
    woods: false,
    locked: false,
};

/// The target at (32, 40), the horse from `field.from`.
fn charge(real: &Real, report: &mut Report, field: Field) -> Hit {
    let mut arena = Arena::new(TILES);
    if field.hill {
        // A summit four tiles up, its sides falling over eight: climbable
        // going, half a tile a tile, the horse still on the slope when it
        // meets the target.
        arena.hill((32, 40), 1, 4, 8);
    }
    if field.woods {
        arena.woods(20..44, 24..32);
    }
    let mut world = arena.world(real, 21, 0);
    let target = spawn(
        &mut world,
        1,
        &[(real.unit(field.target.0), centre(32, 40))],
        field.target.1,
    )[0];
    let from = field.from.unwrap_or((centre(32, 10), SOUTH));
    let horse = spawn(&mut world, 0, &[(real.unit(HORSE), from.0)], from.1)[0];
    let mut orders = Vec::new();
    if field.braced {
        orders.push(order(
            1,
            Command::Brace {
                regiment: target,
                on: true,
            },
        ));
    }
    if field.locked {
        let katana = spawn(&mut world, 0, &[(real.unit(KATANA), centre(32, 37))], SOUTH)[0];
        orders.push(order(
            0,
            Command::Attack {
                regiment: katana,
                target,
            },
        ));
        report.tick(&mut world, &orders);
        orders.clear();
        for _ in 0..3 * TICKS_PER_SECOND {
            report.tick(&mut world, &[]);
        }
    }
    orders.push(order(
        0,
        Command::Attack {
            regiment: horse,
            target,
        },
    ));
    report.tick(&mut world, &orders);
    let (mut theirs, mut ours) = (hp(&world, target), hp(&world, horse));
    let mut hit = None;
    for _ in 0..25 * TICKS_PER_SECOND {
        report.tick(&mut world, &[]);
        if let Some(v) = world.regiments.get(horse).filter(|v| v.engaged) {
            hit = Some((v.charge_bonus, v.disorder));
            break;
        }
        (theirs, ours) = (hp(&world, target), hp(&world, horse));
    }
    let (bonus, disorder) = hit.unwrap_or_default();
    let braced_bonus = world.regiments.get(target).map_or(0, |v| v.charge_bonus);
    let mut ploughed = 0;
    for _ in 0..4 * TICKS_PER_SECOND {
        ploughed = (world.regiments.get(target)).map_or(ploughed, |v| v.ploughed.max(ploughed));
        report.tick(&mut world, &[]);
    }
    Hit {
        bonus: if field.braced { braced_bonus } else { bonus },
        dealt: theirs - hp(&world, target),
        taken: ours - hp(&world, horse),
        disorder,
        ploughed,
    }
}

fn run(real: &Real, report: &mut Report) {
    let open = charge(
        real,
        report,
        Field {
            target: (PIKES, NORTH),
            ..FRONT
        },
    );
    let braced = charge(
        real,
        report,
        Field {
            target: (PIKES, NORTH),
            braced: true,
            ..FRONT
        },
    );
    report.metric(
        format!("{HORSE} into {PIKES}' front, unbraced"),
        open.line(),
    );
    report.metric("braced (bonus is the pikes')", braced.line());
    report.check(
        "braced points take the charge: the horse loses more, deals less and is disordered",
        braced.taken > open.taken && braced.dealt < open.dealt && braced.disorder > 0,
    );

    let front = charge(real, report, FRONT);
    let flank = charge(
        real,
        report,
        Field {
            from: Some((centre(62, 40), WEST)),
            ..FRONT
        },
    );
    let rear = charge(
        real,
        report,
        Field {
            from: Some((centre(32, 62), NORTH)),
            ..FRONT
        },
    );
    report.metric(format!("{HORSE} into {KATANA}, front"), front.line());
    report.metric("flank", flank.line());
    report.metric("rear", rear.line());
    report.check(
        "a charge into the flank or rear ploughs deeper than into the front",
        flank.ploughed > front.ploughed && rear.ploughed > front.ploughed,
    );

    let locked = charge(
        real,
        report,
        Field {
            target: (PIKES, NORTH),
            from: Some((centre(62, 40), WEST)),
            locked: true,
            ..FRONT
        },
    );
    report.metric(
        format!("{HORSE} into the flank of {PIKES} locked with {KATANA}"),
        locked.line(),
    );
    report.check(
        "pikes locked in a fight and hit in the flank are ploughed, and cost the horse less than braced ones",
        locked.ploughed > 0 && braced.ploughed == 0 && locked.taken < braced.taken,
    );

    let uphill = charge(
        real,
        report,
        Field {
            hill: true,
            ..FRONT
        },
    );
    let woods = charge(
        real,
        report,
        Field {
            woods: true,
            ..FRONT
        },
    );
    report.metric("uphill into Katana on a hill", uphill.line());
    report.metric("through woods first", woods.line());
    report.check(
        "a charge uphill or out of woods hits home with less than one on the flat",
        uphill.bonus < front.bonus && woods.bonus < front.bonus,
    );

    // A match goes its own way with any change to how battles play out, so
    // a few are played, until both have been seen.
    let mut seen = [0u32; 2];
    for seed in [117, 118, 119, 120, 121] {
        let more = ai_match(real, report, seed);
        seen = [seen[0] + more[0], seen[1] + more[1]];
        if seen.iter().all(|&n| n > 0) {
            break;
        }
    }
    report.metric("Generals' brace orders in a Conquest match", seen[0]);
    report.metric("pull-outs to charge again", seen[1]);
    report.check(
        "two Generals in Conquest brace spears against horse and pull horse out to charge again",
        seen.iter().all(|&n| n > 0),
    );
}

/// Two Generals, Ember Clans (Mounted Samurai, Yari spears) against
/// Carrion Concord (Bone Pikes, Debt Collectors), play Conquest match
/// `seed` on Four Teeth for up to 40 minutes: how many brace orders and
/// pull-outs (fall backs) they give, stopping once both have been given.
fn ai_match(real: &Real, report: &mut Report, seed: u64) -> [u32; 2] {
    let factions = [Faction::EmberClans, Faction::CarrionConcord];
    let game = Headless::new(real, conquest::make_map(1, 0, seed), &factions, seed);
    let mut world = game.world(&game.plan(Tempo::Standard, Victories::default()));
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    for p in 0..factions.len() {
        minds.add(game.mind(p, Difficulty::General, 7, None));
    }
    let mut seen = [0u32; 2];
    let mut cmds = Vec::new();
    while world.verdict().is_none() && world.tick_count() < 40 * 60 * TICKS_PER_SECOND {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        for c in &cmds {
            match c.command {
                Command::Brace { on: true, .. } => seen[0] += 1,
                Command::FallBack { .. } => seen[1] += 1,
                _ => {}
            }
        }
        report.tick(&mut world, &cmds);
        if seen.iter().all(|&n| n > 0) {
            break;
        }
    }
    report.metric(
        format!("Conquest match {seed} played"),
        super::clock(Some(world.tick_count())),
    );
    seen
}
