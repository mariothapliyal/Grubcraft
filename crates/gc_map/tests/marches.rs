//! Armies marching across grown land: how close their marches come to the
//! quickest way there. A report, not a gate (yet):
//! `cargo test -p gc_map --release --test marches -- --ignored --nocapture`.
//!
//! Each march orders a mixed army of six regiments from one place to
//! another 50 to 90 tiles away, preferring places whose straight line runs
//! through woods (the case of an army routed through a forest). The ideal
//! is each regiment's quickest walk to its own spot, worked out cell by
//! cell from the ground's true pace (woods, soft ground, wading), at its
//! own speed; the march's score is the ideal over the time taken.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use gc_fixed::{Fx, FxVec2, Pcg32};
use gc_map::{FieldMap, Map};
use gc_sim::formation::{self, GroupMember, GroupOrder};
use gc_sim::nav::{BLOCKED, CELLS_PER_TILE};
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::{
    Capacities, Command, Cover, Mass, MatchSetup, PlayerCommand, RegimentHandle, UnitStats, World,
};

/// Infantry, archers, two infantry, heavy cavalry and a monster block.
const ARMY: [u16; 6] = [0, 2, 0, 4, 5, 3];
/// Two of them: an army the size of a player's in a big battle.
const BIG: [u16; 12] = [0, 2, 0, 4, 5, 3, 0, 2, 0, 4, 5, 3];
const TILES: u32 = 192;
/// 4 minutes.
const LIMIT: u32 = 20 * 240;

fn units() -> Vec<UnitStats> {
    let mut u = DRILL_UNITS.to_vec();
    // Heavy cavalry.
    u.push(UnitStats {
        models: 12,
        files: 6,
        file_spacing: Fx::from_ratio(6, 5),
        rank_spacing: Fx::from_ratio(8, 5),
        speed: Fx::from_ratio(11, 2),
        mass: Mass::Heavy,
        ..DRILL_UNITS[1]
    });
    // A big loosely spaced monster block.
    u.push(UnitStats {
        models: 16,
        files: 6,
        file_spacing: Fx::from_ratio(3, 2),
        rank_spacing: Fx::from_ratio(3, 2),
        speed: Fx::from_ratio(7, 2),
        mass: Mass::Heavy,
        ..DRILL_UNITS[1]
    });
    u
}

struct Field {
    map: Map,
    /// Nav costs with thicket blocked, for the ideal.
    nav: Vec<u8>,
    sim_nav: Vec<u8>,
    side: i32,
}

impl Field {
    fn grown(kind: usize, seed: u64) -> Field {
        let map = FieldMap {
            choice: gc_map::GROWN + 1 + kind,
            tiles: TILES,
            seed,
        }
        .make();
        let nav = map.nav_costs();
        let sim_nav = map.sim_nav_costs();
        let side = map.tiles as i32 * CELLS_PER_TILE;
        Field {
            map,
            nav,
            sim_nav,
            side,
        }
    }

    fn world(&self, units: &[UnitStats]) -> World {
        World::new(&MatchSetup {
            seed: 1,
            capacities: Capacities { regiments: 64 },
            units,
            map_tiles: self.map.tiles,
            nav: &self.sim_nav,
            teams: &[],
            ground: self.map.ground(),
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
        })
    }

    fn cell(&self, p: FxVec2) -> Option<usize> {
        let c = |v: Fx| (i64::from(v.raw()) * i64::from(CELLS_PER_TILE)) >> 16;
        let (x, y) = (c(p.x), c(p.y));
        let s = i64::from(self.side);
        (x >= 0 && y >= 0 && x < s && y < s).then(|| (y * s + x) as usize)
    }

    fn centre(&self, c: usize) -> FxVec2 {
        let (x, y) = (c as i32 % self.side, c as i32 / self.side);
        FxVec2::new(
            Fx::from_raw((2 * x + 1) << 14),
            Fx::from_raw((2 * y + 1) << 14),
        )
    }

