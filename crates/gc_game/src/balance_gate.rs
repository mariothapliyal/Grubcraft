//! The balance tournament and its gate (docs/29, "Automatic balance
//! tournament"; docs/17-unit-rosters.md, "Balance tournament"): computer
//! players fight every pair of factions in Conquest one-on-one on two maps
//! and in Field Battles on the lobby's full rosters, each on two seed sets,
//! and the report says who wins, when each faction's first army is ready
//! and its first attack lands, what it earns, and which units decide its
//! games. The test fails when a pair is clearly outside the band or a
//! faction's first army comes a minute before everybody else's.
//!
//! ```text
//! cargo test --release -p gc_game balance_tournament -- --ignored --nocapture
//! ```
//!
//! `BALANCE_GAMES` seeds a set for Conquest (each played from both starts),
//! `BALANCE_BATTLES` Field Battles a pair a set, `BALANCE_LEVEL` the
//! computer players' level, `BALANCE_MAPS` the Conquest maps (`ribcage`,
//! `grown`, comma separated) and `BALANCE_FIELD=off` leaves the Field
//! Battles out. The report is written to `target/balance/tournament.md`.

use gc_ai::Difficulty;
use gc_data::Faction;
use gc_ui::lobby::{DEFAULT_CONQUEST_MAP, MapSize};

use crate::conquest_ai::tests::{ARMY_READY, MARKS, Played, level_named, play_pairs};

/// Whether failures fail the test. Off while main itself is outside the
/// band (the tournament of 2026-11-22 failed 8 pairs): the report still says
/// FAIL, and `BALANCE_STRICT=1` makes it fail anyway. Turned on by the
/// rebalance that brings main inside.
const GATE_ON: bool = false;

/// A pair's win share must stay inside this band, percent.
const BAND: (f64, f64) = (40.0, 60.0);
/// The first seed of each Conquest seed set, and of each Field Battle one.
const CONQUEST_SETS: [u64; 2] = [1, 101];
const FIELD_SETS: [u64; 2] = [1, 1001];
/// A faction whose first army is ready this many seconds before every
/// other faction's fails the gate (the Brood swarm worry, docs/29).
const RUSH_SECONDS: u32 = 60;
/// The even early game's target: first armies within this many seconds.
const EVEN_SECONDS: u32 = 30;
/// How sure a failure must be: the z of a one-sided 95% interval.
const Z: f64 = 1.645;

/// One Conquest map the tournament plays: its lobby name and
/// [`crate::conquest::make_map`]'s choice and size.
struct ConquestMap {
    name: &'static str,
    key: &'static str,
    choice: usize,
    tiles: u32,
}

const MAPS: [ConquestMap; 2] = [
    ConquestMap {
        name: "Ribcage Pass",
        key: "ribcage",
        choice: 0,
        tiles: 0,
    },
    ConquestMap {
        name: "Grown land, Medium",
        key: "grown",
        choice: DEFAULT_CONQUEST_MAP,
        tiles: MapSize::Medium.conquest_tiles(),
    },
];

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// Wins of the pair's first faction and of its second over a set.
#[derive(Clone, Copy, Default)]
struct Tally {
    won: u32,
    lost: u32,
    open: u32,
}

impl Tally {
    fn decided(self) -> u32 {
        self.won + self.lost
    }

    /// The first faction's share of the decided games, percent.
    fn share(self) -> Option<f64> {
        (self.decided() > 0).then(|| 100.0 * f64::from(self.won) / f64::from(self.decided()))
    }

    fn add(self, o: Tally) -> Tally {
        Tally {
            won: self.won + o.won,
            lost: self.lost + o.lost,
            open: self.open + o.open,
        }
    }

    /// The Wilson interval of the share at [`Z`], percent.
    fn interval(self) -> Option<(f64, f64)> {
        let n = f64::from(self.decided());
        let p = self.share()? / 100.0;
        let z2 = Z * Z;
        let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
        let half = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt() / (1.0 + z2 / n);
        Some((100.0 * (centre - half), 100.0 * (centre + half)))
    }
}

