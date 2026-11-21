//! The Brood's evolutions and the Concord's Tithe in real use
//! (docs/03-factions.md, "Brood" and "Carrion Concord"), on the real game
//! data.
//!
//! - **Cocoons.** A Brood regiment hurt in a skirmish cocoons where it
//!   stands and comes out as its evolution at the same share of strength;
//!   a second cocoon is attacked by Ember spearmen and stands put, taking
//!   blows and striking none, then comes out; a third regiment marches
//!   close past the hurt cocoon on its way.
//! - **Tithe.** Concord and Ember regiments fight by a Reliquary, fresh
//!   ones of both sides coming up as the fight thins out, and the dead pay
//!   Tithe and lie in graves. As soon as it can pay, the Concord raises the
//!   Bound at the fullest grave, into the fight (gone within their 90 s).
//!   After the fight it indentures a regiment at the fullest grave left and
//!   marches it far from its keepers (it crumbles), is refused militia from
//!   a hamlet an enemy holds and hires it from one nobody does (gone after
//!   its time), and its hurt undead by the Reliquary are refilled, paying
//!   Tithe.

use gc_data::Faction;
use gc_fixed::FxVec2;
use gc_sim::{Command, PlayerCommand, PowerTarget, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, WEST, attack_nearest, centre, move_to, put_up, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "evolve_tithe",
    about: "Brood cocoons stand, are passed and come out evolved; the Concord's Tithe, contracts and crumbling",
    budget_ms: 60_000,
    run,
};

const TILES: u32 = 128;
const SECOND: u32 = TICKS_PER_SECOND;
/// How long the Concord's reserve fights before it falls back, in ticks.
const RESERVE_FIGHTS: u32 = 6 * SECOND;

fn run(real: &Real, report: &mut Report) {
    cocoons(real, report);
    tithe(real, report);
}

/// The regiments on the field that `before` didn't hold.
fn new_since(world: &World, before: &[RegimentHandle]) -> Vec<RegimentHandle> {
    (world.regiments.iter().map(|r| r.handle))
        .filter(|h| !before.contains(h))
        .collect()
}

fn handles(world: &World) -> Vec<RegimentHandle> {
    world.regiments.iter().map(|r| r.handle).collect()
}

fn models(world: &World, h: RegimentHandle) -> u16 {
    world.regiments.get(h).map_or(0, |r| r.models)
}

fn order(player: u8, command: Command) -> PlayerCommand {
    PlayerCommand { player, command }
}

