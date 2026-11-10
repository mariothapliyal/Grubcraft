//! A watching aid for the computer players' walls: play Conquest matches
//! with no window and print each base as a map of letters, with what is
//! wrong with its walls counted. Run by hand:
//!
//! ```text
//! AI_MAP=0 AI_SEED=7 AI_MINUTES=30 cargo test -p gc_game --release wall_survey -- --ignored --nocapture
//! ```

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_fixed::{Fx, FxVec2};
use gc_sim::structures::{GATE, LINE, Role};
use gc_sim::{PlayerId, TICKS_PER_SECOND, World};
use gc_ui::lobby::Tempo;

use crate::conquest;
use crate::headless::{Headless, Real};

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Player `p`'s base as letters, `reach` tiles each way from its main
/// building: `#` wall, `G` gate, `M` main, `B` building, `T` tower, `o`
/// seep or vein, `^` ground nobody walks, `~` water, `w` woods a wall
/// can't go on, `u` a wall tile going up, `.` open.
pub(crate) fn base_map(world: &World, real: &Real, p: PlayerId, reach: i32) -> Option<String> {
    let all = world.structures()?;
    let stats = |b: u16| &real.table.stats[usize::from(b)];
    let main = all
        .iter()
        .find(|s| s.owner == p && stats(s.building).role == Role::Main)?;
    let (cx, cy) = (main.tile.0 + 2, main.tile.1 + 2);
    let side = (2 * reach + 1) as usize;
    let mut grid = vec![b'.'; side * side];
    let at = |x: i32, y: i32| {
        let (dx, dy) = (x - cx + reach, y - cy + reach);
        ((0..side as i32).contains(&dx) && (0..side as i32).contains(&dy))
            .then(|| dy as usize * side + dx as usize)
    };
    let nav = world.nav();
    let terrain = world.terrain();
    for y in cy - reach..=cy + reach {
        for x in cx - reach..=cx + reach {
            let c = FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF);
            let i = at(x, y).unwrap();
            if terrain.wet(c) {
                grid[i] = b'~';
            } else if !nav.walkable(p, c) {
                grid[i] = b'^';
            } else if world.overgrown([x, y, 1, 1]) {
                grid[i] = b'w';
            }
        }
    }
    for s in all.iter() {
        let st = stats(s.building);
        let mine = s.owner == p;
        let c = if st.flags & GATE != 0 {
            b'G'
        } else if st.flags & LINE != 0 {
            match st.role {
                Role::Wall if s.finished() => b'#',
                Role::Wall => b'u',
                Role::Bridge => b'=',
                _ => b'x',
            }
        } else {
            match st.role {
                Role::Main => b'M',
                Role::Defence => b'T',
                Role::Trap => b't',
                _ => b'B',
            }
        };
        let c = if mine { c } else { c.to_ascii_lowercase() };
        for y in 0..i32::from(s.depth) {
            for x in 0..i32::from(s.width) {
                if let Some(i) = at(s.tile.0 + x, s.tile.1 + y) {
                    grid[i] = c;
                }
            }
        }
    }
    if let Some(e) = world.economy() {
        for n in e.nodes() {
            if let Some(i) = at(n.at.x.floor_int(), n.at.y.floor_int()) {
                grid[i] = b'o';
            }
        }
    }
    let mut out = String::new();
    for row in grid.chunks(side) {
        out.push_str(std::str::from_utf8(row).unwrap());
        out.push('\n');
    }
    Some(out)
}

/// A marching regiment that made no headway: where, who and for how long.
struct Stuck {
    since: u32,
    seconds: u32,
    owner: PlayerId,
    unit: String,
    at: FxVec2,
    goal: Option<FxVec2>,
    files: (u16, u16),
    waiting: String,
    worker: bool,
    map: String,
}

/// Watches every regiment that is marching and not fighting: each second
/// it has moved less than a tile in the last ten is a stuck second.
#[derive(Default)]
struct Marches {
    /// By regiment: where it stood each of the last ten seconds it marched.
    trail: std::collections::HashMap<gc_sim::RegimentHandle, Vec<FxVec2>>,
    open: std::collections::HashMap<gc_sim::RegimentHandle, usize>,
    stuck: Vec<Stuck>,
    marched: [u64; 2],
    stood: [u64; 2],
}

const WINDOW: usize = 10;

