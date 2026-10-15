//! Rehearsal: thinking ahead with the real simulation (docs/18-ai.md,
//! "Rehearsal").
//!
//! The estimator is quick but rough. For the decision that matters most,
//! which plan to fight with, the Warlord does what a real general wishes
//! they could: it plays the battle forward in its head under the real
//! rules. Every few seconds it sets the field out in scratch worlds exactly
//! as it sees it (on the same map, shared rather than rebuilt, with every
//! regiment where it stands, as many soldiers as it shows and the nerve its
//! banner shows: engine hooks 5 and 6), and in each one tries one plan
//! against one way the enemy might answer:
//!
//! - **Plans:** each doctrine its army can fight with, waiting for the
//!   enemy, marching on them, or deciding that as it goes.
//! - **Answers:** a sound commander (a General of middling temper),
//!   charging straight in, or standing fast.
//!
//! Each scratch world runs the real simulation [`HORIZON`] ahead, its own
//! Captain following the plan and the enemy answering, and is scored by who
//! is left standing and how the fight from there looks to the estimator.
//! The plan with the best score across the answers is the one it follows (a
//! cautious mind weighs the worst answer more).
//!
//! It is fair: a scratch world holds only what the mind can see, never the
//! real world's hidden state (orders, wounds, charge and reload timers), so
//! a hidden reserve or an ambush still surprises it. It runs on worker
//! threads while the game goes on; the answer is taken a fixed time after
//! it was asked for, so the same battle always plays out the same way on
//! any machine (a machine too slow to finish in time waits for it).

use std::sync::{Arc, OnceLock};
use std::thread::{self, JoinHandle};

use gc_sim::{MoraleState, PlayerCommand, PlayerId, Standing, World};

use crate::captain::{self, Captain, Doctrine, Plan, Stance};
use crate::difficulty::{Difficulty, Knobs};
use crate::estimator::{self, Approach, Force};
use crate::perception::{Class, Perception, Seen, to_fx_vec, vec_angle};
use crate::reading::Guess;
use crate::personality::{DoctrineLikes, Faction, Personality};
use crate::scripted::ChargeBot;
use crate::terrain::Terrain;
use crate::{Mind, MindConfig};

/// How far ahead a rehearsal plays, ticks.
pub const HORIZON: u32 = 90 * 20;
/// Ticks from asking for a rehearsal to taking its answer.
pub const DELAY: u32 = 20;
/// Ticks between rehearsals.
pub const EVERY: u32 = 10 * 20;
/// A rehearsal stops early once one side has held the field this long.
const DECIDED: u32 = 5 * 20;
/// The estimator's view of the fight left at the end counts this much
/// against what has happened by then.
const LOOK_BEYOND: f32 = 0.5;
/// A plan must beat the one it follows by this much to replace it.
const STICK: f32 = 0.03;
/// How much a regiment counts for, by its state of mind, per share of its
/// full strength still standing.
const WAVERING: f32 = 0.75;
const ROUTING: f32 = 0.2;

/// How the enemy might answer a plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Answer {
    /// A sound commander: the General's tactics with a middling temper.
    Sound,
    /// Every regiment charges the nearest enemy.
    Charge,
    /// Nobody moves; the simulation's reflexes fight back.
    Stand,
}

impl Answer {
    pub const ALL: [Answer; 3] = [Answer::Sound, Answer::Charge, Answer::Stand];

    pub fn name(self) -> &'static str {
        match self {
            Answer::Sound => "a sound commander",
            Answer::Charge => "charging straight in",
            Answer::Stand => "standing fast",
        }
    }
}

/// One rehearsal: a plan against an answer, and how it came out.
#[derive(Clone, Copy, Debug)]
pub struct Trial {
    pub plan: Plan,
    pub answer: Answer,
    /// From about -1.5 (we are swept away) to +1.5 (they are).
    pub score: f32,
}

/// What a round of rehearsals decided.
#[derive(Clone, Debug)]
pub struct Verdict {
    /// The tick the field was set out from.
    pub tick: u32,
    pub plan: Plan,
    /// Its score, and the score of the plan it followed before (if it had one).
    pub score: f32,
    pub before: Option<f32>,
    /// Every plan's score, best first.
    pub plans: Vec<(Plan, f32)>,
    pub trials: Vec<Trial>,
}

/// A round of rehearsals under way.
pub struct Round {
    /// The tick the field was set out from, and the tick to take the answer.
    pub asked: u32,
    pub due: u32,
    job: JoinHandle<Vec<Trial>>,
}

/// What every rehearsal of a round shares.
struct Brief {
    party: PlayerId,
    view: Perception,
    terrain: Arc<Terrain>,
    captain: Captain,
    knobs: Knobs,
    persona: &'static Personality,
    /// Who the enemy seems to be, by party (see [`crate::reading`]).
    foes: Vec<Guess>,
    seed: u64,
}

