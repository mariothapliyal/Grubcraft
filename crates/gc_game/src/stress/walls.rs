//! A General plays the Ember Clans against an Ember base walled all round
//! with a Palisade and a Gate, on Ribcage Pass: it has to get its army
//! through or over the wall, the way a player would.

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_sim::structures::{Role, StructureView};
use gc_sim::{BuildingStats, TICKS_PER_SECOND, Victories, World};

use super::arena::{Real, put_up};
use super::{Report, Scenario};
use crate::conquest;
use crate::headless::Headless;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "ai-walls",
    about: "a General attacks a base walled all round with a Palisade and a Gate",
    budget_ms: 15_000,
    run,
};

const SEED: u64 = 0x57A1_1500;
/// Ribcage Pass, a one-on-one.
const MAP: usize = 0;
const FACTIONS: [Faction; 2] = [Faction::EmberClans, Faction::EmberClans];
/// How far out from the main building the wall may run, in tiles.
const GAPS: core::ops::RangeInclusive<i32> = 3..=12;
const LIMIT_MINUTES: u32 = 10;

fn run(_: &Real, report: &mut Report) {
    // A shipped map: it keeps its own size whatever size is asked for.
    let game = Headless::new(conquest::make_map(MAP, 0, SEED), &FACTIONS, SEED);
    let victories = Victories {
        annihilation: true,
        heart: false,
    };
    let mut world = game.world(&game.plan(gc_ui::lobby::Tempo::Standard, victories));
    game.open(&mut world, |w, orders| report.tick(w, orders));

    // Wall the defender's main building all round, the Gate on the side
    // facing the attacker, at the nearest distance where the whole ring
    // can go up (seeps and rough ground leave no gap in it).
    let main = (world.structures().unwrap().iter())
        .find(|s| s.owner == 0 && game.table.stats[usize::from(s.building)].role == Role::Main)
        .expect("the defender's main building");
    let foe = game.map.starts()[1];
    let palisade = game.table.named("Palisade");
    let gate = game.table.named("Gate");
    let ring = (GAPS)
        .map(|gap| Ring::round(&main, &game.table.stats[usize::from(gate)], gap, foe))
        .find(|r| r.fits(&world, palisade, gate))
        .expect("somewhere a closed ring fits");
    // In one tick, so nothing walks onto a tile between the check and
    // the building.
    let mut plan: Vec<_> = ring.tiles().map(|t| (palisade, t, 0)).collect();
    plan.push((gate, ring.gate, ring.turn));
    let pieces = put_up(&mut world, 0, &plan);
    let [x0, y0, x1, y1] = ring.corners;
    report.metric("wall pieces (gate included)", pieces.len());
    report.metric("tiles from the main building to the wall", ring.gap);

    let mut minds = game.minds();
    minds.add(game.mind(1, Difficulty::General, 7, None));

    let inside = |w: &World| {
        (w.regiments.iter()).any(|r| {
            let (x, y) = (r.position.x.floor_int(), r.position.y.floor_int());
            r.owner == 1 && (x0 < x && x < x1) && (y0 < y && y < y1)
        })
    };
    let wall_hp = |w: &World| -> (u64, usize) {
        (w.structures().unwrap().iter())
            .filter(|v| pieces.contains(&v.handle))
            .fold((0, 0), |(hp, n), v| (hp + u64::from(v.hp), n + 1))
    };
    let (full_hp, _) = wall_hp(&world);
    let (mut hurt, mut breached, mut entered) = (None, None, None);
    let mut cmds = Vec::new();
    let end = LIMIT_MINUTES * 60 * TICKS_PER_SECOND;
    while world.verdict().is_none() && world.tick_count() < end {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        report.tick(&mut world, &cmds);
        let t = world.tick_count();
        if !t.is_multiple_of(TICKS_PER_SECOND) {
            continue;
        }
        let (hp, standing) = wall_hp(&world);
        if hurt.is_none() && hp < full_hp {
            hurt = Some(t);
        }
        if breached.is_none() && standing < pieces.len() {
            breached = Some(t);
        }
        if entered.is_none() && inside(&world) {
            entered = Some(t);
        }
        // All that is checked is known: no need to play on.
        if hurt.is_some() && breached.is_some() && entered.is_some() {
            break;
        }
    }
    let at = |t: Option<u32>| {
        t.map_or("never".into(), |t| {
            let s = t / TICKS_PER_SECOND;
            format!("{}:{:02}", s / 60, s % 60)
        })
    };
    let (hp, standing) = wall_hp(&world);
    report.metric("game time played", at(Some(world.tick_count())));
    report.metric("first blow on the wall", at(hurt));
    report.metric("first piece knocked down", at(breached));
    report.metric("first attacker inside", at(entered));
    report.metric(
        "wall pieces still standing",
        format!("{standing}/{}", pieces.len()),
    );
    report.metric(
        "wall hit points left",
        format!("{}%", hp * 100 / full_hp.max(1)),
    );
    report.check(
        format!("the General attacks the wall within {LIMIT_MINUTES} minutes"),
        hurt.is_some(),
    );
    report.check(
        format!("the General gets inside the wall within {LIMIT_MINUTES} minutes"),
        entered.is_some(),
    );
}

