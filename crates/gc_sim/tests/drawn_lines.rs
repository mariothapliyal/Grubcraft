//! Drawn lines (docs/29-restart-plan.md, "Dynamic bridges in brief"):
//! bridges and walls drawn along a smooth curve, a bridge at the width
//! picked, and how armies use them in play (the real-play checks): a full
//! army over a curved bridge, through a gate in a curved wall, and the
//! land remembering every line drawn on it.

use gc_fixed::{Angle, Fx, FxVec2};
use gc_sim::drawn::{DrawnPath, STANDARD};
use gc_sim::nav::CELLS_PER_TILE;
use gc_sim::scenario::DRILL_UNITS;
use gc_sim::structures::{Armour, GATE, LINE, Refusal, Role, WallTwist};
use gc_sim::walls::{Line, WallTile};
use gc_sim::{
    BuildingStats, Capacities, Command, ConquestSetup, ConquestUnit, Ground, MatchSetup,
    PlayerCommand, RegimentHandle, TICKS_PER_SECOND, UnitStats, World,
};

const WORKER: u16 = 0;
const FOOT: u16 = 1;
const HALL: u16 = 0;
const BRIDGE: u16 = 1;
const WALL: u16 = 2;
const GATE_3: u16 = 3;
/// The bridge's next two levels: braced piers, then reinforced.
const BRACED: u16 = 4;
const REINFORCED: u16 = 5;
const MAP: u32 = 80;
/// The river runs north to south over these columns, deep (no ford).
const RIVER: core::ops::Range<i32> = 30..34;
const LAND: u16 = 300;
const WATER: u16 = 256;
const TILE_GRUB: u16 = 5;

fn world() -> World {
    let side = (MAP as i32 * CELLS_PER_TILE) as usize;
    let nav: Vec<u8> = (0..side * side)
        .map(|c| u8::from(!RIVER.contains(&((c % side) as i32 / CELLS_PER_TILE))))
        .collect();
    let n = MAP as usize;
    let heights: Vec<u16> = (0..n * n)
        .map(|t| {
            if RIVER.contains(&((t % n) as i32)) {
                0
            } else {
                LAND
            }
        })
        .collect();
    let worker = UnitStats {
        models: 1,
        files: 1,
        ..DRILL_UNITS[0]
    };
    let mut w = World::new(&MatchSetup {
        seed: 7,
        capacities: Capacities { regiments: 32 },
        units: &[worker, DRILL_UNITS[0]],
        map_tiles: MAP,
        nav: &nav,
        teams: &[],
        ground: Ground {
            heights: &heights,
            water_level: WATER,
            ..Ground::FLAT
        },
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: false,
    });
    let hall = BuildingStats {
        role: Role::Main,
        tier: 1,
        upgrades_from: None,
        cost_grub: 100,
        cost_marrow: 0,
        build_ticks: 40,
        hp: 1000,
        armour: Armour::Fortified,
        width: 3,
        depth: 3,
        supply: 0,
        flags: 0,
        attack: None,
        field: 0,
        tunes: None,
        links: 0,
        twist: WallTwist::None,
        trap: gc_sim::structures::Trap::None,
    };
    let line = BuildingStats {
        cost_grub: TILE_GRUB,
        build_ticks: 4,
        hp: 300,
        armour: Armour::Structure,
        width: 1,
        depth: 1,
        flags: LINE,
        ..hall
    };
    let mut start = [(0, 0); gc_sim::MAX_PARTIES];
    start[0] = (20_000, 0);
    start[1] = (20_000, 0);
    assert!(w.conquest(&ConquestSetup {
        buildings: &[
            hall,
            BuildingStats {
                role: Role::Bridge,
                ..line
            },
            BuildingStats {
                role: Role::Wall,
                ..line
            },
            BuildingStats {
                role: Role::Wall,
                depth: 3,
                hp: 1000,
                flags: GATE,
                ..line
            },
            BuildingStats {
                role: Role::Bridge,
                upgrades_from: Some(BRIDGE),
                hp: 600,
                ..line
            },
            BuildingStats {
                role: Role::Bridge,
                upgrades_from: Some(BRACED),
                hp: 1000,
                ..line
            },
        ],
        units: &[
            ConquestUnit {
                worker: true,
                ..ConquestUnit::default()
            },
            ConquestUnit::default(),
        ],
        structures: 1024,
        start,
        ..ConquestSetup::default()
    }));
    for (player, tile) in [(0, (2, 2)), (1, (74, 74))] {
        order(
            &mut w,
            player,
            Command::SpawnStructure {
                building: HALL,
                tile,
                turn: 0,
            },
        );
    }
    w
}

