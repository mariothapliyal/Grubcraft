//! Golden replays: every recorded battle in `replays/` must still play out
//! exactly as it was recorded. CI runs this on macOS, Windows and Linux, so
//! it also checks that every platform plays them identically (what lets Mac
//! and Windows players share a lockstep match).
//!
//! A failure means the simulation now behaves differently. If that is
//! intended, re-bless the replays in the same pull request and say why:
//! `cargo run --release -p gc_replay -- bless replays`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gc_replay::{Replay, pinned_table};
use gc_sim::{Command, SIM_VERSION};

/// Every golden replay, by file.
fn goldens() -> Vec<(PathBuf, Replay)> {
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../replays");
    let files = gc_replay::files_in(&folder).expect("the replays folder");
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
/// here until `kind` names it; add the name to `KINDS` too, and this test
/// fails until a golden gives it.
#[test]
fn the_goldens_cover_every_kind_of_command() {
    let kind = |c: &Command| match c {
        Command::SpawnRegiment { .. } => "SpawnRegiment",
        Command::MoveTo { .. } => "MoveTo",
        Command::Attack { .. } => "Attack",
        Command::AttackMove { .. } => "AttackMove",
        Command::Then { .. } => "Then",
        Command::RemoveRegiment { .. } => "RemoveRegiment",
        Command::TuneUnit { .. } => "TuneUnit",
        Command::SetFormation { .. } => "SetFormation",
        Command::DeployZone { .. } => "DeployZone",
        Command::Ready => "Ready",
        Command::SpawnStructure { .. } => "SpawnStructure",
        Command::AttackStructure { .. } => "AttackStructure",
        Command::RemoveStructure { .. } => "RemoveStructure",
        Command::Gather { .. } => "Gather",
        Command::Trade { .. } => "Trade",
        Command::Build { .. } => "Build",
        Command::Clear { .. } => "Clear",
        Command::Fell { .. } => "Fell",
        Command::Dump { .. } => "Dump",
        Command::Construct { .. } => "Construct",
        Command::CallOff { .. } => "CallOff",
        Command::Train { .. } => "Train",
        Command::Untrain { .. } => "Untrain",
        Command::Upgrade { .. } => "Upgrade",
        Command::Rally { .. } => "Rally",
        Command::FallBack { .. } => "FallBack",
        Command::Brace { .. } => "Brace",
        Command::Hold { .. } => "Hold",
        Command::Skirmish { .. } => "Skirmish",
        Command::Run { .. } => "Run",
        Command::HandOver { .. } => "HandOver",
        Command::UsePower { .. } => "UsePower",
        Command::Cast { .. } => "Cast",
        Command::Evolve { .. } => "Evolve",
        Command::FocusTower { .. } => "FocusTower",
        Command::SetGate { .. } => "SetGate",
        Command::BuildWall { .. } => "BuildWall",
        Command::DrawLine { .. } => "DrawLine",
        Command::PlanLine { .. } => "PlanLine",
        Command::PlanWall { .. } => "PlanWall",
        Command::DropPlan { .. } => "DropPlan",
        Command::WorkPlan { .. } => "WorkPlan",
        Command::RaiseWall { .. } => "RaiseWall",
        Command::Torches { .. } => "Torches",
        Command::ManWall { .. } => "ManWall",
        Command::Climb { .. } => "Climb",
        Command::PlantStakes { .. } => "PlantStakes",
        Command::Burn { .. } => "Burn",
        Command::Concede => "Concede",
    };
    const KINDS: [&str; 46] = [
        "SpawnRegiment",
        "MoveTo",
        "Attack",
        "AttackMove",
        "Then",
        "RemoveRegiment",
        "TuneUnit",
        "SetFormation",
        "DeployZone",
        "Ready",
        "SpawnStructure",
        "AttackStructure",
        "RemoveStructure",
        "Gather",
        "Trade",
        "Build",
        "Clear",
        "Fell",
        "Dump",
        "Construct",
        "CallOff",
        "Train",
        "Untrain",
        "Upgrade",
        "Rally",
        "FallBack",
        "Brace",
        "HandOver",
        "UsePower",
        "Cast",
        "Evolve",
        "FocusTower",
        "SetGate",
        "BuildWall",
        "DrawLine",
        "PlanLine",
        "PlanWall",
        "DropPlan",
        "WorkPlan",
        "RaiseWall",
        "Torches",
        "ManWall",
        "Climb",
        "Burn",
        "PlantStakes",
        "Concede",
    ];
    let seen: BTreeSet<&str> = goldens()
        .iter()
        .flat_map(|(_, replay)| replay.turns.iter().flat_map(|(_, c)| c))
        .map(|c| kind(&c.command))
        .collect();
    let missing: Vec<&str> = KINDS.into_iter().filter(|k| !seen.contains(k)).collect();
    assert!(missing.is_empty(), "no golden gives {missing:?}");
}

/// How the goldens played under [`SIM_VERSION`] (and were blessed with it).
/// A change that makes them play differently must bump `SIM_VERSION`, so
/// that other builds and old replays know: this table then fails, and is
/// rewritten by `cargo run --release -p gc_replay -- pin` (or by
/// `scripts/rules-bump.py`, which also takes the next free version).
const PINNED: (u32, &[(&str, u64)]) = (
    98,
    &[
        ("conquest-evolve-tithe-1v1", 0xb1d201d26fc0c8f5),
        ("conquest-heroes-1v1", 0x62246c7c37c180e6),
        ("conquest-ribcage-1v1", 0x9a4d3df21094528b),
        ("conquest-river-1v1", 0x84951cece3f23b03),
        ("field-1v1", 0x1080ec9c3a04a99d),
        ("field-2v2", 0x5b4365469e2b9665),
        ("field-8000pt-2v2", 0xefbba63da84c0d6f),
        ("field-brace-1v1", 0x6cec4d7577644c6a),
        ("field-brigade-1v1", 0xe80d993763418f22),
        ("field-champions-1v1", 0x4fac7f060c50c101),
        ("field-deploy-1v1", 0x007ee3d677aba7cc),
        ("field-fire-1v1", 0xcf56085bf1f4bb56),
        ("field-flood-1v1", 0x768a0306b441904d),
        ("field-handover-2v2", 0xa698dfa39180efc1),
        ("field-new-orders-1v1", 0x7d7e576e9fd9890b),
        ("field-night-rain-1v1", 0x0014ea8ec4bc3214),
        ("field-orders-1v1", 0xe6dcc412f803da75),
        ("field-river-storm-1v1", 0xc20e5bc6f4126c92),
        ("field-woods-1v1", 0xafeedf0678b14fce),
        ("sandbox-tuning", 0xf29963b45e8a0dde),
    ],
);

#[test]
fn the_sim_version_changes_when_the_goldens_do() {
    let goldens = goldens();
    let table = pinned_table(
        goldens
            .iter()
            .map(|(file, replay)| (file.file_stem().unwrap().to_string_lossy(), replay)),
    );
    assert_eq!(
        PINNED.0, SIM_VERSION,
        "SIM_VERSION changed: bless the goldens, then run \
         `cargo run --release -p gc_replay -- pin`, which writes\n{table}"
    );
    for (file, replay) in &goldens {
        let name = file.file_stem().unwrap().to_string_lossy();
        assert_eq!(
            replay.header.sim_version,
            Some(SIM_VERSION),
            "{name} wasn't blessed under this sim version: \
             cargo run --release -p gc_replay -- bless replays"
        );
        let pinned = PINNED.1.iter().find(|(n, _)| *n == name).map(|&(_, f)| f);
        assert_eq!(
            pinned,
            Some(replay.fingerprint()),
            "{name} plays differently from how it did under sim version {SIM_VERSION} \
             (or is new). If the sim changed, bump gc_sim::SIM_VERSION, bless, and run \
             `cargo run --release -p gc_replay -- pin` (when main took your number first, \
             `python3 scripts/rules-bump.py` catches up); the table now is\n{table}"
        );
    }
}
