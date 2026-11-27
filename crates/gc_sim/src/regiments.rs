use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::{Combat, OPEN_GROUND, PATIENCE};
use crate::formation::{self, MAX_MODELS};
use crate::morale::{self, MoraleState};
use crate::nav::{self, Nav};
use crate::orders::{NO_PLAN, Plan, PlanIndex, Plans, Queue, Step};
use crate::reinforce::Refill;
use crate::state::{State, packed_state};
use crate::terrain::Terrain;
use crate::{Formation, PlayerId, Stances, StateHasher, TICKS_PER_SECOND, UnitId, UnitStats};

/// Generational handle to a regiment. Stale handles (to a slot that has since
/// been freed and reused) are detected and rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegimentHandle {
    pub index: u32,
    pub generation: u16,
}

/// A move order for one regiment, already validated by [`crate::World`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Order {
    pub target: FxVec2,
    /// Facing on arrival; `None` keeps the direction of travel.
    pub facing: Option<Angle>,
    /// New number of files (frontage in soldiers); 0 keeps the current one.
    pub files: u16,
    /// Tiles per second; zero or negative means the regiment's own speed.
    pub pace: Fx,
}

/// Where a regiment is in its rhythm of melee blows (see `Combat`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Strike {
    /// Strikes on its turn, once a second.
    #[default]
    Turn,
    /// Spears or pikes that have just met their foe: strike this tick.
    Early,
    /// Struck early, so it sits out its next turn: long weapons strike
    /// sooner, never more often.
    Rest,
}

/// Beyond this angle off its way (90 degrees), a regiment turns on the
/// spot before marching (see [`wheeling_pace`]).
const TURN_BEFORE_MARCH: i32 = 0x4000;
/// Within this angle off its way (45 degrees), it marches at full pace.
const WHEEL_FULL_PACE: i32 = 0x2000;
/// Beyond this angle off its way (135 degrees), a block of several men
/// faces about instead of turning: every man turns where he stands and the
/// rear rank becomes the front (see [`Regiments::face_about`]).
const ABOUT_FACE: i32 = 0x6000;
/// Ticks a block stands while it faces about.
pub const ABOUT_FACE_TICKS: u8 = 6;

/// The pace of a block marching `off` (raw angle units) away from where it
/// faces: it wheels as it marches, at full pace within [`WHEEL_FULL_PACE`],
/// easing to half pace at [`TURN_BEFORE_MARCH`]; further off it stands and
/// turns (`None`). So an order to the side gets it moving at once. In tight
/// ground, and for a run-up, only the first 45 degrees count (see
/// `step_movement`).
fn wheeling_pace(pace: Fx, off: i32) -> Option<Fx> {
    let off = off.abs();
    if off <= WHEEL_FULL_PACE {
        return Some(pace);
    }
    if off > TURN_BEFORE_MARCH {
        return None;
    }
    let span = TURN_BEFORE_MARCH - WHEEL_FULL_PACE;
    let cut = i64::from(pace.raw()) * i64::from(off - WHEEL_FULL_PACE) / i64::from(2 * span);
    Some(Fx::from_raw(pace.raw() - cut as i32).max(Fx::from_raw(1)))
}
/// An order with an explicit facing that ends within this distance is done as
/// a shuffle: the block turns toward its final facing and steps there
/// instead of wheeling round to face the way it walks (see
/// [`drilled_pace`]). An order with no facing given to a block standing
/// this near keeps the facing it has (see [`Regiments::order`]).
const SHUFFLE_DISTANCE: Fx = Fx::from_int(4);

/// How fast a block steps without turning toward the way it goes, out of
/// `pace`, for the step `delta` (`dist` long) while it faces `facing`: at
/// full pace ahead (within 45 degrees of its front), three quarters
/// sidestepping and half backing (within 45 degrees of its rear).
fn drilled_pace(pace: Fx, facing: Angle, delta: FxVec2, dist: Fx) -> Fx {
    if dist.raw() <= 0 {
        return Fx::from_raw(pace.raw() / 2);
    }
    // Cosine of 45 degrees, times the step's length.
    let diagonal = dist * Fx::from_ratio(181, 256);
    let ahead = facing.to_vec().dot(delta);
    let percent = if ahead >= diagonal {
        100
    } else if ahead <= Fx::ZERO - diagonal {
        50
    } else {
        SIDESTEP_PERCENT
    };
    Fx::from_raw((i64::from(pace.raw()) * percent / 100) as i32)
}

/// How fast a block sidesteps, out of its pace: into a small move's place
/// (see [`drilled_pace`]) and out of friends' way (see `Combat::make_way`).
pub(crate) const SIDESTEP_PERCENT: i64 = 75;
/// A waypoint short of the end of a route counts as reached within this
/// many tiles.
const WAYPOINT_NEAR: Fx = Fx::ONE;
/// A marching regiment that friends (keeping their spacing, or crowding)
/// have kept from getting on for this long marches through them for as
/// long again, so no tangle of blocks at a gap holds anyone for good.
const UNSTICK_TICKS: u8 = 2 * crate::TICKS_PER_SECOND as u8;
/// The byte a [`Stuck::Through`] is written as, from here up: this plus
/// its ticks left.
const GHOST: u8 = 200;

/// How far friends have kept a marching regiment from getting on (see
/// [`UNSTICK_TICKS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stuck {
    /// Getting on, or not marching.
    Free,
    /// Kept back by friends for this many ticks running (1 up to one
    /// short of `UNSTICK_TICKS`; a snapshot may hold up to `GHOST - 1`).
    Kept(u8),
    /// Marching on through its friends, for this many ticks after this
    /// one: it starts at `UNSTICK_TICKS`, and still marches through at 0,
    /// so it does for one tick more than it was kept back.
    Through(u8),
}

impl Stuck {
    /// Whether it marches on through its friends this tick.
    fn through(self) -> bool {
        matches!(self, Stuck::Through(_))
    }

    /// A tick marching through goes by: once its last one is done it is
    /// free again.
    fn step_through(&mut self) {
        debug_assert!(self.through(), "not marching through");
        *self = match *self {
            Stuck::Through(left) if left > 0 => Stuck::Through(left - 1),
            _ => Stuck::Free,
        };
    }

    /// Another tick kept back by friends: once it has been for
    /// `UNSTICK_TICKS`, it marches through them for as long.
    fn kept_back(&mut self) {
        debug_assert!(!self.through(), "marching through already");
        let kept = match *self {
            Stuck::Kept(ticks) => ticks + 1,
            _ => 1,
        };
        *self = if kept >= UNSTICK_TICKS {
            Stuck::Through(UNSTICK_TICKS)
        } else {
            Stuck::Kept(kept)
        };
    }

    /// The one byte it is hashed and written as: 0 free, the ticks kept
    /// back, or [`GHOST`] plus the ticks left marching through.
    fn byte(self) -> u8 {
        match self {
            Stuck::Free => 0,
            Stuck::Kept(ticks) => ticks,
            Stuck::Through(left) => GHOST + left,
        }
    }
}

packed_state!(Stuck: u8, Stuck::byte, |byte| Some(match byte {
    0 => Stuck::Free,
    ticks @ 1..GHOST => Stuck::Kept(ticks),
    byte => Stuck::Through(byte - GHOST),
}));
/// It looks for friends to keep its spacing from at most every this
/// many ticks, keeping up its last step aside in between.
const KEEP_EVERY: u8 = 4;
/// The parts of [`Regiments::keep`]: ticks to the next look, and three
/// flags.
const KEEP_APART: u8 = 0x1F;
const KEEP_PLACE: u8 = 0x20;
const KEEP_STEPPED: u8 = 0x40;
const KEEP_CROWDED: u8 = 0x80;

/// Ticks between looks at whether the way ahead has cleared, while queuing.
const RECHECK: u8 = 4;
/// The most ticks a regiment marches without looking for friends in its
/// way (see `clear`).
const CLEAR_TICKS: u8 = 8;

/// Ticks between steps of narrowing (see [`Regiments::squeeze`]), and
/// between widening by a file once there is room again.
const SQUEEZE_TICKS: u8 = 2;
/// `grow` counts in sixteenths of a tile (raw fixed point).
const GROW_UNIT: i32 = Fx::ONE.raw() / 16;
const WIDEN_TICKS: u8 = 6;
/// Room kept between a squeezed block's outermost soldiers and blocked
/// ground, in tiles.
const SQUEEZE_MARGIN: Fx = Fx::from_ratio(1, 2);
/// Room to spare, beyond that, before a squeezed block widens by a file.
const WIDEN_SPARE: Fx = Fx::from_ratio(1, 2);
/// How far beyond its front rank a marching block looks for narrowing
/// ground, in tiles.
const SQUEEZE_LOOK: Fx = Fx::from_int(2);
/// Percent of the pace a held regiment gives ground at (see
/// [`Regiments::hold`]); a charge that ploughs in still drives it at full.
pub const HOLD_DRIVEN_PERCENT: u8 = 50;
/// A run: percent of its pace, and longest, in ticks (8 seconds).
pub const RUN_PERCENT: i64 = 150;
pub const RUN_TICKS: u8 = 8 * TICKS_PER_SECOND as u8;
/// Winded after a run: ticks (10 seconds), and percent of its pace and of
/// its melee blows.
pub const WINDED_TICKS: u8 = 10 * TICKS_PER_SECOND as u8;
pub const WINDED_PACE_PERCENT: i64 = 70;
pub const WINDED_BLOW_PERCENT: u64 = 85;

/// A regiment's breath (see [`Regiments::run`]): fresh, running, or
/// winded after a run, with the ticks left of either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stride {
    /// Neither running nor winded.
    Fresh,
    /// Running, with this many ticks left (1 up to [`RUN_TICKS`]).
    Running(u8),
    /// Winded, with this many ticks left (1 up to [`WINDED_TICKS`]).
    Winded(u8),
}

impl Stride {
    pub(crate) fn running(self) -> bool {
        matches!(self, Stride::Running(_))
    }

    pub(crate) fn winded(self) -> bool {
        matches!(self, Stride::Winded(_))
    }

    /// A run starts: only a fresh regiment runs.
    fn start(&mut self) {
        debug_assert_eq!(*self, Stride::Fresh, "out of breath already");
        *self = Stride::Running(RUN_TICKS);
    }

    /// The run ends, if it is running: it is winded from now.
    fn end(&mut self) {
        if self.running() {
            *self = Stride::Winded(WINDED_TICKS);
        }
    }

    /// A tick goes by. Winded, it gets its breath back a tick. Running, it
    /// runs on a tick, or (on its last tick, or when `stop` says so) ends
    /// the run and is winded for all of [`WINDED_TICKS`] from the next.
    fn wear(&mut self, stop: impl FnOnce() -> bool) {
        *self = match *self {
            Stride::Fresh => Stride::Fresh,
            Stride::Winded(left) if left > 1 => Stride::Winded(left - 1),
            Stride::Winded(_) => Stride::Fresh,
            Stride::Running(left) if left == 1 || stop() => Stride::Winded(WINDED_TICKS),
            Stride::Running(left) => Stride::Running(left - 1),
        };
    }

    /// The two bytes it is hashed and written as: the run's ticks left,
    /// then the winding's.
    fn bytes(self) -> [u8; 2] {
        match self {
            Stride::Fresh => [0, 0],
            Stride::Running(left) => [left, 0],
            Stride::Winded(left) => [0, left],
        }
    }
}

packed_state!(Stride: [u8; 2], Stride::bytes, |bytes| match bytes {
    [0, 0] => Some(Stride::Fresh),
    [run, 0] => Some(Stride::Running(run)),
    [0, winded] => Some(Stride::Winded(winded)),
    _ => None,
});

/// How near (tiles, along each axis) the next waypoint of a block's route
/// is looked at for a narrows to narrow to fit beforehand (see `squeeze`).
const NARROWS_AHEAD: Fx = Fx::from_int(8);

