//! Reinforcement in real use (docs/04-economy.md, "Reinforcement"), on the
//! real game data. Four armies, one of each faction, fight in pairs well
//! away from any building, then march home to their own side's main
//! building and are refilled there, paying Grub: never while fighting, never
//! the undead (refilled only by a Reliquary, and none stands here), every
//! hurt regiment that can be refilled back to full strength. Then two
//! Generals play a Conquest match and each is seen to reinforce its
//! regiments.

use std::collections::HashMap;

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::{MoraleState, PlayerCommand, RegimentHandle, TICKS_PER_SECOND, Victories, World};

use super::arena::{
    Arena, EAST, Real, WEST, attack_nearest, centre, move_to, put_up, soldiers, spawn,
};
use super::{Report, Scenario, clock, seconds};
use crate::conquest;
use crate::headless::Headless;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "reinforce",
    about: "four armies fight, march home and are refilled; two Generals reinforce in a match",
    budget_ms: 60_000,
    run,
};

const TILES: u32 = 128;
const PLAYERS: usize = 4;
/// How long the armies fight, and how long they have to march home and be
/// refilled, in seconds.
const FIGHT_SECONDS: u32 = 6;
const REFILL_SECONDS: u32 = 150;
/// When the survivors that broke have rallied, and are sent home again.
const RALLIED_SECONDS: u32 = 40;
/// The Conquest matches played, one after another until each General has
/// reinforced in one of them (a match goes its own way with any change to
/// how battles play out), the map (Four Teeth) and the time each has.
const SEEDS: [u64; 3] = [105, 104, 106];
const MAP: usize = 1;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::Brood];
const LIMIT_MINUTES: u32 = 20;