    /// Whether a tile-sized square round `p` is all open (room for a block
    /// to stand).
    fn roomy(&self, p: FxVec2, tiles: i32) -> bool {
        let Some(c) = self.cell(p) else { return false };
        let (x, y) = (c as i32 % self.side, c as i32 / self.side);
        let r = tiles * CELLS_PER_TILE;
        for dy in -r..=r {
            for dx in -r..=r {
                let (a, b) = (x + dx, y + dy);
                if a < 0 || b < 0 || a >= self.side || b >= self.side {
                    return false;
                }
                if self.nav[(b * self.side + a) as usize] == BLOCKED {
                    return false;
                }
            }
        }
        true
    }

    /// Tiles of woods along the straight line from `a` to `b`.
    fn woods_between(&self, w: &World, a: FxVec2, b: FxVec2) -> i32 {
        let d = b - a;
        let n = d.length().ceil_int().max(1);
        (0..=n)
            .filter(|&k| {
                let p = a + d * Fx::from_ratio(k, n);
                w.terrain().cover(p) == Cover::Wood
            })
            .count() as i32
    }

    /// Seconds-in-hundredths of the quickest walk from `a` to `b` at one
    /// tile a second over the true pace of the ground for troops of `mass`
    /// (no turning, no berth): Dijkstra over the cells.
    fn ideal(&self, w: &World, a: FxVec2, b: FxVec2, mass: Mass) -> Option<u64> {
        self.ideal_path(w, a, b, mass).map(|(t, _)| t)
    }

    /// [`Field::ideal`] and the share of its cells in woods, percent.
    fn ideal_path(&self, w: &World, a: FxVec2, b: FxVec2, mass: Mass) -> Option<(u64, u32)> {
        let (Some(from), Some(to)) = (self.cell(a), self.cell(b)) else {
            return None;
        };
        let n = (self.side * self.side) as usize;
        // Time to cross half a tile straight, in ten-thousandths of a second at 1 tile/s.
        let pace: Vec<u32> = (0..n)
            .map(|c| {
                if self.nav[c] == BLOCKED {
                    return 0;
                }
                let p = self.centre(c);
                let step = w.terrain().going(Fx::ONE, p, FxVec2::ZERO, mass);
                (5000 * 65536 / i64::from(step.raw()).max(1)) as u32
            })
            .collect();
        let mut best = vec![u64::MAX; n];
        let mut from_cell = vec![usize::MAX; n];
        let mut heap = BinaryHeap::new();
        best[from] = 0;
        heap.push(Reverse((0u64, from)));
        while let Some(Reverse((t, c))) = heap.pop() {
            if c == to {
                let (mut cells, mut wooded, mut at) = (0u32, 0u32, c);
                while at != from && at != usize::MAX {
                    cells += 1;
                    wooded += u32::from(w.terrain().cover(self.centre(at)) == Cover::Wood);
                    at = from_cell[at];
                }
                return Some((t / 100, 100 * wooded / cells.max(1)));
            }
            if t > best[c] {
                continue;
            }
            let (x, y) = (c as i32 % self.side, c as i32 / self.side);
            for (dx, dy) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (-1, 1),
                (1, -1),
                (-1, -1),
            ] {
                let (a, b) = (x + dx, y + dy);
                if a < 0 || b < 0 || a >= self.side || b >= self.side {
                    continue;
                }
                let d = (b * self.side + a) as usize;
                if pace[d] == 0 {
                    continue;
                }
                let diag = dx != 0 && dy != 0;
                if diag
                    && (pace[(y * self.side + a) as usize] == 0
                        || pace[(b * self.side + x) as usize] == 0)
                {
                    continue;
                }
                let step = u64::from(pace[c] + pace[d]) / 2;
                let step = if diag { step * 1414 / 1000 } else { step };
                let nt = t + step;
                if nt < best[d] {
                    best[d] = nt;
                    from_cell[d] = c;
                    heap.push(Reverse((nt, d)));
                }
            }
        }
        None
    }
}

