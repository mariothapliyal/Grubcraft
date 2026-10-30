//! Orders given ahead (docs/29, "Orders that respond instantly"): a short
//! queue of steps per regiment, so Shift-queued waypoints and attacks are
//! carried out one after another, and attack-moves, which march but stop
//! to fight any enemy met on the way.
//!
//! Both live in the simulation, so they go through the lockstep and
//! replays like every other order. A regiment's queue holds at most
//! [`QUEUE_LEN`] steps after the one it is carrying out. Any order given
//! without Shift (a move, an attack, falling back, or setting it to work)
//! clears it. A step queued for a regiment that is working waits for the
//! work to end.
//!
//! **Attack-move.** A regiment on an attack-move looks round every
//! [`HUNT_EVERY`] ticks while it marches. When an enemy regiment it can
//! see is within [`HUNT_RADIUS`] (or its bows' reach, if further) and
//! not up on a wall, it turns aside and attacks that enemy, keeping the
//! attack-move order apart from its queue. Once the fight is over (the
//! enemy gone, routing or shattered, or dropped as out of reach), it takes
//! that order up again, at the pace and width first asked for.
//!
//! **Where they are kept.** Few regiments have anything queued or are on
//! an attack-move at once, so a regiment's [`Plan`] is not kept in every
//! regiment slot but in a shared pool of [`PLANS`] (the simulation's
//! memory is held tight, docs/12). When every plan is in use, a Shift order
//! is refused and an attack-move marches as a plain move.

use gc_fixed::{Angle, Fx, FxVec2};

use crate::hash::StateHasher;
use crate::regiments::RegimentHandle;
use crate::state::State;

/// Steps a regiment can have queued behind the order it is carrying out.
pub const QUEUE_LEN: usize = 4;
/// Regiments that can have orders queued or be on an attack-move at once.
pub const PLANS: usize = 256;
/// How far a regiment on an attack-move looks for an enemy to fight, in
/// tiles (beyond its bows' reach, for archers).
pub const HUNT_RADIUS: Fx = Fx::from_int(12);
/// Ticks between its looks (a quarter of a second), staggered by slot.
pub const HUNT_EVERY: u32 = 5;

/// One queued order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// March to `target`, as [`crate::Command::MoveTo`]; `hunt` makes it an
    /// attack-move (see [`crate::Command::AttackMove`]).
    Move {
        target: FxVec2,
        facing: Option<Angle>,
        files: u16,
        /// Tiles per second; zero is the regiment's own speed.
        pace: Fx,
        hunt: bool,
    },
    /// Close with an enemy regiment and fight it, as
    /// [`crate::Command::Attack`].
    Attack { target: RegimentHandle },
}

/// A regiment's queued steps, first first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Queue {
    steps: [Option<Step>; QUEUE_LEN],
}

impl Queue {
    pub const EMPTY: Queue = Queue {
        steps: [None; QUEUE_LEN],
    };

    pub fn len(&self) -> usize {
        self.steps.iter().take_while(|s| s.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.steps[0].is_none()
    }

    /// The steps, first first.
    pub fn iter(&self) -> impl Iterator<Item = Step> + '_ {
        self.steps.iter().map_while(|s| *s)
    }

    /// Add `step` at the back; false when the queue is full.
    pub(crate) fn push(&mut self, step: Step) -> bool {
        match self.steps.iter_mut().find(|s| s.is_none()) {
            Some(slot) => {
                *slot = Some(step);
                true
            }
            None => false,
        }
    }

    /// Take the first step off.
    pub(crate) fn pop(&mut self) -> Option<Step> {
        let first = self.steps[0];
        self.steps.copy_within(1.., 0);
        self.steps[QUEUE_LEN - 1] = None;
        first
    }
}

impl State for Step {
    fn write(&self, out: &mut alloc::vec::Vec<u8>) {
        match *self {
            Step::Move {
                target,
                facing,
                files,
                pace,
                hunt,
            } => {
                1u8.write(out);
                target.write(out);
                facing.write(out);
                files.write(out);
                pace.write(out);
                hunt.write(out);
            }
            Step::Attack { target } => {
                0u8.write(out);
                target.write(out);
            }
        }
    }

    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(match u8::read(input)? {
            1 => Step::Move {
                target: FxVec2::read(input)?,
                facing: Option::<Angle>::read(input)?,
                files: u16::read(input)?,
                pace: Fx::read(input)?,
                hunt: bool::read(input)?,
            },
            0 => Step::Attack {
                target: RegimentHandle::read(input)?,
            },
            _ => return None,
        })
    }

    fn hash(&self, h: &mut StateHasher) {
        match *self {
            Step::Move {
                target,
                facing,
                files,
                pace,
                hunt,
            } => {
                1u8.hash(h);
                target.hash(h);
                facing.hash(h);
                files.hash(h);
                pace.hash(h);
                hunt.hash(h);
            }
            Step::Attack { target } => {
                0u8.hash(h);
                target.hash(h);
            }
        }
    }
}

impl State for Queue {
    fn write(&self, out: &mut alloc::vec::Vec<u8>) {
        for s in &self.steps {
            s.write(out);
        }
    }

