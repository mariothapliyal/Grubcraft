//! Big armies through narrow gaps: 48 regiments, about nine hundred
//! soldiers, through one Gate, and 16 regiments over a two-lane bridge.
//! Each measures how the crowd flows: when the first and last regiment
//! are through, the soldiers through a second while the gap is busy, and
//! how long regiments stand stuck in the crowd before it (bunching).

use gc_fixed::FxVec2;
use gc_sim::{RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, WEST, centre, grid, move_to, of, put_up, soldiers, spawn};
use super::{Report, Scenario, seconds};

pub(super) const GATE: Scenario = Scenario {
    name: "army-gate",
    about: "48 regiments (about 900 soldiers) march through one Gate",
    budget_ms: 12_000,
    run: gate,
};

pub(super) const BRIDGE: Scenario = Scenario {
    name: "army-bridge",
    about: "16 regiments cross a two-lane bridge, all ordered at once",
    budget_ms: 8_000,
    run: bridge,
};

/// What a crowd's flow through a gap measured.
#[derive(Default)]
struct Flow {
    first: Option<u32>,
    done: Option<u32>,
    /// Ticks when a tenth and nine tenths of the soldiers were through.
    tenth: Option<u32>,
    nine_tenths: Option<u32>,
    /// Regiment-ticks with an order, not yet through, that didn't move.
    stuck: u32,
    /// Most regiments not yet through within `MOUTH` tiles of the gap.
    crowd: usize,
    /// Regiment-ticks in disorder from crowding into friends (nobody
    /// fights here, so all of it is bunching).
    disorder: u32,
}

/// How near the gap counts as its mouth, in tiles.
const MOUTH: i32 = 10;

/// Tick `world` until every regiment of `army` is `through` and standing,
/// at most `limit` seconds, measuring the flow at the gap `at`.
fn flow(
    world: &mut World,
    report: &mut Report,
    army: &[RegimentHandle],
    goals: &[FxVec2],
    at: FxVec2,
    limit: u32,
    through: impl Fn(FxVec2) -> bool,
) -> Flow {
    let orders: Vec<_> = (army.iter().zip(goals)).map(|(&r, &g)| move_to(0, r, g)).collect();
    let total = soldiers(world, army);
    report.tick(world, &orders);
    let mut f = Flow::default();
    let mut last: Vec<FxVec2> = of(world, army).map(|v| v.position).collect();
    for t in 1..=limit * TICKS_PER_SECOND {
        report.tick(world, &[]);
        let (mut alive, mut across, mut men) = (0, 0, 0);
        let mut crowd = 0;
        for (k, v) in of(world, army).enumerate() {
            alive += 1;
            f.disorder += u32::from(v.disorder > 0);
            if through(v.position) {
                across += usize::from(!v.moving);
                men += u32::from(v.models);
                f.first.get_or_insert(t);
                continue;
            }
            let d = v.position - at;
            if d.x.abs().floor_int() < MOUTH && d.y.abs().floor_int() < MOUTH {
                crowd += 1;
            }
            if v.moving && last.get(k) == Some(&v.position) {
                f.stuck += 1;
            }
        }
        f.crowd = f.crowd.max(crowd);
        if std::env::var("TRACE").is_ok() && t % (2 * TICKS_PER_SECOND) == 0 {
            let mut rows = vec![vec![b'.'; 128]; 128];
            for (k, v) in of(world, army).enumerate() {
                let (x, y) = (v.position.x.floor_int(), v.position.y.floor_int());
                if (0..128).contains(&x) && (0..128).contains(&y) {
                    let c = if !v.moving { b'o' } else if last.get(k) == Some(&v.position) { b'X' } else if v.disorder > 0 { b'0' + (v.files as u8).min(9) } else { b'a' + (v.files as u8).min(25) };
                    rows[y as usize][x as usize] = c;
                }
            }
            println!("t={}s", t / TICKS_PER_SECOND);
            let (ax, ay) = (at.x.floor_int(), at.y.floor_int());
            for y in (ay - 20).max(0)..(ay + 20).min(128) {
                println!("{}", String::from_utf8_lossy(&rows[y as usize][(ax - 40).max(0) as usize..(ax + 40).min(128) as usize]));
            }
        }
        if men * 10 >= total {
            f.tenth.get_or_insert(t);
        }
        if men * 10 >= total * 9 {
            f.nine_tenths.get_or_insert(t);
        }
        last = of(world, army).map(|v| v.position).collect();
        if alive > 0 && across == alive {
            f.done = Some(t);
            break;
        }
    }
    report.metric("soldiers", total);
    report.metric("first regiment through", seconds(f.first));
    report.metric("all through and standing", seconds(f.done));
    let rate = match (f.tenth, f.nine_tenths) {
        (Some(a), Some(b)) if b > a => {
            (total * 8 / 10) as f32 * TICKS_PER_SECOND as f32 / (b - a) as f32
        }
        _ => 0.0,
    };
    report.metric("soldiers through a second, while busy", format!("{rate:.1}"));
    report.metric(
        "regiment-seconds stuck before the gap",
        format!("{:.1}", f.stuck as f32 / TICKS_PER_SECOND as f32),
    );
    report.metric("most regiments at its mouth at once", f.crowd);
    report.metric(
        "regiment-seconds in disorder from crowding",
        format!("{:.1}", f.disorder as f32 / TICKS_PER_SECOND as f32),
    );
    report.check(
        format!("every regiment through and standing within {limit} s"),
        f.done.is_some(),
    );
    report.check("no soldier lost", soldiers(world, army) == total);
    f
}

