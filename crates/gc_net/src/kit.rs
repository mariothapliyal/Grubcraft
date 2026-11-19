//! The online match kit (docs/11-networking.md, "The match kit"): chat,
//! pings, surrender votes and pauses. None of it touches the simulation
//! except a passed surrender vote, which the host turns into a
//! [`gc_sim::Command::Concede`] for each player of the team, in a turn like
//! any order. Chat and pings travel beside the turns and are not recorded
//! in replays.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use gc_sim::PlayerId;

pub use gc_ui::match_kit::{MAX_CHAT, PingKind};

/// Chat lines one player may send in [`CHAT_WINDOW`]; more are dropped.
pub const CHAT_BURST: usize = 5;
pub const CHAT_WINDOW: Duration = Duration::from_secs(5);
/// Pings one player may send in [`PING_WINDOW`].
pub const PING_BURST: usize = 4;
pub const PING_WINDOW: Duration = Duration::from_secs(3);
/// Pauses each player has in a match.
pub const PAUSES_EACH: u8 = 3;
/// A pause carries on by itself after this long.
pub const PAUSE_LONGEST: Duration = Duration::from_secs(60);
/// The player who paused can carry on at once; anyone else after this.
pub const RESUME_AFTER: Duration = Duration::from_secs(10);
/// How long a surrender vote stays open.
pub const VOTE_TIME: Duration = Duration::from_secs(30);
/// After a vote fails, its caller waits this long to call another.
pub const VOTE_AGAIN_AFTER: Duration = Duration::from_secs(60);

/// A point on the map, in tiles, as sixteenths of a tile on the wire.
pub(crate) fn to_wire(at: [f32; 2]) -> (u32, u32) {
    let w = |v: f32| (v.max(0.0) * 16.0).round().min(u32::MAX as f32) as u32;
    (w(at[0]), w(at[1]))
}

pub(crate) fn from_wire(x: u32, y: u32) -> [f32; 2] {
    [x as f32 / 16.0, y as f32 / 16.0]
}

/// A chat line as it may be sent: control characters taken out, spaces
/// trimmed, cut to [`MAX_CHAT`] characters. `None` when nothing is left.
pub fn clean_chat(text: &str) -> Option<String> {
    let text: String = text
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_CHAT)
        .collect();
    (!text.is_empty()).then_some(text)
}

/// At most `most` things in any `window`: what a player may send.
#[derive(Clone, Debug, Default)]
pub(crate) struct Burst {
    sent: VecDeque<Instant>,
}

impl Burst {
    /// Whether one more may go now (and count it if so).
    pub(crate) fn allow(&mut self, now: Instant, most: usize, window: Duration) -> bool {
        while self
            .sent
            .front()
            .is_some_and(|&t| now.saturating_duration_since(t) >= window)
        {
            self.sent.pop_front();
        }
        if self.sent.len() >= most {
            return false;
        }
        self.sent.push_back(now);
        true
    }
}

/// A pause, as this machine sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoldView {
    /// Who paused.
    pub who: String,
    /// Until it carries on by itself.
    pub left: Duration,
    /// Whether this machine may carry on now.
    pub can_resume: bool,
}

/// The surrender vote of this machine's team, while one is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VoteView {
    /// Who called it.
    pub by: String,
    /// How many have said yes, and how many must.
    pub yes: u8,
    pub needed: u8,
    pub left: Duration,
    /// Whether this machine's player has said yes.
    pub voted: bool,
}

/// A pause as both ends keep it: who, and when it ends by itself.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Hold {
    pub(crate) player: PlayerId,
    pub(crate) until: Instant,
}

impl Hold {
    /// Whether `by` may carry on at `now`.
    pub(crate) fn can_resume(&self, by: PlayerId, now: Instant) -> bool {
        by == self.player || now + PAUSE_LONGEST >= self.until + RESUME_AFTER
    }
}

/// A team's surrender vote as both ends keep it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Vote {
    pub(crate) by: PlayerId,
    /// The players who said yes, a bit each.
    pub(crate) yes: u8,
    pub(crate) needed: u8,
    pub(crate) until: Instant,
}

impl Vote {
    pub(crate) fn view(&self, names: &[String], me: PlayerId, now: Instant) -> VoteView {
        VoteView {
            by: name_of(names, self.by),
            yes: self.yes.count_ones() as u8,
            needed: self.needed,
            left: self.until.saturating_duration_since(now),
            voted: self.yes & (1 << me) != 0,
        }
    }
}

pub(crate) fn name_of(names: &[String], player: PlayerId) -> String {
    names
        .get(usize::from(player))
        .cloned()
        .unwrap_or_else(|| format!("Player {}", player + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_lines_are_cleaned_and_cut() {
        assert_eq!(clean_chat("  hi\u{7}\n "), Some("hi".into()));
        assert_eq!(clean_chat(" \t\n"), None);
        let long = "é".repeat(500);
        assert_eq!(clean_chat(&long).unwrap().chars().count(), MAX_CHAT);
    }

    #[test]
    fn a_burst_allows_so_many_a_window() {
        let t = Instant::now();
        let mut b = Burst::default();
        for _ in 0..3 {
            assert!(b.allow(t, 3, Duration::from_secs(1)));
        }
        assert!(!b.allow(t, 3, Duration::from_secs(1)));
        assert!(b.allow(t + Duration::from_secs(1), 3, Duration::from_secs(1)));
    }

    #[test]
    fn points_cross_the_wire_to_a_sixteenth() {
        let (x, y) = to_wire([12.5, 300.0625]);
        assert_eq!(from_wire(x, y), [12.5, 300.0625]);
        assert_eq!(to_wire([-3.0, 1.0]), (0, 16));
        for k in PingKind::ALL {
            assert_eq!(PingKind::from_u8(k as u8), Some(k));
        }
        assert_eq!(PingKind::from_u8(9), None);
    }

    #[test]
    fn others_can_resume_a_pause_after_a_while() {
        let t = Instant::now();
        let h = Hold {
            player: 1,
            until: t + PAUSE_LONGEST,
        };
        assert!(h.can_resume(1, t));
        assert!(!h.can_resume(2, t));
        assert!(h.can_resume(2, t + RESUME_AFTER));
    }
}
