//! A column crosses a bridge 3 tiles wide (the narrowest a player can
//! draw) as a catastrophic flood comes down
//! the river with logs torn off the banks above (docs/39-water-engine.md,
//! stage 4e). Enemy archers have weakened one span first. The flood
//! strains it and the logs strike it until it is swept away under the
//! column. The regiments on it wash up out of the water; the rest halt
//! at the gap and go round by a bridge upstream.

use gc_fixed::{Fx, FxVec2};
use gc_sim::rivers::Band;
use gc_sim::{Command, MoraleState, PlayerCommand, TICKS_PER_SECOND, World};

use super::arena::{
    Arena, EAST, Real, SOUTH, WEST, centre, grid, move_to, of, put_up, soldiers, spawn, squall,
};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "bridge-flood",
    about: "a column on a bridge when a catastrophic flood and its logs sweep a weak span away",
    budget_ms: 1_500,
    run,
};

const TILES: u32 = 64;
const SECOND: u32 = TICKS_PER_SECOND;
/// The river's columns, running south; a ford at its head that the flood
/// shuts.
const RIVER: core::ops::Range<i32> = 30..34;
const FORD: core::ops::Range<i32> = 2..5;
/// The bridge the column takes, its two rows from bank to bank, and the
/// one upstream it can go round by.
const ROW: i32 = 40;
const UPSTREAM_ROW: i32 = 12;
const SPAN: core::ops::RangeInclusive<i32> = 27..=36;
/// The span the archers weaken: the bridge's first tiles out over the
/// water, all three rows.
const WEAK: [(i32, i32); 3] = [(30, ROW), (30, ROW + 1), (30, ROW + 2)];
/// How far the archers wear it: under the strain line (75%), over the
/// sweep line (50%), so the flood and its logs decide when it goes.
const WEAKEN_TO: u32 = 58;
/// A seed whose squall rolls a catastrophic front (checked).
const SEED: u64 = 23;
/// When the column sets off: its third regiment is on the weak span when
/// the flood takes it (checked).
const SET_OFF: u32 = 44 * SECOND;
const LIMIT_SECONDS: u32 = 150;
/// How soon each regiment still short of the gap when the crossing breaks
/// must be on its way round.
const REROUTE_SECONDS: u32 = 10;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.river(RIVER, 0..TILES as i32);
    arena.ford(RIVER, FORD);
    // Bank woods between the bridges, for the flood to tear logs from.
    arena.woods(26..30, 18..28);
    arena.woods(34..38, 18..28);
    arena.flowing(SOUTH);
    let mut world = arena.world_under(real, SEED, 0, squall());
    let bridge = real.building("Timber Bridge");
    let deck: Vec<_> = (SPAN.flat_map(|x| {
        [
            ROW,
            ROW + 1,
            ROW + 2,
            UPSTREAM_ROW,
            UPSTREAM_ROW + 1,
            UPSTREAM_ROW + 2,
        ]
        .map(|y| (x, y))
    }))
    .map(|tile| (bridge, tile, 0))
    .collect();
    put_up(&mut world, 0, &deck);

    let [yari, katana, yumi] = real.ember_line();
    let archers = spawn(
        &mut world,
        1,
        &[(yumi, centre(42, ROW - 2)), (yumi, centre(42, ROW + 4))],
        WEST,
    );
    let units = [yari, katana, yari, yumi, katana, yari];
    let column: Vec<_> = (units.iter().enumerate())
        .map(|(i, &u)| {
            let i = i as i32;
            (u, centre(20 - 3 * i, ROW - 2 + (i % 2) * 4))
        })
        .collect();
    let column = spawn(&mut world, 0, &column, EAST);
    let start = soldiers(&world, &column);
    report.metric("soldiers in the column", start);
    let goals: Vec<_> = (0..column.len())
        .map(|i| grid((50, ROW - 6), 2, (6, 6), i))
        .collect();

    let hp = |w: &World, tile| {
        (w.structures().unwrap().iter())
            .find(|v| v.tile == tile)
            .map(|v| (v.handle, v.hp * 100 / v.max_hp.max(1)))
    };
    let sound = |w: &World| {
        (w.structures().unwrap().iter())
            .filter(|v| RIVER.contains(&v.tile.0) && !WEAK.contains(&v.tile))
            .count()
    };
    let sound_at_start = sound(&world);
    let mut weakened = None;
    let mut catastrophic = false;
    let mut fell: [Option<u32>; 3] = [None; 3];
    let mut fell_in_flood = true;
    // Regiments on a weak tile when it went, and whether each came out of
    // the water.
    let mut on_span = 0;
    let mut washed_up = true;
    // Orders given again to those that came to a halt short of the far
    // bank (washed up, or routed by the fall and rallied), as a player
    // would.
    let mut reordered = 0;
    // Regiment-ticks over the river, off the deck, after the first tick of
    // washing up.
    let mut in_river = 0u32;
    // When the crossing broke, each regiment still short of the gap and how
    // far it was from the upstream bridge then, and when it was on its way
    // round.
    let mut broke = None;
    let mut behind: Vec<(usize, Fx, Option<u32>)> = Vec::new();
    let upstream = centre(SPAN.start() - 1, UPSTREAM_ROW);
    let mut done = None;
    for t in 1..=LIMIT_SECONDS * SECOND {
        let mut orders = Vec::new();
        // The archers wear the span, a row at a time, then leave.
        if weakened.is_none() && t % SECOND == 0 {
            match WEAK
                .iter()
                .find_map(|&tile| hp(&world, tile).filter(|h| h.1 > WEAKEN_TO))
            {
                Some((structure, _)) => {
                    orders.extend(archers.iter().map(|&regiment| PlayerCommand {
                        player: 1,
                        command: Command::AttackStructure {
                            regiment,
                            structure,
                        },
                    }));
                }
                None => {
                    weakened = Some(t);
                    orders.extend(archers.iter().map(|&regiment| PlayerCommand {
                        player: 1,
                        command: Command::RemoveRegiment { regiment },
                    }));
                }
            }
        }
        if t > SET_OFF && t % SECOND == 0 {
            for v in of(&world, &column) {
                let i = column.iter().position(|&c| c == v.handle).unwrap();
                let short = (goals[i] - v.position).length() > Fx::from_int(3);
                if !v.moving && short && v.morale != MoraleState::Routing {
                    orders.push(move_to(0, v.handle, goals[i]));
                    reordered += 1;
                }
            }
        }
        if t == SET_OFF {
            orders.extend((column.iter().zip(&goals)).map(|(&r, &g)| move_to(0, r, g)));
        }
        let was: Vec<_> = WEAK
            .iter()
            .map(|&tile| hp(&world, tile).is_some())
            .collect();
        let before: Vec<_> = of(&world, &column)
            .map(|v| (v.handle, v.position))
            .collect();
        report.tick(&mut world, &orders);
        let rivers = world.flood().rivers();
        catastrophic |= rivers.fronts().any(|f| f.band() == Band::Catastrophic);
        for (k, &tile) in WEAK.iter().enumerate() {
            if !was[k] || hp(&world, tile).is_some() {
                continue;
            }
            fell[k] = Some(t);
            fell_in_flood &= rivers.fronts().any(|f| f.band() == Band::Catastrophic);
            for &(h, p) in &before {
                // On it: its middle within a tile and a half of the
                // tile's (its block spans more than a tile).
                if (p - centre(tile.0, tile.1)).length() > Fx::from_ratio(3, 2) {
                    continue;
                }
                on_span += 1;
                let now = of(&world, &[h]).next().map(|v| v.position);
                washed_up &= now.is_some_and(|q| !in_water(&arena, &world, q));
            }
        }
        // Over the water off the deck, but for the tick a regiment washes
        // up in.
        if fell.iter().all(|f| f.is_none_or(|f| f < t)) {
            in_river += of(&world, &column)
                .filter(|v| in_water(&arena, &world, v.position))
                .count() as u32;
        }
        if broke.is_none() && fell.iter().all(Option::is_some) {
            broke = Some(t);
            behind = (of(&world, &column))
                .filter(|v| v.position.x.floor_int() < RIVER.start)
                .map(|v| {
                    let i = column.iter().position(|&r| r == v.handle).unwrap();
                    (i, (upstream - v.position).length(), None)
                })
                .collect();
        }
        for (i, from, round) in behind.iter_mut().filter(|b| b.2.is_none()) {
            let on_its_way = of(&world, &column[*i..=*i]).next().is_some_and(|v| {
                v.moving && (upstream - v.position).length() + Fx::from_int(2) < *from
            });
            if on_its_way {
                *round = Some(t);
            }
        }
        let mut left = of(&world, &column).peekable();
        if left.peek().is_some()
            && left.all(|v| v.position.x.floor_int() > RIVER.end + 3 && !v.moving)
        {
            done = Some(t);
            break;
        }
    }
    report.metric("span weakened by", seconds(weakened));
    report.metric("span swept away, row by row", fell.map(seconds).join(", "));
    report.metric("regiments on the span when it went", on_span);
    let slowest = (behind.iter())
        .map(|b| b.2.map(|t| t - broke.unwrap_or(t)))
        .max()
        .flatten();
    report.metric("orders given again to regiments halted short", reordered);
    report.metric("regiments short of the gap", behind.len());
    report.metric("slowest on its way round", seconds(slowest));
    report.metric("all across and standing", seconds(done));
    report.metric("soldiers lost", start - soldiers(&world, &column));
    report.check("a catastrophic flood comes down the river", catastrophic);
    report.check(
        "the weakened span is swept away while the flood runs",
        fell.iter().all(Option::is_some) && fell_in_flood,
    );
    report.check("every sound pier stands", sound(&world) == sound_at_start);
    report.check(
        "a regiment is on the span when it goes, and washes up out of the water",
        on_span > 0 && washed_up,
    );
    report.check(
        "no regiment walks off the deck into the river",
        in_river == 0,
    );
    report.check(
        format!("the column behind goes round by the upstream bridge within {REROUTE_SECONDS} s"),
        !behind.is_empty()
            && behind.iter().all(|b| b.2.is_some())
            && slowest.is_some_and(|s| s <= REROUTE_SECONDS * SECOND),
    );
    report.check(
        format!("the column is across and standing within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
}

/// Whether `p` is over the river with no standing deck under it.
fn in_water(arena: &Arena, world: &World, p: FxVec2) -> bool {
    let tile = (p.x.floor_int(), p.y.floor_int());
    arena.is_river(tile.0, tile.1) && !(world.structures().unwrap().iter()).any(|v| v.tile == tile)
}
