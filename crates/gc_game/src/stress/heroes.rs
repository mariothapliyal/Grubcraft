//! Heroes in real use (docs/43-heroes.md), on the real game data.
//!
//! - **Hired.** The Ember Clans put up a Clan Hall and an Ancestor Shrine
//!   and hire the Daimyo, who comes out of the Shrine 40 s later; a second
//!   hero at tier 1 is refused.
//! - **Head-on.** The Daimyo, beside a Yari regiment of his own, fights a
//!   Skitterlings regiment of the Brood's head-on: how long it takes, what
//!   it costs him, and the levels he gains from the dead.
//! - **Surrounded.** He is then sent alone into six regiments (three Chitin
//!   Guard, three Mantid Reapers) standing round him: he falls, his Yari
//!   near him lose heart, and he is revived at the Shrine at his level, for
//!   its price.
//! - **Three regiments.** Alone on open ground, two Chitin Guard and a
//!   Mantid Reapers regiment attack him: they take at least 70% of his hit
//!   points in 30 s (Mario: three regiments kill a hero in about 30 s).
//! - **Breaking out.** Revived, he is ringed by the same three, fights for
//!   3 s, and is ordered 30 tiles off: he cuts his way clear within 15 s.

use gc_data::Faction;
use gc_sim::{Command, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, WEST, centre, hp, move_to, order, put_up, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "heroes",
    about: "the Daimyo hired, fighting head-on, surrounded, against three regiments and breaking out",
    budget_ms: 30_000,
    run,
};

const TILES: u32 = 128;
const SECOND: u32 = TICKS_PER_SECOND;

fn hero_handle(world: &World) -> Option<RegimentHandle> {
    world
        .heroes()
        .find(|h| h.owner == 0)
        .and_then(|h| h.regiment)
}

