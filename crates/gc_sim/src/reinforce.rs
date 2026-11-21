//! Reinforcement (docs/04-economy.md, "Reinforcement"): a regiment that has
//! lost soldiers takes on new ones near its own side's reinforcement points,
//! so a veteran regiment is refilled rather than replaced.
//!
//! - Once a second, a regiment short of soldiers that stands within reach of
//!   one of its owner's reinforcement points, out of the fight, counts a
//!   second. Every [`REINFORCE_SECONDS`] (in its own walled town,
//!   [`TOWN_REINFORCE_SECONDS`]) a new soldier at full hit points joins its
//!   back rank, and its owner pays his share of what the regiment cost.
//! - Reinforcement points, the same rule for every faction: within
//!   [`REINFORCE_REACH`] tiles of a finished main building, outpost hall or
//!   building that trains soldiers (the Clans' Hall, Dojo and Stables, the
//!   Concord's Guildhall), anywhere on the owner's linked root-veins (the
//!   Brood) and anywhere in the owner's fields of power (the Choir).
//! - It doesn't reinforce while fighting, routing or shattered, and a hurt
//!   regiment (any hit point lost, to a blow, an arrow, fire or a fall)
//!   starts counting again. Units nothing trains (summoned, raised by a
//!   tower, evolved) and regiments that never rout (the Concord's undead,
//!   who will be refilled only with Tithe) don't reinforce.
//! - Short of Grub or Marrow, it waits at the point, marked as short, and
//!   takes the next soldier as soon as its owner can pay. Paying the share
//!   of each soldier exactly (by whole numbers, the regiment's cost times
//!   its soldiers after, less times its soldiers before, over its full
//!   strength), refilling a regiment from any strength costs what those
//!   soldiers were worth of it new, no more.
//! - Experience is kept: the new soldiers join a veteran regiment at its
//!   rank (see [`crate::veterancy`]).

use alloc::vec::Vec;

use gc_fixed::{Fx, FxVec2};

use crate::conquest::{Conquest, ConquestUnit};
use crate::regiments::Regiments;
use crate::structures::{POWERS_FIELD, Role, Structures, VEINS};
use crate::veins::within;
use crate::{PlayerId, UnitStats};

/// Seconds a regiment stands quiet in reach for each new soldier.
pub const REINFORCE_SECONDS: u8 = 3;
/// The same in its own walled town (see [`crate::towns`]).
pub const TOWN_REINFORCE_SECONDS: u8 = 2;
/// How far from a reinforcing building's edge its reach goes, in tiles.
pub const REINFORCE_REACH: i32 = 8;
/// The bit of `Regiments::refill` saying its owner can't pay for the next
/// soldier; the rest count its seconds.
pub(crate) const REFILL_SHORT: u8 = 0x80;
const REFILL_SECONDS: u8 = !REFILL_SHORT;
/// How far out from a reinforcement point's edge a regiment sent to
/// reinforce stands (see [`Conquest::reinforce_spot`]), in tiles.
const SPOT_OUT: i32 = 3;

/// Which building types reinforce, by building id: main buildings,
/// outposts, and every building that trains a unit (or is a tier upgrade
/// of one that does).
pub(crate) fn reinforcing_types(structures: &Structures, units: &[ConquestUnit]) -> Vec<bool> {
    (structures.buildings().iter().enumerate())
        .map(|(b, stats)| {
            matches!(stats.role, Role::Main | Role::Outpost)
                || units.iter().any(|u| {
                    u.trained_at
                        .is_some_and(|at| structures.grew_from(b as u16, at))
                })
        })
        .collect()
}

/// What the soldier after the first `models` of a regiment of `full` costs:
/// the share of `cost` that brings it to `models + 1`.
fn share(cost: u16, models: u16, full: u16) -> u32 {
    let (cost, n) = (u32::from(cost), u32::from(full.max(1)));
    let m = u32::from(models);
    cost * (m + 1) / n - cost * m / n
}

impl Conquest {
    /// Whether regiment slot `i`'s type reinforces at all: something
    /// trains it, and it can rout (the undead wait for Tithe).
    pub(crate) fn refills(&self, regs: &Regiments, units: &[UnitStats], i: usize) -> bool {
        let unit = regs.unit[i];
        self.unit(unit).trained_at.is_some()
            && units.get(usize::from(unit)).is_some_and(|s| s.morale > 0)
            && regs.summoned_left[i] == 0
    }