/// Declares [`Regiments`] from one list of its per-regiment fields, each
/// with the value a free slot holds, and from that list alone makes the
/// pool, resets a slot, copies one from another world and writes and reads
/// one for a snapshot, and hashes it, so no field can be left out of any
/// of them (review fix R2/T6).
macro_rules! regiment_fields {
    ($($(#[$doc:meta])* $name:ident: $ty:ty = $free:expr,)*) => {
        /// Structure-of-arrays pool of regiments with a fixed capacity.
        ///
        /// Every array is allocated to full capacity up front; spawning and
        /// freeing only flip flags and push/pop a free list that never grows
        /// past capacity. A new per-regiment field goes in the list given to
        /// `regiment_fields!`.
        pub struct Regiments {
            /// Morale tenths added to (or taken from) the ceiling of every
            /// regiment of each party for the rest of the battle, by its
            /// side's champion winning or losing (see [`crate::champions`]).
            /// By party id.
            pub(crate) party_morale: Vec<i16>,
            pub(crate) alive: Vec<bool>,
            pub(crate) generation: Vec<u16>,
            /// Hit points per soldier, `MAX_MODELS` per regiment, by
            /// formation slot (slot `k` is rank `k / files`, file
            /// `k % files`; rank 0 is the front).
            pub(crate) hp: Vec<u16>,
            /// Each slot's entry in `plans`, or [`NO_PLAN`]: its queued
            /// orders and attack-move (see [`crate::orders`]).
            plan: Vec<PlanIndex>,
            plans: Plans,
            /// Why its nerve is going: `morale::WHY_` bits, for the screen
            /// only. Nothing in the sim reads it, so it is kept out of the
            /// hash and snapshots (a loaded game shows no reasons until the
            /// next knock or check).
            pub(crate) why: Vec<u16>,
            /// The brigade steering it (see [`crate::brigades`]), as a
            /// tag, or 0; with [`crate::brigades::IN_STEP`] set while it
            /// is at its place in it. Set by its brigade each tick it
            /// marches and hashed with it (see `Brigades::hash_into`), not
            /// with the regiment: a match without brigades hashes as it
            /// did before there were any. An order to move, attack or
            /// fall back clears it: the pace is that order's from then,
            /// not its brigade's.
            pub(crate) step_with: Vec<u8>,
            /// The fewest files a brigade lets it narrow to in tight
            /// ground (see [`Regiments::squeeze`]), or 0 for no say; set
            /// each tick like `step_with`.
            pub(crate) files_floor: Vec<u16>,
            /// How far along its brigade's way it is, in tiles (see
            /// [`Regiments::leaves_be`]); set each tick like `step_with`.
            pub(crate) step_arc: Vec<Fx>,
            $($(#[$doc])* pub(crate) $name: Vec<$ty>,)*
            /// Free slot indices, popped from the back. Initialised so slot
            /// 0 is used first.
            pub(crate) free: Vec<u32>,
        }

        impl Regiments {
            pub(crate) fn with_capacity(capacity: u32) -> Regiments {
                let n = capacity as usize;
                Regiments {
                    party_morale: vec![0; PlayerId::MAX as usize + 1],
                    alive: vec![false; n],
                    generation: vec![0; n],
                    hp: vec![0; n * MAX_MODELS as usize],
                    plan: vec![NO_PLAN; n],
                    plans: Plans::with_capacity(n),
                    why: vec![0; n],
                    step_with: vec![0; n],
                    files_floor: vec![0; n],
                    step_arc: vec![Fx::ZERO; n],
                    $($name: vec![$free; n],)*
                    free: (0..capacity).rev().collect(),
                }
            }

            /// Slot `i` as a free slot holds it (its handle aside).
            fn reset_slot(&mut self, i: usize) {
                self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize].fill(0);
                self.plans.release(&mut self.plan[i]);
                self.why[i] = 0;
                self.step_with[i] = 0;
                self.files_floor[i] = 0;
                self.step_arc[i] = Fx::ZERO;
                $(self.$name[i] = $free;)*
            }

            /// Slot `i`'s fields as `from` holds them (its handle aside).
            fn copy_slot(&mut self, from: &Regiments, i: usize) {
                let m = MAX_MODELS as usize;
                self.hp[i * m..][..m].copy_from_slice(&from.hp[i * m..][..m]);
                self.why[i] = from.why[i];
                $(self.$name[i] = from.$name[i];)*
                // The same players hold the same plans in both worlds.
                let copied = self.set_plan(i, from.plan(i));
                debug_assert!(copied, "no plan free for a copied regiment");
            }

            /// Slot `i`'s fields (its handle aside), into the state hash:
            /// the soldiers still standing's hit points, then every listed
            /// field.
            pub(crate) fn hash_fields(&self, i: usize, h: &mut StateHasher) {
                let m = MAX_MODELS as usize;
                for hp in &self.hp[i * m..][..self.models[i] as usize] {
                    hp.hash(h);
                }
                self.plan(i).hash(h);
                $(self.$name[i].hash(h);)*
            }

            /// Slot `i`'s fields (its handle aside), for a snapshot.
            #[cfg_attr(not(test), allow(dead_code, reason = "snapshots of a whole world come with step 6c"))]
            fn write_slot(&self, i: usize, out: &mut Vec<u8>) {
                for hp in &self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize] {
                    hp.write(out);
                }
                self.plan(i).write(out);
                $(self.$name[i].write(out);)*
            }

            /// What [`Self::write_slot`] wrote, into slot `i`.
            #[cfg_attr(not(test), allow(dead_code, reason = "snapshots of a whole world come with step 6c"))]
            fn read_slot(&mut self, i: usize, input: &mut &[u8]) -> Option<()> {
                for k in 0..MAX_MODELS as usize {
                    self.hp[i * MAX_MODELS as usize + k] = u16::read(input)?;
                }
                let plan = Plan::read(input)?;
                $(self.$name[i] = <$ty>::read(input)?;)*
                if usize::from(self.owner[i]) >= crate::MAX_PARTIES || !self.set_plan(i, plan) {
                    return None;
                }
                Some(())
            }

            /// The listed fields' names, in order.
            #[cfg(test)]
            const FIELDS: &[&str] = &[$(stringify!($name)),*];

            /// Change the `field`th listed field of slot `i` to some other
            /// value.
            #[cfg(test)]
            fn change(&mut self, i: usize, field: usize) {
                let mut k = 0;
                $(
                    if k == field {
                        self.$name[i] = crate::state::other(self.$name[i]);
                    }
                    k += 1;
                )*
                let _ = k;
            }
        }
    };
}

regiment_fields! {
    owner: PlayerId = 0,
    unit: UnitId = 0,
    /// Centre of the block.
    position: FxVec2 = FxVec2::ZERO,
    facing: Angle = Angle::ZERO,
    /// Soldiers per rank.
    files: u16 = 0,
    /// The frontage it was given (soldiers per rank): `files` is narrower
    /// while it squeezes through ground too tight for its block (see
    /// [`Regiments::squeeze`]).
    frontage: u16 = 0,
    /// Ticks until it re-forms by another file, squeezing or widening.
    reform: u8 = 0,
    /// How far its column still grows forward after narrowing, in
    /// sixteenths of a tile (see [`Regiments::squeeze`]): added to its
    /// next steps.
    grow: u8 = 0,
    /// The spot its order sent it to, while it [`Regiments::seeks_place`]:
    /// its target is that spot or a place of its own beside it (see
    /// `Combat::own_place`).
    spot: FxVec2 = FxVec2::ZERO,
    /// How closely the soldiers stand.
    formation: Formation = Formation::Line,
    /// Soldiers still standing.
    models: u16 = 0,
    /// Full marching speed, tiles per tick.
    speed: Fx = Fx::ZERO,
    /// Turn rate, angle steps per tick.
    turn: u16 = 0,
    target: Option<FxVec2> = None,
    target_facing: Option<Angle> = None,
    /// Speed for the current order, tiles per tick.
    pace: Fx = Fx::ZERO,
    /// Enemy regiment this one was ordered to attack.
    attack_target: Option<RegimentHandle> = None,
    /// The enemy it is fighting in melee this tick (the nearest one in contact).
    engaged: Option<RegimentHandle> = None,
    /// Ticks of charge bonus left.
    charge: u16 = 0,
    /// That charge's bonus, percent: the unit's own, or for braced spears
    /// a charge turned back on its charger (see `Combat::before_move`).
    charge_bonus: u8 = 0,
    strike: Strike = Strike::Turn,
    /// Ticks until a ranged regiment can loose its next volley.
    reload: u16 = 0,
    /// Damage taken but not yet a whole hit point, in 1/256ths.
    damage_carry: u16 = 0,
    /// Soldiers at full strength (the unit's), for casualty fractions.
    full_models: u16 = 0,
    /// Morale in tenths of a point (see [`crate::morale`]), and what it means.
    morale: i16 = 0,
    morale_state: MoraleState = MoraleState::Steady,
    /// Its breaking point: the percent of its full hit points it must be
    /// down to before it can rout or waver (see [`crate::morale`]), rolled
    /// when it is raised.
    break_at: u8 = 0,
    /// Experience from kills (see [`crate::veterancy`]): the hit points of
    /// the enemy soldiers it has killed, in thousandths of its own full hit
    /// points.
    xp: u16 = 0,
    /// Soldiers lost, and flank (bit 0) or rear (bit 1) hits taken, since the
    /// regiment's last morale check.
    losses: u16 = 0,
    shock: u8 = 0,
    /// Hit points lost since the regiment's last morale check, wounds and
    /// deaths alike: what its nerve feels (see [`crate::morale`]).
    bled: u16 = 0,
    /// While routing: ticks spent with no enemy near.
    rout_clock: u16 = 0,
    /// Tick the regiment last broke and ran, plus one (0: never).
    last_rout: u32 = 0,
    /// Ticks spent waiting behind a friendly block in its way (see
    /// [`crate::combat::Combat::friend_in_the_way`]), counted RECHECK at a
    /// time at each look; 0 while marching freely.
    held: u16 = 0,
    /// Ticks before it looks for friends in its way again (see
    /// `Regiments::step_movement`): standing, while it waits behind one;
    /// marching, one after a look, or more while none holding their ground
    /// were near enough to reach sooner.
    clear: u8 = 0,
    /// How deep the block overlaps its friends, in tiles (see
    /// `Combat::crowding`).
    crowd: Fx = Fx::ZERO,
    /// How far friends have kept it from marching on (see [`Stuck`]).
    stuck: Stuck = Stuck::Free,
    /// Keeping its spacing, packed in a byte to save sim memory:
    /// [`KEEP_APART`] the ticks before it looks again for friends to keep
    /// its spacing from ([`KEEP_EVERY`] apart, or longer with none near, see
    /// `Combat::keep_apart`); [`KEEP_STEPPED`] whether it marched on its
    /// last step, so friends behind can tell a block filing on ahead from
    /// one stopped in the way; [`KEEP_CROWDED`] whether it overlaps a
    /// friend past [`crate::combat::PUSHING`] (see `Combat::crowding`), so
    /// both fall out of order; [`KEEP_PLACE`] whether it marches to a spot
    /// it was sent to stand at, and takes a place of its own beside it if
    /// a friend has the spot (see `Combat::own_place`).
    keep: u8 = 0,
    /// The step aside it last found it needed, kept up between looks.
    aside: i16 = 0,
    /// Where it stood before it stepped aside to make way for friends
    /// marching through (see `Combat::make_way`): it steps back once they
    /// are by. None while it stands where it was put.
    home: Option<FxVec2> = None,
    /// Looks (see `Combat::make_way`) it has spent stepping back since the
    /// last friend marched by.
    way_wait: u8 = 0,
    /// On a small move (see `SHUFFLE_DISTANCE`): it steps at drilled pace
    /// (see [`drilled_pace`]), not the half pace of the last steps into
    /// place at the end of a march.
    drill: bool = false,
    /// Ticks left until it is back in order (0: in order).
    disorder: u8 = 0,
    /// Ticks it has stepped forward since it last halted or fought, up to
    /// [`crate::combat::RUN_UP_TICKS`]: only a run-up makes a charge.
    run_up: u8 = 0,
    /// Its real forward speed, smoothed over about the last second, in
    /// hundredths of a tile a second (see [`Regiments::step_movement`]):
    /// what a charge hits home with (see `combat::impact_percent`).
    momentum: u16 = 0,
    /// Ticks left standing while it faces about (see [`ABOUT_FACE`]).
    about_left: u8 = 0,
    /// How many times it has faced about, counting round: the screen turns
    /// its men's places round when this changes.
    abouts: u8 = 0,
    /// Melee damage dealt and taken lately, in 1/256ths of a hit point
    /// weighted by the striker's mass: who is winning the exchange, for
    /// lines that push (see [`Combat::push_lines`]). Both fade by an
    /// eighth each second.
    pressing: u32 = 0,
    pressed: u32 = 0,
    /// How fast it is giving ground, in percent of
    /// [`GIVE_GROUND`](crate::combat::GIVE_GROUND), from the exchange as it
    /// stood at its last blow.
    giving: u8 = 0,
    /// Ticks left of being driven back at full pace by a charge that
    /// ploughed into it (see `combat::ploughs`), whatever the exchange says.
    ploughed: u8 = 0,
    /// The charger that ploughed into it.
    ploughed_by: Option<RegimentHandle> = None,
    /// Room along each side of its block (front, right flank, left flank,
    /// rear) taken by foes' strikers since its last turn, in quarters of a
    /// tile (up to 63): one striker per soldier's width of side (see
    /// `combat::strikers`). Kept narrow for the sim's memory budget.
    room_taken: [u8; 4] = [0; 4],
    /// Crowded out, under orders to attack: going round its foe to a less
    /// crowded side (see `combat::Round`).
    round: Option<core::num::NonZeroU8> = None,
    /// Turns in a row it was locked in one fight with nobody able to strike
    /// (no room on the foe's side, or only a corner touching) and free to
    /// go round (see `combat::crowded_out`); while going round, ticks spent
    /// on the way. Back to 0 whenever its foe changes in `Combat::before_move`
    /// (a new fight, or none) and whenever its going round ends (see
    /// [`Regiments::end_round`]), so no fight inherits the turns of
    /// another. A fight left elsewhere (falling back, swept off, onto a
    /// wall) keeps the count until the next fight starts, which then resets
    /// it before counting.
    crowded: u8 = 0,
    /// Spears or pikes set against a charge (see [`Regiments::brace`]):
    /// standing until given another order.
    braced: bool = false,
    /// Standing fast (see [`Regiments::hold`]) until given another order.
    hold: bool = false,
    /// Keeping away from melee (see [`Regiments::set_skirmish`]), until
    /// lifted.
    skirmish: bool = false,
    /// Ticks left of a skirmisher's step back (see
    /// [`crate::combat::Combat::skirmish`]); 0 when not stepping back.
    backing: u8 = 0,
    /// Running, or winded after a run, and for how long (see
    /// [`Regiments::run`]).
    stride: Stride = Stride::Fresh,
    /// In a column or a wedge, the frontage it was asked for (the width it
    /// stood at going in, or a line drawn for it since): it forms up that
    /// wide again on leaving. 0 for none, when the width it was raised
    /// with is used. On a wall, whatever its shape, also a line drawn for
    /// it up there, which it forms up at on coming down.
    width_kept: u16 = 0,
    /// Torches lit (see [`Regiments::light`]): they burn only in the dark.
    torches: bool = false,
    /// Ticks left of pulling out of a fight in good order (see
    /// [`Regiments::fall_back`]); 0 when not falling back. It starts at
    /// [`FALL_BACK_TICKS`](crate::combat::FALL_BACK_TICKS) for the tick the
    /// order lands, when its foes still hold it for their parting blow.
    falling_back: u16 = 0,
    /// A haste (see [`crate::powers`]): ticks left (0 for none), and how
    /// many percent faster it reloads, strikes and builds.
    haste_left: u16 = 0,
    haste: u8 = 0,
    /// Ticks left of a march (see [`crate::powers`]) and how many percent
    /// faster it goes.
    march_left: u16 = 0,
    march: u8 = 0,
    /// Ticks left that it can't rout (see [`crate::powers`]).
    steadfast_left: u16 = 0,
    /// Ticks left of a summoned regiment's life (see [`crate::powers`]);
    /// 0 for one that isn't summoned. It never routs while it lasts.
    summoned_left: u16 = 0,
    /// The slot its side's general stands in, if he leads it (see
    /// [`crate::generals`]). `None` once he has fallen.
    general: Option<u8> = None,
    /// The tick a general's speech stops lifting its morale ceiling (see
    /// [`crate::generals`]); 0 for none.
    heartened_until: u32 = 0,
    /// Led this tick by its side's general, near him (see
    /// [`crate::generals`]).
    led: bool = false,
    /// Standing in its own side's walled town, as of the last survey
    /// (see [`crate::towns`]).
    in_town: bool = false,
    /// Reinforcing (see [`crate::reinforce`]): counting the seconds it
    /// has stood quiet within reach of its side's reinforcement points
    /// since its last new soldier, or short while its owner can't pay for
    /// the next one.
    refill: Refill = Refill::Idle,
    /// Standing on a wall's walkway (see [`crate::manning`]): off the
    /// walking grid, out of reach of melee from the ground, and moved only
    /// by the wall.
    on_wall: bool = false,
    /// On a wall, fighting at the head of a ladder it climbed (see
    /// [`crate::manning`]): head on, a few wide, at a disadvantage.
    climber: bool = false,
    /// On a wall with loopholes (an Ember Castle Wall, see
    /// [`crate::twists`]): its shooters reload faster.
    loophole: bool = false,
    /// Ticks until the next stone may fall on it from an Ember Castle
    /// Wall it strikes (see [`crate::twists`]).
    stoned_left: u16 = 0,
    /// Ticks left of a Choir Singing Wall's slowing (see
    /// [`crate::twists`]): half as many blows on buildings, a slower walk.
    sung_left: u16 = 0,
    /// Ticks left of being held fast by a trap (see [`crate::traps`]): it
    /// can't move from where it stands.
    trapped_left: u16 = 0,
    /// Soldiers it lost since the Concord's Mausoleum Walls last counted
    /// the dead (see [`crate::twists`]); cleared every tick.
    deaths: u16 = 0,
    /// Ticks left in a Brood cocoon (see [`crate::evolve`]), 0 for none,
    /// and the unit type it comes out as.
    cocoon_left: u16 = 0,
    cocoon_into: UnitId = 0,
    /// Seconds an undead regiment has stood out of reach of its keepers
    /// since it last crumbled (see [`crate::tithe`]).
    crumble: u8 = 0,
    /// A hero's level (see [`crate::heroes`]); 0 for a regiment that
    /// isn't a hero.
    hero_level: u8 = 0,
    /// What a hero's level adds: hit points per model past its unit's, and
    /// melee and missile damage in percent of its unit's.
    hp_bonus: u16 = 0,
    melee_bonus: u16 = 0,
    missile_bonus: u16 = 0,
    /// A hero cutting its way out of a fight (see [`Regiments::break_out`]),
    /// while it falls back.
    breaking_out: bool = false,
}

impl Regiments {
    pub fn capacity(&self) -> usize {
        self.alive.len()
    }

    pub fn len(&self) -> usize {
        self.capacity() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Create a full-strength regiment. `stats` must already be sanitised
    /// (see [`UnitStats::sanitized`]). `None` when the pool is full or the
    /// owner is past [`MAX_PARTIES`](crate::MAX_PARTIES).
    pub(crate) fn spawn(
        &mut self,
        owner: PlayerId,
        unit: UnitId,
        position: FxVec2,
        facing: Angle,
        stats: &UnitStats,
    ) -> Option<RegimentHandle> {
        if usize::from(owner) >= crate::MAX_PARTIES {
            return None;
        }
        let index = self.free.pop()?;
        let i = index as usize;
        self.reset_slot(i);
        self.alive[i] = true;
        self.owner[i] = owner;
        self.unit[i] = unit;
        self.position[i] = position;
        self.facing[i] = facing;
        self.files[i] = stats.files;
        self.frontage[i] = stats.files;
        self.models[i] = stats.models;
        self.hp[i * MAX_MODELS as usize..][..stats.models as usize].fill(stats.hp_per_model);
        self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
        self.turn[i] = (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u32::from(u16::MAX)) as u16;
        self.pace[i] = self.speed[i];
        self.full_models[i] = stats.models;
        self.morale[i] = morale::fresh(stats);
        self.break_at[i] = morale::breaking_point(stats, index, self.generation[i]);
        Some(RegimentHandle {
            index,
            generation: self.generation[i],
        })
    }

    /// Give every living regiment of type `unit` its new speed and turn rate
    /// (the rest of its stats are read from the unit table as needed).
    pub(crate) fn retune(&mut self, unit: UnitId, stats: &UnitStats) {
        for i in 0..self.alive.len() {
            if self.alive[i] && self.unit[i] == unit {
                self.retune_slot(i, stats);
            }
        }
    }

    /// Give slot `i` the speed, turn rate and kit of `stats` (see
    /// [`Regiments::retune`]).
    pub(crate) fn retune_slot(&mut self, i: usize, stats: &UnitStats) {
        {
            let old_speed = self.speed[i];
            self.speed[i] = Fx::from_raw(stats.speed.raw() / TICKS_PER_SECOND as i32);
            self.turn[i] =
                (stats.turn_rate / TICKS_PER_SECOND).clamp(1, u32::from(u16::MAX)) as u16;
            // A regiment marching at full speed keeps doing so; a slower
            // ordered pace is kept, but never above the new top speed.
            self.pace[i] = if self.pace[i] == old_speed {
                self.speed[i]
            } else {
                self.pace[i].min(self.speed[i])
            };
            // Shields taken away: the wall comes apart.
            if !stats.can_form(self.formation[i]) {
                self.break_formation(i, stats.files);
            }
            // Long weapons taken away: nothing left to brace.
            if !stats.reach {
                self.braced[i] = false;
            }
            // Bows taken away: nothing left to skirmish with.
            if !stats.can_skirmish() {
                self.skirmish[i] = false;
                self.backing[i] = 0;
            }
        }
    }

    /// Change regiment `i`'s formation. Routing regiments don't listen, and
    /// only shielded ones can form a shield wall.
    pub(crate) fn set_formation(
        &mut self,
        h: RegimentHandle,
        formation: Formation,
        units: &[UnitStats],
    ) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        // Ranks re-form out of the fight: locked in melee, or running, a
        // regiment can't change them.
        if self.morale_state[i].broken()
            || self.engaged[i].is_some()
            || !units[self.unit[i] as usize].can_form(formation)
        {
            return false;
        }
        let was = core::mem::replace(&mut self.formation[i], formation);
        // A column or a wedge has a frontage of its own; leaving one, the
        // regiment forms up as wide as when raised.
        if was != formation && formation.own_frontage() {
            // Out of the other shape with a frontage of its own (a column
            // into a wedge, say), the width to widen out to is the one
            // that shape kept, not its own narrowed frontage.
            let asked = if was.own_frontage() {
                self.kept_or(i, self.frontage[i])
            } else {
                self.frontage[i]
            };
            self.set_frontage(i, asked);
        } else if was != formation && was.own_frontage() {
            self.leave_own(i, units[self.unit[i] as usize].files);
        }
        true
    }

    /// Slot `i` drops into Line without an order (routing, retuned):
    /// out of a column or a wedge it takes back the frontage it kept, or
    /// the one it was raised with, `files`.
    pub(crate) fn break_formation(&mut self, i: usize, files: u16) {
        let was = core::mem::replace(&mut self.formation[i], Formation::Line);
        if was.own_frontage() {
            self.leave_own(i, files);
        }
    }

    /// Slot `i`, just out of a column or a wedge, forms up as wide as it
    /// kept (see `width_kept`), or else `files`.
    fn leave_own(&mut self, i: usize, files: u16) {
        let files = self.kept_or(i, files);
        self.width_kept[i] = 0;
        self.set_frontage(i, files);
    }

    /// The width slot `i` kept (see `width_kept`), or else `files`.
    pub(crate) fn kept_or(&self, i: usize, files: u16) -> u16 {
        match self.width_kept[i] {
            0 => files,
            kept => kept,
        }
    }

    pub(crate) fn despawn(&mut self, h: RegimentHandle) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        self.alive[i] = false;
        self.plans.release(&mut self.plan[i]);
        self.generation[i] = self.generation[i].wrapping_add(1);
        self.free.push(h.index);
        true
    }

    /// Make free slot `h.index` the next one [`Regiments::spawn`] fills, with
    /// `h`'s generation, so a regiment copied from another world keeps its
    /// handle. False if that slot is taken or out of range.
    pub(crate) fn claim(&mut self, h: RegimentHandle) -> bool {
        let i = h.index as usize;
        if i >= self.alive.len() || self.alive[i] {
            return false;
        }
        let Some(k) = self.free.iter().rposition(|&f| f == h.index) else {
            return false;
        };
        let index = self.free.remove(k);
        self.free.push(index);
        self.generation[i] = h.generation;
        true
    }

    /// Copy everything about regiment slot `i` of `from` into the same slot
    /// here, which must be free.
    pub(crate) fn copy_from(&mut self, from: &Regiments, i: usize) -> bool {
        if !from.alive[i] || !self.claim(from.handle(i)) {
            return false;
        }
        self.free.pop();
        self.alive[i] = true;
        self.copy_slot(from, i);
        // What its brigade told it last tick, read again before the
        // brigade next steers (see `Brigades::copy_from`).
        self.step_with[i] = from.step_with[i];
        self.files_floor[i] = from.files_floor[i];
        self.step_arc[i] = from.step_arc[i];
        true
    }

    /// The whole pool, for a snapshot: [`Self::read`] gives it back exactly,
    /// free list and handles included.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    pub(crate) fn write(&self, out: &mut Vec<u8>) {
        (self.alive.len() as u32).write(out);
        for bonus in &self.party_morale[..crate::MAX_PARTIES] {
            bonus.write(out);
        }
        (self.free.len() as u32).write(out);
        for f in &self.free {
            f.write(out);
        }
        for i in 0..self.alive.len() {
            self.alive[i].write(out);
            self.generation[i].write(out);
            if self.alive[i] {
                self.write_slot(i, out);
            }
        }
    }

    /// A pool [`Self::write`] wrote, taken off the front of `input`; `None`
    /// if it is cut short or doesn't hold a pool.
    #[cfg_attr(
        not(test),
        allow(dead_code, reason = "snapshots of a whole world come with step 6c")
    )]
    pub(crate) fn read(input: &mut &[u8]) -> Option<Regiments> {
        let capacity = u32::read(input)?;
        // Every slot takes at least three bytes.
        if capacity as usize > input.len() / 3 {
            return None;
        }
        let mut r = Regiments::with_capacity(capacity);
        for bonus in &mut r.party_morale[..crate::MAX_PARTIES] {
            *bonus = i16::read(input)?;
        }
        let free = u32::read(input)?;
        if free > capacity {
            return None;
        }
        r.free.clear();
        for _ in 0..free {
            let f = u32::read(input)?;
            if f >= capacity {
                return None;
            }
            r.free.push(f);
        }
        for i in 0..capacity as usize {
            r.alive[i] = bool::read(input)?;
            r.generation[i] = u16::read(input)?;
            if r.alive[i] {
                r.read_slot(i, input)?;
                // What the simulation relies on of a live regiment (its
                // unit type is the caller's to check against the table).
                if r.models[i] > MAX_MODELS
                    || r.models[i] > r.full_models[i]
                    || usize::from(r.owner[i]) >= crate::MAX_PARTIES
                {
                    return None;
                }
            }
        }
        // The free list holds every other slot, once.
        let mut listed = vec![false; capacity as usize];
        for &f in &r.free {
            if r.alive[f as usize] || core::mem::replace(&mut listed[f as usize], true) {
                return None;
            }
        }
        (r.free.len() + r.alive.iter().filter(|&&a| a).count() == capacity as usize).then_some(r)
    }

    /// The handle of the regiment in slot `i`.
    pub(crate) fn handle(&self, i: usize) -> RegimentHandle {
        RegimentHandle {
            index: i as u32,
            generation: self.generation[i],
        }
    }

    pub(crate) fn resolve(&self, h: RegimentHandle) -> Option<usize> {
        let i = h.index as usize;
        (i < self.alive.len() && self.alive[i] && self.generation[i] == h.generation).then_some(i)
    }

    /// Regiment `h`'s slot, if it is alive and `player`'s.
    pub(crate) fn owned(&self, h: RegimentHandle, player: PlayerId) -> Option<usize> {
        self.resolve(h).filter(|&i| self.owner[i] == player)
    }

    /// Regiment `h`'s slot, if it is alive, `player`'s and not routing: one
    /// that takes their orders.
    pub(crate) fn standing(&self, h: RegimentHandle, player: PlayerId) -> Option<usize> {
        self.owned(h, player)
            .filter(|&i| !self.morale_state[i].broken())
    }

    /// Whether slots `i` and `j` stand where melee can reach between them:
    /// both on the ground, or both up on walls (see [`crate::manning`]).
    pub(crate) fn meet(&self, i: usize, j: usize) -> bool {
        self.on_wall[i] == self.on_wall[j]
    }

    /// The live regiment `i` is locked in melee with, if any.
    pub(crate) fn foe(&self, i: usize) -> Option<usize> {
        self.engaged[i].and_then(|h| self.resolve(h))
    }

    pub fn owner(&self, h: RegimentHandle) -> Option<PlayerId> {
        self.resolve(h).map(|i| self.owner[i])
    }

    pub fn position(&self, h: RegimentHandle) -> Option<FxVec2> {
        self.resolve(h).map(|i| self.position[i])
    }

    pub fn get(&self, h: RegimentHandle) -> Option<RegimentView> {
        self.resolve(h).map(|i| self.view(i))
    }

    /// Hit points of each standing soldier, by formation slot.
    pub fn model_hp(&self, h: RegimentHandle) -> Option<&[u16]> {
        self.resolve(h)
            .map(|i| &self.hp[i * MAX_MODELS as usize..][..self.models[i] as usize])
    }

    /// Throw regiment `i` into full disorder, as pushing through friends
    /// does (other combat rules may too); it re-forms over
    /// [`DISORDER_TICKS`](crate::combat::DISORDER_TICKS) once nothing keeps
    /// it there.
    pub(crate) fn disorder(&mut self, i: usize) {
        self.disorder[i] = crate::combat::DISORDER_TICKS;
    }

    /// Throw slot `i` into disorder for `percent` of the usual time (a
    /// charge stopped on braced points, by how hard it came), at least the
    /// usual time and at most twice it.
    pub(crate) fn disorder_for(&mut self, i: usize, percent: u64) {
        let full = u64::from(crate::combat::DISORDER_TICKS);
        self.disorder[i] = (full * percent / 100).clamp(full, 2 * full) as u8;
    }

    /// Give a move order. Routing regiments don't listen.
    pub(crate) fn order(&mut self, h: RegimentHandle, order: Order) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.morale_state[i].broken() {
            return false;
        }
        // A block standing a few tiles from where it is sent, with no
        // facing given, steps there as drilled men do: facing the way it
        // faces, sidestepping or backing, not turning round to walk and
        // turning back (see `SHUFFLE_DISTANCE`).
        // (A lone body, a worker or a beast, just turns and walks.)
        // (Standing, or already that near where it was sent: the same
        // short order given again is still one.)
        let near = |p: FxVec2| {
            (p - self.position[i]).length_sq_raw() <= (SHUFFLE_DISTANCE.raw() as u64).pow(2)
        };
        let short = self.is_block(i)
            && self.target[i].is_none_or(near)
            && self.attack_target[i].is_none()
            && near(order.target);
        self.target[i] = Some(order.target);
        self.target_facing[i] = order.facing.or(short.then_some(self.facing[i]));
        self.drill[i] = short;
        self.set_seeks_place(i, false);
        self.home[i] = None;
        self.way_wait[i] = 0;
        self.attack_target[i] = None;
        self.braced[i] = false;
        self.hold[i] = false;
        self.backing[i] = 0;
        self.falling_back[i] = 0;
        self.held[i] = 0;
        self.clear[i] = 0;
        // Growth owed on the last march (see `squeeze`) is not this one's.
        self.grow[i] = 0;
        if order.files != 0 {
            // Up on a wall it stands in Line for the wall's sake, and
            // takes back the shape it had when it comes down (see
            // `Conquest::come_down`): the width drawn for it is kept for
            // then, or it would be lost.
            if self.on_wall[i] {
                self.width_kept[i] = order.files;
            }
            self.set_frontage(i, order.files);
        }
        self.pace[i] = self.pace_for(i, order.pace);
        self.step_with[i] = 0;
        true
    }

    /// The pace slot `i` marches at, in tiles a tick, on an order to
    /// march at `pace` tiles a second (its own when zero or more than
    /// it can do).
    pub(crate) fn pace_for(&self, i: usize, pace: Fx) -> Fx {
        let pace = Fx::from_raw(pace.raw() / TICKS_PER_SECOND as i32);
        if pace.raw() <= 0 {
            self.speed[i]
        } else {
            pace.min(self.speed[i])
        }
    }

    /// Slot `i` has nothing to carry out: no march, no enemy to attack.
    pub(crate) fn idle(&self, i: usize) -> bool {
        self.target[i].is_none() && self.attack_target[i].is_none()
    }

    /// Regiment `h` forgets its queued orders and any attack-move: a
    /// player's order given without Shift has taken their place (`hunt`:
    /// the attack-move it now is).
    pub(crate) fn fresh(&mut self, h: RegimentHandle, hunt: Option<Step>) {
        if let Some(i) = self.resolve(h) {
            self.set_plan(i, Plan { hunt, ..Plan::NONE });
        }
    }

    /// Slot `i` is on the attack-move `hunt` from now, or on none (and
    /// marches plainly if no plan is free to keep it in).
    pub(crate) fn on_hunt(&mut self, i: usize, hunt: Option<Step>) {
        let plan = Plan {
            hunt,
            diverted: false,
            ..self.plan(i)
        };
        self.set_plan(i, plan);
    }

    /// Slot `i`'s queued orders and attack-move.
    pub(crate) fn plan(&self, i: usize) -> Plan {
        self.plans.get(self.plan[i])
    }

    /// Slot `i`'s queued orders and attack-move from now; false (and
    /// nothing changed) when there is no room left to keep them.
    pub(crate) fn set_plan(&mut self, i: usize, plan: Plan) -> bool {
        self.plans.set(&mut self.plan[i], self.owner[i], plan)
    }

    /// Whether slot `i` has anything queued or is on an attack-move.
    pub(crate) fn planned(&self, i: usize) -> bool {
        self.plan[i] != NO_PLAN
    }

    /// Regiment `h`'s queued orders, first first.
    pub fn queued(&self, h: RegimentHandle) -> Option<Queue> {
        self.resolve(h).map(|i| self.plan(i).queue)
    }

    /// Put a regiment straight down at `position` (deployment): no march,
    /// no order left over. `files` 0 keeps its frontage. Returns its slot.
    pub(crate) fn place(
        &mut self,
        h: RegimentHandle,
        position: FxVec2,
        facing: Angle,
        files: u16,
    ) -> Option<usize> {
        let i = self.resolve(h)?;
        self.position[i] = position;
        self.facing[i] = facing;
        if files != 0 {
            self.set_frontage(i, files);
        }
        self.halt(i);
        Some(i)
    }

    /// Whether slot `i` is narrower than the frontage it was given: in a
    /// column for tight ground, or still widening out of one (see
    /// [`Regiments::squeeze`]).
    pub(crate) fn narrowed(&self, i: usize) -> bool {
        self.files[i] < self.frontage[i].min(self.models[i])
    }

    /// Whether slot `i` is re-forming file by file just now: one narrowed
    /// that is not stands where its frontage doesn't fit, and stays so.
    pub(crate) fn re_forming(&self, i: usize) -> bool {
        self.narrowed(i) && self.reform[i] > 0
    }

    /// Slot `i` forms up `files` soldiers wide (it narrows again at once
    /// if the ground is too tight for that).
    pub(crate) fn set_frontage(&mut self, i: usize, files: u16) {
        if self.formation[i].own_frontage() {
            self.width_kept[i] = files;
        }
        let files = self.formation[i].files(files, self.models[i]);
        self.files[i] = files;
        self.frontage[i] = files;
        self.reform[i] = 0;
    }

    /// Cells a route should keep between slot `i`'s centre and blocked
    /// ground: half the larger side of its block at its full frontage,
    /// and half a tile more, between [`nav::CLEARANCE`] and
    /// [`nav::MAX_BERTH`].
    pub(crate) fn berth(&self, units: &[UnitStats], i: usize) -> u8 {
        let stats = &units[self.unit[i] as usize];
        let files = self.frontage[i].max(self.files[i]);
        Self::berth_of(stats, self.formation[i], files, self.models[i])
    }

    /// The berth of a block of `models` soldiers of `stats` in `files`
    /// files in `formation` (see [`Regiments::berth`]).
    pub(crate) fn berth_of(stats: &UnitStats, formation: Formation, files: u16, models: u16) -> u8 {
        let (w, d) = stats.block_size_in(formation, files, models);
        // Half the side in cells is the side in tiles (2 cells a tile),
        // rounded up, plus one cell.
        let cells = w.max(d).ceil_int() + 1;
        cells.clamp(i32::from(nav::CLEARANCE), i32::from(nav::MAX_BERTH)) as u8
    }

    /// Whether slot `i` is standing, not marching (it may be fighting).
    /// What the formation discipline bonus and its readouts go by.
    pub(crate) fn still(&self, i: usize) -> bool {
        self.target[i].is_none() || self.held_fast(i)
    }

    /// Whether slot `i` can't move from where it stands: held by a trap
    /// (see [`crate::traps`]) or in a cocoon (see [`crate::evolve`]).
    pub(crate) fn held_fast(&self, i: usize) -> bool {
        self.trapped_left[i] > 0 || self.cocoon_left[i] > 0
    }

    /// Whether slot `i` is in a cocoon (see [`crate::evolve`]): it can't
    /// move, strike, shoot or rout.
    pub(crate) fn cocooned(&self, i: usize) -> bool {
        self.cocoon_left[i] > 0
    }

    /// Whether slot `i` can't rout: steadfast, a summoned regiment (see
    /// [`crate::powers`]) or in a cocoon.
    pub(crate) fn cant_rout(&self, i: usize) -> bool {
        self.steadfast_left[i] > 0 || self.summoned_left[i] > 0 || self.cocoon_left[i] > 0
    }

    /// Slot `i` narrows into a deeper column where the ground beside it
    /// is too tight for its block (a ford, a pass, a gap between woods on
    /// the edge of the water), a file at a time, looking a little ahead
    /// while it marches; once there is room again it widens back to its
    /// frontage, more slowly, so it doesn't flicker. Only while it marches
    /// or is still narrowed, never in melee or braced.
    fn squeeze(&mut self, i: usize, nav: &Nav, terrain: &Terrain, units: &[UnitStats]) {
        let models = self.models[i].max(1);
        let frontage = self.frontage[i].clamp(1, models);
        let marching = self.target[i].is_some();
        let owner = self.owner[i];
        if !nav.has_map()
            || (!marching && self.files[i] == frontage && !nav.on_bridge(owner, self.position[i]))
        {
            return;
        }
        if self.reform[i] > 0 {
            self.reform[i] -= 1;
            return;
        }
        let stats = &units[self.unit[i] as usize];
        let (fs, rs) = stats.spacing(self.formation[i]);
        let look = if marching { SQUEEZE_LOOK } else { Fx::ZERO };
        // Open all round for the whole block at full frontage: no need to
        // measure.
        let (w, d) = formation::block_size(frontage, models, fs, rs);
        let reach = (w + d) / 2 + look + SQUEEZE_MARGIN + WIDEN_SPARE;
        let reach_tiles = reach
            .ceil_int()
            .min(i32::from(nav::CLEAR_CAP) / nav::CELLS_PER_TILE);
        let files = self.files[i];
        // The outermost soldiers stand half a spacing in from the block's
        // edge.
        let outer = |f: u16| Fx::from_raw(fs.raw() * i32::from(f - 1) / 2) + SQUEEZE_MARGIN;
        // Room either side of the centre, unless open all round (then the
        // whole frontage fits, however wide: the measure stops at
        // `CLEAR_CAP`).
        // On a bridge (any of the lines across the block, so the front
        // narrows as it steps on), the deck's edges over a ford count too
        // (see `Nav::room_across`).
        let half = formation::block_size(files, models, fs, rs).1 / 2;
        let lines = [half + look, half, Fx::ZERO, -half];
        let (at, ahead) = (self.position[i], self.facing[i].to_vec());
        let tight = !nav.open_around(owner, at, reach_tiles)
            || lines.iter().any(|&a| nav.on_bridge(owner, at + ahead * a));
        let reach = Fx::from_int(reach_tiles);
        let room = tight.then(|| {
            let here =
                nav.room_across(owner, at, self.facing[i], reach, &lines, |p| terrain.wet(p));
            // A narrows its route turns into just ahead (a gate, a
            // bridge's deck): it narrows to fit on the way up to it, so it
            // files in as a column instead of jamming the mouth broadside.
            // (Open all round there, it is no narrows.)
            // (Not a regiment a brigade steers: what it walks at is a
            // point just ahead of it, not a turn of its route.)
            let ahead = (self.target[i])
                .filter(|_| self.step_with[i] == 0)
                .and_then(|t| nav.steer(i, t))
                .and_then(|(w, _)| {
                    let d = w - at;
                    // (On it, there is no telling which way is across.)
                    let near =
                        d != FxVec2::ZERO && d.x.abs() < NARROWS_AHEAD && d.y.abs() < NARROWS_AHEAD;
                    let narrows = near && !nav.open_around(owner, w, reach_tiles);
                    narrows.then(|| {
                        nav.half_width_across(owner, w, Angle::of_vec(d), reach, |p| terrain.wet(p))
                    })
                });
            ahead.map_or(here, |width| here.min(width))
        });
        let fits = |f: u16, spare: Fx| room.is_none_or(|room| outer(f) + spare <= room);
        let fit = (1..=frontage)
            .rev()
            .find(|&f| fits(f, Fx::ZERO))
            .unwrap_or(1)
            .max(self.files_floor[i].min(frontage));
        if fit < files {
            // Halve the gap each step: a sudden narrows closes quickly.
            let narrower = files - (files - fit).div_ceil(2);
            // The column grows forward, not back into whoever follows: its
            // next steps take it on by half the added depth (but not an
            // attack column's, into the foe, nor a block turning to its
            // way, whose depth lies across it, nor one whose head would
            // grow into a ford, which it comes out of a block at a time).
            let deeper = formation::block_size(narrower, models, fs, rs).1 - half * 2;
            let head = at + ahead * (half + deeper);
            let along = |t| {
                nav.steer(i, t).is_some_and(|(w, _)| {
                    let d = w - at;
                    d != FxVec2::ZERO
                        && i32::from(self.facing[i].delta_to(Angle::of_vec(d))).abs()
                            <= WHEEL_FULL_PACE
                })
            };
            if self.attack_target[i].is_none()
                && !terrain.wet(at)
                && !terrain.wet(head)
                && self.target[i].is_some_and(along)
            {
                let owed = i32::from(self.grow[i]) + (deeper / 2).raw() / GROW_UNIT;
                self.grow[i] = owed.clamp(0, i32::from(u8::MAX)) as u8;
            }
            self.files[i] = narrower;
            self.reform[i] = SQUEEZE_TICKS;
        } else if fit > files && fits(files + 1, WIDEN_SPARE) {
            self.files[i] = files + 1;
            self.reform[i] = WIDEN_TICKS;
        }
    }

    /// Whether slot `j` has a move order and isn't fighting.
    /// Whether regiment `j` marched on its last step (see [`KEEP_STEPPED`]).
    pub(crate) fn stepped(&self, j: usize) -> bool {
        self.keep[j] & KEEP_STEPPED != 0
    }

    /// Whether regiment `j` overlaps a friend past
    /// [`crate::combat::PUSHING`] (see [`KEEP_CROWDED`]).
    pub(crate) fn crowded(&self, j: usize) -> bool {
        self.keep[j] & KEEP_CROWDED != 0
    }

    /// Whether slot `i` keeps no room from slot `j` because they march in
    /// one brigade and either both are in step, each at its place, or `j`
    /// is behind `i` on their way: the one behind keeps its distance, and
    /// the one ahead does not step aside for it (stepping away from a
    /// friend at its heels, it would never get across to its own lane).
    pub(crate) fn leaves_be(&self, i: usize, j: usize) -> bool {
        use crate::brigades::{IN_STEP, STEP_BEHIND};
        let (a, b) = (self.step_with[i], self.step_with[j]);
        a != 0
            && a & !IN_STEP == b & !IN_STEP
            && (a & b & IN_STEP != 0 || self.step_arc[j] + STEP_BEHIND < self.step_arc[i])
    }

    /// Whether regiment `j` marches to a spot it was sent to stand at
    /// (see [`KEEP_PLACE`]).
    pub(crate) fn seeks_place(&self, j: usize) -> bool {
        self.keep[j] & KEEP_PLACE != 0
    }

    pub(crate) fn set_seeks_place(&mut self, j: usize, seeks: bool) {
        self.set_keep(j, KEEP_PLACE, seeks);
    }

    pub(crate) fn set_crowded(&mut self, j: usize, crowded: bool) {
        self.set_keep(j, KEEP_CROWDED, crowded);
    }

    fn set_keep(&mut self, j: usize, flag: u8, on: bool) {
        self.keep[j] = if on {
            self.keep[j] | flag
        } else {
            self.keep[j] & !flag
        };
    }

    fn apart(&self, j: usize) -> u8 {
        self.keep[j] & KEEP_APART
    }

    fn set_apart(&mut self, j: usize, ticks: u8) {
        debug_assert!(ticks <= KEEP_APART);
        self.keep[j] = (self.keep[j] & !KEEP_APART) | ticks.min(KEEP_APART);
    }

    /// Whether slot `i` is a block of soldiers, which drills (small moves
    /// without turning, facing about), not a lone body (a hero, a worker,
    /// a beast).
    pub(crate) fn is_block(&self, i: usize) -> bool {
        self.models[i] > 1
    }

    /// Where slot `j` stands: where it stood before it stepped aside to
    /// make way (see `Combat::make_way`), else where it is.
    pub(crate) fn stands(&self, j: usize) -> FxVec2 {
        self.home[j].unwrap_or(self.position[j])
    }

    pub(crate) fn marching(&self, j: usize) -> bool {
        self.target[j].is_some() && self.engaged[j].is_none()
    }

    /// Whether marching slot `j` goes before marching slot `i` where the
    /// two can't pass side by side (a ford, a gap): the one nearer its
    /// goal, and between two as near, the lower slot. Every pair agrees
    /// and nobody can wait on itself round a ring, so a crowd at a gap
    /// always has one going through (see `Combat::friend_in_the_way`).
    pub(crate) fn goes_first(&self, j: usize, i: usize) -> bool {
        let (Some(gj), Some(gi)) = (self.target[j], self.target[i]) else {
            return false;
        };
        let left = |g: FxVec2, k: usize| (g - self.position[k]).length_sq_raw();
        (left(gj, j), j) < (left(gi, i), i)
    }

    /// Slot `i` drops its orders: no move, no facing to turn to, no enemy.
    pub(crate) fn halt(&mut self, i: usize) {
        self.target[i] = None;
        self.set_seeks_place(i, false);
        self.target_facing[i] = None;
        self.drill[i] = false;
        self.attack_target[i] = None;
    }

    /// Slot `i` is out of its fight: no foe, and who was winning forgotten.
    pub(crate) fn leave_fight(&mut self, i: usize) {
        self.engaged[i] = None;
        self.pressing[i] = 0;
        self.pressed[i] = 0;
        self.giving[i] = 0;
        self.ploughed[i] = 0;
        self.ploughed_by[i] = None;
    }

    /// The charger driving slot `i` back after ploughing into it: only
    /// while that lasts, `i` is in a fight and holding together, and the
    /// charger is still fighting it.
    pub(crate) fn plougher(&self, i: usize) -> Option<usize> {
        if self.ploughed[i] == 0 || self.engaged[i].is_none() || self.morale_state[i].broken() {
            return None;
        }
        self.ploughed_by[i]
            .and_then(|h| self.resolve(h))
            .filter(|&k| self.foe(k) == Some(i))
    }

    /// How fast slot `i` is being driven back, in percent of
    /// [`GIVE_GROUND`](crate::combat::GIVE_GROUND): at full pace while a
    /// charge that ploughed into it lasts, else as the exchange says.
    pub(crate) fn driven_pace(&self, i: usize) -> u8 {
        if self.plougher(i).is_some() {
            100
        } else if self.hold[i] {
            // Standing fast, it gives ground grudgingly.
            (u16::from(self.giving[i]) * u16::from(HOLD_DRIVEN_PERCENT) / 100) as u8
        } else {
            self.giving[i]
        }
    }

    /// Slot `i` is gone from its fight altogether: out of it, and nobody
    /// still locked with it to press after it.
    pub(crate) fn vanish_from_fight(&mut self, i: usize) {
        let h = self.handle(i);
        for k in 0..self.alive.len() {
            if self.engaged[k] == Some(h) {
                self.leave_fight(k);
            }
        }
        self.leave_fight(i);
    }

    /// Regiment `i` stops going round its foe (see `combat::Round`), if it
    /// was: the ticks it spent on the way are not crowded turns of its next
    /// fight.
    pub(crate) fn end_round(&mut self, i: usize) {
        self.round[i] = None;
        self.crowded[i] = 0;
    }

    /// Slot `i` makes for `foe` as an attack does, no longer marching to a
    /// spot it was sent to stand at (see [`KEEP_PLACE`]): only that part
    /// of [`Regiments::attack`], which also ends going round and lifts
    /// bracing, holding, backing and the rest, for a fight taken on in
    /// the middle of one.
    pub(crate) fn take_on(&mut self, i: usize, foe: RegimentHandle) {
        self.attack_target[i] = Some(foe);
        self.set_seeks_place(i, false);
    }

    /// Order a regiment to attack an enemy regiment: melee regiments close
    /// in and fight, ranged ones move into range and shoot.
    pub(crate) fn attack(&mut self, h: RegimentHandle, target: RegimentHandle) -> bool {
        let (Some(i), Some(t)) = (self.resolve(h), self.resolve(target)) else {
            return false;
        };
        if self.owner[i] == self.owner[t] || self.morale_state[i].broken() {
            return false;
        }
        if self.attack_target[i] != Some(target) {
            // Going round only the foe it was going round (see
            // `combat::Round`): an order repeated keeps it on its way.
            self.end_round(i);
        }
        self.take_on(i, target);
        self.target_facing[i] = None;
        self.drill[i] = false;
        self.braced[i] = false;
        self.hold[i] = false;
        self.backing[i] = 0;
        self.falling_back[i] = 0;
        self.held[i] = 0;
        self.clear[i] = 0;
        self.pace[i] = self.speed[i];
        self.step_with[i] = 0;
        true
    }

    /// Order a regiment to pull out of its fight in good order: it backs
    /// away to `to` facing the way it faces now, at half pace, and the
    /// enemies it was fighting each get one parting blow (they strike at
    /// once) but don't follow it. While it falls back nobody in front of it
    /// can lock it in melee; an enemy reaching its flank or rear catches it
    /// (see [`crate::combat::Combat::before_move`]). Routing regiments don't
    /// listen, and one already falling back carries on as it was.
    pub(crate) fn fall_back(&mut self, h: RegimentHandle, to: FxVec2) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.morale_state[i].broken() || self.falling_back[i] > 0 {
            return false;
        }
        for k in 0..self.alive.len() {
            if self.alive[k] && self.engaged[k] == Some(h) {
                self.strike[k] = Strike::Early;
            }
        }
        self.leave_fight(i);
        self.target[i] = Some(to);
        self.set_seeks_place(i, false);
        self.target_facing[i] = Some(self.facing[i]);
        // Backing away is at half pace (see `step_movement`), not the
        // drilled pace of a short move it may have been on: a drilled
        // step sideways or off straight back would go faster.
        self.drill[i] = false;
        self.attack_target[i] = None;
        self.braced[i] = false;
        self.hold[i] = false;
        self.backing[i] = 0;
        self.end_run(i);
        self.falling_back[i] = crate::combat::FALL_BACK_TICKS;
        self.held[i] = 0;
        self.clear[i] = 0;
        // Backing steps are half steps (see `step_movement`).
        self.pace[i] = self.speed[i];
        self.step_with[i] = 0;
        true
    }

    /// A hero cuts its way out of its fight towards `to` (see
    /// [`crate::heroes`]): as [`Regiments::fall_back`], each foe gets its
    /// parting blow and nobody can lock it in melee for a while, from any
    /// side, but it turns and goes at its full pace. `false` for one that
    /// isn't fighting, or is routing or already pulling out.
    ///
    /// Called after the move order is given, it keeps the way and pace that
    /// order set (a settled point, a brigade's slot); only the hold of the
    /// fight is undone. A second move while breaking out breaks out again.
    pub(crate) fn break_out(&mut self, h: RegimentHandle) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        let (target, pace) = (self.target[i], self.pace[i]);
        let Some(to) = target else {
            return false;
        };
        let again = self.breaking_out[i];
        if (self.engaged[i].is_none() && !again) || !self.fall_back(h, to) {
            return false;
        }
        (self.target[i], self.pace[i]) = (target, pace);
        self.target_facing[i] = None;
        self.breaking_out[i] = true;
        true
    }

    /// Whether slot `i` is a hero cutting its way out (see
    /// [`Regiments::break_out`]).
    pub(crate) fn breaking_out(&self, i: usize) -> bool {
        self.breaking_out[i] && self.falling_back[i] > 0
    }

    /// Slot `i`'s break-out is over: it marches on to where it was sent as
    /// any march does, and can be caught in melee again.
    pub(crate) fn end_break_out(&mut self, i: usize) {
        if self.breaking_out[i] && self.falling_back[i] > 0 {
            self.falling_back[i] = 0;
        }
        self.breaking_out[i] = false;
    }

    /// Light (or put out) a regiment's torches; returns whether that
    /// changed anything.
    pub(crate) fn light(&mut self, h: RegimentHandle, lit: bool) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        core::mem::replace(&mut self.torches[i], lit) != lit
    }

    /// Set (or lift) a regiment's spears against a charge: it halts where
    /// it stands, and a charge into its front loses its bonus and runs
    /// onto the points instead (see [`crate::combat::Combat::before_move`]).
    /// Only regiments with long weapons ([`UnitStats::reach`]) can brace,
    /// and not while locked in melee, falling back or routing. Any move or
    /// attack order lifts it.
    pub(crate) fn brace(&mut self, h: RegimentHandle, on: bool, units: &[UnitStats]) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if !on {
            return core::mem::take(&mut self.braced[i]);
        }
        if self.morale_state[i].broken()
            || self.engaged[i].is_some()
            || self.falling_back[i] > 0
            || !units[self.unit[i] as usize].reach
        {
            return false;
        }
        self.braced[i] = true;
        self.target[i] = None;
        self.target_facing[i] = None;
        self.drill[i] = false;
        self.attack_target[i] = None;
        self.held[i] = 0;
        true
    }

    /// Set regiment `h` to stand fast, or stop (`on` false): it halts
    /// where it stands, fights only what reaches it (turning to face it),
    /// never presses after a foe or steps aside for friends, and a foe that
    /// outfights it drives it back at [`HOLD_DRIVEN_PERCENT`] of the pace
    /// (see [`Regiments::driven_pace`]). Routing regiments don't listen.
    /// Any move, attack, fall back or run lifts it. Returns whether that
    /// changed anything.
    pub(crate) fn hold(&mut self, h: RegimentHandle, on: bool) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if !on {
            return core::mem::take(&mut self.hold[i]);
        }
        if self.morale_state[i].broken() || self.falling_back[i] > 0 {
            return false;
        }
        self.hold[i] = true;
        self.target[i] = None;
        self.target_facing[i] = None;
        self.drill[i] = false;
        self.attack_target[i] = None;
        self.backing[i] = 0;
        self.held[i] = 0;
        self.end_run(i);
        true
    }

    /// Set regiment `h` to keep away from melee, or stop (`on` false); see
    /// [`crate::combat::Combat::skirmish`]. Only regiments that shoot.
    /// Returns whether that changed anything.
    pub(crate) fn set_skirmish(
        &mut self,
        h: RegimentHandle,
        on: bool,
        units: &[UnitStats],
    ) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if on && !units[self.unit[i] as usize].can_skirmish() {
            return false;
        }
        if !on {
            self.backing[i] = 0;
        }
        core::mem::replace(&mut self.skirmish[i], on) != on
    }

    /// Regiment `h` runs where it is going (see [`RUN_PERCENT`]), for up
    /// to [`RUN_TICKS`]. Refused with nowhere to go, in a fight, falling
    /// back, routing, winded, running already, in a shield wall (which
    /// can't run) or in a column (already at its quick march).
    pub(crate) fn run(&mut self, h: RegimentHandle) -> bool {
        let Some(i) = self.resolve(h) else {
            return false;
        };
        if self.idle(i)
            || self.engaged[i].is_some()
            || self.falling_back[i] > 0
            || self.morale_state[i].broken()
            || self.stride[i] != Stride::Fresh
            || matches!(self.formation[i], Formation::ShieldWall | Formation::Column)
        {
            return false;
        }
        self.stride[i].start();
        self.hold[i] = false;
        true
    }

    /// Slot `i`'s run ends, if it is running: it is winded from now.
    pub(crate) fn end_run(&mut self, i: usize) {
        self.stride[i].end();
    }

    /// A tick of every run, winding and skirmisher's step back goes by: a
    /// run ends once the regiment arrives, meets a foe, is routing or runs
    /// out of breath; a step back once it arrives.
    pub(crate) fn wear_orders(&mut self) {
        for i in 0..self.alive.len() {
            if !self.alive[i] {
                continue;
            }
            let mut stride = self.stride[i];
            stride.wear(|| {
                self.idle(i) || self.engaged[i].is_some() || self.morale_state[i].broken()
            });
            self.stride[i] = stride;
            if self.backing[i] > 0 {
                self.backing[i] -= 1;
                if self.target[i].is_none() || self.engaged[i].is_some() {
                    self.backing[i] = 0;
                } else if self.backing[i] == 0 {
                    // Out of time short of the spot: it stops there, free
                    // to step back again.
                    self.target[i] = None;
                    self.target_facing[i] = None;
                    self.drill[i] = false;
                }
            }
        }
    }

    /// Take `amount` hit points off the regiment, from the back ranks forward:
    /// the last slot loses hit points first, and each soldier who falls takes
    /// the next one's place in the damage. Returns soldiers lost.
    pub(crate) fn take_damage(&mut self, i: usize, amount: u32) -> u16 {
        self.take_damage_hp(i, amount).0
    }

    /// Slot `i` runs onto points (stakes, a spike pit): it falls into
    /// disorder, `each` hit points for every soldier of its front rank are
    /// taken off it, and it is gone if that leaves nobody.
    pub(crate) fn onto_points(&mut self, i: usize, each: u32) {
        let front = u32::from(self.files[i].min(self.models[i]));
        self.disorder(i);
        self.take_damage(i, each * front);
        if self.models[i] == 0 {
            self.despawn(self.handle(i));
        }
    }

    /// The last `count` soldiers of slot `i` are killed (drowned, say),
    /// never the last one standing.
    pub(crate) fn kill_last(&mut self, i: usize, count: usize) {
        let models = usize::from(self.models[i]);
        let count = count.min(models.saturating_sub(1));
        let hp = &self.hp[i * MAX_MODELS as usize..][..models];
        let lost: u32 = hp.iter().rev().take(count).map(|&h| u32::from(h)).sum();
        self.take_damage(i, lost);
    }

    /// Each wounded soldier of slot `i` gets `amount` hit points back, up
    /// to `full`.
    pub(crate) fn mend(&mut self, i: usize, amount: u16, full: u16) {
        let models = usize::from(self.models[i]);
        let general = self.general[i].map(usize::from);
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..models];
        for (k, h) in hp.iter_mut().enumerate() {
            // A general is tougher than his men (see `generals`).
            let full = if general == Some(k) {
                (u32::from(full) * crate::generals::GENERAL_HP_PERCENT / 100)
                    .min(u32::from(u16::MAX)) as u16
            } else {
                full
            };
            if *h < full {
                *h = h.saturating_add(amount).min(full);
            }
        }
    }

    /// A new soldier of `hp` hit points joins slot `i` at the back, up to
    /// its full strength (see [`crate::reinforce`]). A rank that was short
    /// only for want of men widens back towards its frontage, and a wedge
    /// stays three deep.
    pub(crate) fn add_soldier(&mut self, i: usize, hp: u16) {
        let k = self.models[i];
        debug_assert!(k < self.full_models[i].min(MAX_MODELS), "a full regiment");
        if k >= self.full_models[i].min(MAX_MODELS) {
            return;
        }
        self.hp[i * MAX_MODELS as usize + usize::from(k)] = hp;
        self.models[i] = k + 1;
        if self.files[i] == k && self.files[i] < self.frontage[i] {
            self.files[i] = k + 1;
        }
        self.keep_wedge(i);
    }

    /// A wedge stays a wedge, three deep, as its riders fall or join.
    fn keep_wedge(&mut self, i: usize) {
        if self.formation[i] == Formation::Wedge {
            let files = Formation::Wedge.files(0, self.models[i]);
            (self.files[i], self.frontage[i]) = (files, files);
        }
    }

    /// The last `count` soldiers of slot `i` lose `amount` hit points
    /// each, none of them below one: hurt, not killed (a fall, say).
    pub(crate) fn wound_last(&mut self, i: usize, count: usize, amount: u32) {
        let models = usize::from(self.models[i]);
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..models];
        let mut bled = 0u16;
        for h in hp.iter_mut().rev().take(count) {
            let lose = (*h - 1).min(amount.min(u32::from(u16::MAX)) as u16);
            *h -= lose;
            bled = bled.saturating_add(lose);
        }
        self.bleed(i, bled, 0);
    }

    /// [`Regiments::take_damage`], also telling the hit points taken (less
    /// than `amount` when the regiment had fewer left).
    pub(crate) fn take_damage_hp(&mut self, i: usize, mut amount: u32) -> (u16, u32) {
        let asked = amount;
        let before = self.models[i];
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        let mut bled = 0u16;
        while amount > 0 && self.models[i] > 0 {
            let k = self.models[i] as usize - 1;
            if u32::from(hp[k]) > amount {
                hp[k] -= amount as u16;
                bled = bled.saturating_add(amount as u16);
                amount = 0;
            } else {
                amount -= u32::from(hp[k]);
                bled = bled.saturating_add(hp[k]);
                hp[k] = 0;
                if self.general[i] == Some(k as u8) {
                    self.general[i] = None;
                }
                self.models[i] -= 1;
            }
        }
        let lost = before - self.models[i];
        self.bleed(i, bled, lost);
        (lost, asked - amount)
    }

    /// Regiment `i`'s hit points at full strength (at least 1).
    pub(crate) fn full_hp(&self, i: usize, stats: &UnitStats) -> u32 {
        u32::from(self.full_models[i].max(1))
            * (u32::from(stats.hp_per_model.max(1)) + u32::from(self.hp_bonus[i]))
    }

    /// Soldier `k` of regiment `i`'s hit points.
    pub(crate) fn hp_mut(&mut self, i: usize, k: u8) -> &mut u16 {
        &mut self.hp[i * MAX_MODELS as usize + usize::from(k)]
    }

    /// Soldiers `a` and `b` of regiment `i` change places.
    pub(crate) fn swap_men(&mut self, i: usize, a: u8, b: u8) {
        let base = i * MAX_MODELS as usize;
        self.hp.swap(base + usize::from(a), base + usize::from(b));
    }

    /// Slot `i` faces about: it turns half round where it stands, and its
    /// men's places turn with it, so the rear rank is the front and every
    /// man keeps his ground (the general then steps back to his post, see
    /// [`crate::generals`]). It then stands
    /// [`ABOUT_FACE_TICKS`] while they turn.
    fn face_about(&mut self, i: usize) {
        let n = usize::from(self.models[i]);
        self.facing[i] = self.facing[i].wrapping_add(Angle::HALF);
        self.hp[i * MAX_MODELS as usize..][..n].reverse();
        self.general[i] = self.general[i].map(|g| (n - 1 - usize::from(g)) as u8);
        self.about_left[i] = ABOUT_FACE_TICKS;
        self.abouts[i] = self.abouts[i].wrapping_add(1);
        self.run_up[i] = 0;
    }

    /// Take `damage` hit points off soldier `k` of regiment `i`. Each time
    /// that kills the man in slot `k`, the soldier in the last slot steps up
    /// into his place and takes what the blow had to spare. Returns soldiers
    /// lost.
    pub(crate) fn wound(&mut self, i: usize, k: u16, mut damage: u16) -> u16 {
        let hp = &mut self.hp[i * MAX_MODELS as usize..][..MAX_MODELS as usize];
        let k = k as usize;
        let mut lost = 0;
        let mut bled = 0;
        while damage > 0 && k < self.models[i] as usize {
            if hp[k] > damage {
                hp[k] -= damage;
                bled += damage;
                break;
            }
            damage -= hp[k];
            bled += hp[k];
            let last = self.models[i] as usize - 1;
            // A general who falls is gone; one in the last slot steps up
            // into the gap with everyone else's.
            if self.general[i] == Some(k as u8) {
                self.general[i] = None;
            } else if self.general[i] == Some(last as u8) {
                self.general[i] = Some(k as u8);
            }
            hp[k] = hp[last];
            hp[last] = 0;
            self.models[i] -= 1;
            lost += 1;
        }
        self.bleed(i, bled, lost);
        lost
    }

    /// Books `bled` hit points and `lost` soldiers taken off regiment `i`.
    fn bleed(&mut self, i: usize, bled: u16, lost: u16) {
        self.keep_wedge(i);
        self.files[i] = self.files[i].min(self.models[i]).max(1);
        self.bled[i] = self.bled[i].saturating_add(bled);
        self.losses[i] = self.losses[i].saturating_add(lost);
        // Hurt: no new soldiers until it has stood quiet again.
        if bled > 0 || lost > 0 {
            self.refill[i] = Refill::Idle;
        }
        self.deaths[i] = self.deaths[i].saturating_add(lost);
    }

    fn view(&self, i: usize) -> RegimentView {
        let plan = self.plan(i);
        RegimentView {
            handle: RegimentHandle {
                index: i as u32,
                generation: self.generation[i],
            },
            owner: self.owner[i],
            unit: self.unit[i],
            position: self.position[i],
            facing: self.facing[i],
            files: self.files[i],
            frontage: self.frontage[i].clamp(1, self.models[i].max(1)),
            formation: self.formation[i],
            models: self.models[i],
            moving: self.target[i].is_some() || self.target_facing[i].is_some(),
            still: self.still(i),
            engaged: self.engaged[i].is_some(),
            charge: self.charge[i],
            charge_bonus: self.charge_bonus[i],
            morale: self.morale_state[i],
            morale_points: (self.morale[i].max(0) / morale::SCALE) as u8,
            break_at: morale::break_at_now(self, i),
            rank: crate::veterancy::rank_of(self, i),
            disorder: self.disorder[i],
            giving_ground: self.driven_pace(i) > 0 && self.engaged[i].is_some(),
            ploughed: if self.plougher(i).is_some() {
                self.ploughed[i]
            } else {
                0
            },
            ploughed_by: self.plougher(i).map(|k| self.handle(k)),
            general: self.general[i],
            braced: self.braced[i],
            hold: self.hold[i],
            skirmish: self.skirmish[i],
            stepping_back: self.backing[i] > 0,
            made_way: self.home[i],
            running: self.stride[i].running(),
            winded: self.stride[i].winded(),
            torches: self.torches[i],
            falling_back: self.falling_back[i] > 0,
            on_wall: self.on_wall[i],
            in_town: self.in_town[i],
            climber: self.climber[i],
            sung: self.sung_left[i] > 0,
            hunting: plan.hunt.is_some(),
            diverted: plan.diverted,
            abouts: self.abouts[i],
            queued: plan.queue.len() as u8,
        }
    }

    /// What regiment `h` is doing, what holds it up and why its nerve is
    /// going, for the screen (docs/29, "Clear orders and feedback"). Kept
    /// out of [`RegimentView`] so the sim's and the AI's many reads of it
    /// don't pay for it.
    pub fn feedback(&self, h: RegimentHandle) -> Option<Feedback> {
        let i = self.resolve(h)?;
        Some(Feedback {
            doing: self.doing(i),
            waiting: self.waiting(i),
            why: self.why[i],
        })
    }

    /// The enemy regiment `h` is fighting in melee, or failing that the
    /// one it is ordered or has turned to attack.
    pub fn foe_of(&self, h: RegimentHandle) -> Option<RegimentHandle> {
        let i = self.resolve(h)?;
        self.engaged[i].or(self.attack_target[i])
    }

    /// What regiment `i` is doing, for the screen.
    fn doing(&self, i: usize) -> Doing {
        if self.morale_state[i].broken() {
            Doing::Running
        } else if self.falling_back[i] > 0 {
            Doing::FallingBack
        } else if self.backing[i] > 0 {
            Doing::Skirmishing
        } else if self.engaged[i].is_some() {
            Doing::Fighting
        } else if !self.idle(i) && self.stride[i].running() {
            Doing::AtTheRun
        } else if self.attack_target[i].is_some() {
            Doing::Attacking
        } else if self.plan(i).hunt.is_some() {
            Doing::Advancing
        } else if self.braced[i] {
            Doing::Braced
        } else if self.hold[i] {
            Doing::StandingFast
        } else if self.target[i].is_some() && self.march_left[i] > 0 {
            Doing::ForcedMarch
        } else if self.target[i].is_some() {
            Doing::Marching
        } else {
            Doing::Holding
        }
    }

    /// What is holding regiment `i` up, if anything, for the screen.
    fn waiting(&self, i: usize) -> Option<Waiting> {
        if self.morale_state[i].broken() {
            None
        } else if self.cocoon_left[i] > 0 {
            Some(Waiting::Cocooned(self.cocoon_left[i]))
        } else if self.trapped_left[i] > 0 {
            Some(Waiting::Trapped)
        } else if self.engaged[i].is_some() && self.why[i] & morale::WHY_PINNED != 0 {
            Some(Waiting::Pinned)
        } else if self.engaged[i].is_some() && self.driven_pace(i) > 0 {
            Some(Waiting::DrivenBack)
        } else if self.target[i].is_some() && self.disorder[i] > 0 {
            Some(Waiting::Reforming)
        } else if self.target[i].is_some() && self.narrowed(i) {
            Some(Waiting::FilingThrough)
        } else if self.stride[i].winded() {
            Some(Waiting::Winded)
        } else {
            match self.refill[i] {
                Refill::Short => Some(Waiting::ShortToReinforce),
                Refill::Counting(_) => Some(Waiting::Reinforcing),
                Refill::Idle => None,
            }
        }
    }

    /// Live regiments in index order, for presentation and tests.
    pub fn iter(&self) -> impl Iterator<Item = RegimentView> + '_ {
        (0..self.alive.len())
            .filter(|&i| self.alive[i])
            .map(|i| self.view(i))
    }

    pub(crate) fn step_movement(
        &mut self,
        nav: &mut Nav,
        terrain: &Terrain,
        combat: &Combat,
        units: &[UnitStats],
        stances: &Stances,
    ) {
        for i in 0..self.alive.len() {
            // On a wall, the wall holds it where it stands.
            if !self.alive[i] || self.on_wall[i] {
                continue;
            }
            let was = self.position[i];
            self.set_keep(i, KEEP_STEPPED, false);
            // Its speed eases toward nothing unless it steps forward below.
            let coasting = self.momentum[i];
            if coasting != 0 {
                self.momentum[i] = ease_momentum(coasting, 0);
            }
            // Falling back ends when time runs out, wherever it got to.
            if self.falling_back[i] > 0 {
                self.falling_back[i] -= 1;
                if self.falling_back[i] == 0 {
                    self.target[i] = None;
                    self.target_facing[i] = None;
                    self.drill[i] = false;
                }
            }
            // Halting, fighting or backing away ends a run-up.
            // So does facing about: it takes up no later order.
            if self.target[i].is_none() || self.engaged[i].is_some() || self.falling_back[i] > 0 {
                self.run_up[i] = 0;
                self.about_left[i] = 0;
            }
            if self.target[i].is_none() {
                self.held[i] = 0;
                self.clear[i] = 0;
                self.stuck[i] = Stuck::Free;
                // Staggered, so a whole army ordered at once doesn't all
                // look on the same ticks.
                self.set_apart(i, i as u8 % KEEP_EVERY);
                self.aside[i] = 0;
            }
            if self.engaged[i].is_none() && !self.braced[i] {
                self.squeeze(i, nav, terrain, units);
            }
            let turn = self.turn[i];
            if self.engaged[i].is_some() {
                // Locked in melee: only turn toward the enemy (set by combat).
                if let Some(want) = self.target_facing[i] {
                    self.facing[i] = self.facing[i].turn_toward(want, turn / 2);
                }
                continue;
            }
            // Held fast by a trap: it may turn where it stands, and goes
            // on where it was going after. A cocoon sits as it is.
            if self.cocoon_left[i] > 0 {
                self.run_up[i] = 0;
                continue;
            }
            if self.trapped_left[i] > 0 {
                self.run_up[i] = 0;
                if let Some(want) = self.target_facing[i] {
                    self.facing[i] = self.facing[i].turn_toward(want, turn);
                }
                continue;
            }
            if let Some(target) = self.target[i] {
                // Facing about: it stands until every man has turned.
                if self.about_left[i] > 0 {
                    self.about_left[i] -= 1;
                    continue;
                }
                // The next waypoint on the way to the target; none while the
                // route is being searched for.
                let Some((mut point, mut last)) = nav.steer(i, target) else {
                    continue;
                };
                // A turn on the way counts as reached once the block is
                // near it and can see the next (stepping aside for friends
                // may never let it stand on the very point).
                let near = u64::from(WAYPOINT_NEAR.raw().unsigned_abs()) + 1;
                if (point - self.position[i]).length_sq_raw() < near * near
                    && nav
                        .after_next(i)
                        .is_some_and(|p| !nav.blocks(self.owner[i], self.position[i], p))
                {
                    nav.reached(i);
                    let Some(next) = nav.steer(i, target) else {
                        continue;
                    };
                    (point, last) = next;
                }
                let delta = point - self.position[i];
                let dist = delta.length();
                // A shield wall shuffles along at half its speed.
                let pace = match self.formation[i].pace_percent() {
                    100 => self.pace[i],
                    p => self.pace[i].min(Fx::from_raw((self.speed[i].raw() * p / 100).max(1))),
                };
                // On a march (see [`crate::powers`]), further.
                let pace = self.marched(i, pace);
                // Falling back, it backs away all the way, facing its foe.
                // A skirmisher stepping back marches all the way and faces
                // about only on the spot.
                let near = if self.backing[i] > 0 {
                    Fx::HALF
                } else {
                    SHUFFLE_DISTANCE
                };
                let shuffle = ((last && dist <= near) || self.falling_back[i] > 0)
                    && self.target_facing[i].is_some();
                let mut wheeling = false;
                let step = if shuffle {
                    let want = self.target_facing[i].unwrap();
                    // Told to face back the way it came: it faces about.
                    if i32::from(self.facing[i].delta_to(want)).abs() > ABOUT_FACE
                        && self.is_block(i)
                        && self.falling_back[i] == 0
                        && !self.morale_state[i].broken()
                    {
                        self.face_about(i);
                        continue;
                    }
                    self.facing[i] = self.facing[i].turn_toward(want, turn);
                    // Into place at the end of a march, or a lone body (a
                    // worker, a beast): at half pace.
                    let drilled = if self.drill[i] {
                        drilled_pace(pace, self.facing[i], delta, dist)
                    } else {
                        Fx::from_raw(pace.raw() / 2)
                    };
                    drilled.max(Fx::from_raw(1))
                } else {
                    let travel = Angle::of_vec(delta);
                    // Its way lies behind it: it faces about (in good order;
                    // a routing block just turns and runs).
                    if i32::from(self.facing[i].delta_to(travel)).abs() > ABOUT_FACE
                        && self.is_block(i)
                        && !self.morale_state[i].broken()
                    {
                        self.face_about(i);
                        continue;
                    }
                    self.facing[i] = self.facing[i].turn_toward(travel, turn);
                    // How far off it still faces after this tick's turn.
                    let off = i32::from(self.facing[i].delta_to(travel));
                    wheeling = off.abs() > WHEEL_FULL_PACE;
                    // In tight ground (a ford, a gate, a pass) it squares up
                    // first, so it narrows into a column across its way
                    // rather than going in broadside (see `squeeze`).
                    if wheeling && !nav.open_around(self.owner[i], self.position[i], OPEN_GROUND) {
                        continue;
                    }
                    let Some(pace) = wheeling_pace(pace, off) else {
                        continue;
                    };
                    pace
                };
                let dir = if dist.raw() > 0 {
                    FxVec2::new(delta.x / dist, delta.y / dist)
                } else {
                    FxVec2::ZERO
                };
                let stats = &units[self.unit[i] as usize];
                let size = stats.block_size_in(self.formation[i], self.files[i], self.models[i]);
                let step = combat.air().march(terrain.block_going(
                    step,
                    self.position[i],
                    dir,
                    stats.mass,
                    self.facing[i],
                    size,
                ));
                let ghost = self.stuck[i].through();
                let full = step;
                let step = if ghost {
                    step
                } else {
                    crate::combat::pushing(step, self.crowd[i])
                };
                // A column still growing forward after narrowing steps on
                // further: half as far again at most (so it stays as slow as
                // whatever slows it), and as far as the crowd check allows.
                let step = if self.grow[i] > 0 && !shuffle {
                    let owed = Fx::from_raw(i32::from(self.grow[i]) * GROW_UNIT);
                    let more = owed
                        .min(step / 2)
                        .min(combat.max_step() - step)
                        .max(Fx::ZERO);
                    let used = (more.raw() + GROW_UNIT - 1) / GROW_UNIT;
                    self.grow[i] = (i32::from(self.grow[i]) - used).max(0) as u8;
                    step + more
                } else {
                    step
                };
                // Keeping its spacing from friends (docs/29): a step aside
                // from any it comes too near.
                let goal = last.then_some(point);
                // In cramped ground (a gateway, a street, and any ford,
                // wide as it may be: blocks wading side by side pile up
                // there) it queues instead (below), where a step aside
                // would only push it into the wall or the water; in a fight
                // it keeps to its foe, friends beside it or not.
                let tight = !nav.open_around(self.owner[i], self.position[i], OPEN_GROUND);
                let cramped = tight
                    && (terrain.wet(self.position[i])
                        || !nav.open_around(
                            self.owner[i],
                            self.position[i],
                            crate::combat::ELBOW_ROOM,
                        ));
                let aside = if ghost || cramped || self.engaged[i].is_some() {
                    Fx::ZERO
                } else if self.apart(i) > 0 {
                    self.set_apart(i, self.apart(i) - 1);
                    Fx::from_raw(i32::from(self.aside[i])).clamp(-step, step)
                } else {
                    let (aside, wait) =
                        combat.keep_apart(self, units, stances, i, dir, goal, step, KEEP_EVERY - 1);
                    debug_assert!(
                        i16::try_from(aside.raw()).is_ok(),
                        "a step aside of half a tile or more"
                    );
                    self.aside[i] = aside.raw().clamp(i16::MIN.into(), i16::MAX.into()) as i16;
                    self.set_apart(i, wait);
                    aside
                };
                debug_assert!(step <= combat.max_step(), "a step outruns the crowd check");
                let here = self.position[i];
                let advance = |s: Fx| {
                    let arriving = dist <= s;
                    (arriving, if arriving { point } else { here + dir * s })
                };
                let (mut arriving, mut next) = advance(step);
                // A friendly block holding its ground in the way, where the
                // ground leaves no room beside it (a ford, a pass): wait
                // behind it. In the open, blocks still pass through.
                // Marching, it looks at most every other tick (`clear`);
                // waiting, every RECHECK ticks, standing in between.
                if ghost {
                    // Marching through: friends neither hold nor queue it,
                    // but a fight is never walked into.
                    if tight
                        && combat
                            .friend_in_the_way(self, units, stances, i, next, dir)
                            .is_ok_and(|j| self.engaged[j].is_some())
                    {
                        continue;
                    }
                } else if self.clear[i] > 0 {
                    self.clear[i] -= 1;
                    if (1..PATIENCE).contains(&self.held[i]) {
                        continue;
                    }
                } else if tight {
                    let found = combat.friend_in_the_way(self, units, stances, i, next, dir);
                    // Following a friend marching on ahead (its route goes
                    // where this one's does, room beside it or not) uses
                    // up no patience: the queue is moving.
                    let following = |j: usize| {
                        self.marching(j)
                            && !(1..PATIENCE).contains(&self.held[j])
                            && self.stepped(j)
                    };
                    match found {
                        Ok(j)
                            if following(j)
                                && (self.attack_target[i].is_some()
                                    || [here, next, self.position[j]]
                                        .into_iter()
                                        .any(|p| terrain.wet(p))) =>
                        {
                            // Waiting its turn: in or into a ford's wading queue
                            // (where a block pressing on would be pushed
                            // off into deep water), and in a column going
                            // in to attack, which comes on to the fight a
                            // block at a time.
                            self.held[i] = self.held[i].max(1);
                            self.clear[i] = RECHECK - 1;
                            continue;
                        }
                        Ok(j) if following(j) => {
                            // Marching on behind it: trailing at half pace
                            // this tick and full the next (when it doesn't
                            // look), so the column keeps moving instead of
                            // standing and starting, and falls back off its
                            // heels.
                            (arriving, next) = advance(step / 2);
                            self.clear[i] = 1;
                        }
                        Ok(j) if !combat.room_beside(self, units, nav, i, j, dir) => {
                            // Patience runs for the ticks until it looks
                            // again; once it has run out it looks every
                            // tick, standing only while it must.
                            self.held[i] = self.held[i].saturating_add(u16::from(RECHECK));
                            if self.held[i] < PATIENCE {
                                self.clear[i] = RECHECK - 1;
                            }
                            continue;
                        }
                        Ok(_) => self.clear[i] = 1,
                        Err(gap) => {
                            let ticks = gap.raw() / step.raw().max(1);
                            self.clear[i] = ticks.clamp(1, i32::from(CLEAR_TICKS)) as u8;
                        }
                    }
                }
                // Marching freely again. Once its patience ran out it stays
                // out until this order is done, so it can finish slipping
                // past whoever was in the way.
                if self.held[i] < PATIENCE {
                    self.held[i] = 0;
                }
                if arriving {
                    self.position[i] = point;
                    if last {
                        self.target[i] = None;
                        self.falling_back[i] = 0;
                        self.grow[i] = 0;
                    } else {
                        nav.reached(i);
                    }
                } else if let Some(next) = {
                    // The step aside goes with the step, as far as the
                    // ground lets it; where it can't, the step alone.
                    let aside = FxVec2::new(-dir.y, dir.x) * aside;
                    nav.slide(self.owner[i], here, next + aside).or_else(|| {
                        if aside == FxVec2::ZERO {
                            None
                        } else {
                            nav.slide(self.owner[i], here, next)
                        }
                    })
                } {
                    self.position[i] = next;
                    // Wading across a river, the current carries it down a
                    // little (it steers back onto its way each tick), unless
                    // that would take it onto blocked ground.
                    let drift = terrain.drift(next, dir, step);
                    if drift != FxVec2::ZERO && !nav.blocks(self.owner[i], next, next + drift) {
                        self.position[i] = next + drift;
                    }
                } else {
                    nav.lost(i);
                    continue;
                }
                let stepped = (self.position[i] - was).dot(dir).max(Fx::ZERO);
                self.set_keep(i, KEEP_STEPPED, stepped > Fx::ZERO);
                self.momentum[i] = ease_momentum(coasting, hundredths_per_second(stepped));
                if ghost {
                    self.stuck[i].step_through();
                } else if stepped * 4 < full
                    && (self.crowd[i] > Fx::ZERO || self.held[i] > 0 || aside != Fx::ZERO)
                {
                    // Kept back by friends (not just by the ground): once
                    // for UNSTICK_TICKS, it marches through them for as
                    // long.
                    self.stuck[i].kept_back();
                } else {
                    self.stuck[i] = Stuck::Free;
                }
                // A run-up is made on ground that bears a charge: wading or
                // slogging through mud starts it again.
                // Wheeling, its side leads: no run-up either.
                if wheeling || !terrain.bears_charge(self.position[i]) {
                    self.run_up[i] = 0;
                } else if self.falling_back[i] == 0 {
                    self.run_up[i] = (self.run_up[i] + 1).min(crate::combat::RUN_UP_TICKS);
                }
            } else if let Some(want) = self.target_facing[i] {
                self.facing[i] = self.facing[i].turn_toward(want, turn);
                if self.facing[i] == want {
                    self.target_facing[i] = None;
                    self.drill[i] = false;
                }
            }
        }
    }

    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        // No party past MAX_PARTIES ever gets a bonus.
        for bonus in &self.party_morale[..crate::MAX_PARTIES] {
            bonus.hash(h);
        }
        h.u32(self.len() as u32);
        for i in 0..self.alive.len() {
            h.u8(u8::from(self.alive[i]));
            h.u16(self.generation[i]);
            if self.alive[i] {
                self.hash_fields(i, h);
            }
        }
    }
}

