//! Prisoners after battles (docs/45-prisoners.md), on the real game data.
//!
//! - **Run down.** In Conquest, Mounted Samurai and Yari Ashigaru of the
//!   Ember Clans attack a regiment of Brood Skitterlings with no friend
//!   near. The Skitterlings break and run, the horse runs them down, and
//!   within two minutes they give themselves up. They are not told as
//!   killed, and the Clans hold them at their Clan Hall.
//! - **Ransom.** The Brood buy them back: the Clans get the price, and the
//!   Skitterlings come out at the Brood's Hive, as many as were held.
//! - **Escape.** More are taken; the Clan Hall is pulled down, and they
//!   stand up there as the Brood's again.

use gc_data::Faction;
use gc_sim::events::Event;
use gc_sim::{Command, RegimentHandle, TICKS_PER_SECOND, World};

use super::arena::{Arena, EAST, Real, WEST, centre, order, put_up, spawn};
use super::{Report, Scenario};

pub(super) const SCENARIO: Scenario = Scenario {
    name: "prisoners",
    about: "Skitterlings run down alone give themselves up, are held, ransomed and freed",
    budget_ms: 10_000,
    run,
};

const TILES: u32 = 128;
const SECOND: u32 = TICKS_PER_SECOND;

/// Hunt `skitters` with horse and spears until they give themselves up,
/// for up to two minutes: when, and whether they were told as killed.
fn run_down(
    real: &Real,
    world: &mut World,
    report: &mut Report,
    skitters: RegimentHandle,
) -> (Option<u32>, bool) {
    let hunters = spawn(
        world,
        0,
        &[
            (real.unit("Mounted Samurai"), centre(52, 64)),
            (real.unit("Yari Ashigaru"), centre(54, 60)),
        ],
        EAST,
    );
    let attack: Vec<_> = (hunters.iter())
        .map(|&regiment| {
            order(
                0,
                Command::Attack {
                    regiment,
                    target: skitters,
                },
            )
        })
        .collect();
    report.tick(world, &attack);
    let mut killed = false;
    for t in 0..120 * SECOND {
        for e in world.events() {
            match *e {
                Event::Surrendered { regiment, .. } if regiment == skitters => {
                    return (Some(t), killed);
                }
                Event::Losses { regiment, .. }
                    if regiment == skitters && world.regiments.get(skitters).is_none() =>
                {
                    killed = true;
                }
                _ => {}
            }
        }
        // Keep the hunters on them while they run.
        let again: Vec<_> = if t % (2 * SECOND) == 0 && world.regiments.get(skitters).is_some() {
            attack.clone()
        } else {
            Vec::new()
        };
        report.tick(world, &again);
    }
    (None, killed)
}

fn run(real: &Real, report: &mut Report) {
    let factions = [Some(Faction::EmberClans), Some(Faction::Brood)];
    let mut world = Arena::new(TILES).world_for(real, 45, 3000, &factions, &[]);
    let hall = put_up(&mut world, 0, &[(real.building("Clan Hall"), (10, 10), 0)])[0];
    put_up(&mut world, 1, &[(real.building("Hive"), (110, 110), 0)]);
    let skitterlings = real.unit("Skitterlings");

    // Run down.
    let skitters = spawn(&mut world, 1, &[(skitterlings, centre(62, 62))], WEST)[0];
    let (when, killed) = run_down(real, &mut world, report, skitters);
    report.metric(
        "Skitterlings gave themselves up after",
        super::seconds(when),
    );
    report.check(
        "Skitterlings run down alone give themselves up, not told as killed",
        when.is_some() && !killed,
    );
    let held: Vec<_> = (world.prisoners_held().iter())
        .filter(|h| h.owner == 1 && h.captor == 0)
        .copied()
        .collect();
    let soldiers: u16 = held.iter().map(|h| h.soldiers).sum();
    report.metric("Skitterlings held", soldiers);
    report.check(
        "the Clans hold them at their Clan Hall",
        soldiers > 0 && held.iter().all(|h| h.stockade == hall),
    );

    // Ransom.
    let price = world.ransom_price(1, 0, skitterlings);
    let before = world.bank(0).map_or(0, |b| b.grub);
    let ours = world.regiments.iter().filter(|r| r.owner == 1).count();
    let ransom = order(
        1,
        Command::Ransom {
            from: 0,
            unit: skitterlings,
        },
    );
    let paid = report.tick(&mut world, &[ransom]).commands_applied == 1;
    let back = (world.regiments.iter())
        .filter(|r| r.owner == 1 && r.unit == skitterlings)
        .map(|r| r.models)
        .max();
    report.metric("ransom (Grub, Marrow, soldiers)", format!("{price:?}"));
    report.check(
        "the Brood ransom them: the Clans are paid, and they come out at the Hive",
        paid && world.regiments.iter().filter(|r| r.owner == 1).count() == ours + 1
            && back == price.map(|p| p.2)
            && world.bank(0).map_or(0, |b| b.grub) > before,
    );

    // Escape.
    let more = spawn(&mut world, 1, &[(skitterlings, centre(62, 62))], WEST)[0];
    let (again, _) = run_down(real, &mut world, report, more);
    let ours = world.regiments.iter().filter(|r| r.owner == 1).count();
    report.tick(
        &mut world,
        &[order(0, Command::RemoveStructure { structure: hall })],
    );
    for _ in 0..2 * SECOND {
        report.tick(&mut world, &[]);
    }
    let freed = world.regiments.iter().filter(|r| r.owner == 1).count();
    report.check(
        "prisoners stand up as the Brood's again when the Clan Hall falls",
        again.is_some() && freed > ours && world.prisoners_held().is_empty(),
    );
}