/// `None` where the ground has no room for it.
fn spawn(w: &mut World, unit: u16, at: FxVec2, facing: gc_fixed::Angle) -> Option<RegimentHandle> {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::SpawnRegiment {
            unit,
            position: at,
            facing,
        },
    }]);
    w.regiments
        .iter()
        .map(|r| r.handle)
        .find(|h| !before.contains(h))
}

#[derive(Debug, Default, Clone)]
pub struct March {
    /// Ticks until the last regiment arrived (LIMIT if one never did).
    pub ticks: u32,
    /// The slowest regiment's ideal, in ticks.
    pub ideal: u32,
    /// Regiments still marching at the limit.
    pub stuck: usize,
    /// Mean over regiments of ideal / taken, percent.
    pub score: u32,
    /// The slowest regiment's ideal over the ticks until the last arrived, percent.
    pub army: u32,
    /// Soldier-ticks in woods, percent of all soldier-ticks marching.
    pub in_woods: u32,
    /// Share of the ideal routes' soldier steps in woods, percent.
    pub ideal_woods: u32,
    /// Regiment-ticks in disorder.
    pub disordered: u32,
    /// Regiment-ticks standing still while ordered to march (waiting on a
    /// route, on a friend or turning).
    pub held: u32,
    /// Marching regiment-ticks by [full, slowed, stopped] x [in disorder,
    /// narrowed into a column, otherwise].
    pub ticks_by: [[u32; 3]; 3],
    /// Cohesion (docs/42): ticks from the first regiment's arrival to the
    /// last's, and regiment-ticks marching in disorder as a percent of all
    /// regiment-ticks marching.
    pub spread: u32,
    pub disorder_share: u32,
}

/// Pick the `k`th march on a field: a start where the army fits and a goal
/// 50 to 90 tiles away, preferring ones whose straight line crosses woods.
fn pick(field: &Field, w: &World, k: u64) -> Option<(FxVec2, FxVec2)> {
    let mut rng = Pcg32::new(k, 0x6d61_7263);
    let t = TILES as i32;
    let mut fallback = None;
    for _ in 0..400 {
        let a = FxVec2::from_ints(
            24 + rng.below((t - 48) as u32) as i32,
            24 + rng.below((t - 48) as u32) as i32,
        );
        let b = FxVec2::from_ints(
            24 + rng.below((t - 48) as u32) as i32,
            24 + rng.below((t - 48) as u32) as i32,
        );
        let d = (b - a).length();
        if d < Fx::from_int(50) || d > Fx::from_int(90) {
            continue;
        }
        if !field.roomy(a, 10) || !field.roomy(b, 10) {
            continue;
        }
        if field.ideal(w, a, b, Mass::Medium).is_none() {
            continue;
        }
        if field.woods_between(w, a, b) >= 8 {
            return Some((a, b));
        }
        fallback.get_or_insert((a, b));
    }
    fallback
}