/// How a regiment's momentum follows its speed: it eases a
/// [`MOMENTUM_EASE`]th of the way there each tick, so it is about two
/// thirds there after two seconds and all but there after six.
const MOMENTUM_EASE: u32 = 2 * crate::TICKS_PER_SECOND;

/// Momentum `was`, eased a step toward `now` (both hundredths of a tile a
/// second), rounding toward `now` so it settles on a steady speed exactly.
fn ease_momentum(was: u16, now: u32) -> u16 {
    let toward = if now > u32::from(was) {
        MOMENTUM_EASE - 1
    } else {
        0
    };
    let eased = (u32::from(was) * (MOMENTUM_EASE - 1) + now + toward) / MOMENTUM_EASE;
    eased.min(u32::from(u16::MAX)) as u16
}

/// A step of `stepped` tiles in one tick, as hundredths of a tile a second.
pub(crate) fn hundredths_per_second(stepped: Fx) -> u32 {
    let raw = u64::try_from(stepped.raw()).unwrap_or(0);
    // Rounded to the nearest, as a speed in whole tiles a tick isn't
    // quite whole in fixed point.
    ((raw * u64::from(crate::TICKS_PER_SECOND) * 100 + (1 << 15)) >> 16).min(u64::from(u16::MAX))
        as u32
}

