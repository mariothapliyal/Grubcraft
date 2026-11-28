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
//!   tower) and regiments that never rout don't reinforce, save the
//!   Concord's undead, refilled with Tithe near their Reliquaries instead
//!   (see [`crate::tithe`]). A Brood evolution reinforces as the form it
//!   grew from (see [`crate::evolve`]).
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
use crate::state::packed_state;
use crate::structures::{POWERS_FIELD, Role, Structures, nearest_point};
use crate::veins::within;
use crate::{PlayerId, UnitStats};

/// Seconds a regiment stands quiet in reach for each new soldier.
pub const REINFORCE_SECONDS: u8 = 3;
/// The same in its own walled town (see [`crate::towns`]).
pub const TOWN_REINFORCE_SECONDS: u8 = 2;
/// How far from a reinforcing building's edge its reach goes, in tiles.
pub const REINFORCE_REACH: i32 = 8;
/// The byte a [`Refill::Short`] is written as: above any count.
const REFILL_SHORT: u8 = u8::MAX;
/// How far out from a reinforcement point's edge a regiment sent to
/// reinforce stands (see [`Conquest::reinforce_spot`]), in tiles.
const SPOT_OUT: i32 = 3;

/// How a regiment's reinforcing stands (`Regiments::refill`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refill {
    /// Not reinforcing.
    Idle,
    /// The seconds it has stood quiet in reach since its last new soldier
    /// (1 up to [`REINFORCE_SECONDS`]; a snapshot may hold more). It is 1
    /// straight after a new soldier joins, so the screen shows it
    /// reinforcing throughout.
    Counting(u8),
    /// Its time is up but its owner can't pay for the next soldier: it
    /// tries again every quiet second.
    Short,
}

impl Refill {
    /// A quiet second in reach goes by: how it counts on towards `need`,
    /// or `None` once it is time to pay for a soldier. Short of pay counts
    /// as past `need`, so it tries again.
    fn count(self, need: u8) -> Option<Refill> {
        let counted = match self {
            Refill::Idle => 1,
            Refill::Counting(seconds) => seconds.saturating_add(1),
            Refill::Short => return None,
        };
        (counted <= need).then_some(Refill::Counting(counted))
    }

    /// The one byte it is hashed and written as: 0 idle, the seconds
    /// counted, or [`REFILL_SHORT`].
    fn byte(self) -> u8 {
        match self {
            Refill::Idle => 0,
            Refill::Counting(seconds) => seconds,
            Refill::Short => REFILL_SHORT,
        }
    }
}

packed_state!(Refill: u8, Refill::byte, |byte| Some(match byte {
    0 => Refill::Idle,
    REFILL_SHORT => Refill::Short,
    seconds => Refill::Counting(seconds),
}));

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

/// What a regiment pays for its new soldiers with.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pay {
    /// Grub and Marrow, at the reinforcement points.
    Grub,
    /// Tithe, near its owner's Reliquaries (the Concord's undead).
    Tithe,
}

/// What the soldier after the first `models` of a regiment of `full` costs:
/// the share of `cost` that brings it to `models + 1`.
fn share(cost: u16, models: u16, full: u16) -> u32 {
    let (cost, n) = (u32::from(cost), u32::from(full.max(1)));
    let m = u32::from(models);
    cost * (m + 1) / n - cost * m / n
}

impl Conquest {
    /// Whether regiment slot `i` is short of soldiers and of a type that
    /// reinforces, and how it pays (see [`Conquest::refills_by`]).
    pub(crate) fn short_of_men(&self, regs: &Regiments, units: &[UnitStats], i: usize) -> bool {
        self.refills_by(regs, units, i).is_some()
    }

    /// How regiment slot `i`, short of soldiers, would pay for new ones:
    /// with Grub, if something trains it (or the form it evolved from) and
    /// it can rout; with Tithe, if it is undead. `None` for a full
    /// regiment, a summoned one, one in a cocoon and any other.
    fn refills_by(&self, regs: &Regiments, units: &[UnitStats], i: usize) -> Option<Pay> {
        let unit = regs.unit[i];
        if regs.models[i] >= regs.full_models[i]
            || regs.summoned_left[i] > 0
            || regs.cocooned(i)
            || regs.turncoat[i]
        {
            return None;
        }
        if self.unit(unit).undead {
            return Some(Pay::Tithe);
        }
        let (trained, ..) = self.reinforces_as(unit);
        (trained.trained_at.is_some() && units.get(usize::from(unit)).is_some_and(|s| s.morale > 0))
            .then_some(Pay::Grub)
    }