pub fn march(kind: usize, seed: u64, k: u64) -> Option<March> {
    let field = Field::grown(kind, seed);
    let units = units();
    let mut w = field.world(&units);
    let (a, b) = pick(&field, &w, seed * 1000 + k)?;
    let facing = gc_fixed::Angle::of_vec(b - a);
    // Two rows of three, square to the way.
    let dir = (b - a) * (Fx::ONE / (b - a).length());
    let side = FxVec2::new(-dir.y, dir.x);
    let army: &[u16] = match std::env::var("ARMY").as_deref() {
        Ok("solo") => &ARMY[..1],
        Ok("cav") => &ARMY[4..5],
        Ok("mon") => &ARMY[5..6],
        Ok("back") => &ARMY[3..6],
        Ok("big") => &BIG,
        _ => &ARMY,
    };
    let handles: Vec<RegimentHandle> = army
        .iter()
        .enumerate()
        .map(|(n, &unit)| {
            let n = n as i32;
            let at = a + side * Fx::from_int((n % 3 - 1) * 9) - dir * Fx::from_int(n / 3 * 8);
            spawn(&mut w, unit, at, facing)
        })
        .collect::<Option<_>>()?;
    let mut members: Vec<GroupMember> = handles
        .iter()
        .map(|&h| {
            let r = w.regiments.get(h).unwrap();
            GroupMember::of(&r, &units[r.unit as usize])
        })
        .collect();
    let mut cmds = Vec::new();
    formation::group_move(
        &mut members,
        GroupOrder {
            target: b,
            facing: Some(facing),
            width: None,
            brigade: std::env::var("BRIGADE").ok().and_then(|v| v.parse().ok()).unwrap_or(0),
        },
        &mut cmds,
    );
    // Each regiment's ideal to its own spot.
    let mut ideal = vec![0u32; handles.len()];
    let mut pace_of = vec![Fx::ONE; handles.len()];
    let mut ideal_woods = (0u64, 0u64);
    for c in &cmds {
        if let Command::MoveTo {
            regiment,
            target,
            pace,
            ..
        } = c
        {
            let n = handles.iter().position(|h| h == regiment).unwrap();
            let r = w.regiments.get(*regiment).unwrap();
            let u = &units[r.unit as usize];
            let (t, wooded) = field
                .ideal_path(&w, r.position, *target, u.mass)
                .unwrap_or((0, 0));
            ideal_woods.0 += u64::from(wooded);
            ideal_woods.1 += 1;
            // hundredths of a second at 1 tile/s -> ticks at speed.
            // The group marches at its slowest regiment's pace.
            let speed = if pace.raw() > 0 {
                (*pace).min(u.speed)
            } else {
                u.speed
            };
            pace_of[n] = speed;
            ideal[n] = (t as i64 * 20 * 65536 / 100 / i64::from(speed.raw())) as u32;
        }
    }
    let orders: Vec<PlayerCommand> = cmds
        .into_iter()
        .map(|command| PlayerCommand { player: 0, command })
        .collect();
    let goals: Vec<(RegimentHandle, FxVec2)> = orders
        .iter()
        .filter_map(|c| match c.command {
            Command::MoveTo {
                regiment, target, ..
            } => Some((regiment, target)),
            _ => None,
        })
        .collect();
    w.tick(&orders);
    let mut out = March::default();
    let mut trace = Trace::start(&field, &w, &units, &handles, &goals);
    let mut arrived = vec![None; handles.len()];
    let mut last_pos: Vec<FxVec2> = handles
        .iter()
        .map(|&h| w.regiments.get(h).unwrap().position)
        .collect();
    let (mut woods, mut all) = (0u64, 0u64);
    let mut marching = 0u32;
    for t in 1..=LIMIT {
        w.tick(&[]);
        trace.frame(&w, &units, &handles);
        let mut moving = 0;
        for (n, &h) in handles.iter().enumerate() {
            let r = w.regiments.get(h).unwrap();
            if r.moving {
                // (Not yet: it had only halted on the way, as one waiting
                // its turn at a narrows does.)
                arrived[n] = None;
                moving += 1;
                marching += 1;
                all += u64::from(r.models);
                if w.terrain().cover(r.position) == Cover::Wood {
                    woods += u64::from(r.models);
                }
                if r.disorder > 0 {
                    out.disordered += 1;
                }
                if r.position == last_pos[n] {
                    out.held += 1;
                }
                // How far it got against how far the ground lets it go.
                let moved = (r.position - last_pos[n]).length();
                let u = &units[r.unit as usize];
                let dir = r.position - last_pos[n];
                let full = w.terrain().going(
                    pace_of[n] / 20,
                    last_pos[n],
                    if moved > Fx::ZERO {
                        dir * (Fx::ONE / moved)
                    } else {
                        FxVec2::ZERO
                    },
                    u.mass,
                );
                let kind = if moved * 10 >= full * 9 {
                    0
                } else if moved * 10 >= full {
                    1
                } else {
                    2
                };
                let why = if r.disorder > 0 {
                    0
                } else if r.files < r.frontage {
                    1
                } else {
                    2
                };
                out.ticks_by[kind][why] += 1;
            } else if arrived[n].is_none() {
                arrived[n] = Some(t);
            }
            last_pos[n] = r.position;
        }
        if std::env::var("TRACE").is_ok()
            && t % std::env::var("TRACE").unwrap().parse::<u32>().unwrap_or(40) == 0
        {
            let line: Vec<String> = handles
                .iter()
                .map(|&h| {
                    let r = w.regiments.get(h).unwrap();
                    if std::env::var("DEBUG_B").is_ok() {
                        return format!("{:?}", w.brigade_debug(h));
                    }
                    format!(
                        "({:3},{:3}) d{:2} f{}/{} w{}",
                        r.position.x.floor_int(),
                        r.position.y.floor_int(),
                        r.disorder,
                        r.files,
                        r.frontage,
                        w.route(h).len(),
                    )
                })
                .collect();
            println!("t{t:5} {}", line.join(" | "));
        }
        if std::env::var("AT").ok().and_then(|s| s.parse::<u32>().ok()) == Some(t) {
            for &h in &handles {
                let r = w.regiments.get(h).unwrap();
                let route: Vec<(i32, i32)> = w
                    .route(h)
                    .iter()
                    .map(|p| (p.x.floor_int(), p.y.floor_int()))
                    .collect();
                println!(
                    "AT {:?} facing {:?} files {} route {route:?}",
                    (
                        f64::from(r.position.x.raw()) / 65536.0,
                        f64::from(r.position.y.raw()) / 65536.0
                    ),
                    r.facing,
                    r.files
                );
            }
        }
        out.ticks = t;
        if moving == 0 && handles.iter().all(|&h| w.goal(h).is_none()) {
            break;
        }
    }
    trace.finish();
    if std::env::var("VERBOSE").is_ok() {
        for n in 0..handles.len() {
            println!(
                "   regiment {n} (unit {}): arrived {:?} ideal {}",
                army[n], arrived[n], ideal[n]
            );
        }
    }
    out.stuck = arrived.iter().filter(|a| a.is_none()).count();
    let first = arrived.iter().flatten().min().copied().unwrap_or(out.ticks);
    out.spread = out.ticks - first;
    out.disorder_share = 100 * out.disordered / marching.max(1);
    out.ideal = *ideal.iter().max().unwrap();
    let scores: Vec<u32> = (0..handles.len())
        .map(|n| {
            let taken = arrived[n].unwrap_or(LIMIT * 2).max(1);
            (100 * ideal[n] / taken).min(100)
        })
        .collect();
    out.score = scores.iter().sum::<u32>() / scores.len() as u32;
    out.army = 100 * out.ideal / out.ticks.max(1);
    out.in_woods = (100 * woods / all.max(1)) as u32;
    out.ideal_woods = (ideal_woods.0 / ideal_woods.1.max(1)) as u32;
    Some(out)
}

