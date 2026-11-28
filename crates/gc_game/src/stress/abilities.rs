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
//! - **A computer player casts.** In a Conquest match on Ribcage Pass a
//!   General plays the Clans; the host gives it the Daimyo, then raises
//!   two Bone Pikes regiments of the Concord's 5 tiles from him: the
//!   General has him cast an ability within 10 s.

use gc_data::Faction;
use gc_sim::abilities::CastTarget;
use gc_sim::events::Event;
use gc_sim::structures::Refusal;
use gc_sim::{Command, PlayerId, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, centre, hp, move_to, order, put_up, spawn};
use super::{Report, Scenario};
use crate::conquest;
use crate::conquest_golden::{hero_of, nearest, own};
use crate::headless::Headless;

pub(super) const SCENARIO: Scenario = Scenario {
    name: "abilities",
    about: "the Daimyo's Rally and the Usurer's Leech cast, the Banner locked at level 1",
    budget_ms: 20_000,
    run,
};

const TILES: u32 = 128;
const SECOND: u32 = TICKS_PER_SECOND;

fn cast(
    p: PlayerId,
    hero: RegimentHandle,
    ability: u8,
    target: CastTarget,
) -> gc_sim::PlayerCommand {
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
    let yari = spawn(
        &mut world,
        0,
        &[(real.unit("Yari Ashigaru"), centre(60, 60))],
        EAST,
    )[0];
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
    report.check(
        "the Usurer walks into reach and leeches",
        cast_after.is_some(),
    );
    report.check(
        "the Yari lose hit points to Leech",
        hp(&world, yari) < before,
    );

    a_computer_player_casts(real, report);
}

/// A General's hero, with foes near, casts.
fn a_computer_player_casts(real: &Real, report: &mut Report) {
    use gc_fixed::{Angle, Fx};
    let factions = [Faction::EmberClans, Faction::CarrionConcord];
    let game = Headless::new(real, conquest::make_map(0, 0, 0), &factions, 44);
    let victories = gc_sim::Victories {
        annihilation: false,
        heart: false,
    };
    let mut plan = game.plan(gc_ui::lobby::Tempo::Swift, victories);
    plan.start[0] = (2000, 800);
    let mut world = game.world(&plan);
    game.open(&mut world, |w, orders| report.tick(w, orders));
    let mut minds = game.minds();
    minds.add(game.mind(0, gc_ai::Difficulty::General, 7, None));
    let start = game.map.starts()[0];
    let shrine = real.building("Ancestor Shrine");
    let (x, y) = nearest(&world, (start.0 + 10, start.1), |x, y| {
        world.can_build(shrine, (x, y), 0).is_ok()
    })
    .expect("room for the Shrine");
    let put = order(
        0,
        Command::SpawnStructure {
            building: shrine,
            tile: (x as i16, y as i16),
            turn: 0,
        },
    );
    report.tick(&mut world, &[put]);
    let structure = own(&world, 0, Some(shrine)).first().map(|v| v.handle);
    let Some(structure) = structure else {
        report.check("the host puts up the Shrine", false);
        return;
    };
    let hire = order(
        0,
        Command::Train {
            structure,
            unit: real.unit("The Daimyo"),
        },
    );
    report.tick(&mut world, &[hire]);
    let mut cmds = Vec::new();
    let mut play = |world: &mut World, report: &mut Report, extra: &[gc_sim::PlayerCommand]| {
        cmds.clear();
        minds.tick(world, &mut cmds);
        cmds.extend_from_slice(extra);
        report.tick(world, &cmds);
    };
    for _ in 0..60 * SECOND {
        if hero_of(&world, 0).is_some() {
            break;
        }
        play(&mut world, report, &[]);
    }
    let Some(daimyo) = hero_of(&world, 0) else {
        report.check("the General's Daimyo comes out", false);
        return;
    };
    let at = world.regiments.position(daimyo).expect("standing");
    let (hx, hy) = (at.x.floor_int(), at.y.floor_int());
    let pikes = real.unit("Bone Pikes");
    let foes: Vec<_> = [(5, 0), (0, 5)]
        .iter()
        .filter_map(|&(dx, dy)| {
            let (x, y) = nearest(&world, (hx + dx, hy + dy), |x, y| {
                world.can_build(shrine, (x - 1, y - 1), 0).is_ok()
                    && (x - hx).pow(2) + (y - hy).pow(2) >= 16
            })?;
            Some(order(
                1,
                Command::SpawnRegiment {
                    unit: pikes,
                    position: gc_fixed::FxVec2::new(Fx::from_int(x), Fx::from_int(y)),
                    facing: Angle(0),
                },
            ))
        })
        .collect();
    play(&mut world, report, &foes);
    let mut cast_after = None;
    for t in 0..10 * SECOND {
        if went_off(&world, daimyo) {
            cast_after = Some(t);
            break;
        }
        play(&mut world, report, &[]);
    }
    report.metric(
        "the General's Daimyo cast after",
        super::seconds(cast_after),
    );
    report.check(
        "a General's hero casts with foes near",
        cast_after.is_some(),
    );
}