impl Marches {
    fn second(&mut self, world: &World, real: &Real) {
        use gc_sim::Doing;
        let mut seen = Vec::new();
        for r in world.regiments.iter() {
            let Some(fb) = world.regiments.feedback(r.handle) else {
                continue;
            };
            // Archers and engines going for a foe stand and shoot: only a
            // melee regiment going in, or any regiment marching, counts.
            let ranged = world.units()[usize::from(r.unit)].range > Fx::ZERO;
            let marching = match fb.doing {
                Doing::Marching | Doing::ForcedMarch => true,
                Doing::Attacking => !ranged,
                Doing::Advancing => !r.diverted,
                _ => false,
            } && !r.engaged
                && world.siege_target(r.handle).is_none()
                && world.building_target(r.handle).is_none();
            if !marching {
                continue;
            }
            seen.push(r.handle);
            let worker = world.is_worker(r.handle);
            self.marched[usize::from(worker)] += 1;
            let trail = self.trail.entry(r.handle).or_default();
            trail.push(r.position);
            if trail.len() > WINDOW {
                trail.remove(0);
            }
            let moved = (r.position - trail[0]).length();
            if trail.len() < WINDOW || moved >= Fx::ONE {
                self.open.remove(&r.handle);
                continue;
            }
            self.stood[usize::from(worker)] += 1;
            if let Some(&k) = self.open.get(&r.handle) {
                self.stuck[k].seconds += 1;
                continue;
            }
            let goal = world.route(r.handle).last().copied();
            self.open.insert(r.handle, self.stuck.len());
            self.stuck.push(Stuck {
                since: world.tick_count() / TICKS_PER_SECOND,
                seconds: WINDOW as u32,
                owner: r.owner,
                unit: real.data.conquest_units[usize::from(r.unit)]
                    .name()
                    .to_string(),
                at: r.position,
                goal,
                files: (r.files, r.frontage),
                waiting: format!("{:?} {:?}", fb.doing, fb.waiting),
                worker,
                map: local_map(world, real, &r, 9),
            });
        }
        self.trail.retain(|h, _| seen.contains(h));
        self.open.retain(|h, _| seen.contains(h));
    }

    fn report(&self, most: usize) {
        for (who, k) in [("fighters", 0), ("workers", 1)] {
            println!(
                "{who}: {} regiment-seconds marching, {} of them stuck ({:.2}%)",
                self.marched[k],
                self.stood[k],
                self.stood[k] as f64 * 100.0 / self.marched[k].max(1) as f64
            );
        }
        let mut worst: Vec<&Stuck> = self.stuck.iter().collect();
        worst.sort_by_key(|s| std::cmp::Reverse(s.seconds));
        println!("{} times a march stood still; the longest:", worst.len());
        for s in worst.iter().take(most) {
            let p = |v: FxVec2| {
                let v = crate::from_fx_vec(v);
                format!("({:.1}, {:.1})", v.x, v.y)
            };
            println!(
                "-- {} s from {}:{:02} player {} {}{} at {} going to {} files {}/{} {}",
                s.seconds,
                s.since / 60,
                s.since % 60,
                s.owner,
                s.unit,
                if s.worker { " (worker)" } else { "" },
                p(s.at),
                s.goal.map_or("nowhere".into(), p),
                s.files.0,
                s.files.1,
                s.waiting
            );
            println!("{}", s.map);
        }
    }
}

