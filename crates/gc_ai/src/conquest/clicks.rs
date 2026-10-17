//! The hands a Conquest mind runs its economy with: every gather, build,
//! train or upgrade order costs attention, refilled at its level's orders
//! per minute, and leaves after its reaction time, as a person's clicks
//! would (docs/18-ai.md, "Commands and attention"). The army's orders go
//! through [`crate::Orders`] and have attention of their own, the way a
//! player splits their hands between base and battle.

use std::collections::VecDeque;

use gc_sim::{Command, PlayerCommand, PlayerId};

use crate::perception::TICKS;

#[derive(Clone, Debug)]
pub struct Clicks {
    party: PlayerId,
    reaction: u32,
    refill: f32,
    cap: f32,
    attention: f32,
    last_tick: Option<u32>,
    queue: VecDeque<(u32, Command)>,
    /// Orders sent so far.
    pub count: u32,
}

impl Clicks {
    pub fn new(party: PlayerId, reaction_ticks: u32, orders_per_minute: u32) -> Clicks {
        let per_minute = orders_per_minute.max(1) as f32;
        let cap = (per_minute / 4.0).max(4.0);
        Clicks {
            party,
            reaction: reaction_ticks,
            refill: per_minute / (60.0 * TICKS),
            cap,
            attention: cap,
            last_tick: None,
            queue: VecDeque::new(),
            count: 0,
        }
    }

    /// Top attention up for the time since the last look.
    pub fn refresh(&mut self, now: u32) {
        if let Some(last) = self.last_tick {
            let dt = now.saturating_sub(last) as f32;
            self.attention = (self.attention + dt * self.refill).min(self.cap);
        }
        self.last_tick = Some(now);
    }

    /// Whether there is attention for another order now.
    pub fn free(&self) -> bool {
        self.attention >= 1.0
    }

    /// Give `command` if there is attention for it; it leaves after the
    /// reaction time. Returns whether it was given.
    pub fn give(&mut self, now: u32, command: Command) -> bool {
        if !self.free() {
            return false;
        }
        self.attention -= 1.0;
        self.queue.push_back((now + self.reaction, command));
        true
    }

    /// Give `command` free of attention: one of several orders given in
    /// the same click (several workers sent to one seep).
    pub fn give_with(&mut self, now: u32, command: Command) {
        self.queue.push_back((now + self.reaction, command));
    }

    /// Send the orders whose reaction time is up.
    pub fn release(&mut self, tick: u32, out: &mut Vec<PlayerCommand>) {
        while let Some(&(at, command)) = self.queue.front() {
            if at > tick {
                break;
            }
            self.queue.pop_front();
            self.count += 1;
            out.push(PlayerCommand {
                player: self.party,
                command,
            });
        }
    }

    /// Orders waiting out their reaction time.
    pub fn pending(&self) -> impl Iterator<Item = &Command> {
        self.queue.iter().map(|(_, c)| c)
    }
}
