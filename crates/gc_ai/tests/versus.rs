//! The battle AI in the arena (docs/18-ai.md, "Testing, tuning and
//! explaining"): against a scripted "charge straight in" opponent, which
//! the General must beat nine battles in ten (AI-1), and against itself,
//! where each level must beat the one below it at least 70% of the time.
//!
//! Both sides field the same army, set out the same way, facing each other
//! across open ground. Which side plays which end of the field alternates,
//! and each battle starts from a different seed and spot.
//!
//! Tuning tools (ignored by default, run with `--release`):
//! - `AI_BATTLES=120 cargo test -p gc_ai --release --test versus sweep -- --ignored --nocapture`
//! - `AI_TRACE=<seed> cargo test -p gc_ai --release --test versus one -- --ignored --nocapture`
//!   prints the field every two seconds of one battle.
//! - `knob_study` and `yardsticks`: see below.

use gc_ai::army::{Placement, deploy, spawn_commands};
use gc_ai::scripted::ChargeBot;
use gc_ai::{
    Difficulty, Doctrine, Knobs, Mind, MindConfig, Minds, Perception, Plan, Stance, Terrain,
    holding_the_field,
};
use gc_fixed::Pcg32;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, PlayerId, UnitId, World};
use glam::Vec2;

const MAP: u32 = 256;
/// Spears, pikes, fast troops and archers (DRILL_UNITS 0, 3, 1, 2).
const ARMY: [UnitId; 10] = [0, 0, 0, 0, 3, 3, 1, 1, 2, 2];
/// A battle is over once one side has held the field this long, ticks.
const HELD: u32 = 10 * 20;
/// Or after eight minutes, when whoever has more soldiers still fighting wins.
const LIMIT: u32 = 8 * 60 * 20;

/// Who plays a side.
#[derive(Clone, Copy, Debug)]
enum Side {
    Mind(Difficulty),
    /// A difficulty with some knobs changed, for tuning.
    Tuned(Difficulty, fn(&mut Knobs)),
    /// A difficulty told to fight with one plan throughout (no rehearsals).
    Planned(Difficulty, Plan),
    /// Charge straight in.
    Charge,
    /// Stands still; only the simulation's own reflexes fight back.
    Idle,
}

#[derive(Debug)]
struct Outcome {
    /// Whether the side under test won.
    won: bool,
    ticks: u32,
    /// Share of each side's soldiers still standing and unbroken.
    left: f32,
    their_left: f32,
    orders: u32,
    /// The side under test's orders by priority, and attention spent.
    order_mix: String,
    journal: Vec<String>,
    /// The opponent's journal, if it is a mind.
    their_journal: Vec<String>,
}

enum Player {
    Mind,
    Bot(ChargeBot),
    Idle,
}

/// One battle: `side` against `against`.
fn fight(seed: u64, side: Side, against: Side) -> Outcome {
    fight_on(seed, seed, side, against)
}

fn fight_on(seed: u64, sim_seed: u64, side: Side, against: Side) -> Outcome {
    let mut rng = Pcg32::new(seed, 7);
    let us: PlayerId = (seed % 2) as PlayerId;
    let them: PlayerId = 1 - us;
    let mut world = World::new(&MatchSetup {
        seed: sim_seed,
        capacities: Capacities { regiments: 32 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
    });
    let terrain = Terrain::open(MAP);
    // Two armies facing each other across 70 to 100 tiles of open ground.
    let apart = 70.0 + rng.below(31) as f32;
    let x = 100.0 + rng.below(57) as f32;
    let tilt = (rng.below(41) as f32 - 20.0) / 100.0;
    let axis = Vec2::new(tilt, 1.0).normalize();
    let mid = Vec2::new(x, 128.0);
    let mut out: Vec<PlayerCommand> = Vec::new();
    for party in 0..2 {
        let towards = if party == 0 { axis } else { -axis };
        let centre = mid - towards * apart * 0.5;
        let placed: Vec<Placement> = deploy(&DRILL_UNITS, &ARMY, centre, towards, &terrain);
        spawn_commands(party, &placed, &mut out);
    }
    world.tick(&out);

    let mut minds = Minds::new(terrain);
    let mut seat = |party: PlayerId, side: Side| -> Player {
        let config = MindConfig {
            party,
            difficulty: Difficulty::General,
            // Personalities take turns, so every one is tested.
            personality: (seed / 2) as usize + party as usize * 3,
            seed: seed * 2 + party as u64,
        };
        match side {
            Side::Mind(difficulty) => {
                minds.add(Mind::new(MindConfig {
                    difficulty,
                    ..config
                }));
                Player::Mind
            }
            Side::Tuned(difficulty, tune) => {
                let mut knobs = difficulty.knobs();
                tune(&mut knobs);
                minds.add(Mind::with_knobs(
                    MindConfig {
                        difficulty,
                        ..config
                    },
                    knobs,
                ));
                Player::Mind
            }
            Side::Planned(difficulty, plan) => {
                let mut knobs = difficulty.knobs();
                knobs.rehearse = false;
                let mut mind = Mind::with_knobs(
                    MindConfig {
                        difficulty,
                        ..config
                    },
                    knobs,
                );
                mind.follow(plan);
                minds.add(mind);
                Player::Mind
            }
            Side::Charge => Player::Bot(ChargeBot::new(party)),
            Side::Idle => Player::Idle,
        }
    };
    let mut players = [seat(us, side), seat(them, against)];
    let mut view = Perception::new();
    let mut held_by: Option<(PlayerId, u32)> = None;
    let mut winner = None;
    let mut last_verdict = None;
    while world.tick_count() < LIMIT {
        out.clear();
        view.capture(&world);
        for p in &mut players {
            if let Player::Bot(bot) = p {
                bot.tick(&view, &mut out);
            }
        }
        minds.tick(&world, &mut out);
        world.tick(&out);
        if trace(seed)
            && let Some(v) = minds.get(us).and_then(Mind::verdict)
            && Some(v.tick) != last_verdict
        {
            last_verdict = Some(v.tick);
            print_verdict(v);
        }
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            if trace(seed) && world.tick_count().is_multiple_of(40) {
                print_field(&view, us, minds.get(us));
            }
            match (holding_the_field(&view), held_by) {
                (Some(p), Some((q, since))) if p == q => {
                    if world.tick_count() >= since + HELD {
                        winner = Some(p);
                        break;
                    }
                }
                (Some(p), _) => held_by = Some((p, world.tick_count())),
                (None, _) => held_by = None,
            }
        }
    }
    view.capture(&world);
    let standing = |party: PlayerId| {
        let (mut left, mut all) = (0.0, 0.0);
        for r in view.regiments.iter().filter(|r| r.owner == party) {
            all += r.full as f32;
            if !r.broken() {
                left += r.models as f32;
            }
        }
        left / all.max(1.0)
    };
    let (left, their_left) = (standing(us), standing(them));
    let mind = minds.get(us);
    Outcome {
        won: match winner {
            Some(p) => p == us,
            None => left > their_left,
        },
        ticks: world.tick_count(),
        left,
        their_left,
        orders: mind.map_or(0, Mind::orders_sent),
        order_mix: mind.map_or(String::new(), |m| {
            let o = m.orders();
            format!(
                "orders by priority {:?}, waited for attention {:?}, attention spent {}",
                &o.decided[..5],
                &o.waited[..5],
                o.spent
            )
        }),
        journal: journal_of(mind),
        their_journal: journal_of(minds.get(them)),
    }
}

