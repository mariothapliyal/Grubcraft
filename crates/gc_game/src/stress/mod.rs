//! The stress-scenario suite (docs/40-real-play-checks.md): armies of the
//! real game data played through the hard places of real play, a gate, a
//! bridge under fire, a retreat through woods, a computer player attacking
//! walls, with no window. Each scenario says what must hold (its checks)
//! and what it measured (its metrics); the runner times every tick and the
//! whole scenario. A failed check, or a scenario over its time budget,
//! fails the suite, and CI runs it on every pull request.
//!
//! ```text
//! cargo test --release -p gc_game stress_suite -- --ignored --nocapture
//! STRESS=gate cargo test --release -p gc_game stress_suite -- --ignored --nocapture
//! ```
//!
//! Adding a scenario: write a file beside this one with a `pub(super)
//! const SCENARIO: Scenario` and its `run` function, and list it in
//! [`SCENARIOS`]. Build the world with [`arena::Arena`] (a flat map you
//! paint rivers, woods and thicket on) or any map of gc_map, tick it only
//! through [`Report::tick`] so the time is measured (the setting-up
//! helpers `spawn` and `put_up` tick untimed), and end with
//! [`Report::check`] for each thing that must hold. Checks should be on
//! what the simulation decides (ticks, positions, losses), which are the
//! same on every machine; time is held only to the generous budget.

mod arena;
mod big_army;
mod bridge;
mod crossing;
mod forest;
mod forest_pass;
mod gate;
mod mountain;
mod orders;
mod town;
mod walls;

use std::fmt::Write as _;
use std::time::Instant;

use gc_sim::{PlayerCommand, TickStats, World};

use arena::Real;

/// One scenario of the suite.
pub(crate) struct Scenario {
    /// Short name, for `STRESS=name` and the report.
    pub name: &'static str,
    /// One line on what it plays.
    pub about: &'static str,
    /// The longest the whole scenario may take, in milliseconds of wall
    /// time on a CI runner. Generous: it catches a slowdown of several
    /// times, not noise.
    pub budget_ms: u64,
    /// Plays it on the game data, loaded once for the whole suite.
    pub run: fn(&Real, &mut Report),
}

/// Every scenario, in the order they run.
const SCENARIOS: &[Scenario] = &[
    gate::SCENARIO,
    bridge::SCENARIO,
    orders::SCENARIO,
    forest::SCENARIO,
    walls::SCENARIO,
    forest_pass::SCENARIO,
    crossing::SCENARIO,
    town::SCENARIO,
    big_army::GATE,
    big_army::BRIDGE,
    mountain::SCENARIO,
];

/// What one scenario found: its checks, its metrics and its tick times.
#[derive(Default)]
pub(crate) struct Report {
    checks: Vec<(String, bool)>,
    metrics: Vec<(String, String)>,
    /// Each tick's time, in milliseconds.
    times: Vec<f64>,
}

impl Report {
    /// Tick `world` with `commands`, timing it.
    pub fn tick(&mut self, world: &mut World, commands: &[PlayerCommand]) -> TickStats {
        let began = Instant::now();
        let stats = world.tick(commands);
        self.times.push(began.elapsed().as_secs_f64() * 1000.0);
        stats
    }

    /// Something that must hold.
    pub fn check(&mut self, what: impl Into<String>, ok: bool) {
        self.checks.push((what.into(), ok));
    }

    /// Something measured, for the report.
    pub fn metric(&mut self, what: impl Into<String>, value: impl std::fmt::Display) {
        self.metrics.push((what.into(), value.to_string()));
    }
}

/// Game time `t` in ticks as seconds, or "never".
pub(crate) fn seconds(t: Option<u32>) -> String {
    t.map_or("never".into(), |t| {
        format!("{:.1} s", t as f32 / gc_sim::TICKS_PER_SECOND as f32)
    })
}

/// Run `scenario`, print what it found and say whether it passed.
fn run(scenario: &Scenario, real: &Real, out: &mut String) -> bool {
    let mut report = Report::default();
    let began = Instant::now();
    // A scenario that panics fails, and the rest still run and report.
    let played = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (scenario.run)(real, &mut report);
    }));
    if let Err(e) = played {
        let why = (e.downcast_ref::<String>().cloned())
            .or_else(|| e.downcast_ref::<&str>().map(ToString::to_string))
            .unwrap_or_default();
        report.check(format!("ran to the end (panicked: {why})"), false);
    }
    let wall_ms = began.elapsed().as_millis() as u64;
    let in_time = wall_ms <= scenario.budget_ms;
    let passed = in_time && report.checks.iter().all(|(_, ok)| *ok);
    let mut times = report.times.clone();
    times.sort_by(f64::total_cmp);
    let at = |q: f64| {
        let i = ((times.len().max(1) - 1) as f64 * q) as usize;
        times.get(i).copied().unwrap_or(0.0)
    };
    let _ = writeln!(
        out,
        "{} {}: {}\n  {} ticks, p50 {:.2} ms, p99 {:.2} ms, max {:.2} ms, total {} ms (budget {} ms{})",
        if passed { "PASS" } else { "FAIL" },
        scenario.name,
        scenario.about,
        report.times.len(),
        at(0.5),
        at(0.99),
        at(1.0),
        wall_ms,
        scenario.budget_ms,
        if in_time { "" } else { ", OVER" },
    );
    for (what, value) in &report.metrics {
        let _ = writeln!(out, "  {what}: {value}");
    }
    for (what, ok) in &report.checks {
        let _ = writeln!(out, "  [{}] {what}", if *ok { "ok" } else { "FAILED" });
    }
    passed
}

/// The whole suite, or the scenario `STRESS` names. Slow in a debug build,
/// so CI runs it in release.
#[test]
#[ignore = "slow in debug builds: CI runs it with --release"]
fn stress_suite() {
    let only = std::env::var("STRESS").ok().filter(|s| !s.is_empty());
    let chosen: Vec<&Scenario> = SCENARIOS
        .iter()
        .filter(|s| only.as_deref().is_none_or(|o| o == s.name))
        .collect();
    assert!(
        !chosen.is_empty(),
        "no scenario named {only:?}; there are {:?}",
        SCENARIOS.iter().map(|s| s.name).collect::<Vec<_>>()
    );
    let real = Real::load();
    let mut out = String::new();
    let failed: Vec<&str> = chosen
        .iter()
        .filter(|s| !run(s, &real, &mut out))
        .map(|s| s.name)
        .collect();
    println!("{out}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stress");
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(dir.join("report.txt"), &out);
    }
    assert!(failed.is_empty(), "stress scenarios failed: {failed:?}");
}

#[test]
fn scenario_names_are_unique() {
    let mut names: Vec<&str> = SCENARIOS.iter().map(|s| s.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), SCENARIOS.len());
}