impl Round {
    /// Set out the field as `view` shows it and start rehearsing `plans`
    /// against every answer, for the mind playing `party` with `captain`.
    /// `world` lends its map. None if there is nothing to rehearse.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        world: &World,
        view: &Perception,
        terrain: &Arc<Terrain>,
        party: PlayerId,
        captain: &Captain,
        knobs: &Knobs,
        persona: &'static Personality,
        foes: Vec<Guess>,
        plans: Vec<Plan>,
    ) -> Option<Round> {
        if plans.is_empty() || view.regiments.is_empty() {
            return None;
        }
        let capacity = view
            .regiments
            .iter()
            .map(|r| r.handle.index + 1)
            .max()
            .unwrap_or(1);
        let seed = u64::from(view.tick) << 8 | u64::from(party);
        let template = world.scratch(capacity, seed);
        let brief = Brief {
            party,
            view: view.clone(),
            terrain: terrain.clone(),
            captain: captain.clone(),
            knobs: Knobs {
                rehearse: false,
                slip_percent: 0,
                ..*knobs
            },
            persona,
            foes,
            seed,
        };
        let job = thread::spawn(move || rehearse_all(template, &brief, &plans));
        Some(Round {
            asked: view.tick,
            due: view.tick + DELAY,
            job,
        })
    }

    /// Wait for the round's rehearsals and pick the plan to follow. A
    /// cautious mind (`caution` 0 to 1) weighs each plan's worst answer
    /// more; `current` is the plan it follows now, which a new one must
    /// beat by a little to replace.
    pub fn finish(self, caution: f32, current: Option<Plan>) -> Verdict {
        let trials = self.job.join().expect("a rehearsal panicked");
        let mut plans: Vec<(Plan, f32)> = Vec::new();
        for t in &trials {
            if plans.iter().any(|(p, _)| *p == t.plan) {
                continue;
            }
            // Each answer's average over the random streams, then those
            // averaged, leaning toward the worst for a cautious mind.
            let mut by_answer: Vec<f32> = Vec::new();
            for a in Answer::ALL {
                let all: Vec<f32> = trials
                    .iter()
                    .filter(|o| o.plan == t.plan && o.answer == a)
                    .map(|o| o.score)
                    .collect();
                if !all.is_empty() {
                    by_answer.push(all.iter().sum::<f32>() / all.len() as f32);
                }
            }
            let mean = by_answer.iter().sum::<f32>() / by_answer.len() as f32;
            let worst = by_answer.iter().copied().fold(f32::MAX, f32::min);
            let w = 0.5 * caution;
            plans.push((t.plan, (1.0 - w) * mean + w * worst));
        }
        plans.sort_by(|a, b| b.1.total_cmp(&a.1));
        let before = current.and_then(|c| plans.iter().find(|(p, _)| *p == c).map(|(_, s)| *s));
        let (mut plan, mut score) = plans[0];
        if let (Some(c), Some(b)) = (current, before)
            && score < b + stick()
        {
            (plan, score) = (c, b);
        }
        Verdict {
            tick: self.asked,
            plan,
            score,
            before,
            plans,
            trials,
        }
    }
}

/// One rehearsal on its own, for tests and tuning: `mind` (seeing `view`
/// of `world`) plays `plan` against `answer`, with random streams from
/// `seed`. Returns its score.
pub fn trial(
    world: &World,
    view: &Perception,
    terrain: &Arc<Terrain>,
    mind: &Mind,
    foes: &[Guess],
    plan: Plan,
    answer: Answer,
    seed: u64,
) -> f32 {
    let capacity = view
        .regiments
        .iter()
        .map(|r| r.handle.index + 1)
        .max()
        .unwrap_or(1);
    let brief = Brief {
        party: mind.party(),
        view: view.clone(),
        terrain: terrain.clone(),
        captain: mind.captain().clone(),
        knobs: Knobs {
            rehearse: false,
            slip_percent: 0,
            ..*mind.knobs()
        },
        persona: mind.personality(),
        foes: foes.to_vec(),
        seed,
    };
    let template = world.scratch(capacity, seed);
    play(set_out(&template, &brief, seed), &brief, plan, answer, 0)
}