fn journal_of(mind: Option<&Mind>) -> Vec<String> {
    mind.map_or(Vec::new(), |m| {
        m.journal()
            .entries()
            .map(|e| format!("{:>5}: {}", e.tick, e.text))
            .collect()
    })
}

/// Battles `seeds` of `side` against `against`; returns the wins.
fn contest(side: Side, against: Side, seeds: std::ops::RangeInclusive<u64>) -> u32 {
    let mut wins = 0;
    let verbose = std::env::var_os("AI_VERBOSE").is_some();
    for seed in seeds {
        let r = fight(seed, side, against);
        if verbose || !r.won {
            println!(
                "{side:?} v {against:?}, seed {seed}: {} in {}s, {:.0}% left to {:.0}%, {} orders",
                if r.won { "won" } else { "LOST" },
                r.ticks / 20,
                r.left * 100.0,
                r.their_left * 100.0,
                r.orders,
            );
        }
        if std::env::var_os("AI_ORDERS").is_some() {
            println!("    {}", r.order_mix);
        }
        if std::env::var_os("AI_JOURNAL").is_some() {
            for line in &r.journal {
                println!("    {line}");
            }
        }
        wins += u32::from(r.won);
    }
    wins
}

/// `AI_FROM=<seed>`: start tuning runs from another seed, to check a
/// change on battles it wasn't tuned on.
fn first_seed() -> u64 {
    std::env::var("AI_FROM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1)
}

fn battles() -> u64 {
    std::env::var("AI_BATTLES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(120)
}

#[test]
fn the_general_beats_charge_straight_in_nine_times_in_ten() {
    let wins = contest(Side::Mind(Difficulty::General), Side::Charge, 1..=40);
    assert!(wins >= 36, "won {wins} of 40");
}

#[test]
fn the_veteran_beats_charge_straight_in_more_often_than_not() {
    let wins = contest(Side::Mind(Difficulty::Veteran), Side::Charge, 1..=40);
    assert!(wins > 20, "won {wins} of 40");
}

#[test]
fn the_general_beats_the_veteran_seven_times_in_ten() {
    let general = Side::Mind(Difficulty::General);
    let wins = contest(general, Side::Mind(Difficulty::Veteran), 1..=30);
    assert!(wins >= 21, "won {wins} of 30");
}

#[test]
fn the_veteran_beats_the_recruit_seven_times_in_ten() {
    let veteran = Side::Mind(Difficulty::Veteran);
    let wins = contest(veteran, Side::Mind(Difficulty::Recruit), 1..=30);
    assert!(wins >= 21, "won {wins} of 30");
}

#[test]
fn a_battle_plays_out_the_same_every_time() {
    let general = Side::Mind(Difficulty::General);
    let a = fight(3, general, Side::Charge);
    let b = fight(3, general, Side::Charge);
    assert!(a.orders > 20, "{a:?}");
    assert_eq!(
        (a.ticks, a.orders, a.journal),
        (b.ticks, b.orders, b.journal)
    );
}

fn print_verdict(v: &gc_ai::Verdict) {
    println!(
        "t={}s rehearsed: {} ({:+.2}, before {:?})",
        v.tick / 20,
        v.plan.describe(),
        v.score,
        v.before
    );
    for (plan, score) in &v.plans {
        let by: Vec<String> = v
            .trials
            .iter()
            .filter(|t| t.plan == *plan)
            .map(|t| format!("{:+.2}", t.score))
            .collect();
        println!("    {:>40} {score:+.2}  [{}]", plan.describe(), by.join(" "));
    }
}

/// `AI_TRACE=<seed>` prints the field every two seconds of that battle.
fn trace(seed: u64) -> bool {
    std::env::var("AI_TRACE").is_ok_and(|v| v == seed.to_string())
}

fn print_field(view: &Perception, us: PlayerId, mind: Option<&Mind>) {
    println!(
        "t={}s  {}",
        view.tick / 20,
        mind.map_or(String::new(), Mind::summary)
    );
    for r in &view.regiments {
        let role = mind
            .and_then(|mind| mind.captain().members.iter().find(|m| m.handle == r.handle))
            .map_or(String::new(), |m| {
                let target = m
                    .target
                    .map_or(String::new(), |t| format!(" ->{}", t.index));
                format!("{}{target}", m.role.name())
            });
        println!(
            "  {}{:>2} u{} ({:5.1},{:5.1}) f({:4.1},{:4.1}) {:>2}/{:>2} {:?}{}{} m{} {}",
            if r.owner == us { "A" } else { "b" },
            r.handle.index,
            r.unit,
            r.pos.x,
            r.pos.y,
            r.facing.x,
            r.facing.y,
            r.models,
            r.full,
            r.morale,
            if r.engaged { " ENG" } else { "" },
            if r.moving { " mov" } else { "" },
            r.morale_points,
            role,
        );
    }
}

/// One General battle, traced: against charge-straight-in, or against
/// `AI_AGAINST=Recruit|Veteran|General`.
#[test]
#[ignore]
fn one() {
    let seed = std::env::var("AI_TRACE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let r = fight(seed, Side::Mind(level()), against());
    println!(
        "{} in {}s: {}",
        if r.won { "won" } else { "LOST" },
        r.ticks / 20,
        r.order_mix
    );
    for line in &r.journal {
        println!("    {line}");
    }
    if !r.their_journal.is_empty() {
        println!("  they:");
        for line in &r.their_journal {
            println!("    {line}");
        }
    }
}

/// `AI_AGAINST`: the opponent for tuning runs (charge straight in by default).
fn against() -> Side {
    match std::env::var("AI_AGAINST").as_deref() {
        Ok("Recruit") => Side::Mind(Difficulty::Recruit),
        Ok("Veteran") => Side::Mind(Difficulty::Veteran),
        Ok("General") => Side::Mind(Difficulty::General),
        Ok("Warlord") => Side::Mind(Difficulty::Warlord),
        _ => Side::Charge,
    }
}

/// `AI_LEVEL`: the level under test in tuning runs (the General by default).
fn level() -> Difficulty {
    match std::env::var("AI_LEVEL").as_deref() {
        Ok("Recruit") => Difficulty::Recruit,
        Ok("Veteran") => Difficulty::Veteran,
        Ok("Warlord") => Difficulty::Warlord,
        _ => Difficulty::General,
    }
}

/// `AI_LEVEL` against `AI_AGAINST` over `AI_BATTLES` from `AI_FROM`, in
/// parallel, with the time it took.
#[test]
#[ignore]
fn duel() {
    let n = battles();
    let from = first_seed();
    let level = match std::env::var("AI_LEVEL").as_deref() {
        // The Warlord's quicker hands without its rehearsals.
        Ok("Hands") => Side::Tuned(Difficulty::Warlord, |k| k.rehearse = false),
        _ => Side::Mind(level()),
    };
    let against = against();
    let t0 = std::time::Instant::now();
    let seeds: Vec<u64> = (from..from + n).collect();
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    let wins: u32 = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(threads))
            .map(|chunk| s.spawn(move || contest(level, against, *chunk.first().unwrap()..=*chunk.last().unwrap())))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .sum()
    });
    println!("{level:?} v {against:?}: won {wins} of {n} in {:?}", t0.elapsed());
}