/// The Brood part: player 0 the Brood, player 1 the Ember Clans.
fn cocoons(real: &Real, report: &mut Report) {
    let mut world = Arena::new(TILES).world_of(real, 31, 3000, gc_sim::Sky::STILL, 2);
    let pustule = real.building("Pustule");
    put_up(
        &mut world,
        0,
        &[
            (real.building("Great Hive"), (10, 10), 0),
            (real.building("Chrysalis Chamber"), (18, 10), 0),
            (pustule, (10, 18), 0),
            (pustule, (14, 18), 0),
            (pustule, (18, 18), 0),
        ],
    );
    put_up(
        &mut world,
        1,
        &[(real.building("Clan Hall"), (112, 112), 0)],
    );
    let [skitter, guard] = ["Skitterlings", "Chitin Guard"].map(|n| real.unit(n));
    let [spitters, shellbacks] = ["Spitters", "Shellbacks"].map(|n| real.unit(n));
    let brood = spawn(
        &mut world,
        0,
        &[(skitter, centre(56, 64)), (guard, centre(72, 64))],
        EAST,
    );
    let (a, b) = (brood[0], brood[1]);
    let spears = spawn(
        &mut world,
        1,
        &[(real.unit("Yari Ashigaru"), centre(56, 44))],
        WEST,
    )[0];

    // A skirmish hurts the first regiment, then the spearmen march off.
    for t in 0..6 * SECOND {
        let orders = if t % (2 * SECOND) == 0 {
            attack_nearest(&world, 1, &[spears], 0)
        } else {
            Vec::new()
        };
        report.tick(&mut world, &orders);
    }
    // The spearmen wait to the north of the whole regiment; the hurt one,
    // once steady (it may have broken and run), goes back to its place.
    report.tick(&mut world, &[move_to(1, spears, centre(72, 44))]);
    let mut quiet = None;
    let mut sent_back = false;
    for t in 0..60 * SECOND {
        let r = world.regiments.get(a);
        let steady = r.is_some_and(|r| !r.engaged && !r.morale.broken());
        let orders = if steady && !sent_back {
            sent_back = true;
            vec![move_to(0, a, centre(56, 64))]
        } else {
            Vec::new()
        };
        report.tick(&mut world, &orders);
        if sent_back
            && t % SECOND == 0
            && world
                .regiments
                .get(a)
                .is_some_and(|r| !r.moving && !r.engaged)
        {
            quiet = Some(t);
            break;
        }
    }
    let hurt = models(&world, a);
    let full = real.units[usize::from(skitter)].models;
    report.metric(
        "Skitterlings left after the skirmish",
        format!("{hurt} of {full}"),
    );
    report.check(
        "the skirmish hurt the Skitterlings",
        hurt > 0 && hurt < full && quiet.is_some(),
    );

    // Both cocoon: the hurt one into Spitters, the whole one into Shellbacks.
    let ok_a = world.can_evolve(0, a, spitters);
    let ok_b = world.can_evolve(0, b, shellbacks);
    report.check(
        format!("both can evolve ({ok_a:?}, {ok_b:?})"),
        ok_a.is_ok() && ok_b.is_ok(),
    );
    let grub = world.bank(0).unwrap().grub;
    let stats = report.tick(
        &mut world,
        &[
            order(
                0,
                Command::Evolve {
                    regiment: a,
                    into: spitters,
                },
            ),
            order(
                0,
                Command::Evolve {
                    regiment: b,
                    into: shellbacks,
                },
            ),
        ],
    );
    report.check("both cocoon", stats.commands_applied == 2);
    report.metric(
        "Grub paid for the two evolutions",
        grub - world.bank(0).unwrap().grub,
    );
    let left = |world: &World, h| world.cocoon_of(h).map_or(0, |(t, _)| u32::from(t));
    let cocoon_ticks = left(&world, a).max(left(&world, b));
    report.metric("cocoon time", super::seconds(Some(cocoon_ticks)));
    let at = |world: &World, h| world.regiments.get(h).map(|r| r.position);
    let (a_at, b_at) = (at(&world, a), at(&world, b));

    // A third regiment comes up behind the hurt cocoon and marches on
    // past it, while the spearmen come back for the whole cocoon in its
    // last stretch.
    let (Some(a_pos), Some(_)) = (a_at, b_at) else {
        report.check("both cocoons stand", false);
        return;
    };
    // Its way runs close by the hurt cocoon and on to the south-east,
    // clear of the spearmen's way in from the north.
    let behind = a_pos - FxVec2::from_ints(14, 0);
    let across = a_pos + FxVec2::from_ints(12, 16);
    let c = spawn(&mut world, 0, &[(skitter, behind)], EAST)[0];
    report.tick(&mut world, &[move_to(0, c, across)]);
    let attack_from = cocoon_ticks.saturating_sub(12 * SECOND);
    let mut spears_before = None;
    let mut spears_lost_on_cocoons = 0;
    let mut cocoons_moved = false;
    let mut b_lost = 0u16;
    let mut passed = None;
    let mut a_last = models(&world, a);
    let mut b_last = models(&world, b);
    let mut emerged: [Option<(u16, u16, u16)>; 2] = [None, None];
    for t in 0..(cocoon_ticks + 2 * SECOND).max(40 * SECOND) {
        let mut orders = Vec::new();
        if t >= attack_from && t % (2 * SECOND) == 0 && world.cocoon_of(b).is_some() {
            orders.push(order(
                1,
                Command::Attack {
                    regiment: spears,
                    target: b,
                },
            ));
        }
        let spears_now = models(&world, spears);
        let in_cocoon = [world.cocoon_of(a).is_some(), world.cocoon_of(b).is_some()];
        report.tick(&mut world, &orders);
        if in_cocoon[1] {
            if t >= attack_from {
                let before = *spears_before.get_or_insert(spears_now);
                spears_lost_on_cocoons = before - models(&world, spears).min(before);
            }
            b_lost += b_last.saturating_sub(models(&world, b));
        }
        if in_cocoon[0] && world.cocoon_of(a).is_some() && at(&world, a) != a_at {
            cocoons_moved = true;
        }
        if in_cocoon[1] && world.cocoon_of(b).is_some() && at(&world, b) != b_at {
            cocoons_moved = true;
        }
        for (k, (h, last, was)) in [(a, a_last, in_cocoon[0]), (b, b_last, in_cocoon[1])]
            .into_iter()
            .enumerate()
        {
            if was && world.cocoon_of(h).is_none() {
                let r = world.regiments.get(h);
                emerged[k] = r.map(|r| (r.unit, last, r.models));
            }
        }
        a_last = models(&world, a);
        b_last = models(&world, b);
        let c_there = world
            .regiments
            .get(c)
            .is_some_and(|r| (r.position - across).length() < gc_fixed::Fx::from_int(4));
        if c_there && passed.is_none() {
            passed = Some(t);
        }
    }
    report.check("cocoons stand where they cocooned", !cocoons_moved);
    report.metric("soldiers the attacked cocoon lost", b_lost);
    report.check("the attacked cocoon took blows", b_lost > 0);
    report.check(
        format!("the spearmen lost none to the cocoons (lost {spears_lost_on_cocoons})"),
        spears_lost_on_cocoons == 0,
    );
    report.metric(
        "third regiment marched past the cocoons in",
        super::seconds(passed),
    );
    report.check("the third regiment got past the cocoons", passed.is_some());
    // A cocoon comes out at its share of strength in the new form.
    let share = |was: u16, of: u16, into: u16| -> u16 {
        let into_full = real.units[usize::from(into)].models;
        (u32::from(was) * u32::from(into_full)).div_ceil(u32::from(of)) as u16
    };
    match emerged[0] {
        Some((unit, was, now)) => {
            let want = share(was, full, spitters);
            report.metric(
                "Spitters out of the hurt cocoon",
                format!("{now} (from {was} of {full})"),
            );
            report.check(
                "the hurt Skitterlings come out as Spitters at their share",
                unit == spitters && now == want,
            );
        }
        None => report.check("the hurt Skitterlings come out", false),
    }
    match emerged[1] {
        Some((unit, was, now)) => {
            let of = real.units[usize::from(guard)].models;
            report.metric(
                "Shellbacks out of the attacked cocoon",
                format!("{now} (from {was} of {of})"),
            );
            report.check(
                "the attacked Chitin Guard come out as Shellbacks",
                unit == shellbacks && now >= 1 && now <= share(was, of, shellbacks),
            );
        }
        None => report.check("the attacked Chitin Guard come out", false),
    }
}

