//! A Field Battle over the network (`gc_net`), as this machine sees it.
//!
//! What changes from a match on one machine: turns come from the host
//! (`Lan::next_turn`) instead of this machine's clock, this machine's
//! orders go to the host, the computer players think only on the host, the
//! sim keeps running behind the menu, and the sandbox tools (pause, speed,
//! switching sides, placing, tuning) are off because they would change one
//! machine's game and not the others'.

use std::time::{Duration, Instant};

use gc_match::cover::Cover;
use gc_platform::logln;
use gc_ui::lobby::MatchStart;
use gc_ui::match_kit::{self, ChatBox, ChatLine, LineKind, PING_LIFE_MS, PingKind};

/// How long a notice ("Alex lost connection") stays on screen.
const NOTICE_TIME: Duration = Duration::from_secs(8);

/// The build, as players compare it in the lobby: the version and the
/// simulation's rules version, which must match to play together (see
/// [`gc_ui::lobby::plays_with`]), then the commit it was built from
/// ("-dirty" with uncommitted changes), for people only.
pub fn version() -> String {
    format!(
        "{} sim {} ({})",
        env!("CARGO_PKG_VERSION"),
        gc_sim::SIM_VERSION,
        env!("GRUBCRAFT_COMMIT")
    )
}

/// Most pings on the ground at once; a new one pushes out the oldest.
const MAX_PINGS: usize = 8;

/// A ping from you or an ally, still on the ground.
#[derive(Clone, Debug)]
pub struct LivePing {
    pub id: u32,
    pub from: String,
    /// The pinger's colour.
    pub color: [u8; 4],
    pub choice: PingKind,
    /// Where, in map tiles.
    pub at: [f32; 2],
    /// When it came, on the HUD's clock (`Running::now_ms`).
    pub born_ms: u64,
}

/// The Alt+left press that may become a ping: where on the screen (UI
/// pixels, the wheel's middle) and on the map (tiles).
#[derive(Clone, Copy, Debug)]
pub struct PingPress {
    pub screen: (i32, i32),
    pub at: [f32; 2],
}

/// A networked match under way.
pub struct Online {
    /// This machine is the host (it runs the computer players).
    pub host: bool,
    /// The latest notice and when it arrived.
    pub notice: Option<(String, Instant)>,
    /// Set once a state hash disagreed with the host's.
    pub desync: Option<u32>,
    /// Where the desync report went (or why it couldn't be saved).
    pub report: Option<String>,
    /// Set when the match is over for this machine, with why.
    pub ended: Option<String>,
    /// When the last turn was simulated, for smooth movement in between.
    pub last_turn: Instant,
    /// The chat box (Enter, Shift+Enter) and the chat and ping lines so far.
    pub chat_box: ChatBox,
    pub lines: Vec<ChatLine>,
    pub pings: Vec<LivePing>,
    next_ping: u32,
    /// An Alt+left press waiting to become a ping.
    pub ping_press: Option<PingPress>,
    /// The match as the lobby started it (the host makes the minds that
    /// stand in for dropped players from it).
    pub start: MatchStart,
    /// The sides the host's computer plays for people who dropped out.
    pub cover: Cover,
}

impl Online {
    pub fn new(host: bool, start: MatchStart) -> Online {
        Online {
            host,
            notice: None,
            desync: None,
            report: None,
            ended: None,
            last_turn: Instant::now(),
            chat_box: ChatBox::default(),
            lines: Vec::new(),
            pings: Vec::new(),
            next_ping: 0,
            ping_press: None,
            start,
            cover: Cover::default(),
        }
    }

    /// A chat line arrived (or this player's own came back).
    pub fn hear(&mut self, from: String, color: [u8; 4], team: bool, text: String, now_ms: u64) {
        let kind = if team { LineKind::Team } else { LineKind::All };
        let line = ChatLine {
            kind,
            from,
            color,
            text,
            born_ms: now_ms,
        };
        match_kit::push_line(&mut self.lines, line);
    }

    /// A ping arrived: a marker on the ground for a few seconds, and a
    /// line in the log ("Ana: Attack here").
    pub fn ping(
        &mut self,
        from: String,
        color: [u8; 4],
        kind: PingKind,
        at: [f32; 2],
        now_ms: u64,
    ) {
        self.next_ping += 1;
        self.pings.push(LivePing {
            id: self.next_ping,
            from: from.clone(),
            color,
            choice: kind,
            at,
            born_ms: now_ms,
        });
        if self.pings.len() > MAX_PINGS {
            self.pings.remove(0);
        }
        let line = ChatLine {
            kind: LineKind::Ping,
            from,
            color,
            text: kind.label().to_string(),
            born_ms: now_ms,
        };
        match_kit::push_line(&mut self.lines, line);
    }

