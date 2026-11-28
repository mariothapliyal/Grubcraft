//! The Field Battle heroes golden replay (`replays/field-heroes-1v1.gcr`):
//! a lobby Field Battle on the roster table between two Veteran computer
//! players, the Ember Clans and the Carrion Concord, at the Battle budget
//! with champions allowed (docs/43-heroes.md, H3). Each buys its hero at
//! the level a quarter of its points buys (`SpawnHero`, replay format 62),
//! who is its general, steps out as its champion and levels as the battle
//! is fought, casting as it goes.
//!
//! To record it again (only when the data or the computer players change
//! what it does; a simulation change that only moves the hashes is blessed
//! instead):
//!
//! ```text
//! GOLDEN=field-heroes-1v1 cargo test --release -p gc_game record_golden_replays -- --ignored --nocapture
//! ```

use gc_ai::Difficulty;
use gc_replay::Replay;
use gc_sim::TICKS_PER_SECOND;
use gc_ui::lobby::{Budget, Faction as F, LobbySettings, MatchStart, Slot, SlotKind};

use crate::roster;

pub const NAME: &str = "field-heroes-1v1";
const SEED: u64 = 0x4e52;
const MINUTES: u32 = 6;

/// The lobby's match: two Veteran computer players, champions allowed.
fn setup() -> MatchStart {
    let players = [F::EmberClans, F::CarrionConcord]
        .into_iter()
        .enumerate()
        .map(|(k, faction)| {
            let mut s = Slot::open(k);
            s.team = k as u8;
            s.faction = faction;
            s.kind = SlotKind::Ai;
            s.name = format!("AI {}", k + 1);
            s.ai_level = Difficulty::Veteran.index() as u8;
            s
        })
        .collect();
    MatchStart {
        settings: LobbySettings {
            map_seed: SEED,
            budget: Budget::Battle,
            champions: true,
            ..LobbySettings::default()
        },
        players,
        local_player: 0,
        seed: None,
    }
}

/// Play the match through a recorder.
pub fn record() -> Replay {
    let data = crate::data::load();
    let tables = roster::field_tables(&data);
    let setup = setup();
    let map = gc_ui::menu::preview_key(&setup.settings, 2);
    let map = crate::MapKey::from(map).make();
    let note = format!(
        "{NAME}: Field Battle recorded from gc_game's field_heroes_golden.rs, seed {SEED:#x}; \
         Ember Clans v Carrion Concord, Veteran computer players with their heroes, Battle \
         budget, champions allowed, {MINUTES} minutes"
    );
    let mut m = gc_match::play::Match::new(&setup, SEED, &map, &tables, false, Some(note));
    while m.world.tick_count() < MINUTES * 60 * TICKS_PER_SECOND && !m.step(&[]).decided {}
    let mut replay = m.finish().expect("recorded");
    // The roster table's names, so the game knows which table to show it
    // with (as the game's own recordings keep them).
    replay.header.unit_names = data
        .conquest_units
        .iter()
        .map(|u| u.name().to_string())
        .collect();
    replay
}

#[cfg(test)]
mod tests {
    use gc_sim::Command;

    use super::*;

    /// Each side's hero takes the field at a level, is its general, and
    /// gains experience as the battle is fought.
    #[test]
    fn each_sides_hero_leads_it_and_levels() {
        let replay = record();
        let spawned: Vec<(u8, u8)> = (replay.turns.iter())
            .flat_map(|(_, c)| c)
            .filter_map(|c| match c.command {
                Command::SpawnHero { level, .. } => Some((c.player, level)),
                _ => None,
            })
            .collect();
        assert_eq!(spawned.len(), 2, "{spawned:?}");
        assert!(spawned.iter().all(|&(_, l)| l > 1), "{spawned:?}");
        let mut world = replay.new_world();
        let start: Vec<u32> = {
            world.tick(replay.commands_at(0));
            world.heroes().map(|h| h.xp).collect()
        };
        for t in 1..replay.info().ticks {
            world.tick(replay.commands_at(t));
        }
        assert_eq!(world.generals().filter(|g| g.hero).count(), 2);
        let gained = (world.heroes().zip(&start)).filter(|(h, xp)| h.xp > **xp);
        assert!(
            gained.count() >= 1,
            "{:?}",
            world.heroes().collect::<Vec<_>>()
        );
    }
}