fn order(w: &mut World, player: u8, command: Command) -> bool {
    w.tick(&[PlayerCommand { player, command }])
        .commands_applied
        == 1
}

fn spawn(w: &mut World, player: u8, unit: u16, x: i32, y: i32) -> RegimentHandle {
    let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
    assert!(order(
        w,
        player,
        Command::SpawnRegiment {
            unit,
            position: FxVec2::from_ints(x, y),
            facing: Angle(0x4000),
        }
    ));
    (w.regiments.iter().map(|r| r.handle))
        .find(|h| !before.contains(h))
        .unwrap()
}

/// The point at the centre of tile `t`, in half tiles.
fn c(t: (i32, i32)) -> (i16, i16) {
    ((2 * t.0 + 1) as i16, (2 * t.1 + 1) as i16)
}

/// A gentle curve across the river from the west bank to the east, bending
/// south: the bridge every test here draws.
fn crossing() -> DrawnPath {
    DrawnPath::new(&[c((22, 20)), c((32, 24)), c((42, 30))], |_| false).unwrap()
}

fn bridge(path: DrawnPath, width: u8) -> Line {
    Line {
        path,
        width,
        old: false,
    }
}

fn draw(w: &mut World, worker: RegimentHandle, path: DrawnPath, width: u8) -> bool {
    order(
        w,
        0,
        Command::DrawLine {
            regiment: Some(worker),
            building: BRIDGE,
            path,
            width,
        },
    )
}

fn run(w: &mut World, seconds: u32) {
    for _ in 0..seconds * TICKS_PER_SECOND {
        w.tick(&[]);
    }
}

fn position(w: &World, h: RegimentHandle) -> FxVec2 {
    w.regiments.iter().find(|r| r.handle == h).unwrap().position
}

/// The narrowest the deck `tiles` is across, in tiles, measured square to
/// the centre line `path` every quarter tile along it, the first and last
/// two tiles (where it meets the banks) left out: the clearance check.
fn narrowest(path: &DrawnPath, tiles: &[(i32, i32)]) -> f32 {
    let s = path.samples();
    let on = |x: f32, y: f32| tiles.contains(&(x.floor() as i32, y.floor() as i32));
    let sub = gc_sim::drawn::SUB as f32;
    let mut along = 0.0;
    let mut least = f32::MAX;
    for k in 1..s.len() {
        let (ax, ay) = (s[k - 1].x as f32 / sub, s[k - 1].y as f32 / sub);
        let (bx, by) = (s[k].x as f32 / sub, s[k].y as f32 / sub);
        let (dx, dy) = (bx - ax, by - ay);
        let len = (dx * dx + dy * dy).sqrt();
        along += len;
        if along < 2.0 || along > total(path) - 2.0 || len == 0.0 {
            continue;
        }
        let (nx, ny) = (-dy / len, dx / len);
        let reach = |sign: f32| {
            let mut r = 0.0;
            while on(bx + nx * sign * (r + 0.05), by + ny * sign * (r + 0.05)) && r < 10.0 {
                r += 0.05;
            }
            r
        };
        least = least.min(reach(1.0) + reach(-1.0));
    }
    least
}

fn total(path: &DrawnPath) -> f32 {
    path.length() as f32 / gc_sim::drawn::SUB as f32
}

/// Tiles of the plan, and whether all of them fit.
fn tiles(plan: &[WallTile]) -> Vec<(i32, i32)> {
    assert!(plan.iter().all(|t| t.fits.is_ok()), "{plan:?}");
    plan.iter().map(|t| t.tile).collect()
}