pub(crate) fn hash_handle(h: &mut StateHasher, handle: Option<RegimentHandle>) {
    match handle {
        Some(r) => {
            h.u8(1);
            h.u32(r.index);
            h.u16(r.generation);
        }
        None => h.u8(0),
    }
}

/// Read-only snapshot of one regiment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegimentView {
    pub handle: RegimentHandle,
    pub owner: PlayerId,
    pub unit: UnitId,
    /// Centre of the block.
    pub position: FxVec2,
    pub facing: Angle,
    /// Soldiers per rank.
    pub files: u16,
    /// Soldiers per rank it was given: more than `files` while it is
    /// squeezed into a column through tight ground.
    pub frontage: u16,
    /// How closely the soldiers stand.
    pub formation: Formation,
    /// Soldiers still standing.
    pub models: u16,
    /// Walking or turning to an ordered facing (not stepping aside to make
    /// way and back: see `made_way`).
    pub moving: bool,
    /// Not marching: standing, turning or fighting. Formation discipline
    /// goes by this, not `moving` (see [`Regiments::still`]).
    pub still: bool,
    /// Fighting in melee.
    pub engaged: bool,
    /// Ticks of charge bonus left: it hit home at speed within the last
    /// few seconds.
    pub charge: u16,
    /// That charge's bonus, percent: its unit's own, scaled by how fast it
    /// hit home (or for braced spears, the charge they turned back).
    pub charge_bonus: u8,
    pub morale: MoraleState,
    /// Morale in points (0 to 100); meaningless for regiments that never rout.
    pub morale_points: u8,
    /// Its breaking point as it stands (lowered while its general is near):
    /// the percent of its full hit points it must be down to before it can
    /// rout or waver (100 for non-fighters such as Caravans, whose nerve
    /// is low by design, and for regiments that never rout at all).
    pub break_at: u8,
    /// Its veteran rank, 0 to 3, from its kills (see [`crate::veterancy`]).
    pub rank: u8,
    /// Out of order from pushing through friends: ticks left until it has
    /// re-formed, from [`DISORDER_TICKS`](crate::combat::DISORDER_TICKS)
    /// (full disorder) down to 0 (in order).
    pub disorder: u8,
    /// Losing its melee and being driven back (see
    /// [`Combat::push_lines`](crate::combat::Combat::push_lines)), or
    /// pinned where it can't give ground.
    pub giving_ground: bool,
    /// Spears or pikes set against a charge (see [`Regiments::brace`]).
    pub braced: bool,
    /// Standing fast on a Hold order (see [`Regiments::hold`]).
    pub hold: bool,
    /// Keeping away from melee (see [`Regiments::set_skirmish`]), and
    /// stepping back from it just now.
    pub skirmish: bool,
    pub stepping_back: bool,
    /// Stepped aside to make way for friends marching through, where it
    /// stood before (see `Combat::make_way`): it steps back once they are by.
    pub made_way: Option<FxVec2>,
    /// Running (see [`Regiments::run`]), or winded after a run.
    pub running: bool,
    pub winded: bool,
    /// Torches lit (see [`Command::Torches`](crate::Command::Torches)):
    /// they burn only in the dark.
    pub torches: bool,
    /// Pulling out of a fight in good order (see [`Regiments::fall_back`]).
    pub falling_back: bool,
    /// Ticks left of being driven back by a charge that ploughed into its
    /// ranks, up to [`PLOUGH_TICKS`](crate::combat::PLOUGH_TICKS).
    pub ploughed: u8,
    /// The charger that ploughed in, while it does.
    pub ploughed_by: Option<RegimentHandle>,
    /// The slot its side's general stands in, if he rides with it and
    /// stands (see [`crate::generals`]).
    pub general: Option<u8>,
    /// Standing on a wall's walkway (see [`crate::manning`]; its soldiers
    /// stand on the tiles [`crate::World::wall_tiles`] names).
    pub on_wall: bool,
    /// Inside its own walled town at the last survey (see [`crate::towns`]).
    pub in_town: bool,
    /// On a wall at the head of a ladder, fighting its way onto it.
    pub climber: bool,
    /// Slowed by a Choir Singing Wall it struck (see [`crate::twists`]).
    pub sung: bool,
    /// On an attack-move (see [`crate::orders`]).
    pub hunting: bool,
    /// On an attack-move and turned aside to fight what it met.
    pub diverted: bool,
    /// How many times it has faced about, counting round (see
    /// [`ABOUT_FACE_TICKS`]): when it changes, its men's places have turned
    /// round with the block.
    pub abouts: u8,
    /// Orders queued behind the current one ([`Regiments::queued`] has them).
    pub queued: u8,
}

