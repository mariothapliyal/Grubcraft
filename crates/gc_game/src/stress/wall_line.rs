//! An Ember worker is told to draw one Palisade across open ground, a
//! belt of woods, a patch of thicket and a regiment standing on the line,
//! the way a player drags a wall across a wild map: what fits goes up at
//! once, and the worker clears and builds the rest, tile by tile, with no
//! second order.

use gc_sim::{Command, PlayerCommand, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, centre, move_to, put_up, spawn};
use super::{Report, Scenario, seconds};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "wall-through-woods",
    about: "one Palisade drawn through woods, thicket and a standing regiment all goes up",
    budget_ms: 4_000,
    run,
};

const TILES: u32 = 64;
/// The wall's row and its two ends.
const Y: i32 = 32;
const FROM: i32 = 18;
const TO: i32 = 46;
/// When the regiment in the way is marched off, as a player would.
const STEP_ASIDE_SECONDS: u32 = 20;
const LIMIT_SECONDS: u32 = 300;

fn run(real: &Real, report: &mut Report) {
    let mut arena = Arena::new(TILES);
    arena.woods(28..32, 20..44);
    arena.thicket(38..40, 28..36);
    let mut world = arena.world(real, 31, 5_000);
    put_up(&mut world, 0, &[(real.building("Clan Hall"), (6, 30), 0)]);
    let [yari, _, _] = real.ember_line();
    let hands = real.unit("Hands");
    let worker = spawn(&mut world, 0, &[(hands, centre(20, Y + 3))], EAST)[0];
    let guard = spawn(&mut world, 0, &[(yari, centre(23, Y))], EAST)[0];
    let palisade = real.building("Palisade");

    let length = (TO - FROM + 1) as usize;
    let walls = |w: &World| {
        let s = w.structures().unwrap();
        let mine = s.iter().filter(|v| v.owner == 0 && v.building == palisade);
        mine.fold((0, 0), |(n, done), v| {
            (n + 1, done + usize::from(v.finished()))
        })
    };
    let order = PlayerCommand {
        player: 0,
        command: Command::BuildWall {
            regiment: Some(worker),
            building: palisade,
            from: (FROM as i16, Y as i16),
            to: (TO as i16, Y as i16),
        },
    };
    let taken = report.tick(&mut world, &[order]).commands_applied == 1;
    let (laid, _) = walls(&world);
    let planned = world.plans().len();
    report.metric("tiles in the line", length);
    report.metric("laid at once", laid);
    report.metric("waiting on trees or soldiers", planned);
    report.check(
        "the wall is ordered though trees and soldiers stand on its line",
        taken,
    );
    report.check(
        "every tile is laid or waits as a plan: none is lost",
        laid + planned == length && planned > 0,
    );

    let mut done = None;
    for t in 1..=LIMIT_SECONDS * TICKS_PER_SECOND {
        let aside = move_to(0, guard, centre(23, Y - 8));
        let orders = [aside];
        let orders: &[PlayerCommand] = if t == STEP_ASIDE_SECONDS * TICKS_PER_SECOND {
            &orders
        } else {
            &[]
        };
        report.tick(&mut world, orders);
        if walls(&world).1 == length {
            done = Some(t);
            break;
        }
    }
    let (n, finished) = walls(&world);
    report.metric("tiles standing at the end", format!("{finished}/{length}"));
    report.metric("the whole wall stands after", seconds(done));
    report.check(
        format!("the whole wall is built within {LIMIT_SECONDS} s with no second order"),
        done.is_some() && n == length,
    );
    report.check("no plan is left over", world.plans().is_empty());
}
