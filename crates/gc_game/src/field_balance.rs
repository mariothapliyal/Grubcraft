//! The Field Battle balance table as a lobby plays it: the Conquest roster
//! table, regiments priced from their roster cost ([`roster::field_points`]),
//! each AI buying its own preset mix, on the lobby's default map kind with
//! a fresh map each battle (docs/17-unit-rosters.md#roster-field-table).
//! `gc_match`'s `faction_table` plays the five `data/units.ron` regiments
//! instead, which only the sandbox and the golden replays still use.

use gc_ai::Difficulty;
use gc_ui::lobby::{Faction as F, LobbySettings, MatchStart, Slot, SlotKind};

use crate::roster;

/// Two Veteran AIs, `a` in slot 0 on `map_seed`'s map.
fn setup(a: F, b: F, map_seed: u64) -> MatchStart {
    let players = [a, b]
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
            map_seed,
            ..LobbySettings::default()
        },
        players,
        local_player: 0,
        seed: None,
    }
}

/// Each pair of factions, `AI_BATTLES` battles a pair (default 24) from seed
/// `AI_FROM` (default 1), swapping ends every battle. Prints the first
/// faction's wins, losses and battles nobody had won after ten minutes.
/// `AI_PAIR=Brood` or `AI_PAIR=Brood,Choir` narrows it.
/// `cargo test -p gc_game --release roster_field_table -- --ignored --nocapture`
#[test]
#[ignore = "slow: the full roster Field Battle table, run by hand"]
fn roster_field_table() {
    let n: u64 = std::env::var("AI_BATTLES").map_or(24, |v| v.parse().unwrap());
    let from: u64 = std::env::var("AI_FROM").map_or(1, |v| v.parse().unwrap());
    let data = crate::data::load();
    let tables = gc_match::Tables {
        units: (data.conquest_units.iter())
            .map(|u| gc_match::stats_of(&u.stats))
            .collect(),
        factions: (data.conquest_units.iter())
            .map(|u| u.stats.faction())
            .collect(),
        points: (data.conquest_units.iter())
            .map(roster::field_points)
            .collect(),
    };
    let mut pairs = Vec::new();
    for (i, &a) in F::ALL.iter().enumerate() {
        for &b in &F::ALL[i + 1..] {
            pairs.push((a, b));
        }
    }
    if let Ok(only) = std::env::var("AI_PAIR") {
        pairs.retain(|(a, b)| only.split(',').all(|f| f == a.name() || f == b.name()));
    }
    let limit = 10 * 60 * gc_sim::TICKS_PER_SECOND;
    let tables = &tables;
    let rows: Vec<String> = std::thread::scope(|s| {
        let jobs: Vec<_> = pairs
            .iter()
            .map(|&(a, b)| {
                s.spawn(move || {
                    let (mut won, mut lost) = (0u64, 0u64);
                    for seed in from..from + n {
                        // `a` takes slot 0 (whose result `won` is) on even seeds.
                        let a_first = seed % 2 == 0;
                        let setup = if a_first {
                            setup(a, b, seed)
                        } else {
                            setup(b, a, seed)
                        };
                        let map = gc_ui::menu::preview_key(&setup.settings, 2);
                        let map = crate::MapKey::from(map).make();
                        let mut m =
                            gc_match::play::Match::new(&setup, seed, &map, tables, false, None);
                        // `AI_ARMY` prints the armies of the first battle.
                        if seed == from && std::env::var_os("AI_ARMY").is_some() {
                            for p in 0..2 {
                                let mut army: Vec<String> = (m.world.regiments.iter())
                                    .filter(|r| r.owner == p)
                                    .map(|r| {
                                        let u = &data.conquest_units[r.unit as usize];
                                        format!(
                                            "{} ({} pts, {} sup)",
                                            u.name(),
                                            roster::field_points(u),
                                            u.supply
                                        )
                                    })
                                    .collect();
                                army.sort();
                                army.dedup_by(|a, b| a == b);
                                println!(
                                    "{} seed {seed} slot {p}: {} regiments: {}",
                                    setup.players[p as usize].faction.name(),
                                    m.world.regiments.iter().filter(|r| r.owner == p).count(),
                                    army.join(", ")
                                );
                            }
                        }
                        while m.world.tick_count() < limit && !m.step(&[]).decided {}
                        match m.battle.won {
                            Some(w) if w == a_first => won += 1,
                            Some(_) => lost += 1,
                            None => {}
                        }
                    }
                    format!(
                        "{:>15} v {:<15} won {won:>3}, lost {lost:>3}, undecided {:>3}  ({:.0}%)",
                        a.name(),
                        b.name(),
                        n - won - lost,
                        100.0 * won as f64 / n as f64,
                    )
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    for row in rows {
        println!("{row}");
    }
}