    /// Whether regiment slot `i` is short of soldiers and could reinforce
    /// now, as far as it is concerned: not fighting or broken.
    fn wants_men(&self, regs: &Regiments, units: &[UnitStats], i: usize) -> bool {
        regs.models[i] < regs.full_models[i]
            && regs.engaged[i].is_none()
            && !regs.morale_state[i].broken()
            && self.refills(regs, units, i)
    }

    /// Whether `point` is in reach of one of `owner`'s reinforcement
    /// points; `posts` lists the reinforcing building slots standing
    /// finished.
    fn in_reach(&self, posts: &[u32], owner: PlayerId, point: FxVec2) -> bool {
        let st = &self.structures;
        posts.iter().any(|&p| {
            let p = p as usize;
            st.owner_of(p) == owner && within(st.footprint(p), point, REINFORCE_REACH)
        }) || st.on_veins(owner, point)
            || st.power_at(owner, point)
    }

    /// The reinforcing building slots standing finished, into `posts`.
    fn list_posts(&self, posts: &mut Vec<u32>) {
        let st = &self.structures;
        posts.clear();
        posts.extend(
            st.standing()
                .filter(|&i| st.finished(i) && self.reinforcing[usize::from(st.kind_of(i))])
                .map(|i| i as u32),
        );
    }

