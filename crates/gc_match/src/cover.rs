//! The computer standing in for people who dropped out of a network match
//! (docs/11-networking.md, "Disconnects and rejoining"). Only the host
//! does this: it asks its `gc_net::Lan` which people's places nobody
//! commands (`gc_net::Lan::uncommanded`), seats a mind in each, and
//! takes the mind away again once its player is back and caught up. The
//! minds' orders reach every machine through the turns, as any computer
//! player's do, so nothing here can make two machines' worlds differ, and
//! a replay records the orders like a person's.

use gc_ai::{Difficulty, Mind, MindConfig, Minds};
use gc_sim::{Command, PlayerCommand, PlayerId, World};
use gc_ui::lobby::MatchStart;

/// How well the computer plays a dropped player's side: well enough to
/// hold the line, without a Warlord's rehearsals adding to the host's work.
pub const COVER_LEVEL: Difficulty = Difficulty::General;

/// A side changing hands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The computer took over this player's side.
    Took(PlayerId),
    /// The computer gave this player's side back.
    Gave(PlayerId),
}

/// The sides the computer is playing for people who are away.
#[derive(Debug, Default)]
pub struct Cover {
    covering: Vec<PlayerId>,
}

impl Cover {
    /// Whether the computer is playing `player`'s side for them.
    pub fn covers(&self, player: PlayerId) -> bool {
        self.covering.contains(&player)
    }

    /// The sides the computer is playing for someone, in player order.
    pub fn covering(&self) -> &[PlayerId] {
        &self.covering
    }

    /// Follow `away` (the places nobody commands, from
    /// `gc_net::Lan::uncommanded`): seat a mind made by `seat` in each
    /// new one and take the mind out of each side whose player is back.
    /// A side taken over while the armies are still deploying is made
    /// ready in `out`, so the battle doesn't wait out the clock for someone
    /// who isn't there. Returns what changed, for the notices.
    pub fn follow(
        &mut self,
        away: &[PlayerId],
        world: &World,
        minds: &mut Minds,
        out: &mut Vec<PlayerCommand>,
        mut seat: impl FnMut(PlayerId) -> Mind,
    ) -> Vec<Change> {
        let mut changes = Vec::new();
        self.covering.retain(|&p| {
            let back = !away.contains(&p);
            if back {
                minds.remove(p);
                changes.push(Change::Gave(p));
            }
            !back
        });
        for &p in away {
            // A side a computer player already holds (the lobby's) is left be.
            if self.covers(p) || minds.get(p).is_some() {
                continue;
            }
            minds.add(seat(p));
            self.covering.push(p);
            changes.push(Change::Took(p));
            if world.deploying() {
                out.push(PlayerCommand {
                    player: p,
                    command: Command::Ready,
                });
            }
        }
        self.covering.sort_unstable();
        changes
    }
}

/// The Field Battle mind that plays `player`'s side of `setup` for them, of
/// their faction, its temper and choices seeded from `seed`.
pub fn field_mind(setup: &MatchStart, player: PlayerId, seed: u64) -> Mind {
    let faction = setup
        .players
        .get(usize::from(player))
        .map_or(gc_ai::Faction::EmberClans, |s| s.faction);
    let pick = gc_fixed::Pcg32::new(seed, u64::from(player)).next_u32();
    Mind::new(MindConfig {
        party: player,
        difficulty: COVER_LEVEL,
        personality: gc_ai::personality::pick(faction, pick),
        seed: seed ^ u64::from(player),
    })
}

/// The notice for everyone when the computer takes over or gives back
/// `name`'s side.
pub fn notice(change: Change, name: &str) -> String {
    match change {
        Change::Took(_) => format!(
            "The computer ({}) is playing {name}'s side until they rejoin.",
            COVER_LEVEL.name()
        ),
        Change::Gave(_) => format!("{name} has caught up and commands their side again."),
    }
}