/// Which side of the band a share is on: -1 below, 1 above, 0 inside.
fn side_of(share: Option<f64>) -> i8 {
    match share {
        Some(s) if s < BAND.0 => -1,
        Some(s) if s > BAND.1 => 1,
        _ => 0,
    }
}

/// A pair fails when both seed sets put it outside the band on the same
/// side and the interval over both lies wholly outside it, so noise alone
/// seldom fails a pair that is really even.
fn pair_fails(sets: &[Tally]) -> bool {
    let all = sets.iter().fold(Tally::default(), |a, &t| a.add(t));
    let side = side_of(sets[0].share());
    let agree = side != 0 && sets.iter().all(|t| side_of(t.share()) == side);
    let sure = all
        .interval()
        .is_some_and(|(lo, hi)| hi < BAND.0 || lo > BAND.1);
    agree && sure
}

/// Judge one pair over its seed sets: a failure or a note for `where_`,
/// and the start of its table row (the pair, each set, both, undecided).
fn judge(
    where_: &str,
    label: &str,
    sets: &[Tally],
    failures: &mut Vec<String>,
    warnings: &mut Vec<String>,
) -> (Tally, String) {
    let all = sets.iter().fold(Tally::default(), |x, &t| x.add(t));
    let fails = pair_fails(sets);
    let score = format!("{} ({}-{})", pct(all.share()), all.won, all.lost);
    if fails {
        failures.push(format!(
            "{where_}: {label} {score} over both seed sets, outside {:.0} to {:.0}%",
            BAND.0, BAND.1
        ));
    } else if side_of(all.share()) != 0 {
        warnings.push(format!(
            "{where_}: {label} {score}, outside the band but within the noise"
        ));
    }
    let row = format!(
        "| {label}{} | {} | {} | {score} | {}",
        if fails { " **FAIL**" } else { "" },
        pct(sets[0].share()),
        pct(sets[1].share()),
        all.open
    );
    (all, row)
}

fn pct(s: Option<f64>) -> String {
    s.map_or("-".into(), |s| format!("{s:.0}%"))
}

fn clock(seconds: u32) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// What one faction did over its Conquest games.
#[derive(Default)]
struct FactionLine {
    games: u32,
    ready: Vec<u32>,
    first_kill: Vec<u32>,
    /// Grub and Marrow a minute by 5 and 10 minutes (sums, and games).
    income: [(u32, u32, u32); 2],
    kills: Vec<u32>,
    kills_other: u32,
    /// Worth of each unit in its army at the end of the games it won.
    winning_army: Vec<u32>,
    won: u32,
}

fn mean(v: &[u32]) -> Option<u32> {
    (!v.is_empty()).then(|| v.iter().sum::<u32>() / v.len() as u32)
}

