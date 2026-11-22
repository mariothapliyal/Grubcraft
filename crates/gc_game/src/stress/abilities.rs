//! Hero abilities in real use (docs/44-hero-abilities.md), on the real game
//! data.
//!
//! - **Hired.** The Ember Clans hire the Daimyo at their Ancestor Shrine,
//!   the Carrion Concord the Usurer at their Vault of Names.
//! - **Locked.** At level 1 the Daimyo's ultimate, the Banner, is refused:
//!   it opens at level 6.
//! - **Rally.** Beside a Yari regiment of his own, the Daimyo casts Rally:
//!   it goes off at once, and then cools down.
//! - **Leech.** The Usurer walks up to the Yari and casts Leech on them
//!   from out of reach: he walks into reach, the cast goes off, and the
//!   Yari lose hit points.

use gc_data::Faction;
use gc_sim::abilities::CastTarget;
use gc_sim::events::Event;
use gc_sim::structures::Refusal;
use gc_sim::{Command, PlayerId, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, centre, hp, move_to, order, put_up, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "abilities",
    about: "the Daimyo's Rally and the Usurer's Leech cast, the Banner locked at level 1",
    budget_ms: 20_000,
    run,
};

const TILES: u32 = 128;
const SECOND: u32 = TICKS_PER_SECOND;

fn hero_of(world: &World, p: PlayerId) -> Option<RegimentHandle> {
    world.heroes().find(|h| h.owner == p).and_then(|h| h.regiment)
}

fn cast(p: PlayerId, hero: RegimentHandle, ability: u8, target: CastTarget) -> gc_sim::PlayerCommand {
    order(
        p,
        Command::Cast {
            regiment: hero,
            ability,
            target,
        },
    )
}

/// Whether hero `hero` cast in the last tick.
fn went_off(world: &World, hero: RegimentHandle) -> bool {
    (world.events().iter()).any(|e| matches!(*e, Event::Cast { hero: h, .. } if h == hero))
}

fn run(real: &Real, report: &mut Report) {
    let factions = [Some(Faction::EmberClans), Some(Faction::CarrionConcord)];
    let mut world = Arena::new(TILES).world_for(real, 44, 3000, &factions, &[]);
    let shrine = put_up(
        &mut world,
        0,
        &[
            (real.building("Clan Hall"), (10, 10), 0),
            (real.building("Ancestor Shrine"), (20, 10), 0),
        ],
    )[1];
    let vault = put_up(
        &mut world,
        1,
        &[
            (real.building("Counting House"), (110, 110), 0),
            (real.building("Vault of Names"), (100, 110), 0),
        ],
    )[1];

    // Hired.
    let hire = |p, structure, unit| order(p, Command::Train { structure, unit });
    report.tick(
        &mut world,
        &[
            hire(0, shrine, real.unit("The Daimyo")),
            hire(1, vault, real.unit("The Usurer")),
        ],
    );
    for _ in 0..60 * SECOND {
        if hero_of(&world, 0).is_some() && hero_of(&world, 1).is_some() {
            break;
        }
        report.tick(&mut world, &[]);
    }
    let (Some(daimyo), Some(usurer)) = (hero_of(&world, 0), hero_of(&world, 1)) else {
        report.check("the Daimyo and the Usurer are hired", false);
        return;
    };
    report.check("the Daimyo and the Usurer are hired", true);

    // Locked.
    report.check(
        "the Banner is locked at level 1",
        world.cast_check(0, daimyo, 1, CastTarget::Hero) == Err(Refusal::Locked),
    );

    // Rally, beside his Yari.
    let yari = spawn(&mut world, 0, &[(real.unit("Yari Ashigaru"), centre(60, 60))], EAST)[0];
    report.tick(&mut world, &[move_to(0, daimyo, centre(60, 56))]);
    for _ in 0..40 * SECOND {
        report.tick(&mut world, &[]);
    }
    report.tick(&mut world, &[cast(0, daimyo, 0, CastTarget::Hero)]);
    report.check("Rally goes off at once", went_off(&world, daimyo));
    report.check(
        "Rally then cools down",
        world.cast_check(0, daimyo, 0, CastTarget::Hero) == Err(Refusal::Cooling),
    );

    // Leech, from out of reach: the Usurer walks into reach first.
    report.tick(&mut world, &[move_to(1, usurer, centre(75, 60))]);
    for _ in 0..90 * SECOND {
        report.tick(&mut world, &[]);
    }
    let before = hp(&world, yari);
    let leech = cast(1, usurer, 0, CastTarget::Regiment(yari));
    let took = report.tick(&mut world, &[leech]).commands_applied == 1;
    report.check("the Usurer takes the Leech order", took);
    let mut cast_after = None;
    for t in 0..20 * SECOND {
        if went_off(&world, usurer) {
            cast_after = Some(t);
            break;
        }
        report.tick(&mut world, &[]);
    }
    report.metric("Leech went off after", super::seconds(cast_after));
    report.metric(
        "Yari hit points before and after",
        format!("{before} -> {}", hp(&world, yari)),
    );
    report.check("the Usurer walks into reach and leeches", cast_after.is_some());
    report.check("the Yari lose hit points to Leech", hp(&world, yari) < before);
}
