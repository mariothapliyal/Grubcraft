//! Six walled towns, one a side, each with a Gate in its south wall that
//! only its own side may pass, and a regiment of each marching out of its
//! town and home again twice, so every gate swings again and again: what a
//! six-player match of Generals does all match long (docs/12-performance.md,
//! "Match speed").
//!
//! From its first gate each side walks a grid of its own. They must share
//! all but the cells round their own gates (six whole copies were most of
//! a six-player match's memory), and a gate swinging must settle the
//! walking areas near it rather than across the map.

use gc_platform::tagged_alloc::{self, Tag};
use gc_sim::{PlayerCommand, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, Real, WEST, centre, move_to, of, put_up, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "gated-towns",
    about: "six walled towns, each side's regiment marching out of its own Gate and home twice",
    budget_ms: 20_000,
    run,
};

const TILES: u32 = 192;
const SIDES: usize = 6;
/// Each town's walls run round a square this many tiles across.
const TOWN: i32 = 24;
/// Each march (out or home) must be done within this long.
const LIMIT_SECONDS: u32 = 60;
/// Marches: out, home, out, home.
const MARCHES: usize = 4;

/// Town `k`'s north-west corner: two rows of three.
fn corner(k: usize) -> (i32, i32) {
    (16 + 60 * (k as i32 % 3), 20 + 90 * (k as i32 / 3))
}

fn run(real: &Real, report: &mut Report) {
    let mut world = tagged_alloc::scope(Tag::Sim, || {
        Arena::new(TILES).world_of(real, 7, 0, gc_sim::Sky::STILL, SIDES)
    });
    let (palisade, gate) = (real.building("Palisade"), real.building("Gate"));
    let stats = &real.table.stats[usize::from(gate)];
    let gate_w = i32::from(stats.width.max(stats.depth));
    let [yari, ..] = real.ember_line();
    let mut armies: Vec<RegimentHandle> = Vec::new();
    let mut gates = Vec::new();
    // Where each side's regiment stands inside its town, and outside it.
    let mut home = Vec::new();
    let mut away = Vec::new();
    for k in 0..SIDES {
        let (x0, y0) = corner(k);
        let (x1, y1) = (x0 + TOWN - 1, y0 + TOWN - 1);
        let gx = x0 + (TOWN - gate_w) / 2;
        let ring: Vec<_> = (x0..=x1)
            .flat_map(|x| [(x, y0), (x, y1)])
            .chain((y0 + 1..y1).flat_map(|y| [(x0, y), (x1, y)]))
            .filter(|&(x, y)| !(y == y1 && (gx..gx + gate_w).contains(&x)))
            .map(|t| (palisade, t, 0))
            .collect();
        let p = k as u8;
        tagged_alloc::scope(Tag::Sim, || put_up(&mut world, p, &ring));
        gates.push((p, (gx, y1)));
        let inside = centre(gx + gate_w / 2, y0 + TOWN / 2);
        let outside = centre(gx + gate_w / 2, y1 + 16);
        let r = tagged_alloc::scope(Tag::Sim, || spawn(&mut world, p, &[(yari, inside)], WEST));
        armies.extend(r);
        home.push(inside);
        away.push(outside);
    }
    // Every wall up, with a gap for its gate: one grid for all. Then the
    // gates, and with them a grid for each side.
    let one_grid = tagged_alloc::live_bytes(Tag::Sim);
    for &(p, tile) in &gates {
        tagged_alloc::scope(Tag::Sim, || put_up(&mut world, p, &[(gate, tile, 1)]));
    }
    let mut swings = Vec::new();
    let mut swung_gates = 0;
    let mut slowest = 0.0f64;
    let mut done_all = true;
    for m in 0..MARCHES {
        let to = if m % 2 == 0 { &away } else { &home };
        let orders: Vec<PlayerCommand> = (0..SIDES)
            .map(|k| move_to(k as u8, armies[k], to[k]))
            .collect();
        let mut t = 0;
        let mut cmds = orders;
        let arrived =
            |w: &World| of(w, &armies).count() == SIDES && of(w, &armies).all(|v| !v.moving);
        loop {
            let open_before = gates_open(&world);
            let n = report.times.len();
            tagged_alloc::scope(Tag::Sim, || report.tick(&mut world, &cmds));
            cmds.clear();
            t += 1;
            let open_now = gates_open(&world);
            let swung = open_now
                .iter()
                .zip(&open_before)
                .filter(|(a, b)| a != b)
                .count();
            if swung > 0 {
                swung_gates += swung;
                swings.push(report.times[n]);
                slowest = slowest.max(report.times[n]);
            }
            if t > 2 && arrived(&world) {
                break;
            }
            if t > LIMIT_SECONDS * TICKS_PER_SECOND {
                done_all = false;
                break;
            }
        }
        // Each regiment where it was sent.
        let near = of(&world, &armies)
            .zip(to)
            .all(|(v, &p)| (v.position - p).length() < gc_fixed::Fx::from_int(6));
        report.check(
            format!(
                "march {} done within {LIMIT_SECONDS} s, every regiment where sent",
                m + 1
            ),
            done_all && near,
        );
    }
    let six_grids = tagged_alloc::live_bytes(Tag::Sim);
    let cells = (TILES * gc_sim::nav::CELLS_PER_TILE as u32).pow(2) as usize;
    swings.sort_by(f64::total_cmp);
    report.metric(
        "gate swings, and ticks they swung in",
        format!("{swung_gates}, {}", swings.len()),
    );
    report.metric(
        "gate swing tick, typical and slowest",
        format!(
            "{:.2} ms, {slowest:.2} ms",
            swings.get(swings.len() / 2).unwrap_or(&0.0)
        ),
    );
    report.metric(
        "sim heap, walled towns on one grid, then gated on six sides' grids",
        format!("{:.2} MB, {:.2} MB", mb(one_grid), mb(six_grids)),
    );
    // Out and home, each gate opens and shuts at least once a march.
    report.check("every gate swung", swung_gates >= 2 * SIDES * MARCHES);
    // Six grids that shared nothing would cost five more whole grids'
    // cells (six bytes a cell, and the portal graphs); shared, they cost
    // the cells round six gates and their own portal graphs.
    report.check(
        "six sides' grids cost less than one more grid's cells",
        six_grids.saturating_sub(one_grid) < 6 * cells,
    );
}

/// Whether each building stands open, in slot order.
fn gates_open(world: &World) -> Vec<bool> {
    world
        .structures()
        .map_or(Vec::new(), |s| s.iter().map(|v| v.open).collect())
}

fn mb(bytes: usize) -> f64 {
    bytes as f64 / 1_048_576.0
}