    /// Forget pings that have run their course.
    pub fn tidy_pings(&mut self, now_ms: u64) {
        self.pings
            .retain(|p| now_ms.saturating_sub(p.born_ms) < PING_LIFE_MS);
    }

    pub fn note(&mut self, text: String) {
        logln!("grubcraft: {text}");
        self.notice = Some((text, Instant::now()));
    }

    /// Lines for the top of the battle panel.
    pub fn lines(&self, status: Option<String>, waiting: Option<String>) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(why) = &self.ended {
            lines.push(format!(
                "NETWORK  the match is over here: {why}   Esc: menu"
            ));
            return lines;
        }
        lines.push(format!(
            "NETWORK  {}",
            status.unwrap_or_else(|| "not connected".into())
        ));
        if let Some(w) = waiting {
            lines.push(if self.host && w.starts_with("Paused") {
                format!("{w}   P: carry on without them")
            } else {
                w
            });
        }
        if let Some(tick) = self.desync {
            lines.push(format!(
                "Out of sync with the host since turn {tick}: this battle may look different on each machine."
            ));
            if let Some(r) = &self.report {
                lines.push(r.clone());
            }
        }
        if let Some((text, at)) = &self.notice
            && at.elapsed() < NOTICE_TIME
        {
            lines.push(text.clone());
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_map::map_hash;

    #[test]
    fn map_hash_sees_every_cell() {
        let flat = gc_sim::Ground::FLAT;
        let a = vec![1u8; 1000];
        let mut b = a.clone();
        b[999] = 2;
        assert_ne!(map_hash(&a, &flat), map_hash(&b, &flat));
        assert_ne!(map_hash(&a, &flat), map_hash(&a[..999], &flat));
        assert_eq!(map_hash(&a, &flat), map_hash(&a.clone(), &flat));
        // Heights and water count too.
        let heights = vec![7u16; 1000];
        let mut raised = heights.clone();
        raised[3] = 8;
        let ground = |heights, water_level| gc_sim::Ground {
            heights,
            water_level,
            cover: &[],
            growth: &[],
            footing: &[],
            flow: &[],
            water_levels: &[],
        };
        let base = map_hash(&a, &ground(&heights, 2));
        assert_ne!(base, map_hash(&a, &ground(&raised, 2)));
        assert_ne!(base, map_hash(&a, &ground(&heights, 3)));
        assert_ne!(base, map_hash(&a, &flat));
        // And so do the woods and tall grass.
        let wood = vec![2u8; 1000];
        let mut grass = wood.clone();
        grass[500] = 1;
        let covered = |cover| gc_sim::Ground {
            cover,
            growth: &[],
            ..ground(&heights, 2)
        };
        assert_ne!(base, map_hash(&a, &covered(&wood)));
        assert_ne!(
            map_hash(&a, &covered(&wood)),
            map_hash(&a, &covered(&grass))
        );
    }

    #[test]
    fn pings_land_in_the_log_and_run_out() {
        let start = MatchStart {
            settings: gc_ui::lobby::LobbySettings::default(),
            players: Vec::new(),
            local_player: 0,
            seed: Some(1),
        };
        let mut o = Online::new(false, start);
        o.ping(
            "Ana".into(),
            [1, 2, 3, 255],
            PingKind::Attack,
            [10.0, 20.0],
            1_000,
        );
        o.hear("Bo".into(), [4, 5, 6, 255], true, "hi".into(), 1_500);
        assert_eq!(o.lines.len(), 2);
        assert_eq!(o.lines[0].text, "Attack here");
        assert_eq!(o.lines[0].kind, LineKind::Ping);
        assert_eq!(o.lines[1].kind, LineKind::Team);
        o.tidy_pings(1_000 + PING_LIFE_MS - 1);
        assert_eq!(o.pings.len(), 1);
        o.tidy_pings(1_000 + PING_LIFE_MS);
        assert!(o.pings.is_empty());
        for i in 0..20 {
            o.ping("Ana".into(), [0; 4], PingKind::Look, [0.0; 2], i);
        }
        assert_eq!(o.pings.len(), MAX_PINGS);
        assert_eq!(o.pings.last().unwrap().id, 21);
    }

    #[test]
    fn version_names_the_commit() {
        assert!(version().starts_with(env!("CARGO_PKG_VERSION")));
        assert!(version().split(' ').nth(1).is_some_and(|c| !c.is_empty()));
    }
}