/// The plans worth rehearsing for `party`'s army in `view`: every doctrine
/// its level knows and its army can fight with, each waiting, advancing
/// and as it sees fit.
pub fn plans_for(view: &Perception, terrain: &Terrain, party: PlayerId, knobs: &Knobs) -> Vec<Plan> {
    let ours = view
        .regiments
        .iter()
        .filter(|r| r.owner == party && !r.broken());
    let (mut line, mut shock, mut ranged) = (0, 0, 0);
    for r in ours {
        match view.unit(r).class {
            Class::Line => line += 1,
            Class::Shock => shock += 1,
            Class::Ranged => ranged += 1,
        }
    }
    let mut plans = Vec::new();
    for &doctrine in knobs.doctrines {
        if !captain::fits(doctrine, line, shock, ranged)
            || doctrine == Doctrine::Ambush && !terrain.has_cover()
        {
            continue;
        }
        for stance in Stance::ALL {
            plans.push(Plan { doctrine, stance });
        }
    }
    plans
}

/// Rehearse every plan against every answer, spread over the machine's
/// cores. Results come back in the order of `plans` then answers, whatever
/// the cores.
fn rehearse_all(template: World, brief: &Brief, plans: &[Plan]) -> Vec<Trial> {
    let answers: Vec<Answer> = answers();
    let jobs: Vec<(Plan, Answer, u64)> = plans
        .iter()
        .flat_map(|&p| {
            answers
                .iter()
                .flat_map(move |&a| (0..seeds()).map(move |k| (p, a, k)))
        })
        .collect();
    let cores = thread::available_parallelism().map_or(1, |n| n.get());
    let per = jobs.len().div_ceil(cores.max(1)).max(1);
    let mut worlds: Vec<World> = jobs
        .iter()
        .map(|&(_, _, k)| set_out(&template, brief, brief.seed ^ k << 32))
        .collect();
    let mut trials = Vec::with_capacity(jobs.len());
    thread::scope(|s| {
        let mut handles = Vec::new();
        let mut rest = jobs.as_slice();
        while !rest.is_empty() {
            let (mine, more) = rest.split_at(per.min(rest.len()));
            rest = more;
            let ws: Vec<World> = worlds.drain(..mine.len()).collect();
            handles.push(s.spawn(move || {
                mine.iter()
                    .zip(ws)
                    .map(|(&(plan, answer, k), w)| Trial {
                        plan,
                        answer,
                        score: play(w, brief, plan, answer, k),
                    })
                    .collect::<Vec<_>>()
            }));
        }
        for h in handles {
            trials.extend(h.join().expect("a rehearsal panicked"));
        }
    });
    trials
}

/// A scratch world with every regiment the mind can see, each as it stands
/// and with the handle it has in the real world.
fn set_out(template: &World, brief: &Brief, seed: u64) -> World {
    let capacity = template.regiments.capacity() as u32;
    let mut w = template.scratch(capacity, seed);
    for r in &brief.view.regiments {
        w.spawn_standing(&standing(r), Some(r.handle));
    }
    w
}

pub fn standing(r: &Seen) -> Standing {
    Standing {
        owner: r.owner,
        unit: r.unit,
        position: to_fx_vec(r.pos),
        facing: vec_angle(r.facing),
        files: r.files,
        formation: r.formation,
        models: r.models,
        morale_points: r.morale_points,
        morale: r.morale,
    }
}

/// Who plays a party in a rehearsal.
enum Seat {
    Mind(Box<Mind>),
    Charge(ChargeBot),
    Stand,
}

/// Play `plan` against `answer` in `world` and score how it went.
fn play(mut world: World, brief: &Brief, plan: Plan, answer: Answer, k: u64) -> f32 {
    let party = brief.party;
    let start = &brief.view;
    let seed = brief.seed ^ k << 32;
    let mut us = Mind::seat(
        MindConfig {
            party,
            difficulty: Difficulty::Warlord,
            personality: 0,
            seed,
        },
        brief.knobs,
        brief.persona,
        {
            let mut c = brief.captain.clone();
            c.follow(plan);
            c
        },
    );
    let mut others: Vec<(PlayerId, Seat)> = Vec::new();
    for r in &start.regiments {
        if r.owner == party || others.iter().any(|(p, _)| *p == r.owner) {
            continue;
        }
        // The commander the enemy seems to be, as it stands; anyone else
        // (an ally, or an enemy not read yet) a sound one starting afresh.
        let sound = |p: PlayerId| {
            let guess = brief.foes.iter().find(|g| g.party == p && reading());
            Seat::Mind(Box::new(Mind::seat(
                MindConfig {
                    party: p,
                    difficulty: Difficulty::General,
                    personality: 0,
                    seed: seed ^ u64::from(p),
                },
                Knobs {
                    slip_percent: 0,
                    ..Difficulty::General.knobs()
                },
                guess.map_or(middling(), |g| g.persona),
                guess.map_or_else(Captain::new, |g| g.captain.clone()),
            )))
        };
        let seat = if !start.hostile(party, r.owner) {
            sound(r.owner)
        } else {
            match answer {
                Answer::Sound => sound(r.owner),
                Answer::Charge => Seat::Charge(ChargeBot::new(r.owner)),
                Answer::Stand => Seat::Stand,
            }
        };
        others.push((r.owner, seat));
    }
    let mut view = Perception::new();
    let mut out: Vec<PlayerCommand> = Vec::new();
    let end = world.tick_count() + horizon();
    let mut held: Option<(PlayerId, u32)> = None;
    while world.tick_count() < end {
        view.capture(&world);
        out.clear();
        us.tick(&view, &brief.terrain, &world, &mut out);
        for (_, seat) in &mut others {
            match seat {
                Seat::Mind(m) => m.tick(&view, &brief.terrain, &world, &mut out),
                Seat::Charge(bot) => bot.tick(&view, &mut out),
                Seat::Stand => {}
            }
        }
        world.tick(&out);
        if world.tick_count().is_multiple_of(20) {
            view.capture(&world);
            held = match (crate::holding_the_field(&view), held) {
                (Some(p), Some((q, since))) if p == q => Some((q, since)),
                (Some(p), _) => Some((p, world.tick_count())),
                (None, _) => None,
            };
            if held.is_some_and(|(_, since)| world.tick_count() >= since + DECIDED) {
                break;
            }
        }
    }
    view.capture(&world);
    score(start, &view, party)
}

