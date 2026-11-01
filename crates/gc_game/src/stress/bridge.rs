//! A column of six Ember regiments crosses a two-lane Timber Bridge over a
//! deep river while enemy archers on the far bank shoot at it.

use gc_sim::{MoraleState, TICKS_PER_SECOND};

use super::arena::{
    Arena, EAST, Real, WEST, attack_nearest, centre, grid, move_to, of, soldiers, spawn,
    timber_bridge,
};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "bridge",
    about: "a column of 6 regiments crosses a two-lane bridge under archer fire",
    budget_ms: 3_000,
    run,
};

const TILES: u32 = 64;
/// The river's columns, and the bridge's two rows from bank to bank.
const RIVER: core::ops::Range<i32> = 30..34;
const ROW: i32 = 30;
const SPAN: core::ops::RangeInclusive<i32> = 27..=36;
const LIMIT_SECONDS: u32 = 70;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.river(RIVER, 0..TILES as i32);
    let mut world = arena.world(real, 12, 0);
    timber_bridge(real, &mut world, SPAN, ROW..ROW + 2);

    // In file down the west bank, two abreast, the head 7 tiles off the
    // bridge.
    let [yari, katana, yumi] = real.ember_line();
    let column: Vec<_> = ([yari, katana, yari, yumi, katana, yari].iter().enumerate())
        .map(|(i, &u)| {
            let i = i as i32;
            (u, centre(20 - 3 * i, ROW - 2 + (i % 2) * 4))
        })
        .collect();
    let column = spawn(&mut world, 0, &column, EAST);
    let archers: Vec<_> = (0..3)
        .map(|i| (yumi, grid((46, ROW + 12), 1, (0, 4), i)))
        .collect();
    let archers = spawn(&mut world, 1, &archers, WEST);
    let start = soldiers(&world, &column);
    report.metric("soldiers crossing", start);
    let orders: Vec<_> = (column.iter().enumerate())
        .map(|(i, &r)| move_to(0, r, grid((50, ROW - 6), 2, (6, 6), i)))
        .collect();
    report.tick(&mut world, &orders);

    // Regiment-ticks with the block's middle over the river off the deck.
    let mut in_river = 0u32;
    let mut done = None;
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        let shoot = if t % TICKS_PER_SECOND == 0 {
            attack_nearest(&world, 1, &archers, 0)
        } else {
            Vec::new()
        };
        report.tick(&mut world, &shoot);
        in_river += of(&world, &column)
            .filter(|v| {
                let (x, y) = (v.position.x.floor_int(), v.position.y.floor_int());
                arena.is_river(x, y) && !(ROW..=ROW + 1).contains(&y)
            })
            .count() as u32;
        // The survivors across and standing (not none left: a column
        // shot to pieces has not crossed). A routed regiment has left the
        // column: it runs from the archers until they are gone.
        let mut left = of(&world, &column)
            .filter(|v| v.morale != MoraleState::Routing)
            .peekable();
        if left.peek().is_some()
            && left.all(|v| v.position.x.floor_int() > RIVER.end + 3 && !v.moving)
        {
            done = Some(t);
            break;
        }
    }
    report.metric("all across and standing", seconds(done));
    report.metric(
        "soldiers lost to arrows or water",
        start - soldiers(&world, &column),
    );
    report.metric("regiment-ticks over the river off the deck", in_river);
    report.check(
        format!("the column is across and standing within {LIMIT_SECONDS} s"),
        done.is_some(),
    );
    report.check(
        "no regiment strays off the deck into the river",
        in_river == 0,
    );
}