/// A recording of one march for pictures (TRACE_JSON=file): the ground,
/// each regiment's goal, and every regiment's block each tick.
struct Trace {
    out: Option<(String, String)>,
}

impl Trace {
    fn start(
        field: &Field,
        w: &World,
        units: &[UnitStats],
        handles: &[RegimentHandle],
        goals: &[(RegimentHandle, FxVec2)],
    ) -> Trace {
        let Ok(path) = std::env::var("TRACE_JSON") else {
            return Trace { out: None };
        };
        let f = |v: Fx| f64::from(v.raw()) / 65536.0;
        let mut s = String::new();
        s.push_str(&format!("{{\"tiles\":{},\"ground\":\"", field.map.tiles));
        for y in 0..field.map.tiles as i32 {
            for x in 0..field.map.tiles as i32 {
                let p = FxVec2::from_ints(x, y) + FxVec2::new(Fx::ONE / 2, Fx::ONE / 2);
                let c = field.cell(p).unwrap();
                s.push(if field.sim_nav[c] == BLOCKED {
                    '#'
                } else if w.terrain().cover(p) == Cover::Wood {
                    'w'
                } else if field.sim_nav[c] > 1 {
                    '~'
                } else {
                    '.'
                });
            }
        }
        s.push_str("\",\"goals\":[");
        for (n, h) in handles.iter().enumerate() {
            let g = goals.iter().find(|g| g.0 == *h).map(|g| g.1).unwrap();
            let r = w.regiments.get(*h).unwrap();
            let u = &units[r.unit as usize];
            s.push_str(&format!(
                "{}[{:.2},{:.2},{},{:.2}]",
                if n > 0 { "," } else { "" },
                f(g.x),
                f(g.y),
                r.unit,
                f(u.speed)
            ));
        }
        s.push_str("],\"frames\":[");
        Trace {
            out: Some((path, s)),
        }
    }