/// How the rehearsal went for `party`, from `start` to `end`: the share of
/// its strength it kept less the share the enemy kept, and the estimator's
/// view of the fight still to come.
pub fn score(start: &Perception, end: &Perception, party: PlayerId) -> f32 {
    let ours = |p: PlayerId| !start.hostile(party, p);
    let worth = |v: &Perception, mine: bool| -> f32 {
        v.regiments
            .iter()
            .filter(|r| ours(r.owner) == mine)
            .map(|r| {
                r.strength()
                    * match r.morale {
                        MoraleState::Steady => 1.0,
                        MoraleState::Wavering => WAVERING,
                        MoraleState::Routing => ROUTING,
                        MoraleState::Shattered => 0.0,
                    }
            })
            .sum()
    };
    let kept = |mine: bool| worth(end, mine) / worth(start, mine).max(1e-3);
    let material = kept(true) - kept(false);
    let standing =
        |mine: bool| end.regiments.iter().filter(move |r| ours(r.owner) == mine && !r.broken());
    let (a, b) = (Force::of(end, standing(true)), Force::of(end, standing(false)));
    let gap = standing(true)
        .flat_map(|r| standing(false).map(move |f| captain::gap_between(r, f)))
        .fold(f32::MAX, f32::min)
        .clamp(0.0, 200.0);
    let closing = (a.speed + b.speed).max(0.5) * 0.5;
    let beyond = estimator::estimate(&a, &b, Approach { gap, closing }).margin;
    material + LOOK_BEYOND * beyond
}

/// The temper a rehearsal gives an enemy it knows nothing about: every
/// trait and every doctrine in the middle.
fn middling() -> &'static Personality {
    static MIDDLING: OnceLock<Personality> = OnceLock::new();
    MIDDLING.get_or_init(|| Personality {
        name: "a sound commander".into(),
        faction: Faction::EmberClans,
        summary: String::new(),
        aggression: 50,
        caution: 50,
        patience: 50,
        cunning: 50,
        opportunism: 50,
        greed: 50,
        honour: 50,
        vengefulness: 50,
        doctrines: DoctrineLikes {
            hold_the_line: 50,
            hammer_and_anvil: 50,
            refused_flank: 50,
            skirmish_screen: 50,
            ambush: 50,
        },
        army: crate::army::Preset::Balanced,
    })
}

// Tuning knobs, read from the environment while tuning. TODO remove.
pub fn reading() -> bool {
    env("R_READ", 1.0) > 0.0
}
pub fn first() -> u32 {
    (env("R_FIRST", 0.0) * 20.0) as u32
}
fn env(name: &str, default: f32) -> f32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}
fn seeds() -> u64 {
    env("R_SEEDS", 6.0) as u64
}
fn stick() -> f32 {
    env("R_STICK", 0.1)
}
fn horizon() -> u32 {
    (env("R_HORIZON", 90.0) * 20.0) as u32
}
pub fn every() -> u32 {
    (env("R_EVERY", 10.0) * 20.0) as u32
}
pub fn once() -> bool {
    env("R_ONCE", 0.0) > 0.0
}
fn answers() -> Vec<Answer> {
    let a = std::env::var("R_ANSWERS").unwrap_or("S".into());
    let mut v = Vec::new();
    if a.contains('S') {
        v.push(Answer::Sound);
    }
    if a.contains('C') {
        v.push(Answer::Charge);
    }
    if a.contains('T') {
        v.push(Answer::Stand);
    }
    v
}