#[test]
fn a_bridge_follows_a_drawn_curve_at_the_width_picked() {
    let w = world();
    let mut deck = Vec::new();
    for width in gc_sim::drawn::BRIDGE_WIDTHS {
        let plan = w.line_plan(0, BRIDGE, &bridge(crossing(), width)).unwrap();
        let t = tiles(&plan);
        // The clearance check: wide enough all along, round the bend too,
        // for a block of soldiers several files across, and even the
        // narrow one for the widest walking unit, the Broodmother Bulk
        // (2.7 tiles).
        let least = narrowest(&crossing(), &t);
        eprintln!("width {width}: {least:.2} tiles across at the narrowest");
        assert!(
            least >= f32::from(width) - 0.25 && least >= 2.7,
            "width {width}: {least} tiles at the narrowest"
        );
        // Every tile joins the deck edge to edge (no corner-only step).
        for &(x, y) in &t {
            let has = |dx: i32, dy: i32| t.contains(&(x + dx, y + dy));
            for (dx, dy) in [(1, 1), (1, -1), (-1, 1), (-1, -1)] {
                if has(dx, dy) {
                    assert!(has(dx, 0) || has(0, dy), "corner step at {x},{y}");
                }
            }
        }
        deck.push(t.len());
    }
    // Wider costs more tiles.
    assert!(deck[0] < deck[1] && deck[1] < deck[2], "{deck:?}");
    // No other width, no sharp bend, no corner, nor too long.
    let refused = |path: DrawnPath, width: u8| w.line_plan(0, BRIDGE, &bridge(path, width));
    assert_eq!(refused(crossing(), 2), Err(Refusal::Unknown));
    assert_eq!(refused(crossing(), 6), Err(Refusal::Unknown));
    let sharp = DrawnPath::new(&[c((24, 20)), c((32, 20)), c((32, 30))], |_| false).unwrap();
    assert_eq!(refused(sharp, STANDARD), Err(Refusal::Crooked));
    let corner = DrawnPath::new(&[c((22, 20)), c((32, 24)), c((42, 30))], |_| true).unwrap();
    assert_eq!(refused(corner, STANDARD), Err(Refusal::Crooked));
    let long = DrawnPath::new(&[c((2, 20)), c((32, 24)), c((62, 30))], |_| false).unwrap();
    assert_eq!(refused(long, STANDARD), Err(Refusal::Unknown));
    // Ending in the water.
    let short = DrawnPath::new(&[c((24, 20)), c((31, 22))], |_| true).unwrap();
    assert_eq!(refused(short, STANDARD), Err(Refusal::Rough));
}

#[test]
fn a_full_army_crosses_a_curved_bridge_dry_shod() {
    let mut w = world();
    let worker = spawn(&mut w, 0, WORKER, 21, 20);
    let grub = w.bank(0).unwrap().grub;
    let plan = w
        .line_plan(0, BRIDGE, &bridge(crossing(), STANDARD))
        .unwrap();
    assert!(draw(&mut w, worker, crossing(), STANDARD));
    assert_eq!(
        grub - w.bank(0).unwrap().grub,
        plan.len() as u32 * u32::from(TILE_GRUB)
    );
    run(&mut w, 240);
    let s = w.structures().unwrap();
    let deck: Vec<_> = s.iter().filter(|v| v.building == BRIDGE).collect();
    assert_eq!(deck.len(), plan.len());
    assert!(
        deck.iter().all(gc_sim::StructureView::finished),
        "built across"
    );
    // Four full regiments in line march over from the west bank to the
    // east, and nobody wades.
    let army: Vec<_> = (0..4)
        .map(|k| spawn(&mut w, 0, FOOT, 14, 12 + 6 * k))
        .collect();
    for (k, &r) in army.iter().enumerate() {
        assert!(order(
            &mut w,
            0,
            Command::MoveTo {
                regiment: r,
                target: FxVec2::from_ints(52, 30 + 4 * k as i32),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            }
        ));
    }
    let mut dry = true;
    let mut over = None;
    for t in 0..180 * TICKS_PER_SECOND {
        w.tick(&[]);
        for &r in &army {
            dry &= !w.terrain().wet(position(&w, r));
        }
        if over.is_none() && army.iter().all(|&r| position(&w, r).x > Fx::from_int(44)) {
            over = Some(t / TICKS_PER_SECOND);
        }
    }
    assert!(dry, "nobody wades on a bridge");
    let over = over.expect("the whole army crossed");
    // Real use: four regiments over a 20-tile curved bridge in under two
    // minutes, single file nowhere.
    assert!(over < 120, "the army took {over} s to cross");
    eprintln!("four regiments crossed the curved bridge in {over} s");
}