/// The top `n` unit names by worth in `by`, as "Name 34%" of `total`
/// (at least everything in `by`).
fn top(by: &[u32], names: &[String], n: usize, total: u32) -> String {
    let total = total.max(by.iter().sum());
    if total == 0 {
        return "-".into();
    }
    let mut ranked: Vec<(usize, u32)> = by
        .iter()
        .copied()
        .enumerate()
        .filter(|&(_, w)| w > 0)
        .collect();
    ranked.sort_by_key(|&(u, w)| (std::cmp::Reverse(w), u));
    ranked
        .iter()
        .take(n)
        .map(|&(u, w)| {
            let share = (100 * u64::from(w) + u64::from(total) / 2) / u64::from(total);
            format!("{} {share}%", names[u])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[test]
#[ignore = "the balance tournament: run in release by the batch, by balance PRs and by hand"]
fn balance_tournament() {
    let started = std::time::Instant::now();
    let seeds = env_number("BALANCE_GAMES", 8);
    let battles = env_number("BALANCE_BATTLES", 32);
    let minutes = env_number("BALANCE_MINUTES", 40) as u32;
    let level = std::env::var("BALANCE_LEVEL")
        .ok()
        .and_then(|v| level_named(&v))
        .unwrap_or(Difficulty::Veteran);
    let maps = std::env::var("BALANCE_MAPS").unwrap_or_else(|_| "ribcage,grown".into());
    let chosen: Vec<&ConquestMap> = MAPS
        .iter()
        .filter(|m| maps.split(',').any(|k| k.trim() == m.key))
        .collect();
    let keys = maps.split(',').filter(|k| !k.trim().is_empty()).count();
    assert_eq!(chosen.len(), keys, "BALANCE_MAPS takes ribcage and grown");
    let maps = chosen;
    let field = std::env::var("BALANCE_FIELD").as_deref() != Ok("off");

    let data = crate::data::load();
    let names: Vec<String> = data
        .conquest_units
        .iter()
        .map(|u| u.name().to_string())
        .collect();
    let n = Faction::ALL.len();
    // `BALANCE_FACTION=Brood` plays only the pairs with that faction in
    // them, for tuning (the gate itself plays every pair).
    let only = std::env::var("BALANCE_FACTION").unwrap_or_default();
    let has = |f: usize| format!("{:?}", Faction::ALL[f]).eq_ignore_ascii_case(&only);
    let pairs: Vec<(usize, usize)> = (0..n)
        .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
        .filter(|&(a, b)| only.is_empty() || has(a) || has(b))
        .collect();
    let fname = |f: usize| format!("{:?}", Faction::ALL[f]);

    let mut failures = Vec::new();
    let mut warnings = Vec::new();
    let mut out = String::new();
    let mut lines: Vec<FactionLine> = (0..n).map(|_| FactionLine::default()).collect();
    for l in &mut lines {
        l.kills = vec![0; names.len()];
        l.winning_army = vec![0; names.len()];
    }

    out += &format!(
        "## Conquest, one-on-one at {}\n\n{} seeds a set, each from both starts, seed sets from {} and {}; \
         up to {minutes} minutes. Share is the first faction's wins of the decided games.\n",
        level.name(),
        seeds,
        CONQUEST_SETS[0],
        CONQUEST_SETS[1]
    );
    for map in &maps {
        out += &format!(
            "\n### {}\n\n| Pair | Set 1 | Set 2 | Both | Undecided | Decided games last | Worth killed |\n|---|---|---|---|---|---|---|\n",
            map.name
        );
        let mut sets: Vec<Vec<([usize; 2], u64, Played)>> = Vec::new();
        for from in CONQUEST_SETS {
            let s: Vec<u64> = (from..from + seeds).collect();
            sets.push(play_pairs(
                &pairs,
                &s,
                (map.choice, map.tiles),
                &[level],
                minutes,
            ));
        }
        for &(a, b) in &pairs {
            let mut tallies = Vec::new();
            let (mut length, mut ka, mut kb) = (Vec::new(), 0, 0);
            for set in &sets {
                let mut t = Tally::default();
                for (f, _, p) in set
                    .iter()
                    .filter(|(f, _, _)| (f[0], f[1]) == (a, b) || (f[0], f[1]) == (b, a))
                {
                    match p.winner() {
                        Some(w) if f[usize::from(w)] == a => t.won += 1,
                        Some(_) => t.lost += 1,
                        None => t.open += 1,
                    }
                    if let Some(v) = p.verdict {
                        length.push(v.tick / gc_sim::TICKS_PER_SECOND);
                    }
                    let i = usize::from(f[0] != a);
                    ka += p.sides[i].ledger.killed;
                    kb += p.sides[1 - i].ledger.killed;
                }
                tallies.push(t);
            }
            let label = format!("{} v {}", fname(a), fname(b));
            let (all, row) = judge(map.name, &label, &tallies, &mut failures, &mut warnings);
            let games = all.decided() + all.open;
            if all.open * 4 > games {
                warnings.push(format!(
                    "{}: {label}: {} of {games} games undecided after {minutes} minutes",
                    map.name, all.open
                ));
            }
            out += &format!(
                "{row} | {} | {ka} v {kb} |\n",
                mean(&length).map_or("-".into(), clock)
            );
        }
        for (f, _, p) in sets.iter().flatten() {
            for (i, side) in p.sides.iter().enumerate() {
                let l = &mut lines[f[i]];
                let l_ = &side.ledger;
                l.games += 1;
                let tick = gc_sim::TICKS_PER_SECOND;
                l.ready.extend(l_.army_ready.map(|t| t / tick));
                l.first_kill.extend(l_.first_kill.map(|t| t / tick));
                for (k, m) in [5, 10].into_iter().enumerate() {
                    let at = MARKS.iter().position(|&x| x == m).expect("a mark");
                    if l_.reached[at] {
                        let (g, mr) = l_.income[at];
                        let inc = &mut l.income[k];
                        *inc = (inc.0 + g / m, inc.1 + mr / m, inc.2 + 1);
                    }
                }
                for (u, w) in side.kills_by_unit.iter().enumerate() {
                    l.kills[u] += w;
                }
                l.kills_other += side.kills_other;
                if p.winner() == Some(i as gc_sim::PlayerId) {
                    l.won += 1;
                    for (u, w) in side.army_by_unit.iter().enumerate() {
                        l.winning_army[u] += w;
                    }
                }
            }
        }
    }

    // Timings and the first-army rules.
    if !maps.is_empty() {
        out += &format!(
            "\n## Each faction over its Conquest games\n\nFirst army: its fighters first worth {ARMY_READY} \
             (Grub + 2 Marrow of their cost). First attack: its fighters first kill an enemy fighter. \
             Kill shares are estimated: a loss is shared among the killer's fighters in melee near it or \
             in shooting range; \"other\" is towers, powers and fire.\n\n\
             | Faction | Games | Won | First army | Reached in | First attack | Grub/Marrow a min by 5 min | by 10 min | Most kills | Winning armies |\n\
             |---|---|---|---|---|---|---|---|---|---|\n"
        );
        let ready: Vec<Option<u32>> = lines.iter().map(|l| mean(&l.ready)).collect();
        for (f, l) in lines.iter().enumerate() {
            let rate = |k: usize| {
                let (g, m, c) = l.income[k];
                if c == 0 {
                    "-".into()
                } else {
                    format!("{}/{}", g / c, m / c)
                }
            };
            let other = l.kills_other;
            let total: u32 = l.kills.iter().sum::<u32>() + other;
            let mut kills = top(&l.kills, &names, 3, total);
            if total > 0 {
                let share = (100 * u64::from(other) + u64::from(total) / 2) / u64::from(total);
                kills += &format!(", other {share}%");
            }
            out += &format!(
                "| {} | {} | {} | {} | {} of {} | {} | {} | {} | {kills} | {} |\n",
                fname(f),
                l.games,
                l.won,
                ready[f].map_or("never".into(), clock),
                l.ready.len(),
                l.games,
                mean(&l.first_kill).map_or("-".into(), clock),
                rate(0),
                rate(1),
                top(&l.winning_army, &names, 3, 0)
            );
            if l.games > 0 && (l.ready.len() as u32) * 5 < l.games * 4 {
                warnings.push(format!(
                    "{}: first army ready in only {} of {} games",
                    fname(f),
                    l.ready.len(),
                    l.games
                ));
            }
        }
        let known: Vec<(usize, u32)> = ready
            .iter()
            .enumerate()
            .filter_map(|(f, r)| r.map(|r| (f, r)))
            .collect();
        for &(f, r) in &known {
            let others = known
                .iter()
                .filter(|&&(g, _)| g != f)
                .map(|&(_, x)| x)
                .min();
            if let Some(o) = others
                && r + RUSH_SECONDS <= o
            {
                failures.push(format!(
                    "{}: first army ready at {}, {} s before the next faction's ({})",
                    fname(f),
                    clock(r),
                    o - r,
                    clock(o)
                ));
            }
        }
        let first = known.iter().map(|&(_, r)| r).min();
        let last = known.iter().map(|&(_, r)| r).max();
        if let (Some(lo), Some(hi)) = (first, last)
            && hi - lo > EVEN_SECONDS
        {
            let slow = known
                .iter()
                .max_by_key(|&&(_, r)| r)
                .map(|&(f, _)| fname(f));
            warnings.push(format!(
                "first armies are {} s apart ({} to {}, slowest {}); the even early game aims for {EVEN_SECONDS} s",
                hi - lo,
                clock(lo),
                clock(hi),
                slow.unwrap_or_default()
            ));
        }
    }

    if field {
        out += &format!(
            "\n## Field Battle, the lobby's full rosters\n\n{battles} battles a pair a set, seed sets from {} and {}, \
             up to ten minutes.\n\n| Pair | Set 1 | Set 2 | Both | Undecided |\n|---|---|---|---|---|\n",
            FIELD_SETS[0], FIELD_SETS[1]
        );
        let mut fpairs = crate::field_balance::pairs();
        let named = |f: gc_ui::lobby::Faction| format!("{:?}", f).eq_ignore_ascii_case(&only);
        fpairs.retain(|&(a, b)| only.is_empty() || named(a) || named(b));
        let sets: Vec<Vec<(u64, u64)>> = FIELD_SETS
            .iter()
            .map(|&from| crate::field_balance::play_pairs(&fpairs, from, battles, false))
            .collect();
        for (k, &(a, b)) in fpairs.iter().enumerate() {
            let tallies: Vec<Tally> = sets
                .iter()
                .map(|s| {
                    let (won, lost) = s[k];
                    Tally {
                        won: won as u32,
                        lost: lost as u32,
                        open: (battles - won - lost) as u32,
                    }
                })
                .collect();
            let label = format!("{} v {}", a.name(), b.name());
            let (_, row) = judge(
                "Field Battle",
                &label,
                &tallies,
                &mut failures,
                &mut warnings,
            );
            out += &format!("{row} |\n");
        }
    }

    let mut head = format!(
        "# Balance tournament: {}\n\nA pair fails when both seed sets put it outside {:.0} to {:.0}% on the same side \
         and the 95% interval over both lies outside the band too; a faction fails when its first army is ready \
         {RUSH_SECONDS} s before every other faction's. Took {:.0?}.\n\n",
        if failures.is_empty() { "PASS" } else { "FAIL" },
        BAND.0,
        BAND.1,
        started.elapsed()
    );
    for f in &failures {
        head += &format!("- **Fails:** {f}\n");
    }
    for w in &warnings {
        head += &format!("- Note: {w}\n");
    }
    let report = format!("{head}\n{out}");
    println!("{report}");
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/balance");
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(dir.join("tournament.md"), &report);
    }
    let strict = GATE_ON || std::env::var("BALANCE_STRICT").as_deref() == Ok("1");
    if strict {
        assert!(
            failures.is_empty(),
            "the balance tournament failed:\n{}",
            failures.join("\n")
        );
    } else if !failures.is_empty() {
        println!(
            "(report only: the gate is off until main is inside the band; BALANCE_STRICT=1 fails it)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(won: u32, lost: u32) -> Tally {
        Tally { won, lost, open: 0 }
    }

    #[test]
    fn an_even_pair_passes_and_a_lopsided_one_fails() {
        assert!(!pair_fails(&[t(16, 16), t(15, 17)]));
        // Outside the band in both sets, but too few games to be sure.
        assert!(!pair_fails(&[t(7, 4), t(7, 4)]));
        // Sure, but the sets disagree.
        assert!(!pair_fails(&[t(30, 2), t(14, 18)]));
        assert!(pair_fails(&[t(4, 12), t(3, 13)]));
        assert!(pair_fails(&[t(26, 6), t(24, 8)]));
    }

    #[test]
    fn the_interval_holds_the_share() {
        let (lo, hi) = t(10, 10).interval().unwrap();
        assert!(lo < 50.0 && hi > 50.0 && (50.0 - lo - (hi - 50.0)).abs() < 1e-9);
        assert!(t(0, 0).interval().is_none());
    }
}