    fn frame(&mut self, w: &World, units: &[UnitStats], handles: &[RegimentHandle]) {
        let Some((_, s)) = &mut self.out else { return };
        let f = |v: Fx| f64::from(v.raw()) / 65536.0;
        if !s.ends_with('[') {
            s.push(',');
        }
        s.push('[');
        for (n, &h) in handles.iter().enumerate() {
            let r = w.regiments.get(h).unwrap();
            let u = &units[r.unit as usize];
            let (fs, rs) = u.spacing(r.formation);
            let (wd, dp) = formation::block_size(r.files, r.models, fs, rs);
            let v = r.facing.to_vec();
            // In a brigade: its place in the body, and whether it is in
            // step there.
            let place = w.brigade(h).and_then(|b| Some((b.place?, b)));
            let place = place.map_or(String::new(), |(p, b)| {
                format!(
                    ",{:.2},{:.2},{},{:.1},{}",
                    f(p.x),
                    f(p.y),
                    u8::from(b.in_step),
                    f(b.behind),
                    b.steering
                )
            });
            s.push_str(&format!(
                "{}[{:.2},{:.2},{:.3},{:.3},{:.2},{:.2},{},{}{place}]",
                if n > 0 { "," } else { "" },
                f(r.position.x),
                f(r.position.y),
                f(v.x),
                f(v.y),
                f(wd),
                f(dp),
                u8::from(r.disorder > 0),
                u8::from(r.moving)
            ));
        }
        s.push(']');
    }

    fn finish(self) {
        if let Some((path, mut s)) = self.out {
            s.push_str("]}");
            std::fs::write(path, s).unwrap();
        }
    }
}

#[test]
#[ignore = "a report: cargo test -p gc_map --release --test marches -- --ignored --nocapture"]
fn report_marches() {
    let only: Option<usize> = std::env::var("MARCHES").ok().and_then(|s| s.parse().ok());
    let mut army = 0;
    let mut by = [[0u32; 3]; 3];
    let (mut sum, mut n, mut stuck, mut woods, mut dis, mut held) = (0, 0, 0, 0, 0, 0);
    let (mut last, mut spread, mut share) = (0, 0, 0);
    let started = std::time::Instant::now();
    for kind in 0..3 {
        for seed in 1..=4u64 {
            for k in 0..2 {
                let id = kind * 100 + seed as usize * 10 + k as usize;
                if only.is_some_and(|o| o != id) {
                    continue;
                }
                let Some(m) = march(kind, seed, k) else {
                    println!("{id}: no march found");
                    continue;
                };
                println!(
                    "{id:4}: last {:5.1}s spread {:4.1}s disorder {:2}% | score {:3}% army {:3}% ticks {:5} ideal {:5} stuck {} woods {:2}% (ideal {:2}%) disordered {:5} held {:5}",
                    f64::from(m.ticks) / 20.0,
                    f64::from(m.spread) / 20.0,
                    m.disorder_share,
                    m.score,
                    m.army,
                    m.ticks,
                    m.ideal,
                    m.stuck,
                    m.in_woods,
                    m.ideal_woods,
                    m.disordered,
                    m.held
                );
                sum += m.score;
                last += m.ticks;
                spread += m.spread;
                share += m.disorder_share;
                army += m.army;
                n += 1;
                stuck += m.stuck;
                woods += m.in_woods;
                dis += m.disordered;
                held += m.held;
                for (row, add) in by.iter_mut().zip(m.ticks_by) {
                    for (cell, add) in row.iter_mut().zip(add) {
                        *cell += add;
                    }
                }
            }
        }
    }
    println!(
        "COHESION over {n} marches: last arrival {:.1}s, first to last {:.1}s, in disorder {}% of the march",
        f64::from(last) / 20.0 / f64::from(n.max(1)),
        f64::from(spread) / 20.0 / f64::from(n.max(1)),
        share / n.max(1)
    );
    println!(
        "MEAN score {}% army {}% over {n} marches, stuck {stuck}, woods {}%, disordered {}, held {} ({:.1}s)",
        sum / n.max(1),
        army / n.max(1),
        woods / n.max(1),
        dis / n.max(1),
        held / n.max(1),
        started.elapsed().as_secs_f64()
    );
    let total: u32 = by.iter().flatten().sum();
    for (a, name) in ["full", "slowed", "stopped"].iter().enumerate() {
        println!(
            "{name:8}: disorder {:4.1}% column {:4.1}% other {:4.1}%",
            100.0 * f64::from(by[a][0]) / f64::from(total),
            100.0 * f64::from(by[a][1]) / f64::from(total),
            100.0 * f64::from(by[a][2]) / f64::from(total)
        );
    }
}