/// What a regiment is doing and why, as the screen shows it (see
/// [`Regiments::feedback`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Feedback {
    /// What it is doing.
    pub doing: Doing,
    /// What is holding it up, if anything.
    pub waiting: Option<Waiting>,
    /// Why its nerve is going: [`morale`]'s `WHY_` bits (flanked, rear,
    /// outnumbered, ally broke nearby and so on). Shown while it wavers or
    /// runs.
    pub why: u16,
}

/// What a regiment is doing, as the screen shows it (docs/29, "Clear orders
/// and feedback"). Derived from its state each time it is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Doing {
    /// Standing where it was put, no order left.
    Holding,
    /// Walking to an ordered spot.
    Marching,
    /// Walking with a march power driving it faster.
    ForcedMarch,
    /// Going for an ordered enemy.
    Attacking,
    /// On an attack-move: marching, fighting whatever it meets on the way.
    Advancing,
    /// In melee.
    Fighting,
    /// Spears set against a charge.
    Braced,
    /// Pulling out of a fight in good order.
    FallingBack,
    /// Routing or shattered: not taking orders.
    Running,
    /// Standing fast on a Hold order.
    StandingFast,
    /// A skirmisher stepping back from melee.
    Skirmishing,
    /// Marching or going for an enemy at the run.
    AtTheRun,
}