/// A wall right round a building: its corner tiles (west, north, east,
/// south), and its Gate's north-west tile, turn and turned footprint.
struct Ring {
    gap: i32,
    corners: [i32; 4],
    gate: (i32, i32),
    turn: u8,
    gate_size: (i32, i32),
}

impl Ring {
    /// The ring `gap` tiles out from `main`, its Gate (`gate`, its
    /// unturned stats, run north to south) in the middle of the side
    /// facing `foe`: unturned along a west or east side, turned a quarter
    /// along the north or south side.
    fn round(main: &StructureView, gate: &BuildingStats, gap: i32, foe: (i32, i32)) -> Ring {
        let (w, d) = (i32::from(gate.width), i32::from(gate.depth));
        let (x0, y0) = (main.tile.0 - gap, main.tile.1 - gap);
        let x1 = main.tile.0 + i32::from(main.width) - 1 + gap;
        let y1 = main.tile.1 + i32::from(main.depth) - 1 + gap;
        let (dx, dy) = (foe.0 - main.tile.0, foe.1 - main.tile.1);
        let (at, turn, gate_size) = if dx.abs() >= dy.abs() {
            let x = if dx > 0 { x1 + 1 - w } else { x0 };
            ((x, (y0 + y1 + 1 - d) / 2), 0, (w, d))
        } else {
            let y = if dy > 0 { y1 + 1 - w } else { y0 };
            (((x0 + x1 + 1 - d) / 2, y), 1, (d, w))
        };
        Ring {
            gap,
            corners: [x0, y0, x1, y1],
            gate: at,
            turn,
            gate_size,
        }
    }

    fn in_gate(&self, (x, y): (i32, i32)) -> bool {
        let (w, d) = self.gate_size;
        (self.gate.0..self.gate.0 + w).contains(&x) && (self.gate.1..self.gate.1 + d).contains(&y)
    }

    /// The ring's wall tiles, the Gate's left out.
    fn tiles(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        let [x0, y0, x1, y1] = self.corners;
        (x0..=x1)
            .flat_map(move |x| [(x, y0), (x, y1)])
            .chain((y0 + 1..y1).flat_map(move |y| [(x0, y), (x1, y)]))
            .filter(|&t| !self.in_gate(t))
    }

    /// Whether every piece of it can go up in `world`.
    fn fits(&self, world: &World, wall: u16, gate: u16) -> bool {
        self.tiles().all(|t| world.can_build(wall, t, 0).is_ok())
            && world.can_build(gate, self.gate, self.turn).is_ok()
    }
}