#[test]
fn a_gate_in_a_curved_wall_lets_its_army_through_and_keeps_others_out() {
    let mut w = world();
    // A wall from the north edge to the south, bowing west, east of the
    // river: the only way through it is its gate.
    let path = DrawnPath::new(&[c((60, 0)), c((54, 40)), c((60, 79))], |_| false).unwrap();
    let wall = Line {
        path,
        width: 1,
        old: false,
    };
    let plan = tiles(&w.line_plan(0, WALL, &wall).unwrap());
    assert!(plan.len() > 79 && plan.len() <= 96, "{}", plan.len());
    let workers: Vec<_> = (0..4)
        .map(|k| spawn(&mut w, 0, WORKER, 50, 10 + 20 * k))
        .collect();
    assert!(order(
        &mut w,
        0,
        Command::DrawLine {
            regiment: Some(workers[0]),
            building: WALL,
            path,
            width: 1,
        }
    ));
    // The rest help build it.
    let first = w
        .structures()
        .unwrap()
        .iter()
        .find(|v| v.building == WALL)
        .unwrap();
    for &k in &workers[1..] {
        order(
            &mut w,
            0,
            Command::Construct {
                regiment: k,
                structure: first.handle,
            },
        );
    }
    run(&mut w, 300);
    let s = w.structures().unwrap();
    assert!(
        s.iter()
            .filter(|v| v.building == WALL)
            .all(|v| v.finished()),
        "walled"
    );
    // The gate goes where the wall runs north to south: at the bend.
    let at = (54..56)
        .flat_map(|x| (36..44).map(move |y| (x, y)))
        .find(|&(x, y)| (0..3).all(|k| plan.contains(&(x, y + k))))
        .expect("three tiles of wall in a column at the bend");
    assert!(order(
        &mut w,
        0,
        Command::Build {
            regiment: workers[0],
            building: GATE_3,
            tile: (at.0 as i16, at.1 as i16),
            turn: 0,
        }
    ));
    run(&mut w, 60);
    let s = w.structures().unwrap();
    assert!(
        s.iter().any(|v| v.building == GATE_3 && v.finished()),
        "the gate is up"
    );
    // Clearance: the gateway is three tiles wide for a block marching
    // through it.
    // Its own army marches out through the gate; an enemy regiment can't
    // come in.
    let army: Vec<_> = (0..3)
        .map(|k| spawn(&mut w, 0, FOOT, 44, 32 + 8 * k))
        .collect();
    let enemy = spawn(&mut w, 1, FOOT, 72, 6);
    for &r in &army {
        assert!(order(
            &mut w,
            0,
            Command::MoveTo {
                regiment: r,
                target: FxVec2::from_ints(68, 68),
                facing: None,
                files: 0,
                pace: Fx::ZERO,
            }
        ));
    }
    assert!(order(
        &mut w,
        1,
        Command::MoveTo {
            regiment: enemy,
            target: FxVec2::from_ints(40, 6),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        }
    ));
    let mut out = None;
    let mut breached = false;
    for t in 0..150 * TICKS_PER_SECOND {
        w.tick(&[]);
        if out.is_none() && army.iter().all(|&r| position(&w, r).x > Fx::from_int(64)) {
            out = Some(t / TICKS_PER_SECOND);
        }
        let enemy_at = w
            .regiments
            .iter()
            .find(|r| r.handle == enemy)
            .map(|r| r.position);
        breached |= enemy_at.is_some_and(|p| p.x < Fx::from_int(52));
    }
    let out = out.expect("the army marched out through the gate");
    assert!(out < 120, "the army took {out} s through the gate");
    eprintln!("three regiments marched through the gate in a curved wall in {out} s");
    assert!(!breached, "an enemy walked through the wall");
}