    /// How far from its edge building slot `i` reinforces, in tiles, if it
    /// is a reinforcement point: a finished reinforcing building
    /// ([`REINFORCE_REACH`]), a linked veins building (its veins' reach)
    /// or a building powering a field (the field's), the furthest of
    /// those that apply. The one rule both
    /// the refilling and [`Conquest::reinforce_spot`] go by.
    fn point_reach(&self, i: usize) -> Option<i32> {
        let st = &self.structures;
        if !st.stands(i) || !st.finished(i) {
            return None;
        }
        let b = st.stats_of(i);
        let field = b.flags & POWERS_FIELD != 0 && b.field > 0;
        let veins = st.links_veins(i).then(|| st.vein_reach(i)).flatten();
        let spread = veins.or(field.then_some(b.field)).map(i32::from);
        let post = self.reinforcing[usize::from(st.kind_of(i))].then_some(REINFORCE_REACH);
        // A Hive is both: its veins reach further than a hall's 8 tiles.
        post.max(spread)
    }

    /// Every reinforcement point standing, as (slot, reach), into `points`.
    fn list_points(&self, points: &mut Vec<(u32, i32)>) {
        points.clear();
        points.extend(
            (0..self.structures.slots()).filter_map(|i| Some((i as u32, self.point_reach(i)?))),
        );
    }

    /// Whether `point` is in reach of one of `owner`'s reinforcement
    /// `points` (as [`Conquest::list_points`] lists them).
    fn in_reach(&self, points: &[(u32, i32)], owner: PlayerId, point: FxVec2) -> bool {
        let st = &self.structures;
        points.iter().any(|&(p, reach)| {
            let p = p as usize;
            st.owner_of(p) == owner && within(st.footprint(p), point, reach)
        })
    }