fn run(real: &Real, report: &mut Report) {
    let factions = [Some(Faction::EmberClans), Some(Faction::Brood)];
    let mut world = Arena::new(TILES).world_for(real, 43, 3000, &factions, &[]);
    let [_hall, shrine] = put_up(
        &mut world,
        0,
        &[
            (real.building("Clan Hall"), (10, 10), 0),
            (real.building("Ancestor Shrine"), (20, 10), 0),
        ],
    )[..] else {
        unreachable!()
    };
    put_up(&mut world, 1, &[(real.building("Hive"), (110, 110), 0)]);
    let (daimyo, ronin) = (real.unit("The Daimyo"), real.unit("The Ronin"));

    // Hired at the Shrine.
    let hire = |unit| {
        order(
            0,
            Command::Train {
                structure: shrine,
                unit,
            },
        )
    };
    let took = report.tick(&mut world, &[hire(daimyo)]).commands_applied == 1;
    report.check("the Shrine takes the Daimyo's hire", took);
    let mut out = None;
    for t in 0..60 * SECOND {
        report.tick(&mut world, &[]);
        if out.is_none() && hero_handle(&world).is_some() {
            out = Some(t);
            break;
        }
    }
    report.metric("Daimyo out after", super::seconds(out));
    report.check("the Daimyo comes out of the Shrine", out.is_some());
    let Some(hero) = hero_handle(&world) else {
        return;
    };
    report.check(
        "a second hero is refused at tier 1",
        world.can_train(0, shrine, ronin) == Err(gc_sim::structures::Refusal::HeroLimit),
    );

    // Head-on, beside his own Yari, against a Skitterlings regiment.
    let yari = real.unit("Yari Ashigaru");
    let (skitters, guard) = (real.unit("Skitterlings"), real.unit("Chitin Guard"));
    let reapers = real.unit("Mantid Reapers");
    let ours = spawn(&mut world, 0, &[(yari, centre(60, 60))], EAST)[0];
    report.tick(&mut world, &[move_to(0, hero, centre(60, 55))]);
    for _ in 0..30 * SECOND {
        report.tick(&mut world, &[]);
    }
    let foe = spawn(&mut world, 1, &[(skitters, centre(70, 55))], WEST)[0];
    let start = hp(&world, hero);
    let attack = |world: &World| {
        let mut orders = Vec::new();
        for r in [hero, ours] {
            if world.regiments.get(r).is_some_and(|v| !v.engaged)
                && world.regiments.get(foe).is_some()
            {
                orders.push(order(
                    0,
                    Command::Attack {
                        regiment: r,
                        target: foe,
                    },
                ));
            }
        }
        orders
    };
    let mut ended = None;
    let mut hero_shaken = false;
    for t in 0..120 * SECOND {
        let orders = if t % SECOND == 0 {
            attack(&world)
        } else {
            Vec::new()
        };
        report.tick(&mut world, &orders);
        hero_shaken |= world
            .regiments
            .feedback(foe)
            .is_some_and(|f| f.why & gc_sim::morale::WHY_HERO != 0);
        let gone = world.regiments.get(foe).is_none_or(|r| r.morale.broken());
        if gone || world.regiments.get(hero).is_none() {
            ended = Some(t);
            break;
        }
    }
    let level = world.hero_of(hero).map_or(0, |h| h.level);
    report.metric(
        "Daimyo experience after the fight",
        world.hero_of(hero).map_or(0, |h| h.xp),
    );
    report.metric(
        "Skitterlings left",
        world.regiments.get(foe).map_or(0, |r| r.models),
    );
    let left = hp(&world, hero);
    report.metric("head-on fight lasted", super::seconds(ended));
    report.metric(
        "Daimyo hit points before and after",
        format!("{start} -> {left}"),
    );
    report.metric("Daimyo level after the fight", level);
    report.check(
        "the Daimyo and his Yari break the Skitterlings",
        ended.is_some() && world.regiments.get(hero).is_some(),
    );
    report.check(
        "the Skitterlings lost heart to the Daimyo's kills",
        hero_shaken,
    );
    report.check(
        "the Daimyo gained experience from the dead",
        world.hero_of(hero).is_some_and(|h| h.xp > 0),
    );

    // Surrounded: alone into six regiments round him, his Yari standing
    // off 13 tiles, out of the fight but near enough to feel his fall.
    let at = world.regiments.get(hero).unwrap().position;
    let (x, y) = (at.x.floor_int(), at.y.floor_int());
    if world.regiments.get(ours).is_some() {
        report.tick(&mut world, &[move_to(0, ours, centre(x, y - 13))]);
        for _ in 0..15 * SECOND {
            report.tick(&mut world, &[]);
        }
    }
    let around = spawn(
        &mut world,
        1,
        &[
            (guard, centre(x + 7, y)),
            (guard, centre(x - 7, y)),
            (guard, centre(x, y + 7)),
            (reapers, centre(x, y - 7)),
            (reapers, centre(x + 6, y + 6)),
            (reapers, centre(x - 6, y - 6)),
        ],
        WEST,
    );
    let friend = world.regiments.get(ours).map(|r| r.handle);
    let morale_before = friend
        .and_then(|f| world.regiments.get(f))
        .map(|r| r.morale_points);
    let mut fell = None;
    let mut hp_at = Vec::new();
    for t in 0..600 * SECOND {
        if t % (30 * SECOND) == 0 {
            hp_at.push(hp(&world, hero));
        }
        let orders: Vec<_> = if t % SECOND == 0 {
            around
                .iter()
                .filter(|&&r| world.regiments.get(r).is_some_and(|v| !v.engaged))
                .map(|&r| {
                    order(
                        1,
                        Command::Attack {
                            regiment: r,
                            target: hero,
                        },
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        report.tick(&mut world, &orders);
        if world.regiments.get(hero).is_none() {
            fell = Some(t);
            break;
        }
    }
    report.metric("surrounded, the Daimyo lasted", super::seconds(fell));
    report.metric("his hit points every 30 s", format!("{hp_at:?}"));
    report.check(
        "surrounded by six regiments, the Daimyo falls",
        fell.is_some(),
    );
    let morale_after = friend
        .and_then(|f| world.regiments.get(f))
        .map(|r| r.morale_points);
    report.metric(
        "his Yari's morale before and after his fall",
        format!("{morale_before:?} -> {morale_after:?}"),
    );

    // Revived at his level.
    let fallen = *world.heroes().find(|h| h.owner == 0).unwrap();
    report.check("a fallen hero lies dead", fallen.dead());
    let price = world.price_of(0, daimyo);
    report.metric("revive price (Grub, Marrow)", format!("{price:?}"));
    let took = report.tick(&mut world, &[hire(daimyo)]).commands_applied == 1;
    report.check("the Shrine takes the revive", took);
    let mut back = None;
    for t in 0..120 * SECOND {
        report.tick(&mut world, &[]);
        if hero_handle(&world).is_some() {
            back = Some(t);
            break;
        }
    }
    report.metric("revived after", super::seconds(back));
    let revived = world.heroes().find(|h| h.owner == 0).copied();
    report.check(
        "he comes back at the level he fell at",
        revived.is_some_and(|h| !h.dead() && h.level == fallen.level),
    );

    // Three regiments against him alone, at full health, on open ground:
    // Mario's mark is that they kill a hero in about 30 s.
    let revive = |world: &mut World, report: &mut Report| {
        report.tick(world, &[hire(daimyo)]);
        for _ in 0..120 * SECOND {
            report.tick(world, &[]);
            if let Some(h) = hero_handle(world) {
                report.tick(world, &[move_to(0, h, centre(64, 64))]);
                for _ in 0..30 * SECOND {
                    report.tick(world, &[]);
                }
                return Some(h);
            }
        }
        None
    };
    let Some(hero) = hero_handle(&world) else {
        return;
    };
    report.tick(&mut world, &[move_to(0, hero, centre(64, 64))]);
    for _ in 0..30 * SECOND {
        report.tick(&mut world, &[]);
    }
    let held = ring_fight(&mut world, report, hero, [guard, reapers], None);
    report.metric(
        "three regiments: his hit points going in, and 30 s after the first blow",
        format!("{} -> {:?}", held.hp_in, held.at_30),
    );
    report.metric("three regiments killed him in", super::seconds(held.fell));
    report.check(
        "three regiments take at least 70% of a whole Daimyo's hit points in 30 s",
        held.at_30.is_some_and(|left| left * 10 <= held.hp_in * 3),
    );

    // Breaking out: the same ring, 3 s of fighting, then out of it to open
    // ground 30 tiles off.
    let Some(hero) = revive(&mut world, report) else {
        report.check("revived for the break-out", false);
        return;
    };
    let out = ring_fight(&mut world, report, hero, [guard, reapers], Some(3 * SECOND));
    report.metric(
        "broke out of a ring of three after",
        super::seconds(out.out),
    );
    report.metric(
        "hit points going in and coming out",
        format!("{} -> {}", out.hp_in, out.hp_out),
    );
    report.check(
        "ordered out of a ring of three, the Daimyo fights his way clear within 15 s",
        out.out.is_some_and(|t| t <= 15 * SECOND),
    );
}

/// How a ring fight went: when he fell or got clear (ticks from the first
/// blow that landed on him, or from the order to leave), and his hit points.
struct RingFight {
    fell: Option<u32>,
    /// His hit points 30 s after the first blow (0 if he fell by then).
    at_30: Option<u32>,
    out: Option<u32>,
    hp_in: u32,
    hp_out: u32,
}

/// Two regiments of `units[0]` and one of `units[1]` round `hero`, 6 tiles
/// off, attack him. With `leave`, he is ordered 30 tiles away that long
/// after the first blow lands; without, he stands.
fn ring_fight(
    world: &mut World,
    report: &mut Report,
    hero: RegimentHandle,
    [guard, reapers]: [gc_sim::UnitId; 2],
    leave: Option<u32>,
) -> RingFight {
    let at = world.regiments.get(hero).unwrap().position;
    let (x, y) = (at.x.floor_int(), at.y.floor_int());
    let ring = spawn(
        world,
        1,
        &[
            (guard, centre(x + 6, y)),
            (guard, centre(x - 6, y)),
            (reapers, centre(x, y + 6)),
        ],
        WEST,
    );
    let hp_in = hp(world, hero);
    let mut fight = RingFight {
        fell: None,
        at_30: None,
        out: None,
        hp_in,
        hp_out: hp_in,
    };
    let mut first = None;
    let mut told = None;
    for t in 0..120 * SECOND {
        let mut orders: Vec<_> = Vec::new();
        if t % SECOND == 0 {
            orders.extend(
                ring.iter()
                    .filter(|&&r| world.regiments.get(r).is_some_and(|v| !v.engaged))
                    .map(|&r| {
                        order(
                            1,
                            Command::Attack {
                                regiment: r,
                                target: hero,
                            },
                        )
                    }),
            );
        }
        if first.is_none() && hp(world, hero) < hp_in {
            first = Some(t);
        }
        if first.is_some_and(|t0| t == t0 + 30 * SECOND) {
            fight.at_30 = Some(hp(world, hero));
        }
        if let (Some(t0), Some(wait), None) = (first, leave, told)
            && t == t0 + wait
        {
            orders.push(move_to(0, hero, centre(x, y - 30)));
            told = Some(t);
        }
        report.tick(world, &orders);
        let Some(v) = world.regiments.get(hero) else {
            fight.fell = first.map(|t0| t - t0);
            fight.at_30 = fight.at_30.or(Some(0));
            fight.hp_out = 0;
            break;
        };
        fight.hp_out = hp(world, hero);
        let clear = (ring.iter().filter_map(|&r| world.regiments.get(r)))
            .all(|r| (r.position - v.position).length() > gc_fixed::Fx::from_int(10));
        if let Some(t1) = told
            && t > t1
            && !v.engaged
            && clear
        {
            fight.out = Some(t - t1);
            break;
        }
    }
    for r in ring {
        if world.regiments.get(r).is_some() {
            report.tick(world, &[order(1, Command::RemoveRegiment { regiment: r })]);
        }
    }
    fight
}