    /// Once a second: every regiment short of soldiers in reach of its
    /// side's reinforcement points counts a second, and takes on a soldier
    /// when its time is up and its owner can pay.
    pub(crate) fn reinforce(&mut self, regs: &mut Regiments, units: &[UnitStats]) {
        let mut posts = core::mem::take(&mut self.posts);
        let mut listed = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            if !self.wants_men(regs, units, i) {
                regs.refill[i] = 0;
                continue;
            }
            if !listed {
                self.list_posts(&mut posts);
                listed = true;
            }
            let owner = regs.owner[i];
            if !self.in_reach(&posts, owner, regs.position[i]) {
                regs.refill[i] = 0;
                continue;
            }
            let need = if regs.in_town[i] {
                TOWN_REINFORCE_SECONDS
            } else {
                REINFORCE_SECONDS
            };
            let counted = (regs.refill[i] & REFILL_SECONDS).saturating_add(1);
            if counted <= need {
                regs.refill[i] = counted;
                continue;
            }
            let u = self.unit(regs.unit[i]);
            let (models, full) = (regs.models[i], regs.full_models[i]);
            let grub = share(u.cost_grub, models, full);
            let marrow = share(u.cost_marrow, models, full);
            if !self.economy.spend(owner, grub, marrow) {
                regs.refill[i] = need | REFILL_SHORT;
                continue;
            }
            let hp = units[usize::from(regs.unit[i])].hp_per_model;
            regs.add_soldier(i, hp);
            // Still counting for the next one, so the screen shows it
            // reinforcing throughout.
            regs.refill[i] = if regs.models[i] < full { 1 } else { 0 };
        }
        self.posts = posts;
    }

    /// Where a regiment of `owner`'s at `from` might stand to reinforce:
    /// a little way out from the edge of the nearest of `owner`'s
    /// reinforcement points (a reinforcing building, a linked veins
    /// building or a field of power), on the side facing `from`. `None`
    /// when `owner` has none.
    pub(crate) fn reinforce_spot(&self, owner: PlayerId, from: FxVec2) -> Option<FxVec2> {
        let st = &self.structures;
        // Each candidate's slot and how far its reach goes from its edge.
        let reach_of = |i: usize| {
            let b = st.stats_of(i);
            if self.reinforcing[usize::from(st.kind_of(i))] {
                REINFORCE_REACH
            } else if b.flags & VEINS != 0 && st.rooted(i) || b.flags & POWERS_FIELD != 0 {
                i32::from(b.field)
            } else {
                0
            }
        };
        let (i, reach) = st
            .finished_of(owner)
            .map(|i| (i, reach_of(i)))
            .filter(|&(_, reach)| reach > 0)
            .min_by_key(|&(i, _)| ((st.centre(i) - from).length_sq_raw(), i))?;
        let [_, _, w, d] = st.footprint(i);
        let centre = st.centre(i);
        let dir = match (from - centre).normalized() {
            FxVec2::ZERO => FxVec2::new(Fx::ZERO, Fx::ONE),
            dir => dir,
        };
        // Out along the way to `from`, past the footprint's edge.
        let out = Fx::from_ratio(w.max(d), 2) + Fx::from_int(SPOT_OUT.min(reach));
        Some(centre + dir * out)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;
    use alloc::vec::Vec;

    use gc_fixed::FxVec2;

    use super::*;
    use crate::nav::CELLS_PER_TILE;
    use crate::scenario::{DRILL_UNITS, NORTH};
    use crate::structures::{Armour, Trap, WallTwist};
    use crate::{
        BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
        RegimentHandle, TICKS_PER_SECOND, Waiting, World,
    };

    const MAP: u32 = 96;
    const HALL: u16 = 0;
    const PYLON: u16 = 1;
    /// Drill unit 0 (24 soldiers of 10 hit points), trained at the hall
    /// for 120 Grub and 24 Marrow: 5 Grub and 1 Marrow a soldier.
    const SPEARS: u16 = 0;
    /// The same, undead: they never rout.
    const DEAD: u16 = 1;

    fn world(grub: u32) -> World {
        let side = (MAP as i32 * CELLS_PER_TILE) as usize;
        let nav = vec![1u8; side * side];
        let units = [
            DRILL_UNITS[0],
            UnitStats {
                morale: 0,
                ..DRILL_UNITS[0]
            },
        ];
        let mut w = World::new(&MatchSetup {
            seed: 3,
            capacities: Capacities { regiments: 16 },
            units: &units,
            map_tiles: MAP,
            nav: &nav,
            teams: &[],
            ground: crate::Ground::FLAT,
            champions: false,
            sky: crate::Sky::STILL,
            generals: false,
        });
        let hall = BuildingStats {
            role: Role::Main,
            tier: 1,
            upgrades_from: None,
            cost_grub: 100,
            cost_marrow: 0,
            build_ticks: 40,
            hp: 3000,
            armour: Armour::Fortified,
            width: 4,
            depth: 4,
            supply: 20,
            flags: 0,
            attack: None,
            field: 0,
            tunes: None,
            links: 0,
            twist: WallTwist::None,
            trap: Trap::None,
        };
        let pylon = BuildingStats {
            role: Role::Economy,
            width: 2,
            depth: 2,
            supply: 0,
            flags: POWERS_FIELD,
            field: 16,
            ..hall
        };
        let trained = ConquestUnit {
            supply: 2,
            cost_grub: 120,
            cost_marrow: 24,
            trained_at: Some(HALL),
            tier: 1,
            ..ConquestUnit::default()
        };
        let mut start = [(0, 0); MAX_PARTIES];
        start[0] = (grub, 1000);
        assert!(w.conquest(&ConquestSetup {
            buildings: &[hall, pylon],
            units: &[trained, trained],
            structures: 64,
            start,
            ..ConquestSetup::default()
        }));
        w
    }

    fn order(w: &mut World, command: Command) {
        let applied = w
            .tick(&[PlayerCommand { player: 0, command }])
            .commands_applied;
        assert_eq!(applied, 1);
    }

    fn put_up(w: &mut World, building: u16, tile: (i16, i16)) {
        order(
            w,
            Command::SpawnStructure {
                building,
                tile,
                turn: 0,
            },
        );
    }

    fn spawn(w: &mut World, unit: u16, x: i32, y: i32) -> RegimentHandle {
        let before: Vec<_> = w.regiments.iter().map(|r| r.handle).collect();
        let position = FxVec2::from_ints(x, y);
        order(
            w,
            Command::SpawnRegiment {
                unit,
                position,
                facing: NORTH,
            },
        );
        (w.regiments.iter().map(|r| r.handle))
            .find(|h| !before.contains(h))
            .unwrap()
    }

    /// Regiment `h` loses `n` soldiers.
    fn hurt(w: &mut World, h: RegimentHandle, n: u32) {
        let i = w.regiments.resolve(h).unwrap();
        w.regiments.take_damage(i, n * 10);
    }

    fn models(w: &World, h: RegimentHandle) -> u16 {
        w.regiments.get(h).unwrap().models
    }

    fn seconds(w: &mut World, s: u32) {
        for _ in 0..s * TICKS_PER_SECOND {
            w.tick(&[]);
        }
    }

    fn store(w: &World) -> (u32, u32) {
        w.conquest.as_ref().unwrap().economy.store(0)
    }

    #[test]
    fn shares_add_up_to_the_cost() {
        for cost in [0u16, 1, 7, 60, 100, 333] {
            for full in [1u16, 3, 12, 16, 28, 32] {
                for from in 0..full {
                    let paid: u32 = (from..full).map(|m| share(cost, m, full)).sum();
                    let (cost, from, full) = (u32::from(cost), u32::from(from), u32::from(full));
                    assert_eq!(paid, cost - cost * from / full, "{cost} {full} {from}");
                }
            }
        }
    }

    #[test]
    fn a_regiment_by_its_hall_takes_a_soldier_every_three_seconds_and_pays_his_share() {
        let mut w = world(1000);
        put_up(&mut w, HALL, (40, 40));
        // Four tiles off the hall's east side.
        let h = spawn(&mut w, SPEARS, 48, 42);
        hurt(&mut w, h, 6);
        assert_eq!(models(&w, h), 18);
        assert!(w.can_reinforce(h));
        let paid = store(&w);
        seconds(&mut w, 10);
        assert_eq!(models(&w, h), 21, "one soldier every three seconds");
        let feedback = w.regiments.feedback(h).unwrap();
        assert_eq!(feedback.waiting, Some(Waiting::Reinforcing));
        seconds(&mut w, 30);
        assert_eq!(models(&w, h), 24, "back to full and no further");
        assert!(!w.can_reinforce(h));
        assert_eq!(w.regiments.feedback(h).unwrap().waiting, None);
        let now = store(&w);
        assert_eq!(
            (paid.0 - now.0, paid.1 - now.1),
            (30, 6),
            "six soldiers' share"
        );
    }

    #[test]
    fn far_from_every_point_or_hurt_each_second_it_takes_no_one() {
        let mut w = world(1000);
        put_up(&mut w, HALL, (40, 40));
        let far = spawn(&mut w, SPEARS, 70, 70);
        let near = spawn(&mut w, SPEARS, 48, 42);
        hurt(&mut w, far, 4);
        hurt(&mut w, near, 4);
        for _ in 0..10 {
            seconds(&mut w, 1);
            let i = w.regiments.resolve(near).unwrap();
            w.regiments.wound_last(i, 1, 1);
        }
        assert_eq!(models(&w, far), 20);
        assert_eq!(models(&w, near), 20, "under fire it takes no one");
    }

    #[test]
    fn the_undead_wait_for_tithe() {
        let mut w = world(1000);
        put_up(&mut w, HALL, (40, 40));
        let h = spawn(&mut w, DEAD, 48, 42);
        hurt(&mut w, h, 4);
        assert!(!w.can_reinforce(h));
        seconds(&mut w, 10);
        assert_eq!(models(&w, h), 20);
    }

    #[test]
    fn short_of_grub_it_waits_and_says_so() {
        let mut w = world(7);
        put_up(&mut w, HALL, (40, 40));
        let h = spawn(&mut w, SPEARS, 48, 42);
        hurt(&mut w, h, 4);
        seconds(&mut w, 10);
        assert_eq!(models(&w, h), 21, "7 Grub pays for one soldier of 5");
        let feedback = w.regiments.feedback(h).unwrap();
        assert_eq!(feedback.waiting, Some(Waiting::ShortToReinforce));
        assert_eq!(store(&w).0, 2);
    }

    #[test]
    fn a_field_of_power_reinforces_and_the_spot_is_in_reach() {
        let mut w = world(1000);
        put_up(&mut w, PYLON, (20, 20));
        // Thirteen tiles off the pylon's edge: past a hall's reach, inside
        // its field.
        let h = spawn(&mut w, SPEARS, 35, 21);
        hurt(&mut w, h, 3);
        seconds(&mut w, 10);
        assert_eq!(models(&w, h), 24);
        let spot = w.reinforce_spot(0, FxVec2::from_ints(80, 21)).unwrap();
        let c = w.conquest.as_ref().unwrap();
        let mut posts = Vec::new();
        c.list_posts(&mut posts);
        assert!(
            c.in_reach(&posts, 0, spot),
            "the spot is a reinforcement point"
        );
        assert!(
            spot.x > FxVec2::from_ints(22, 0).x,
            "on the side facing the asker"
        );
        assert_eq!(w.reinforce_spot(1, spot), None, "player 1 has none");
    }
}
