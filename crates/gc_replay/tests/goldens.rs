//! Golden replays: every recorded battle in `replays/` must still play out
//! exactly as it was recorded. CI runs this on macOS, Windows and Linux, so
//! it also checks that every platform plays them identically (what lets Mac
//! and Windows players share a lockstep match).
//!
//! A failure means the simulation now behaves differently. If that is
//! intended, re-bless the replays in the same pull request and say why:
//! `cargo run --release -p gc_replay -- bless replays`.

use std::path::Path;

use std::path::PathBuf;

use gc_replay::{EXTENSION, Replay};
use gc_sim::Command;

/// Every golden replay, by file.
fn goldens() -> Vec<(PathBuf, Replay)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../replays");
    let mut files: Vec<_> = std::fs::read_dir(&folder)
        .expect("the replays folder")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|f| f.extension().is_some_and(|x| x == EXTENSION))
        .collect();
    files.sort();
    assert!(files.len() >= 4, "golden replays missing from {folder:?}");
    files
        .into_iter()
        .map(|file| {
            let replay = Replay::load(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
            (file, replay)
        })
        .collect()
}

#[test]
fn golden_replays_play_as_recorded() {
    let mut failures = Vec::new();
    for (file, replay) in &goldens() {
        if let Err(d) = replay.verify() {
            failures.push(format!("{}: {d}", file.display()));
        }
    }
    assert!(
        failures.is_empty(),
        "golden replays no longer play as recorded (re-bless them if this is intended: \
         cargo run --release -p gc_replay -- bless replays):\n{}",
        failures.join("\n")
    );
}

/// Between them the goldens give every kind of command, so a change to how
/// any of them plays out shows up. A new kind of command fails to compile
/// here until it has a place in this list, and then this test until a
/// golden gives it.
#[test]
fn the_goldens_cover_every_kind_of_command() {
    let kind = |c: &Command| match c {
        Command::SpawnRegiment { .. } => 0,
        Command::MoveTo { .. } => 1,
        Command::Attack { .. } => 2,
        Command::RemoveRegiment { .. } => 3,
        Command::TuneUnit { .. } => 4,
        Command::SetFormation { .. } => 5,
        Command::DeployZone { .. } => 6,
        Command::Ready => 7,
    };
    let mut seen = [false; 8];
    for (_, replay) in goldens() {
        for (_, commands) in &replay.turns {
            for c in commands {
                seen[kind(&c.command)] = true;
            }
        }
    }
    let missing: Vec<usize> = (0..seen.len()).filter(|&k| !seen[k]).collect();
    assert!(
        missing.is_empty(),
        "no golden gives commands of kind {missing:?}"
    );
}