/// Win rates over many battles, every level against charge-straight-in and
/// against the level below.
#[test]
#[ignore]
fn sweep() {
    let n = battles();
    let seeds = first_seed()..=first_seed() + n - 1;
    let levels = [
        Difficulty::General,
        Difficulty::Veteran,
        Difficulty::Recruit,
    ];
    for d in levels {
        let wins = contest(Side::Mind(d), Side::Charge, seeds.clone());
        println!("{d:?} v charge straight in: won {wins} of {n}");
    }
    for pair in levels.windows(2) {
        let wins = contest(Side::Mind(pair[0]), Side::Mind(pair[1]), seeds.clone());
        println!("{:?} v {:?}: won {wins} of {n}", pair[0], pair[1]);
    }
}

/// What a fair fight and doing nothing score, for scale.
#[test]
#[ignore]
fn yardsticks() {
    let n = battles();
    for side in [Side::Charge, Side::Idle] {
        let wins = contest(side, Side::Charge, 1..=n);
        println!("{side:?} v charge straight in: won {wins} of {n}");
    }
}

type Tune = (&'static str, fn(&mut Knobs));

/// How much each knob is worth: `AI_LEVEL=Veteran` picks the level,
/// `AI_AGAINST=Veteran` the opponent (charge straight in by default), and
/// `AI_TUNE=<text>` runs only the variants whose name contains it.
#[test]
#[ignore]
fn knob_study() {
    let n = battles();
    let level = match std::env::var("AI_LEVEL").as_deref() {
        Ok("Recruit") => Difficulty::Recruit,
        Ok("Veteran") => Difficulty::Veteran,
        _ => Difficulty::General,
    };
    let against = against();
    let only = std::env::var("AI_TUNE").ok();
    let tunes: [Tune; 10] = [
        ("as is", |_| {}),
        ("reaction 0.8 s", |k| k.reaction_ticks = 16),
        ("45 orders a minute", |k| k.orders_per_minute = 45),
        ("thinks every 5 ticks", |k| k.think_ticks = 5),
        ("no morale targeting", |k| k.morale_targeting = false),
        ("no focus", |k| k.focus_fire = false),
        ("no saving wavering", |k| k.save_wavering = false),
        ("untimed hammer", |k| k.timed_hammer = false),
        ("engages at 10", |k| k.engage_tiles = 10.0),
        ("slips 10%", |k| k.slip_percent = 10),
    ];
    for (name, tune) in tunes {
        if only.as_ref().is_some_and(|o| !name.contains(o.as_str())) {
            continue;
        }
        let from = first_seed();
        let wins = contest(Side::Tuned(level, tune), against, from..=from + n - 1);
        println!("{level:?} with {name} v {against:?}: won {wins} of {n}");
    }
}

#[test]
#[ignore]
fn cost() {
    let t0 = std::time::Instant::now();
    let mut ticks = 0;
    for seed in 1..=10 {
        let r = fight(seed, Side::Mind(Difficulty::General), Side::Mind(Difficulty::General));
        ticks += r.ticks;
    }
    let el = t0.elapsed();
    println!("{ticks} ticks in {el:?}: {:.4} ms a tick", el.as_secs_f64() * 1000.0 / ticks as f64);
}

/// Every plan fought throughout by the Warlord's hands against `AI_AGAINST`
/// (the General by default): is one plan simply better?
#[test]
#[ignore]
fn plans() {
    let n = battles();
    let from = first_seed();
    let against = match std::env::var("AI_AGAINST") {
        Ok(_) => against(),
        Err(_) => Side::Mind(Difficulty::General),
    };
    for doctrine in Doctrine::ALL {
        for stance in Stance::ALL {
            let plan = Plan { doctrine, stance };
            let side = Side::Planned(Difficulty::Warlord, plan);
            let seeds: Vec<u64> = (from..from + n).collect();
            let wins: u32 = std::thread::scope(|s| {
                seeds
                    .chunks(seeds.len().div_ceil(4))
                    .map(|c| s.spawn(move || contest(side, against, c[0]..=*c.last().unwrap())))
                    .collect::<Vec<_>>()
                    .into_iter()
                    .map(|h| h.join().unwrap())
                    .sum()
            });
            println!("{:>40}: won {wins} of {n}", plan.describe());
        }
    }
}

/// How noisy one rehearsal is: from two seconds into a General against
/// General battle, every plan against a sound commander with `AI_SEEDS`
/// different random streams (8 by default): mean and spread.
#[test]
#[ignore]
fn noise() {
    use gc_ai::rehearsal::{self, Answer};
    use std::sync::Arc;
    let k: u64 = std::env::var("AI_SEEDS").map_or(8, |v| v.parse().unwrap());
    for seed in first_seed()..first_seed() + battles() {
        let mut rng = Pcg32::new(seed, 7);
        let us: PlayerId = (seed % 2) as PlayerId;
        let mut world = World::new(&MatchSetup {
            seed,
            capacities: Capacities { regiments: 32 },
            units: &DRILL_UNITS,
            map_tiles: MAP,
            nav: &[],
            teams: &[],
            ground: gc_sim::Ground::FLAT,
        });
        let terrain = Arc::new(Terrain::open(MAP));
        let apart = 70.0 + rng.below(31) as f32;
        let x = 100.0 + rng.below(57) as f32;
        let tilt = (rng.below(41) as f32 - 20.0) / 100.0;
        let axis = Vec2::new(tilt, 1.0).normalize();
        let mid = Vec2::new(x, 128.0);
        let mut out: Vec<PlayerCommand> = Vec::new();
        for party in 0..2 {
            let towards = if party == 0 { axis } else { -axis };
            let centre = mid - towards * apart * 0.5;
            let placed: Vec<Placement> = deploy(&DRILL_UNITS, &ARMY, centre, towards, &terrain);
            spawn_commands(party, &placed, &mut out);
        }
        world.tick(&out);
        let mut minds: Vec<Mind> = (0..2)
            .map(|party| {
                let mut knobs = Difficulty::Warlord.knobs();
                knobs.rehearse = false;
                Mind::with_knobs(
                    MindConfig {
                        party,
                        difficulty: Difficulty::Warlord,
                        personality: (seed / 2) as usize + party as usize * 3,
                        seed: seed * 2 + party as u64,
                    },
                    knobs,
                )
            })
            .collect();
        let mut view = Perception::new();
        for _ in 0..40 {
            view.capture(&world);
            out.clear();
            for m in &mut minds {
                m.tick(&view, &terrain, &world, &mut out);
            }
            world.tick(&out);
        }
        view.capture(&world);
        let mind = &minds[us as usize];
        println!("battle {seed}:");
        for plan in rehearsal::plans_for(&view, &terrain, us, mind.knobs()) {
            let scores: Vec<f32> = (0..k)
                .map(|s| rehearsal::trial(&world, &view, &terrain, mind, &[], plan, Answer::Sound, s))
                .collect();
            let mean = scores.iter().sum::<f32>() / k as f32;
            let sd = (scores.iter().map(|x| (x - mean).powi(2)).sum::<f32>() / k as f32).sqrt();
            let all: Vec<String> = scores.iter().map(|x| format!("{x:+.1}")).collect();
            println!("  {:>36}: {mean:+.2} ± {sd:.2}  [{}]", plan.describe(), all.join(" "));
        }
    }
}

/// TODO remove: in how many battles some fixed plan wins.
#[test]
#[ignore]
fn oracle() {
    let n = battles();
    let from = first_seed();
    let seeds: Vec<u64> = (from..from + n).collect();
    let rows: Vec<(u64, Vec<bool>)> = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(4))
            .map(|c| {
                s.spawn(move || {
                    c.iter()
                        .map(|&seed| {
                            let mut w = Vec::new();
                            for doctrine in Doctrine::ALL {
                                for stance in Stance::ALL {
                                    let plan = Plan { doctrine, stance };
                                    let side = Side::Planned(Difficulty::Warlord, plan);
                                    w.push(fight(seed, side, Side::Mind(Difficulty::General)).won);
                                }
                            }
                            (seed, w)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let any = rows.iter().filter(|(_, w)| w.iter().any(|&x| x)).count();
    println!("some plan wins {any} of {n}");
    for (seed, w) in &rows {
        let s: String = w.iter().map(|&x| if x { 'W' } else { '.' }).collect();
        println!("{seed:>4} {s}");
    }
}

/// TODO remove: each fixed plan's chance of winning each battle, from
/// `AI_REPS` replays with other random streams.
#[test]
#[ignore]
fn odds() {
    let n = battles();
    let reps: u64 = std::env::var("AI_REPS").map_or(8, |v| v.parse().unwrap());
    let from = first_seed();
    let seeds: Vec<u64> = (from..from + n).collect();
    let rows: Vec<(u64, Vec<f32>)> = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(4))
            .map(|c| {
                s.spawn(move || {
                    c.iter()
                        .map(|&seed| {
                            let mut w = Vec::new();
                            for doctrine in Doctrine::ALL {
                                for stance in Stance::ALL {
                                    let plan = Plan { doctrine, stance };
                                    let side = Side::Planned(Difficulty::Warlord, plan);
                                    let won = (0..reps)
                                        .filter(|r| {
                                            fight_on(seed, seed + (r + rep_from()) * 1_000_003, side, Side::Mind(Difficulty::General)).won
                                        })
                                        .count();
                                    w.push(won as f32 / reps as f32);
                                }
                            }
                            (seed, w)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let best: f32 = rows.iter().map(|(_, w)| w.iter().cloned().fold(0.0, f32::max)).sum::<f32>() / n as f32;
    let k = rows[0].1.len();
    let fixed: Vec<f32> = (0..k).map(|p| rows.iter().map(|(_, w)| w[p]).sum::<f32>() / n as f32).collect();
    println!("best plan per battle: {:.1}%  each plan: {:?}", best * 100.0, fixed.iter().map(|x| (x * 100.0).round()).collect::<Vec<_>>());
    for (seed, w) in &rows {
        let s: Vec<String> = w.iter().map(|x| format!("{:>3.0}", x * 100.0)).collect();
        println!("ODDS {seed:>4} {}", s.join(" "));
    }
}

/// TODO remove: how good the rehearsal's first pick is, by the odds `odds`
/// measured (`AI_ODDS=<file of its ODDS lines>`).
#[test]
#[ignore]
fn pick() {
    use gc_ai::rehearsal::{self, Answer};
    use std::sync::Arc;
    let k: u64 = std::env::var("AI_SEEDS").map_or(6, |v| v.parse().unwrap());
    let odds: std::collections::HashMap<u64, Vec<f32>> =
        std::fs::read_to_string(std::env::var("AI_ODDS").unwrap())
            .unwrap()
            .lines()
            .filter_map(|l| l.strip_prefix("ODDS"))
            .map(|l| {
                let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
                (v[0] as u64, v[1..].iter().map(|x| x / 100.0).collect())
            })
            .collect();
    let seeds: Vec<u64> = (first_seed()..first_seed() + battles()).collect();
    let rows: Vec<(u64, f32, f32, f32, Vec<(Plan, f32, f32)>)> = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(4))
            .map(|c| {
                let odds = &odds;
                s.spawn(move || {
                    c.iter()
                        .map(|&seed| {
                            let (world, view, terrain, mind) = field_at(seed, 40);
                            let plans = rehearsal::plans_for(&view, &terrain, mind.party(), mind.knobs());
                            let scores: Vec<f32> = plans
                                .iter()
                                .map(|&plan| {
                                    (0..k)
                                        .map(|s| rehearsal::trial(&world, &view, &terrain, &mind, &[], plan, Answer::Sound, s))
                                        .sum::<f32>()
                                        / k as f32
                                })
                                .collect();
                            let all: Vec<Plan> = Doctrine::ALL
                                .iter()
                                .flat_map(|&doctrine| Stance::ALL.iter().map(move |&stance| Plan { doctrine, stance }))
                                .collect();
                            let o = &odds[&seed];
                            let at = |p: Plan| o[all.iter().position(|&q| q == p).unwrap()];
                            let best = (0..plans.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
                            let got = at(plans[best]);
                            let top = plans.iter().map(|&p| at(p)).fold(0.0, f32::max);
                            let mean = plans.iter().map(|&p| at(p)).sum::<f32>() / plans.len() as f32;
                            let each = plans.iter().zip(&scores).map(|(&p, &s)| (p, s, at(p))).collect();
                            (seed, got, top, mean, each)
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let n = rows.len() as f32;
    let got = rows.iter().map(|r| r.1).sum::<f32>() / n;
    let top = rows.iter().map(|r| r.2).sum::<f32>() / n;
    let mean = rows.iter().map(|r| r.3).sum::<f32>() / n;
    println!("picked {:.1}%, best {:.1}%, average plan {:.1}%", got * 100.0, top * 100.0, mean * 100.0);
    for doctrine in Doctrine::ALL {
        for stance in Stance::ALL {
            let plan = Plan { doctrine, stance };
            let e: Vec<(f32, f32)> = rows.iter().flat_map(|r| r.4.iter().filter(|x| x.0 == plan).map(|x| (x.1, x.2))).collect();
            let m = e.len() as f32;
            let (ms, mo) = (e.iter().map(|x| x.0).sum::<f32>() / m, e.iter().map(|x| x.1).sum::<f32>() / m);
            let cov = e.iter().map(|x| (x.0 - ms) * (x.1 - mo)).sum::<f32>();
            let vs = e.iter().map(|x| (x.0 - ms).powi(2)).sum::<f32>();
            let vo = e.iter().map(|x| (x.1 - mo).powi(2)).sum::<f32>();
            println!("{:>40}: score {ms:+.2}  odds {:.0}%  corr {:+.2}  ({m})", plan.describe(), mo * 100.0, cov / (vs * vo).sqrt());
        }
    }
    // Within a battle: does the better-scored plan have the better odds?
    let (mut agree, mut pairs) = (0, 0);
    for r in &rows {
        for a in &r.4 {
            for b in &r.4 {
                if a.1 > b.1 && a.2 != b.2 {
                    pairs += 1;
                    agree += usize::from(a.2 > b.2);
                }
            }
        }
    }
    println!("within a battle, the better score has the better odds {agree} of {pairs}");
}

/// TODO remove: battle `seed` after `ticks`, the Warlord's hands (not
/// rehearsing) against the General.
fn field_at(seed: u64, ticks: u32) -> (World, Perception, std::sync::Arc<Terrain>, Mind) {
    let (w, v, t, us, _) = field_at2(seed, ticks);
    (w, v, t, us)
}

fn field_at2(seed: u64, ticks: u32) -> (World, Perception, std::sync::Arc<Terrain>, Mind, Mind) {
    let mut rng = Pcg32::new(seed, 7);
    let us: PlayerId = (seed % 2) as PlayerId;
    let mut world = World::new(&MatchSetup {
        seed,
        capacities: Capacities { regiments: 32 },
        units: &DRILL_UNITS,
        map_tiles: MAP,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
    });
    let terrain = std::sync::Arc::new(Terrain::open(MAP));
    let apart = 70.0 + rng.below(31) as f32;
    let x = 100.0 + rng.below(57) as f32;
    let tilt = (rng.below(41) as f32 - 20.0) / 100.0;
    let axis = Vec2::new(tilt, 1.0).normalize();
    let mid = Vec2::new(x, 128.0);
    let mut out: Vec<PlayerCommand> = Vec::new();
    for party in 0..2 {
        let towards = if party == 0 { axis } else { -axis };
        let centre = mid - towards * apart * 0.5;
        let placed: Vec<Placement> = deploy(&DRILL_UNITS, &ARMY, centre, towards, &terrain);
        spawn_commands(party, &placed, &mut out);
    }
    world.tick(&out);
    let mut minds: Vec<Mind> = (0..2)
        .map(|party| {
            let difficulty = if party == us { Difficulty::Warlord } else { Difficulty::General };
            let mut knobs = difficulty.knobs();
            knobs.rehearse = false;
            Mind::with_knobs(
                MindConfig {
                    party,
                    difficulty,
                    personality: (seed / 2) as usize + party as usize * 3,
                    seed: seed * 2 + party as u64,
                },
                knobs,
            )
        })
        .collect();
    let mut view = Perception::new();
    for _ in 0..ticks {
        view.capture(&world);
        out.clear();
        for m in &mut minds {
            m.tick(&view, &terrain, &world, &mut out);
        }
        world.tick(&out);
    }
    view.capture(&world);
    let them = minds.swap_remove(1 - us as usize);
    let mind = minds.pop().unwrap();
    (world, view, terrain, mind, them)
}

/// TODO remove: play `world` on with `minds` until someone holds the field
/// or `until`; whether `us` won, and the rehearsal score.
fn play_out(mut world: World, minds: &mut [Mind], terrain: &std::sync::Arc<Terrain>, us: PlayerId, until: u32) -> (bool, f32) {
    let mut view = Perception::new();
    view.capture(&world);
    let start = view.clone();
    let mut out = Vec::new();
    let mut held_by: Option<(PlayerId, u32)> = None;
    let mut winner = None;
    while world.tick_count() < until {
        view.capture(&world);
        out.clear();
        for m in minds.iter_mut() {
            m.tick(&view, terrain, &world, &mut out);
        }
        world.tick(&out);
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            match (holding_the_field(&view), held_by) {
                (Some(p), Some((q, since))) if p == q => {
                    if world.tick_count() >= since + HELD {
                        winner = Some(p);
                        break;
                    }
                }
                (Some(p), _) => held_by = Some((p, world.tick_count())),
                (None, _) => held_by = None,
            }
        }
    }
    view.capture(&world);
    let standing = |party: PlayerId| {
        let (mut left, mut all) = (0.0, 0.0);
        for r in view.regiments.iter().filter(|r| r.owner == party) {
            all += r.full as f32;
            if !r.broken() {
                left += r.models as f32;
            }
        }
        left / all.max(1.0)
    };
    let won = match winner {
        Some(p) => p == us,
        None => standing(us) > standing(1 - us),
    };
    (won, gc_ai::rehearsal::score(&start, &view, us))
}

/// TODO remove: which part of the rehearsal loses the most. `R_WORLD`
/// fork|seen, `R_ENEMY` clone|real, `R_END` win|score (90 s).
#[test]
#[ignore]
fn ablate() {
    use gc_ai::rehearsal;
    let k: u64 = std::env::var("AI_SEEDS").map_or(8, |v| v.parse().unwrap());
    let mode = |name: &str, default: &str| std::env::var(name).unwrap_or(default.into());
    let (wm, em, endm) = (mode("R_WORLD", "fork"), mode("R_ENEMY", "clone"), mode("R_END", "win"));
    let odds: std::collections::HashMap<u64, Vec<f32>> =
        std::fs::read_to_string(std::env::var("AI_ODDS").unwrap())
            .unwrap()
            .lines()
            .filter_map(|l| l.strip_prefix("ODDS"))
            .map(|l| {
                let v: Vec<f32> = l.split_whitespace().map(|x| x.parse().unwrap()).collect();
                (v[0] as u64, v[1..].iter().map(|x| x / 100.0).collect())
            })
            .collect();
    let all: Vec<Plan> = Doctrine::ALL
        .iter()
        .flat_map(|&doctrine| Stance::ALL.iter().map(move |&stance| Plan { doctrine, stance }))
        .collect();
    let seeds: Vec<u64> = (first_seed()..first_seed() + battles()).collect();
    let rows: Vec<(f32, f32)> = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(4))
            .map(|c| {
                let (odds, all, wm, em, endm) = (&odds, &all, &wm, &em, &endm);
                s.spawn(move || {
                    c.iter()
                        .map(|&seed| {
                            let (world, view, terrain, mind, foe) = field_at2(seed, 40);
                            let us = mind.party();
                            let them = 1 - us;
                            let plans = rehearsal::plans_for(&view, &terrain, us, mind.knobs());
                            let scores: Vec<f32> = plans
                                .iter()
                                .map(|&plan| {
                                    (0..k)
                                        .map(|j| {
                                            let rs = seed * 7919 + j * 104_729 + 5;
                                            let w = if wm == "fork" {
                                                let mut w = world.fork_region(gc_fixed::FxVec2::from_ints(128, 128), gc_fixed::Fx::from_int(1000));
                                                w.reseed(rs);
                                                w
                                            } else {
                                                let cap = view.regiments.iter().map(|r| r.handle.index + 1).max().unwrap();
                                                let mut w = world.scratch(cap, rs);
                                                for r in &view.regiments {
                                                    w.spawn_standing(&rehearsal::standing(r), Some(r.handle));
                                                }
                                                w
                                            };
                                            let mut a = mind.fork();
                                            a.follow(plan);
                                            let b = if em == "clone" {
                                                foe.fork()
                                            } else {
                                                Mind::new(MindConfig {
                                                    party: them,
                                                    difficulty: Difficulty::General,
                                                    personality: (seed / 2) as usize + them as usize * 3,
                                                    seed: rs,
                                                })
                                            };
                                            let mut minds = [a, b];
                                            if endm == "win" {
                                                let (won, _) = play_out(w, &mut minds, &terrain, us, LIMIT);
                                                if won { 1.0 } else { 0.0 }
                                            } else {
                                                let t = w.tick_count() + 90 * 20;
                                                play_out(w, &mut minds, &terrain, us, t).1
                                            }
                                        })
                                        .sum::<f32>()
                                        / k as f32
                                })
                                .collect();
                            let o = &odds[&seed];
                            let at = |p: Plan| o[all.iter().position(|&q| q == p).unwrap()];
                            let best = (0..plans.len()).max_by(|&a, &b| scores[a].total_cmp(&scores[b])).unwrap();
                            let mut agree = 0.0;
                            let mut pairs = 0.0;
                            for i in 0..plans.len() {
                                for j in 0..plans.len() {
                                    if scores[i] > scores[j] && at(plans[i]) != at(plans[j]) {
                                        pairs += 1.0;
                                        if at(plans[i]) > at(plans[j]) { agree += 1.0; }
                                    }
                                }
                            }
                            (at(plans[best]), agree / f32::max(pairs, 1.0))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    let n = rows.len() as f32;
    println!(
        "{wm}/{em}/{endm}: picked {:.1}%, agreement {:.1}%",
        rows.iter().map(|r| r.0).sum::<f32>() / n * 100.0,
        rows.iter().map(|r| r.1).sum::<f32>() / n * 100.0
    );
}

fn rep_from() -> u64 {
    std::env::var("AI_REPFROM").map_or(0, |v| v.parse().unwrap())
}

/// TODO remove: how soon the reader names the General it faces, over
/// `AI_BATTLES` General against General battles.
#[test]
#[ignore]
fn reads() {
    use gc_ai::reading::Reading;
    let marks = [5u32, 10, 15, 20, 30, 45, 60];
    let seeds: Vec<u64> = (first_seed()..first_seed() + battles()).collect();
    let all = gc_ai::personality::all();
    let rows: Vec<Vec<usize>> = std::thread::scope(|s| {
        seeds
            .chunks(seeds.len().div_ceil(4))
            .map(|c| {
                s.spawn(move || {
                    c.iter()
                        .map(|&seed| {
                            let mut rng = Pcg32::new(seed, 7);
                            let us: PlayerId = (seed % 2) as PlayerId;
                            let them = 1 - us;
                            let mut world = World::new(&MatchSetup {
                                seed,
                                capacities: Capacities { regiments: 32 },
                                units: &DRILL_UNITS,
                                map_tiles: MAP,
                                nav: &[],
                                teams: &[],
                                ground: gc_sim::Ground::FLAT,
                            });
                            let terrain = std::sync::Arc::new(Terrain::open(MAP));
                            let apart = 70.0 + rng.below(31) as f32;
                            let x = 100.0 + rng.below(57) as f32;
                            let tilt = (rng.below(41) as f32 - 20.0) / 100.0;
                            let axis = Vec2::new(tilt, 1.0).normalize();
                            let mid = Vec2::new(x, 128.0);
                            let mut out: Vec<PlayerCommand> = Vec::new();
                            for party in 0..2 {
                                let towards = if party == 0 { axis } else { -axis };
                                let centre = mid - towards * apart * 0.5;
                                let placed = deploy(&DRILL_UNITS, &ARMY, centre, towards, &terrain);
                                spawn_commands(party, &placed, &mut out);
                            }
                            world.tick(&out);
                            let mut minds: Vec<Mind> = (0..2)
                                .map(|party| {
                                    Mind::new(MindConfig {
                                        party,
                                        difficulty: Difficulty::General,
                                        personality: (seed / 2) as usize + party as usize * 3,
                                        seed: seed * 2 + party as u64,
                                    })
                                })
                                .collect();
                            let truth = &all[((seed / 2) as usize + them as usize * 3) % all.len()].name;
                            let mut reading = Reading::new(them, Difficulty::General, seed ^ 0x5eed);
                            let mut view = Perception::new();
                            let mut ranks = Vec::new();
                            for t in 0..60 * 20 + 1 {
                                view.capture(&world);
                                reading.tick(&view, &terrain, &world);
                                if marks.contains(&(t / 20)) && t % 20 == 0 {
                                    let fits = reading.fits();
                                    let rank = fits.iter().position(|(n, _)| *n == truth.as_str()).unwrap_or(9);
                                    let real = minds[them as usize].captain();
                                    let (doctrine, posture) = match reading.guess() {
                                        Some(g) => (g.captain.doctrine == real.doctrine, g.captain.posture == real.posture),
                                        None => (false, false),
                                    };
                                    ranks.push(rank + 10 * usize::from(doctrine) + 100 * usize::from(posture));
                                    if t / 20 == 30 || t / 20 == 5 {
                                        println!("DOC t={} seed {seed} real {:?}/{:?} guess {:?}", t / 20, real.doctrine, real.posture, reading.guess().map(|g| (g.persona.name.clone(), g.captain.doctrine, g.captain.posture)));
                                    }
                                }
                                out.clear();
                                for m in &mut minds {
                                    m.tick(&view, &terrain, &world, &mut out);
                                }
                                world.tick(&out);
                            }
                            ranks
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    for (k, m) in marks.iter().enumerate() {
        let hist: Vec<usize> = (0..10).map(|r| rows.iter().filter(|x| x[k] % 10 == r).count()).collect();
        let d = rows.iter().filter(|x| x[k] / 10 % 10 == 1).count();
        let p = rows.iter().filter(|x| x[k] / 100 == 1).count();
        println!("t={m:>2}s: true temper ranked {hist:?}; doctrine right {d}, posture right {p} of {}", rows.len());
    }
}
