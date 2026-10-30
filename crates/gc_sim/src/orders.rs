//! Orders given ahead (docs/29, "Orders that respond instantly"): a short
//! queue of steps per regiment, so Shift-queued waypoints and attacks are
//! carried out one after another, and attack-moves, which march but stop
//! to fight any enemy met on the way.
//!
//! Both live in the simulation, so they go through the lockstep and
//! replays like every other order. A regiment's queue holds at most
//! [`QUEUE_LEN`] steps after the one it is carrying out. Any order given
//! without Shift (a move, an attack, falling back) clears it.
//!
//! **Attack-move.** A regiment on an attack-move looks round every
//! [`HUNT_EVERY`] ticks while it marches. When an enemy regiment it can
//! see is within [`HUNT_RADIUS`] (or its bows' reach, if further), it
//! puts the rest of its march back at the front of its queue and attacks
//! that enemy. Once the enemy is beaten (gone, routing or shattered), it
//! picks the march up again where it left off.

use gc_fixed::{Angle, Fx, FxVec2};

use crate::hash::StateHasher;
use crate::regiments::RegimentHandle;
use crate::state::State;

/// Steps a regiment can have queued behind the order it is carrying out.
pub const QUEUE_LEN: usize = 4;
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

impl Step {
    /// Where the step leads, for drawing a regiment's queue: a march's
    /// end, or none for an attack (the target moves).
    pub fn destination(&self) -> Option<FxVec2> {
        match *self {
            Step::Move { target, .. } => Some(target),
            Step::Attack { .. } => None,
        }
    }
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

    /// Put `step` at the front (an attack-move's march, set aside for a
    /// fight). When the queue is full its last step is dropped.
    pub(crate) fn push_front(&mut self, step: Step) {
        self.steps.copy_within(0..QUEUE_LEN - 1, 1);
        self.steps[0] = Some(step);
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
        q.push_front(to(-1));
        assert_eq!(
            q.iter().collect::<alloc::vec::Vec<_>>(),
            [to(-1), to(0), to(1), to(2)]
        );
        assert_eq!(q.pop(), Some(to(-1)));
        assert_eq!(q.pop(), Some(to(0)));
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
}