const GATE_TILES: u32 = 128;
const WALL_ROW: i32 = 56;
const GATE_X: i32 = 62;
const GATE_LIMIT: u32 = 240;

fn gate(real: &Real, report: &mut Report) {
    let mut world = Arena::new(GATE_TILES).world(real, 31, 0);
    let (palisade, gate) = (real.building("Palisade"), real.building("Gate"));
    let stats = &real.table.stats[usize::from(gate)];
    let span = GATE_X..GATE_X + i32::from(stats.width.max(stats.depth));
    let mut wall: Vec<_> = (0..GATE_TILES as i32)
        .filter(|x| !span.contains(x))
        .map(|x| (palisade, (x, WALL_ROW), 0))
        .collect();
    wall.push((gate, (GATE_X, WALL_ROW), 1));
    put_up(&mut world, 0, &wall);
    let middle = (span.start + span.end) / 2;

    // Eight abreast, four deep, south of the wall, to the same grid north
    // of it.
    let [yari, katana, yumi] = real.ember_line();
    let horse = real.unit("Mounted Samurai");
    let kinds = [yari, katana, yari, yumi, yari, katana, horse, yumi];
    let at = |y: i32, i: usize| grid((24, y), 8, (11, 7), i);
    let placed: Vec<_> = (0..48).map(|i| (kinds[i % 8], at(76, i))).collect();
    let army = spawn(&mut world, 0, &placed, WEST);
    let goals: Vec<_> = (0..48).map(|i| at(8, i)).collect();
    let f = flow(
        &mut world,
        report,
        &army,
        &goals,
        centre(middle, WALL_ROW),
        GATE_LIMIT,
        |p| p.y.floor_int() < WALL_ROW - 1,
    );
    let _ = f;
}

const BRIDGE_TILES: u32 = 96;
const RIVER: core::ops::Range<i32> = 46..50;
const ROW: i32 = 46;
const SPAN: core::ops::RangeInclusive<i32> = 43..=52;
const BRIDGE_LIMIT: u32 = 150;

fn bridge(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(BRIDGE_TILES);
    arena.river(RIVER, 0..BRIDGE_TILES as i32);
    let mut world = arena.world(real, 32, 0);
    let bridge = real.building("Timber Bridge");
    let deck: Vec<_> = SPAN
        .flat_map(|x| [(bridge, (x, ROW), 0), (bridge, (x, ROW + 1), 0)])
        .collect();
    put_up(&mut world, 0, &deck);

    // Four deep, four abreast on the west bank, to the same on the east.
    let [yari, katana, yumi] = real.ember_line();
    let kinds = [yari, katana, yumi, yari];
    let at = |x: i32, i: usize| grid((x, 26), 4, (7, 12), i);
    let placed: Vec<_> = (0..16).map(|i| (kinds[i % 4], at(8, i))).collect();
    let army = spawn(&mut world, 0, &placed, EAST);
    let goals: Vec<_> = (0..16).map(|i| at(62, i)).collect();
    flow(
        &mut world,
        report,
        &army,
        &goals,
        centre(RIVER.start + 2, ROW + 1),
        BRIDGE_LIMIT,
        |p| p.x.floor_int() > RIVER.end + 3,
    );
    let _ = WEST;
}