/// The ground `reach` tiles round regiment `r`: `R` itself, `a` its
/// side's other regiments, `e` others, `*` its route, buildings and ground
/// as [`base_map`].
fn local_map(world: &World, real: &Real, r: &gc_sim::RegimentView, reach: i32) -> String {
    let (cx, cy) = (r.position.x.floor_int(), r.position.y.floor_int());
    let side = (2 * reach + 1) as usize;
    let mut grid = vec![b'.'; side * side];
    let at = |x: i32, y: i32| {
        let (dx, dy) = (x - cx + reach, y - cy + reach);
        ((0..side as i32).contains(&dx) && (0..side as i32).contains(&dy))
            .then(|| dy as usize * side + dx as usize)
    };
    let (nav, terrain) = (world.nav(), world.terrain());
    for y in cy - reach..=cy + reach {
        for x in cx - reach..=cx + reach {
            let c = FxVec2::new(Fx::from_int(x) + Fx::HALF, Fx::from_int(y) + Fx::HALF);
            let i = at(x, y).unwrap();
            if !nav.walkable(r.owner, c) {
                grid[i] = if terrain.wet(c) { b'~' } else { b'^' };
            } else if terrain.wet(c) {
                grid[i] = b',';
            } else if world.overgrown([x, y, 1, 1]) {
                grid[i] = b'w';
            }
        }
    }
    if let Some(all) = world.structures() {
        for s in all.iter() {
            let st = &real.table.stats[usize::from(s.building)];
            let c = if st.flags & GATE != 0 {
                b'G'
            } else if st.flags & LINE != 0 {
                if st.role == Role::Bridge { b'=' } else { b'#' }
            } else {
                b'B'
            };
            for y in 0..i32::from(s.depth) {
                for x in 0..i32::from(s.width) {
                    if let Some(i) = at(s.tile.0 + x, s.tile.1 + y) {
                        grid[i] = c;
                    }
                }
            }
        }
    }
    for p in world.route(r.handle) {
        if let Some(i) = at(p.x.floor_int(), p.y.floor_int()) {
            grid[i] = b'*';
        }
    }
    for o in world.regiments.iter() {
        if let Some(i) = at(o.position.x.floor_int(), o.position.y.floor_int()) {
            grid[i] = if o.handle == r.handle {
                b'R'
            } else if o.owner == r.owner {
                b'a'
            } else {
                b'e'
            };
        }
    }
    let mut out = String::new();
    for row in grid.chunks(side) {
        out.push_str(std::str::from_utf8(row).unwrap());
        out.push('\n');
    }
    out
}

fn print_bases(world: &World, real: &Real, factions: &[Faction], choice: usize, seed: u64) {
    let t = world.tick_count();
    for p in 0..factions.len() as PlayerId {
        println!(
            "== map {choice} seed {seed} minute {} player {p} {:?} walled {}",
            t / 60 / TICKS_PER_SECOND,
            factions[usize::from(p)],
            world.walled_town(p)
        );
        if let Some(m) = base_map(world, real, p, 22) {
            println!("{m}");
        }
    }
}

#[test]
#[ignore = "a watching aid, run by hand"]
fn wall_survey() {
    let choice = env_number("AI_MAP", 0) as usize;
    let seed = env_number("AI_SEED", 7);
    let minutes = env_number("AI_MINUTES", 30) as u32;
    let every = env_number("AI_EVERY", 10) as u32;
    let tiles = env_number("AI_TILES", 0) as u32;
    let picked = std::env::var("AI_FACTIONS").unwrap_or_else(|_| "01".into());
    let factions: Vec<Faction> = picked
        .bytes()
        .map(|c| Faction::ALL[usize::from(c - b'0') % Faction::ALL.len()])
        .collect();
    let real = Real::load();
    let map = conquest::make_map(choice, tiles, seed);
    let game = Headless::new(&real, map, &factions, seed);
    let defaults = gc_ui::lobby::LobbySettings::default();
    let plan = game.plan(Tempo::Standard, conquest::victories(defaults.victory));
    let mut world = game.world(&plan);
    let mut minds = game.minds();
    for i in 0..factions.len() {
        let temper = crate::conquest_ai::temper(seed, i);
        minds.add(game.mind(i, Difficulty::General, temper, None));
    }
    game.open(&mut world, World::tick);
    let end = minutes * 60 * TICKS_PER_SECOND;
    let mut cmds = Vec::new();
    let mut marches = Marches::default();
    let walls = std::env::var("AI_WALLS").is_ok_and(|v| v == "1");
    while world.verdict().is_none() && world.tick_count() < end {
        cmds.clear();
        minds.tick(&world, &mut cmds);
        world.tick(&cmds);
        let t = world.tick_count();
        if t.is_multiple_of(TICKS_PER_SECOND) {
            marches.second(&world, &real);
        }
        if !walls {
            continue;
        }
        if t.is_multiple_of(every * 60 * TICKS_PER_SECOND) {
            print_bases(&world, &real, &factions, choice, seed);
        }
    }
    if walls {
        print_bases(&world, &real, &factions, choice, seed);
    }
    marches.report(env_number("AI_STUCK", 12) as usize);
    println!(
        "ended at tick {} verdict {:?}",
        world.tick_count(),
        world.verdict()
    );
}