    /// Once a second: every regiment short of soldiers in reach of its
    /// side's reinforcement points, out of the fight, counts a second, and
    /// takes on a soldier when its time is up and its owner can pay.
    pub(crate) fn reinforce(&mut self, regs: &mut Regiments, units: &[UnitStats]) {
        let mut points = core::mem::take(&mut self.points);
        // Listed the first time a regiment needs them.
        let mut listed = false;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] {
                continue;
            }
            // Standing: not marching, fighting or running.
            let quiet =
                regs.still(i) && regs.engaged[i].is_none() && !regs.morale_state[i].broken();
            let pay = self.refills_by(regs, units, i).filter(|_| quiet);
            let Some(pay) = pay else {
                regs.refill[i] = Refill::Idle;
                continue;
            };
            let owner = regs.owner[i];
            let reached = match pay {
                Pay::Grub => {
                    if !listed {
                        self.list_points(&mut points);
                        listed = true;
                    }
                    self.in_reach(&points, owner, regs.position[i])
                }
                Pay::Tithe => self.tithes_at(owner, regs.position[i]),
            };
            if !reached {
                regs.refill[i] = Refill::Idle;
                continue;
            }
            let need = if regs.in_town[i] {
                TOWN_REINFORCE_SECONDS
            } else {
                REINFORCE_SECONDS
            };
            if let Some(counted) = regs.refill[i].count(need) {
                regs.refill[i] = counted;
                continue;
            }
            let (models, full) = (regs.models[i], regs.full_models[i]);
            let stats = &units[usize::from(regs.unit[i])];
            let paid = match pay {
                Pay::Grub => {
                    let (_, cost_grub, cost_marrow) = self.reinforces_as(regs.unit[i]);
                    let grub = share(cost_grub, models, full);
                    let marrow = share(cost_marrow, models, full);
                    self.economy.spend(owner, grub, marrow)
                }
                Pay::Tithe => (self.tithe).try_spend(owner, crate::tithe::tithe_per_soldier(stats)),
            };
            if !paid {
                regs.refill[i] = Refill::Short;
                continue;
            }
            regs.add_soldier(i, stats.hp_per_model);
            // Still counting for the next one, so the screen shows it
            // reinforcing throughout.
            regs.refill[i] = if models + 1 < full {
                Refill::Counting(1)
            } else {
                Refill::Idle
            };
        }
        self.points = points;
    }

    /// Whether `point` is in reach of one of `owner`'s reinforcement
    /// points now.
    pub(crate) fn reinforces_at(&self, owner: PlayerId, point: FxVec2) -> bool {
        let st = &self.structures;
        st.finished_of(owner).any(|i| {
            self.point_reach(i)
                .is_some_and(|reach| within(st.footprint(i), point, reach))
        })
    }

    /// Where a regiment of `owner`'s at `from` might stand to reinforce:
    /// a little way out from the edge of the nearest of `owner`'s
    /// reinforcement points, on the side facing `from`. `None` when
    /// `owner` has none.
    pub(crate) fn reinforce_spot(&self, owner: PlayerId, from: FxVec2) -> Option<FxVec2> {
        self.spot_by(owner, from, |i| self.point_reach(i))
    }

    /// Whether regiment slot `i` stands where it would be refilled: in
    /// reach of a reinforcement point, or for the undead of a Reliquary.
    pub(crate) fn refills_here(&self, regs: &Regiments, i: usize) -> bool {
        let (owner, at) = (regs.owner[i], regs.position[i]);
        if self.unit(regs.unit[i]).undead {
            self.tithes_at(owner, at)
        } else {
            self.reinforces_at(owner, at)
        }
    }

    /// Where regiment slot `i` might stand to be refilled (see
    /// [`Conquest::reinforce_spot`]): for the undead, by the nearest of
    /// their owner's Reliquaries.
    pub(crate) fn refill_spot(&self, regs: &Regiments, i: usize) -> Option<FxVec2> {
        let (owner, from) = (regs.owner[i], regs.position[i]);
        if !self.unit(regs.unit[i]).undead {
            return self.reinforce_spot(owner, from);
        }
        let st = &self.structures;
        self.spot_by(owner, from, |s| {
            (st.stats_of(s).flags & crate::structures::TITHE != 0)
                .then_some(crate::tithe::TITHE_TILES)
        })
    }

    /// A spot a little way out from the edge of the nearest of `owner`'s
    /// finished buildings that `reach_of` gives a reach, towards `from`.
    fn spot_by(
        &self,
        owner: PlayerId,
        from: FxVec2,
        reach_of: impl Fn(usize) -> Option<i32>,
    ) -> Option<FxVec2> {
        let st = &self.structures;
        let (i, reach) = st
            .finished_of(owner)
            .filter_map(|i| Some((i, reach_of(i)?)))
            .min_by_key(|&(i, _)| {
                let foot = st.footprint(i);
                ((nearest_point(foot, from) - from).length_sq_raw(), i)
            })?;
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
    use crate::state::{State, written};
    use crate::structures::{Armour, Trap, WallTwist};
    use crate::{
        BuildingStats, Capacities, Command, ConquestSetup, MAX_PARTIES, MatchSetup, PlayerCommand,
        RegimentHandle, TICKS_PER_SECOND, Waiting, World,
    };

    #[test]
    fn refill_is_written_as_its_old_byte() {
        let reachable = [(Refill::Idle, 0), (Refill::Short, u8::MAX)]
            .into_iter()
            .chain((1..=REINFORCE_SECONDS).map(|n| (Refill::Counting(n), n)));
        for (refill, old) in reachable {
            assert_eq!(written(refill), [old], "{refill:?}");
        }
        // And every byte reads back as itself.
        for b in 0..=u8::MAX {
            assert_eq!(Refill::read(&mut &[b][..]).map(written), Some(vec![b]));
        }
    }

    /// A check that [`Refill`] counts as the old field did.
    #[test]
    fn refill_counts_as_the_old_byte_did() {
        for b in 0..=u8::MAX {
            let refill = Refill::read(&mut &[b][..]).unwrap();
            for need in [TOWN_REINFORCE_SECONDS, REINFORCE_SECONDS] {
                // The old count: short of pay (255) stays past `need`.
                let counted = b.saturating_add(1);
                let old = (counted <= need).then_some(vec![counted]);
                assert_eq!(refill.count(need).map(written), old, "{b} {need}");
            }
        }
    }

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
        assert!(
            c.reinforces_at(0, spot),
            "the spot is a reinforcement point"
        );
        assert!(
            spot.x > FxVec2::from_ints(22, 0).x,
            "on the side facing the asker"
        );
        assert_eq!(w.reinforce_spot(1, spot), None, "player 1 has none");
    }
}