#[test]
#[ignore = "a picture of one march: KIND=0 SEED=1 K=0 cargo test -p gc_map --release --test marches debug_route -- --ignored --nocapture"]
fn debug_route() {
    let kind: usize = std::env::var("KIND")
        .map(|s| s.parse().unwrap())
        .unwrap_or(0);
    let seed: u64 = std::env::var("SEED")
        .map(|s| s.parse().unwrap())
        .unwrap_or(4);
    let k: u64 = std::env::var("K").map(|s| s.parse().unwrap()).unwrap_or(0);
    let field = Field::grown(kind, seed);
    let units = units();
    let mut w = field.world(&units);
    let (a, b) = pick(&field, &w, seed * 1000 + k).unwrap();
    let h = spawn(&mut w, 0, a, gc_fixed::Angle::of_vec(b - a)).unwrap();
    w.tick(&[PlayerCommand {
        player: 0,
        command: Command::MoveTo {
            regiment: h,
            target: b,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            brigade: 0,
        },
    }]);
    for _ in 0..3 {
        w.tick(&[]);
    }
    let route: Vec<(i32, i32)> = w
        .route(h)
        .iter()
        .map(|p| (p.x.floor_int(), p.y.floor_int()))
        .collect();
    println!(
        "from {:?} to {:?}: {route:?}",
        (a.x.floor_int(), a.y.floor_int()),
        (b.x.floor_int(), b.y.floor_int())
    );
    let wooded = (0..TILES * TILES)
        .filter(|&i| {
            w.terrain()
                .cover(FxVec2::from_ints((i % TILES) as i32, (i / TILES) as i32))
                == Cover::Wood
        })
        .count();
    println!("wooded tiles {wooded}");
    // ASCII around the route.
    let (x0, x1) = (a.x.min(b.x).floor_int() - 10, a.x.max(b.x).floor_int() + 10);
    let (y0, y1) = (a.y.min(b.y).floor_int() - 10, a.y.max(b.y).floor_int() + 10);
    for y in y0..y1 {
        let mut line = String::new();
        for x in x0..x1 {
            let p = FxVec2::from_ints(x, y) + FxVec2::new(Fx::ONE / 2, Fx::ONE / 2);
            let mut pts = vec![a];
            pts.extend_from_slice(w.route(h));
            let on = pts.windows(2).any(|s| {
                let d = s[1] - s[0];
                let n = d.length().ceil_int().max(1) * 2;
                (0..=n).any(|k| {
                    let q = s[0] + d * Fx::from_ratio(k, n);
                    q.x.floor_int() == x && q.y.floor_int() == y
                })
            });
            let c = field.cell(p).unwrap();
            line.push(if x == a.x.floor_int() && y == a.y.floor_int() {
                'S'
            } else if on {
                '*'
            } else if field.nav[c] == BLOCKED {
                '#'
            } else if w.terrain().cover(p) == Cover::Wood {
                'w'
            } else if field.nav[c] == 3 {
                '~'
            } else {
                '.'
            });
        }
        println!("{line}");
    }
}