fn cast(power: u8, at: FxVec2) -> PlayerCommand {
    order(
        0,
        Command::UsePower {
            power,
            target: PowerTarget::Point(at),
        },
    )
}

/// The Concord part: player 0 the Carrion Concord, player 1 the Ember
/// Clans, two hamlets on the map.
fn tithe(real: &Real, report: &mut Report) {
    let hamlets = [FxVec2::from_ints(30, 100), FxVec2::from_ints(100, 28)];
    let factions = [Some(Faction::CarrionConcord), Some(Faction::EmberClans)];
    let mut world = Arena::new(TILES).world_for(real, 41, 3000, &factions, &hamlets);
    let reliquary = (61, 50);
    put_up(
        &mut world,
        0,
        &[
            (real.building("Exchange"), (8, 8), 0),
            (real.building("Reliquary"), reliquary, 0),
        ],
    );
    put_up(
        &mut world,
        1,
        &[(real.building("Clan Hall"), (112, 112), 0)],
    );
    let [pikes, bows] = ["Bone Pikes", "Crossbow Auditors"].map(|n| real.unit(n));
    let [yari, katana] = ["Yari Ashigaru", "Katana Samurai"].map(|n| real.unit(n));
    let mut concord = spawn(
        &mut world,
        0,
        &[
            (pikes, centre(56, 44)),
            (pikes, centre(56, 52)),
            (pikes, centre(56, 60)),
            (bows, centre(48, 44)),
            (bows, centre(48, 52)),
            (bows, centre(48, 60)),
        ],
        EAST,
    );
    // A reserve of Bone Pikes joins the fight for its first stretch only,
    // to come back hurt and be refilled.
    let reserve = spawn(&mut world, 0, &[(pikes, centre(52, 36))], EAST)[0];
    let mut ember = spawn(
        &mut world,
        1,
        &[
            (yari, centre(70, 44)),
            (yari, centre(70, 52)),
            (yari, centre(70, 60)),
            (katana, centre(76, 44)),
            (katana, centre(76, 52)),
            (katana, centre(76, 60)),
        ],
        WEST,
    );
    let points = |world: &World| world.bank(0).unwrap().tithe;

    // The graves with the most corpses within a raising's reach, fullest
    // first.
    let fullest = |world: &World| {
        (world.graves().map(|(at, _)| at))
            .map(|at| (at, world.corpses_near(at, 8)))
            .max_by_key(|&(_, n)| n)
    };

    // The fight by the Reliquary, until the Concord has Tithe for its
    // other contracts or five minutes have gone. As soon as it can pay,
    // it raises the Bound at the fullest grave, into the fight.
    let raise = real.power("Raise the Bound");
    let mut raised = None;
    let mut tried = false;
    let mut raised_at = 0;
    let mut bound_gone = None;
    let mut fought = 0;
    while fought < 480 * SECOND && (!tried || points(&world) < 200) {
        let mut orders = Vec::new();
        if fought % (2 * SECOND) == 0 {
            orders.extend(attack_nearest(&world, 0, &concord, 1));
            if fought < RESERVE_FIGHTS {
                orders.extend(attack_nearest(&world, 0, &[reserve], 1));
            }
            orders.extend(attack_nearest(&world, 1, &ember, 0));
        }
        if fought == RESERVE_FIGHTS {
            orders.push(move_to(0, reserve, centre(12, 110)));
        }
        // More of both sides come up while the fight thins out.
        if fought % (30 * SECOND) == 0 && fought > 0 {
            let standing = (ember.iter())
                .filter(|&&h| world.regiments.get(h).is_some_and(|r| !r.morale.broken()))
                .count();
            if standing < 4 {
                let wave = [(yari, centre(84, 46)), (katana, centre(84, 56))];
                ember.extend(spawn(&mut world, 1, &wave, WEST));
            }
            let standing = (concord.iter())
                .filter(|&&h| world.regiments.get(h).is_some_and(|r| !r.morale.broken()))
                .count();
            if standing < 4 {
                let wave = [(pikes, centre(50, 46)), (bows, centre(44, 56))];
                concord.extend(spawn(&mut world, 0, &wave, EAST));
            }
        }
        let raising = (!tried && fought % SECOND == 0 && points(&world) >= 40)
            .then(|| fullest(&world))
            .flatten()
            .filter(|&(_, corpses)| corpses >= 8);
        let before = handles(&world);
        if let Some((at, corpses)) = raising {
            orders.push(cast(raise, at));
            report.metric("corpses by the grave raised from", corpses);
        }
        report.tick(&mut world, &orders);
        if raising.is_some() {
            tried = true;
            let new = new_since(&world, &before);
            report.metric("Bound raised in the fight at", super::seconds(Some(fought)));
            report.check("the Bound are raised in the fight", new.len() == 1);
            raised = new.first().copied();
            raised_at = fought;
            report.metric("Bound Dead raised", raised.map_or(0, |h| models(&world, h)));
        }
        if bound_gone.is_none() && raised.is_some_and(|h| world.regiments.get(h).is_none()) {
            bound_gone = Some(fought - raised_at);
        }
        fought += 1;
    }
    let graves: Vec<(FxVec2, u16)> = world.graves().collect();
    let dead: u32 = graves.iter().map(|&(_, n)| u32::from(n)).sum();
    report.metric("fight by the Reliquary", super::seconds(Some(fought)));
    report.metric("Tithe after the fight", points(&world));
    report.metric("reserve left after the fight", models(&world, reserve));
    report.metric("graves, corpses", format!("{}, {dead}", graves.len()));
    report.check(
        "the dead paid Tithe for the contracts",
        points(&world) >= 170,
    );
    report.check("the dead lie in graves", dead > 0);

    // The Ember survivors go home; the Concord's form up by the Reliquary.
    let mut orders: Vec<PlayerCommand> = (ember.iter())
        .filter(|&&h| world.regiments.get(h).is_some())
        .map(|&h| move_to(1, h, centre(110, 100)))
        .collect();
    orders.extend(
        (concord.iter())
            .enumerate()
            .filter(|&(_, &h)| world.regiments.get(h).is_some())
            .map(|(i, &h)| {
                move_to(
                    0,
                    h,
                    centre(48 + 8 * (i as i32 % 2), 42 + 8 * (i as i32 / 2)),
                )
            }),
    );
    report.tick(&mut world, &orders);
    for _ in 0..20 * SECOND {
        report.tick(&mut world, &[]);
    }

    // Indenture at the fullest grave left, and march it far off.
    let indentured = fullest(&world)
        .filter(|&(_, n)| n >= 4)
        .and_then(|(at, _)| {
            let before = handles(&world);
            let stats = report.tick(&mut world, &[cast(real.power("Indenture"), at)]);
            (stats.commands_applied == 1)
                .then(|| new_since(&world, &before).first().copied())
                .flatten()
        });
    report.check("a regiment is indentured", indentured.is_some());
    let far = centre(60, 112);
    if let Some(h) = indentured {
        report.tick(&mut world, &[move_to(0, h, far)]);
    }
    let indentured_at_start = indentured.map_or(0, |h| models(&world, h));

    // Militia: refused from the hamlet an Ember regiment stands by, hired
    // from the one nobody holds.
    let before = handles(&world);
    let guard = spawn(&mut world, 1, &[(yari, centre(100, 30))], WEST)[0];
    let hire = real.power("Hire Militia");
    let held = report.tick(&mut world, &[cast(hire, hamlets[1])]);
    report.check(
        "no militia from a hamlet an enemy holds",
        held.commands_applied == 0,
    );
    let free = report.tick(&mut world, &[cast(hire, hamlets[0])]);
    let militia: Vec<_> = new_since(&world, &before)
        .into_iter()
        .filter(|&h| h != guard)
        .collect();
    report.check(
        "militia is hired from a free hamlet",
        free.commands_applied == 1 && militia.len() == 1,
    );

    // The refill: the hurt undead by the Reliquary are made whole with
    // Tithe while the rest plays out.
    let undead_by_relic: Vec<RegimentHandle> = (concord.iter().copied())
        .chain([reserve])
        .filter(|&h| world.regiments.get(h).is_some_and(|r| r.unit == pikes))
        .collect();
    let mut orders = Vec::new();
    for &h in &undead_by_relic {
        if let Some(spot) = world.refill_spot(h) {
            orders.push(move_to(0, h, spot));
        }
    }
    let soldiers_before: u32 = undead_by_relic
        .iter()
        .map(|&h| u32::from(models(&world, h)))
        .sum();
    let tithe_before = points(&world);
    report.tick(&mut world, &orders);
    let mut militia_gone = None;
    for t in 0..130 * SECOND {
        report.tick(&mut world, &[]);
        if militia_gone.is_none()
            && militia
                .first()
                .is_some_and(|&h| world.regiments.get(h).is_none())
        {
            militia_gone = Some(t);
        }
    }
    let soldiers_after: u32 = undead_by_relic
        .iter()
        .map(|&h| u32::from(models(&world, h)))
        .sum();
    report.metric(
        "undead by the Reliquary, soldiers before and after",
        format!("{soldiers_before} -> {soldiers_after}"),
    );
    report.metric(
        "Tithe before and after the refill",
        format!("{tithe_before} -> {}", points(&world)),
    );
    report.check(
        "the undead by the Reliquary were refilled",
        soldiers_after > soldiers_before,
    );
    // Raised for 90 s: gone by then, killed or let go.
    report.metric("the Bound were gone after", super::seconds(bound_gone));
    report.check(
        "the Bound go after their time",
        bound_gone.is_some_and(|t| t <= 91 * SECOND),
    );
    report.metric("the militia went after", super::seconds(militia_gone));
    report.check("the militia goes after its time", militia_gone.is_some());
    let indentured_now = indentured.map_or(0, |h| models(&world, h));
    report.metric(
        "indentured soldiers, far from its keepers",
        format!("{indentured_at_start} -> {indentured_now}"),
    );
    report.check(
        "the indentured regiment crumbles far from its keepers",
        indentured.is_some() && indentured_now < indentured_at_start,
    );
}