    fn read(input: &mut &[u8]) -> Option<Self> {
        let mut q = Queue::EMPTY;
        for s in &mut q.steps {
            *s = Option::<Step>::read(input)?;
        }
        // Steps are kept packed at the front.
        let mut packed = Queue::EMPTY;
        for s in q.steps.into_iter().flatten() {
            packed.push(s);
        }
        Some(packed)
    }

    fn hash(&self, h: &mut StateHasher) {
        for s in &self.steps {
            s.hash(h);
        }
    }
}

/// What a regiment is to do beyond the order it is carrying out: its
/// queued steps and the attack-move it is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub queue: Queue,
    /// The attack-move it is on, kept apart from its queue so it can be
    /// taken up again after a fight.
    pub hunt: Option<Step>,
    /// Turned aside from `hunt` to fight an enemy it met.
    pub diverted: bool,
}

impl Plan {
    pub const NONE: Plan = Plan {
        queue: Queue::EMPTY,
        hunt: None,
        diverted: false,
    };

    fn is_none(&self) -> bool {
        self.queue.is_empty() && self.hunt.is_none()
    }
}

impl State for Plan {
    fn write(&self, out: &mut alloc::vec::Vec<u8>) {
        self.queue.write(out);
        self.hunt.write(out);
        self.diverted.write(out);
    }

    fn read(input: &mut &[u8]) -> Option<Self> {
        Some(Plan {
            queue: Queue::read(input)?,
            hunt: Option::<Step>::read(input)?,
            diverted: bool::read(input)?,
        })
    }

    fn hash(&self, h: &mut StateHasher) {
        self.queue.hash(h);
        self.hunt.hash(h);
        self.diverted.hash(h);
    }
}

/// A regiment slot's index in [`Plans`], or [`NO_PLAN`].
pub(crate) type PlanIndex = u16;
pub(crate) const NO_PLAN: PlanIndex = PlanIndex::MAX;

/// The pool regiments' [`Plan`]s are kept in, allocated in full up front.
/// Which entry a regiment has is no part of the game's state: only what
/// its plan holds is hashed and saved.
pub(crate) struct Plans {
    plans: alloc::vec::Vec<Plan>,
    /// Free entries, popped from the back.
    free: alloc::vec::Vec<PlanIndex>,
}

impl Plans {
    pub(crate) fn with_capacity(n: usize) -> Plans {
        let n = n.min(PLANS);
        Plans {
            plans: alloc::vec![Plan::NONE; n],
            free: (0..n as PlanIndex).rev().collect(),
        }
    }

    pub(crate) fn get(&self, k: PlanIndex) -> Plan {
        self.plans
            .get(usize::from(k))
            .copied()
            .unwrap_or(Plan::NONE)
    }

    /// Store `plan` in entry `*k`, taking an entry if `*k` has none and
    /// giving it back if `plan` is empty. False (and `*k` unchanged) when
    /// none is free.
    pub(crate) fn set(&mut self, k: &mut PlanIndex, plan: Plan) -> bool {
        if plan.is_none() {
            self.release(k);
            return true;
        }
        if *k == NO_PLAN {
            let Some(free) = self.free.pop() else {
                return false;
            };
            *k = free;
        }
        self.plans[usize::from(*k)] = plan;
        true
    }

    /// Give entry `*k` back, if it holds one.
    pub(crate) fn release(&mut self, k: &mut PlanIndex) {
        if *k != NO_PLAN {
            self.plans[usize::from(*k)] = Plan::NONE;
            self.free.push(*k);
            *k = NO_PLAN;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to(x: i32) -> Step {
        Step::Move {
            target: FxVec2::from_ints(x, 0),
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            hunt: false,
        }
    }

    #[test]
    fn queue_keeps_order_and_limit() {
        let mut q = Queue::EMPTY;
        for x in 0..QUEUE_LEN as i32 {
            assert!(q.push(to(x)));
        }
        assert!(!q.push(to(9)));
        assert_eq!(q.len(), QUEUE_LEN);
        assert_eq!(
            q.iter().collect::<alloc::vec::Vec<_>>(),
            [to(0), to(1), to(2), to(3)]
        );
        assert_eq!(q.pop(), Some(to(0)));
        assert_eq!(q.pop(), Some(to(1)));
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn queue_round_trips() {
        let mut q = Queue::EMPTY;
        q.push(to(3));
        q.push(Step::Attack {
            target: RegimentHandle {
                index: 4,
                generation: 2,
            },
        });
        let mut bytes = alloc::vec::Vec::new();
        q.write(&mut bytes);
        assert_eq!(Queue::read(&mut bytes.as_slice()), Some(q));
    }

    #[test]
    fn plans_are_lent_and_given_back() {
        let mut plans = Plans::with_capacity(2);
        let plan = Plan {
            hunt: Some(to(1)),
            ..Plan::NONE
        };
        let (mut a, mut b, mut c) = (NO_PLAN, NO_PLAN, NO_PLAN);
        assert!(plans.set(&mut a, plan) && plans.set(&mut b, plan));
        assert!(!plans.set(&mut c, plan), "only two to lend");
        assert_eq!(c, NO_PLAN);
        assert_eq!(plans.get(a), plan);
        // An empty plan gives its entry back.
        assert!(plans.set(&mut a, Plan::NONE));
        assert_eq!(a, NO_PLAN);
        assert_eq!(plans.get(a), Plan::NONE);
        assert!(plans.set(&mut c, plan));
    }
}