/// A main building, its tile and the piece put up beside it, if any.
type Home = (&'static str, Tile, Option<(&'static str, Tile)>);
/// A regiment's two units and the tiles they stand on.
type Army = ([&'static str; 2], [Tile; 2]);
type Tile = (i32, i32);

/// Each player's main building, the tile it is put up on, and the pieces
/// put up with it: the Choir's Resonator and the Concord's Guildhall stand
/// beside the main building.
const HOMES: [Home; PLAYERS] = [
    ("Clan Hall", (14, 14), None),
    ("Hive", (108, 14), None),
    ("Spire", (14, 108), Some(("Resonator", (24, 108)))),
    ("Counting House", (108, 108), Some(("Guildhall", (98, 108)))),
];
/// Each player's two regiments' units, the tiles they fight on and the
/// way they face; the pairs are player 0 against 1 and 2 against 3.
const ARMIES: [Army; PLAYERS] = [
    (["Yari Ashigaru", "Katana Samurai"], [(54, 36), (54, 50)]),
    (["Skitterlings", "Chitin Guard"], [(74, 36), (74, 50)]),
    (["Shardlings", "Prism Lancers"], [(54, 78), (54, 92)]),
    (["Crossbow Auditors", "Bone Pikes"], [(74, 78), (74, 92)]),
];
/// Where each army's two regiments set out for home from, as seen from
/// the main building: they march to the spot its owner's side offers
/// for a regiment coming from there, a few tiles out from the building
/// on the side facing it, so the two stand on different sides.
const HOME_FROM: [[(i32, i32); 2]; PLAYERS] = [
    [(64, 14), (14, 64)],
    [(64, 14), (108, 64)],
    [(64, 108), (14, 64)],
    [(64, 108), (108, 64)],
];

fn run(real: &Real, report: &mut Report) {
    refill(real, report);
    matches(real, report);
}

/// What each regiment looked like at the start of a tick.
fn snapshot(world: &World) -> HashMap<RegimentHandle, (u16, bool)> {
    (world.regiments.iter())
        .map(|r| (r.handle, (r.models, r.engaged)))
        .collect()
}

/// What the regiments gained over the ticks played.
#[derive(Default)]
struct Tally {
    /// Soldiers gained, by player.
    gained: [u32; PLAYERS],
    /// Regiments that gained a soldier on a tick they were fighting.
    fighting: u32,
    /// The undead regiment watched, and the soldiers it gained.
    pikes: Option<RegimentHandle>,
    pikes_gained: u32,
}

impl Tally {
    /// Tick `world` (timed), comparing every regiment with how it stood at
    /// the start of the tick: a soldier gained is a soldier reinforced.
    fn tick(&mut self, world: &mut World, report: &mut Report, orders: &[PlayerCommand]) {
        let before = snapshot(world);
        report.tick(world, orders);
        for r in world.regiments.iter() {
            let Some(&(models, engaged)) = before.get(&r.handle) else {
                continue;
            };
            if r.models > models {
                let n = u32::from(r.models - models);
                self.gained[usize::from(r.owner)] += n;
                self.fighting += u32::from(engaged || r.engaged);
                if Some(r.handle) == self.pikes {
                    self.pikes_gained += n;
                }
            }
        }
    }
}

/// The part played on the arena.
fn refill(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world_of(real, 17, 3000, gc_sim::Sky::STILL, PLAYERS);
    for (p, &(main, at, beside)) in HOMES.iter().enumerate() {
        let mut pieces = vec![(real.building(main), at, 0)];
        pieces.extend(beside.map(|(name, at)| (real.building(name), at, 0)));
        put_up(&mut world, p as u8, &pieces);
    }
    // Each army's regiments, in the order of `ARMIES`, and who they fight.
    let mut army: Vec<Vec<RegimentHandle>> = Vec::new();
    for (p, (units, at)) in ARMIES.iter().enumerate() {
        let facing = [EAST, WEST][p % 2];
        let placed: Vec<_> = (0..2)
            .map(|i| (real.unit(units[i]), centre(at[i].0, at[i].1)))
            .collect();
        army.push(spawn(&mut world, p as u8, &placed, facing));
    }
    let all: Vec<RegimentHandle> = army.iter().flatten().copied().collect();
    let full = |world: &World, h: RegimentHandle| {
        let v = world.regiments.get(h).unwrap();
        real.units[usize::from(v.unit)].models
    };
    let full_at_start: HashMap<RegimentHandle, u16> =
        all.iter().map(|&h| (h, full(&world, h))).collect();
    let pikes = Some(army[3][1]);
    let foe = |p: usize| p ^ 1;

    // Every tick the regiments are compared with how they stood at its
    // start: a soldier gained is a soldier reinforced, wrong in a fight.
    let mut tally = Tally {
        pikes,
        ..Tally::default()
    };

    // The fight: both sides attack the nearest foe, again every two
    // seconds, for long enough to be hurt and not long enough to break.
    for t in 0..FIGHT_SECONDS * TICKS_PER_SECOND {
        let mut orders = Vec::new();
        if t % (2 * TICKS_PER_SECOND) == 0 {
            for (p, mine) in army.iter().enumerate() {
                orders.extend(attack_nearest(&world, p as u8, mine, foe(p) as u8));
            }
        }
        tally.tick(&mut world, report, &orders);
    }
    let lost: Vec<u32> = army
        .iter()
        .map(|mine| {
            let full: u32 = mine.iter().map(|&h| u32::from(full_at_start[&h])).sum();
            full - soldiers(&world, mine)
        })
        .collect();
    report.metric("soldiers lost in the fight, by player", format!("{lost:?}"));

    // Home: every survivor marches to a place before its main building.
    // Those hurt, able to be refilled and not broken as they set out are
    // owed their soldiers; a regiment that broke stops where it rallies,
    // its march forgotten, so the survivors are sent home again once the
    // rout is over, and those that rallied are owed theirs from then.
    let march_home = |world: &World| -> Vec<PlayerCommand> {
        let mut orders = Vec::new();
        for (p, mine) in army.iter().enumerate() {
            for (i, &h) in mine.iter().enumerate() {
                if world.regiments.get(h).is_none() {
                    continue;
                }
                let (x, y) = HOME_FROM[p][i];
                let spot = world.reinforce_spot(p as u8, centre(x, y)).unwrap();
                orders.push(move_to(p as u8, h, spot));
            }
        }
        orders
    };
    let hurt_and_steady = |world: &World| -> Vec<RegimentHandle> {
        (all.iter().copied())
            .filter(|&h| {
                world.regiments.get(h).is_some_and(|v| {
                    v.models < full(world, h)
                        && !matches!(v.morale, MoraleState::Routing | MoraleState::Shattered)
                }) && world.can_reinforce(h)
                    // Somewhere to be refilled: not the undead, with no
                    // Reliquary here.
                    && world.refill_spot(h).is_some()
            })
            .collect()
    };
    let grub = |world: &World| -> Vec<u32> {
        (0..PLAYERS as u8)
            .map(|p| world.bank(p).unwrap().grub)
            .collect()
    };
    let mut owed = hurt_and_steady(&world);
    let owed_models: Vec<u32> = owed
        .iter()
        .map(|&h| u32::from(full(&world, h) - world.regiments.get(h).unwrap().models))
        .collect();
    let (grub_before, gained_before) = (grub(&world), tally.gained);
    let homeward = march_home(&world);
    report.metric(
        "regiments surviving, and owed soldiers at the march home",
        format!("{} of {}, {owed_models:?}", homeward.len(), all.len()),
    );
    let mut last_full = 0;
    tally.tick(&mut world, report, &homeward);
    for t in 1..=REFILL_SECONDS * TICKS_PER_SECOND {
        if t == RALLIED_SECONDS * TICKS_PER_SECOND {
            for h in hurt_and_steady(&world) {
                if !owed.contains(&h) {
                    owed.push(h);
                }
            }
            let orders = march_home(&world);
            tally.tick(&mut world, report, &orders);
        } else {
            tally.tick(&mut world, report, &[]);
        }
        let short = owed.iter().any(|&h| {
            (world.regiments.get(h))
                .is_some_and(|v| v.models < full(&world, h) && !v.morale.broken())
        });
        if short {
            last_full = t;
        }
    }
    let grub_after = grub(&world);

    let refilled: Vec<u32> = (0..PLAYERS)
        .map(|p| tally.gained[p] - gained_before[p])
        .collect();
    let spent: Vec<u32> = (0..PLAYERS)
        .map(|p| grub_before[p].saturating_sub(grub_after[p]))
        .collect();
    report.metric("soldiers regained, by player", format!("{refilled:?}"));
    report.metric("Grub spent, by player", format!("{spent:?}"));
    report.metric(
        "seconds from the march home to the last regiment full",
        seconds(Some(last_full + TICKS_PER_SECOND)),
    );
    let unfilled: Vec<_> = owed
        .iter()
        .filter(|&&h| {
            world
                .regiments
                .get(h)
                .is_some_and(|v| v.models < full(&world, h))
        })
        .map(|&h| {
            let v = world.regiments.get(h).unwrap();
            format!(
                "player {} unit {} {}/{} {:?}",
                v.owner,
                v.unit,
                v.models,
                full(&world, h),
                v.morale
            )
        })
        .collect();
    report.metric(
        "owed regiments not full at the end",
        format!("{unfilled:?}"),
    );

    report.check(
        "no regiment gains a soldier while it is fighting",
        tally.fighting == 0,
    );
    report.check(
        "every hurt regiment that can be refilled, and was not routing, is full again",
        !owed.is_empty() && unfilled.is_empty(),
    );
    let pikes_short =
        pikes.is_some_and(|h| (world.regiments.get(h)).is_some_and(|v| v.models < full(&world, h)));
    report.metric("Bone Pikes still short at the end", pikes_short);
    report.check(
        "the undead Bone Pikes, hurt and at home, gain no soldier",
        pikes_short && tally.pikes_gained == 0,
    );
    let paid = |p: usize| refilled[p] == 0 || spent[p] > 0;
    report.check(
        "each player's Grub goes down as its regiments refill",
        (0..PLAYERS).all(paid) && refilled.iter().any(|&n| n > 0),
    );
}

/// The Conquest matches: until each General has reinforced soldiers in one
/// of them.
fn matches(real: &Real, report: &mut Report) {
    let mut done = [false; FACTIONS.len()];
    let mut played = 0;
    for seed in SEEDS {
        let reinforced = play(real, report, seed);
        played += 1;
        for (d, n) in done.iter_mut().zip(reinforced) {
            *d |= n > 0;
        }
        if done.iter().all(|&d| d) {
            break;
        }
    }
    report.metric("Conquest matches played", played);
    report.check(
        format!(
            "each General reinforces a soldier within {LIMIT_MINUTES} minutes, in one of {} matches",
            SEEDS.len()
        ),
        done.iter().all(|&d| d),
    );
}

/// Play the match of `seed` for the whole limit, nobody winning: the
/// soldiers each player reinforced, seen as a rise in the soldiers of a
/// regiment from one second to the next.
fn play(real: &Real, report: &mut Report, seed: u64) -> [u32; FACTIONS.len()] {
    let game = Headless::new(real, conquest::make_map(MAP, 0, seed), &FACTIONS, seed);
    let victories = Victories {
        annihilation: false,
        heart: false,
    };
    let mut world = game.world(&game.plan(gc_ui::lobby::Tempo::Standard, victories));
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    for p in 0..FACTIONS.len() {
        minds.add(game.mind(p, Difficulty::General, 7, None));
    }
    let mut seen: HashMap<RegimentHandle, u16> = HashMap::new();
    let mut reinforced = [0u32; FACTIONS.len()];
    let mut cmds = Vec::new();
    let end = LIMIT_MINUTES * 60 * TICKS_PER_SECOND;
    while world.verdict().is_none() && world.tick_count() < end {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        report.tick(&mut world, &cmds);
        if !world.tick_count().is_multiple_of(TICKS_PER_SECOND) {
            continue;
        }
        let mut now = HashMap::with_capacity(seen.len());
        for r in world.regiments.iter() {
            if let Some(&was) = seen.get(&r.handle)
                && r.models > was
                && let Some(n) = reinforced.get_mut(usize::from(r.owner))
            {
                *n += u32::from(r.models - was);
            }
            now.insert(r.handle, r.models);
        }
        seen = now;
    }
    report.metric(
        format!("match {seed}, game time played"),
        clock(Some(world.tick_count())),
    );
    for (p, faction) in FACTIONS.iter().enumerate() {
        report.metric(
            format!("match {seed}, {faction:?} soldiers reinforced"),
            reinforced[p],
        );
    }
    reinforced
}