#[test]
fn the_land_remembers_each_line_drawn_on_it() {
    let mut w = world();
    let worker = spawn(&mut w, 0, WORKER, 21, 20);
    assert!(draw(&mut w, worker, crossing(), STANDARD));
    run(&mut w, 240);
    let s = w.structures().unwrap();
    let deck: Vec<_> = s.iter().filter(|v| v.building == BRIDGE).collect();
    let id = deck[0].drawing.expect("laid along a drawing");
    assert!(deck.iter().all(|v| v.drawing == Some(id)));
    let drawing = w.drawing(id).unwrap();
    assert_eq!(
        (drawing.path, drawing.width, drawing.owner),
        (crossing(), STANDARD, 0)
    );
    // A tile burnt out mid-river: the rest still name the line, and the
    // line itself stays, so a later fight finds the same bridge, broken.
    let gone = deck
        .iter()
        .find(|v| RIVER.contains(&v.tile.0))
        .unwrap()
        .handle;
    assert!(order(
        &mut w,
        0,
        Command::RemoveStructure { structure: gone }
    ));
    run(&mut w, 30);
    assert_eq!(w.drawing(id), Some(drawing));
    let s = w.structures().unwrap();
    let left = s.iter().filter(|v| v.building == BRIDGE).count();
    assert_eq!(left, deck.len() - 1);
    // Drawn the same way again, only the gap is new, laid along a line of
    // its own.
    let again = w
        .line_plan(0, BRIDGE, &bridge(crossing(), STANDARD))
        .unwrap();
    assert_eq!(again.len(), 1);
    assert!(draw(&mut w, worker, crossing(), STANDARD));
    run(&mut w, 60);
    let s = w.structures().unwrap();
    let mended = s
        .iter()
        .find(|v| v.building == BRIDGE && v.drawing != Some(id))
        .unwrap();
    assert!(mended.finished());
    assert!(w.drawing(mended.drawing.unwrap()).is_some());
    // The line is part of the state every machine agrees on: the same
    // match drawn at another width plays differently.
    let hash = |width: u8| {
        let mut w = world();
        let worker = spawn(&mut w, 0, WORKER, 21, 20);
        assert!(draw(&mut w, worker, crossing(), width));
        w.tick(&[]);
        w.state_hash()
    };
    assert_ne!(hash(3), hash(4));
}

#[test]
fn a_curved_plan_is_dropped_whole() {
    let mut w = world();
    let path = DrawnPath::new(&[c((44, 4)), c((50, 10)), c((52, 20))], |_| false).unwrap();
    assert!(order(
        &mut w,
        0,
        Command::PlanLine {
            building: WALL,
            path
        }
    ));
    let plans = w.plans().to_vec();
    assert!(plans.len() > 10);
    assert!(plans.iter().all(|p| p.drawing == plans[0].drawing));
    // Dropping the line under one tile drops the whole curve.
    let p = plans[plans.len() / 2];
    let (a, b) = ((p.line[0], p.line[1]), (p.line[2], p.line[3]));
    assert!(order(&mut w, 0, Command::DropPlan { from: a, to: b }));
    assert!(w.plans().is_empty());
}

#[test]
fn a_bridge_is_raised_to_braced_then_reinforced_piers_and_stays_open() {
    let mut w = world();
    let worker = spawn(&mut w, 0, WORKER, 21, 20);
    assert!(draw(&mut w, worker, crossing(), STANDARD));
    run(&mut w, 240);
    // A regiment stands on the middle of the bridge while it is raised.
    let foot = spawn(&mut w, 0, FOOT, 32, 24);
    for (to, hp) in [(BRACED, 600), (REINFORCED, 1000)] {
        let s = w.structures().unwrap();
        let tile = s.iter().find(|v| v.drawing.is_some()).unwrap().handle;
        let n = s.iter().filter(|v| v.drawing.is_some()).count();
        let raise = w.wall_level(0, tile).unwrap();
        assert_eq!((raise.to, raise.tiles), (to, n), "the whole bridge goes up");
        assert!(order(
            &mut w,
            0,
            Command::RaiseWall {
                structure: tile,
                regiment: Some(worker),
            }
        ));
        for _ in 0..300 * TICKS_PER_SECOND {
            w.tick(&[]);
            assert!(!w.terrain().wet(position(&w, foot)), "the deck stays open");
        }
        let s = w.structures().unwrap();
        let deck: Vec<_> = s.iter().filter(|v| v.drawing.is_some()).collect();
        assert!(
            deck.iter().all(|v| v.building == to && v.max_hp == hp),
            "raised to {to}"
        );
        // Still remembered as the same drawn line.
        assert!(deck.iter().all(|v| v.drawing == deck[0].drawing));
    }
    let s = w.structures().unwrap();
    let top = s.iter().find(|v| v.drawing.is_some()).unwrap().handle;
    assert_eq!(s.pier_level(top.index as usize), 3);
}