/// What is holding a regiment up, as the screen shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// Driven back against something with nowhere to give ground.
    Pinned,
    /// Losing its melee and giving ground.
    DrivenBack,
    /// Out of order (pushed through friends, ploughed, tripped by a trap or
    /// a flood) and re-forming its ranks.
    Reforming,
    /// Squeezed into a column through tight ground.
    FilingThrough,
    /// Held fast by a trap.
    Trapped,
    /// In a cocoon, becoming its evolved form (see [`crate::evolve`]),
    /// with these ticks left.
    Cocooned(u16),
    /// Out of breath after a run: slower, with weaker blows.
    Winded,
    /// Taking on new soldiers near its side's reinforcement points (see
    /// [`crate::reinforce`]).
    Reinforcing,
    /// Near them, but its owner can't pay for the next soldier.
    ShortToReinforce,
}

impl RegimentView {
    pub fn ranks(&self) -> u16 {
        formation::ranks(self.files, self.models)
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;
    use crate::scenario::{DRILL_UNITS, Drill, ribs_map};
    use crate::state::written;
    use crate::{Capacities, Ground, MatchSetup, World};

    /// A world mid-battle, blocks pushing in melee.
    fn mid_melee() -> (World, Drill) {
        let nav = ribs_map(256);
        let mut world = World::new(&MatchSetup {
            seed: 31,
            capacities: Capacities::default(),
            units: &DRILL_UNITS,
            map_tiles: 256,
            nav: &nav,
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
            sky: crate::Sky::STILL,
            generals: true,
        });
        let mut drill = Drill::new(7);
        drill.regiments_per_player = 20;
        drill.unit_of = |p, _, _| [0, 1, 3, 0][p as usize % 4];
        let mut commands = Vec::new();
        while !world.regiments.pressing.iter().any(|&p| p != 0) {
            assert!(world.tick_count() < 900, "no block ever pushed");
            drill.commands_for(&world, &mut commands);
            world.tick(&commands);
        }
        (world, drill)
    }

    #[test]
    fn a_stride_is_written_as_its_old_two_counts() {
        let mut reachable = vec![(Stride::Fresh, [0, 0])];
        reachable.extend((1..=RUN_TICKS).map(|n| (Stride::Running(n), [n, 0])));
        reachable.extend((1..=WINDED_TICKS).map(|n| (Stride::Winded(n), [0, n])));
        for (stride, old) in reachable {
            assert_eq!(written(stride), old, "{stride:?}");
        }
        // Every pair of counts with no more than one running down reads
        // back as itself. The old fields never held both.
        for run in 0..=u8::MAX {
            for winded in 0..=u8::MAX {
                let read = Stride::read(&mut &[run, winded][..]);
                if run > 0 && winded > 0 {
                    assert_eq!(read, None, "{run} {winded}");
                } else {
                    assert_eq!(read.map(written), Some(vec![run, winded]));
                }
            }
        }
    }

    /// A check that [`Stride`] counts as the old fields did.
    #[test]
    fn a_stride_wears_and_ends_as_the_old_counts_did() {
        // The old `wear_orders` and `end_run`, on `run_left` and
        // `winded_left`: winded counts down first, so a run that ends
        // here is winded for all of WINDED_TICKS from the next tick.
        fn wear(mut run: u8, mut winded: u8, stop: bool) -> [u8; 2] {
            winded = winded.saturating_sub(1);
            if run > 0 {
                if run == 1 || stop {
                    run = 0;
                    winded = WINDED_TICKS;
                } else {
                    run -= 1;
                }
            }
            [run, winded]
        }
        fn end(run: u8, winded: u8) -> [u8; 2] {
            if run > 0 {
                [0, WINDED_TICKS]
            } else {
                [run, winded]
            }
        }
        let fresh = [(0, 0)];
        let pairs = (fresh.into_iter())
            .chain((1..=u8::MAX).map(|n| (n, 0)))
            .chain((1..=u8::MAX).map(|n| (0, n)));
        for (run, winded) in pairs {
            let stride = Stride::read(&mut &[run, winded][..]).unwrap();
            for stop in [false, true] {
                let mut worn = stride;
                worn.wear(|| stop);
                assert_eq!(written(worn), wear(run, winded, stop), "{stride:?} {stop}");
            }
            let mut ended = stride;
            ended.end();
            assert_eq!(written(ended), end(run, winded), "{stride:?}");
        }
        let mut fresh = Stride::Fresh;
        fresh.start();
        assert_eq!(written(fresh), [RUN_TICKS, 0]);
    }

    #[test]
    fn stuck_is_written_as_its_old_byte() {
        let reachable = (core::iter::once((Stuck::Free, 0)))
            .chain((1..UNSTICK_TICKS).map(|n| (Stuck::Kept(n), n)))
            .chain((0..=UNSTICK_TICKS).map(|t| (Stuck::Through(t), GHOST + t)));
        for (stuck, old) in reachable {
            assert_eq!(written(stuck), [old], "{stuck:?}");
        }
        // And every byte reads back as itself.
        for b in 0..=u8::MAX {
            assert_eq!(Stuck::read(&mut &[b][..]).map(written), Some(vec![b]));
        }
    }

    /// A check that [`Stuck`] counts as the old field did.
    #[test]
    fn stuck_counts_as_the_old_byte_did() {
        for b in 0..=u8::MAX {
            let stuck = Stuck::read(&mut &[b][..]).unwrap();
            // The old byte marched through from GHOST up, GHOST itself
            // too: a 41-tick march through after 40 kept back.
            assert_eq!(stuck.through(), b >= GHOST, "{b}");
            let mut next = stuck;
            let old = if stuck.through() {
                next.step_through();
                if b - 1 < GHOST { 0 } else { b - 1 }
            } else {
                next.kept_back();
                if b + 1 >= UNSTICK_TICKS {
                    GHOST + UNSTICK_TICKS
                } else {
                    b + 1
                }
            };
            assert_eq!(written(next), [old], "{b}");
        }
    }

    fn hash(r: &Regiments) -> u64 {
        let mut h = StateHasher::new();
        r.hash_into(&mut h);
        h.finish()
    }

    fn copy(r: &Regiments) -> Regiments {
        let mut bytes = Vec::new();
        r.write(&mut bytes);
        let mut input = &bytes[..];
        let back = Regiments::read(&mut input).unwrap();
        assert!(input.is_empty());
        back
    }

    #[test]
    fn every_field_reaches_the_state_hash() {
        let (world, _) = mid_melee();
        let regs = &world.regiments;
        let i = (0..regs.capacity()).find(|&i| regs.alive[i]).unwrap();
        let before = hash(regs);
        for (k, name) in Regiments::FIELDS.iter().enumerate() {
            let mut changed = copy(regs);
            changed.change(i, k);
            assert_ne!(hash(&changed), before, "{name} isn't hashed");
        }
        // And the fields outside the list.
        let mut changed = copy(regs);
        changed.hp[i * MAX_MODELS as usize] ^= 1;
        assert_ne!(hash(&changed), before, "hp isn't hashed");
        let mut changed = copy(regs);
        changed.generation[i] ^= 1;
        assert_ne!(hash(&changed), before, "generation isn't hashed");
        let mut changed = copy(regs);
        changed.party_morale[crate::MAX_PARTIES - 1] = 5;
        assert_ne!(hash(&changed), before, "party morale isn't hashed");
        // A plan's every part, wherever in the pool it is kept.
        let step = crate::orders::Step::Move {
            target: FxVec2::from_ints(3, 4),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            hunt: true,
            brigade: 0,
        };
        let mut queued = Plan::NONE;
        queued.queue.push(step);
        let hunting = Plan {
            hunt: Some(step),
            ..Plan::NONE
        };
        let diverted = Plan {
            diverted: true,
            ..hunting
        };
        let mut hashes = alloc::vec![before];
        for plan in [queued, hunting, diverted] {
            let mut changed = copy(regs);
            assert!(changed.set_plan(i, plan));
            hashes.push(hash(&changed));
        }
        hashes.sort_unstable();
        hashes.dedup();
        assert_eq!(hashes.len(), 4, "a plan isn't hashed in full");
        let mut changed = copy(regs);
        changed.despawn(changed.handle(i));
        assert_ne!(hash(&changed), before, "who is alive isn't hashed");
    }

    #[test]
    fn a_pool_that_breaks_the_rules_is_refused() {
        let (world, _) = mid_melee();
        let regs = &world.regiments;
        let live = (0..regs.capacity()).find(|&i| regs.alive[i]).unwrap() as u32;
        let refused = |edit: &dyn Fn(&mut Regiments)| {
            let mut bad = copy(regs);
            edit(&mut bad);
            let mut bytes = Vec::new();
            bad.write(&mut bytes);
            Regiments::read(&mut &bytes[..]).is_none()
        };
        assert!(refused(&|r| r.free.push(live)), "a live slot listed free");
        assert!(
            refused(&|r| {
                let f = r.free[0];
                r.free.push(f);
            }),
            "a slot listed free twice"
        );
        assert!(
            refused(&|r| {
                r.free.pop();
            }),
            "a free slot left off the list"
        );
        assert!(refused(&|r| r.models[live as usize] = MAX_MODELS + 1));
        assert!(refused(
            &|r| r.owner[live as usize] = crate::MAX_PARTIES as u8
        ));
    }

    #[test]
    fn a_pool_reads_back_exactly_and_plays_on_alike() {
        let (mut world, _) = mid_melee();
        let mut bytes = Vec::new();
        world.regiments.write(&mut bytes);
        let mut other = World::new(&MatchSetup {
            seed: 31,
            capacities: Capacities::default(),
            units: &DRILL_UNITS,
            map_tiles: 256,
            nav: &ribs_map(256),
            teams: &[],
            ground: Ground::FLAT,
            champions: false,
            sky: crate::Sky::STILL,
            generals: true,
        });
        other.regiments = copy(&world.regiments);
        let mut again = Vec::new();
        other.regiments.write(&mut again);
        assert_eq!(again, bytes);
        assert_eq!(hash(&other.regiments), hash(&world.regiments));
        // Cut short anywhere, it reads as nothing.
        for cut in [0, 3, bytes.len() / 2, bytes.len() - 1] {
            assert!(
                Regiments::read(&mut &bytes[..cut]).is_none(),
                "cut at {cut}"
            );
        }
        // The free list came back too: both pools fill the same slot next.
        let stats = DRILL_UNITS[0].sanitized();
        assert_eq!(
            world
                .regiments
                .spawn(0, 0, FxVec2::ZERO, Angle::ZERO, &stats),
            other
                .regiments
                .spawn(0, 0, FxVec2::ZERO, Angle::ZERO, &stats)
        );
    }

    /// Falling back is at half pace, whatever the order before it: a block
    /// still on a short drilled move must not back away at drilled pace
    /// (full ahead, three quarters sideways).
    #[test]
    fn falling_back_ends_a_drilled_move() {
        let mut regs = Regiments::with_capacity(4);
        let stats = DRILL_UNITS[0].sanitized();
        let h = (regs.spawn(0, 0, FxVec2::from_ints(50, 50), Angle::ZERO, &stats)).unwrap();
        let i = regs.resolve(h).unwrap();
        let order = Order {
            target: FxVec2::from_ints(52, 50),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
        };
        assert!(regs.order(h, order));
        assert!(regs.drill[i], "a short move is drilled");
        assert!(regs.fall_back(h, FxVec2::from_ints(50, 56)));
        assert!(!regs.drill[i], "falls back at drilled pace");
    }

    /// An attack on another foe calls off going round the last one, and
    /// its ticks on the way with it: they are not crowded turns.
    #[test]
    fn a_new_attack_leaves_no_crowded_turns() {
        let mut regs = Regiments::with_capacity(4);
        let stats = DRILL_UNITS[0].sanitized();
        let at = |x| FxVec2::from_ints(x, 50);
        let me = (regs.spawn(0, 0, at(50), Angle::ZERO, &stats)).unwrap();
        let old = (regs.spawn(1, 0, at(60), Angle::ZERO, &stats)).unwrap();
        let new = (regs.spawn(1, 0, at(70), Angle::ZERO, &stats)).unwrap();
        let i = regs.resolve(me).unwrap();
        assert!(regs.attack(me, old));
        regs.round[i] = core::num::NonZeroU8::new(1);
        regs.crowded[i] = 30;
        assert!(regs.attack(me, new));
        assert_eq!(regs.round[i], None);
        assert_eq!(regs.crowded[i], 0);
    }
}
