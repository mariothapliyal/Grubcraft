#!/usr/bin/env bash
# The Linux checks GitHub runs on a pull request, run here first, so a
# failure costs minutes instead of a 30 to 45 minute CI round trip.
#
#   scripts/pre-pr-check.sh           everything a PR's Linux jobs check
#   scripts/pre-pr-check.sh --quick   tests only the crates this branch changed
#   STRESS=gate scripts/pre-pr-check.sh   runs that stress scenario only
#                                         (with --quick, adds it to the run)
#
# Run it before opening a PR and before every push to one. It stops at the
# first failure and prints the command to run again. What it leaves out, CI
# still runs: the Mac and Windows test jobs, the app builds and smoke runs,
# and the oldest-Rust build. The slow runs (balance runs, match screenshots)
# run in batches, not per PR: see .github/workflows/batch.yml.
#
# It does not set RUSTFLAGS=-D warnings as CI does (that would rebuild
# everything each time you switch back to a normal build); clippy gets
# `-D warnings` instead, which fails on the same warnings.
set -euo pipefail
# Read whole before running, so editing this file mid-run is safe.
{
cd "$(dirname "$0")/.."

quick=false
case "${1:-}" in
  --quick) quick=true ;;
  "") ;;
  *) sed -n '2,10p' "$0"; exit 2 ;;
esac

start=$SECONDS
step() {
  printf '\n\033[1m== %s (%ds)\033[0m\n' "$1" $((SECONDS - start))
}
run() {
  echo "+ $*"
  if ! "$@"; then
    printf '\n\033[31mFAILED:\033[0m %s\n' "$*"
    exit 1
  fi
}

# Linux builds need the ALSA headers (sound). Fresh cloud containers lack them.
if [ "$(uname)" = Linux ] && ! pkg-config --exists alsa 2>/dev/null; then
  if [ "$(id -u)" = 0 ] && command -v apt-get >/dev/null; then
    step "Installing the ALSA headers"
    (apt-get update -qq && apt-get install -y -qq libasound2-dev pkg-config) >/dev/null
  else
    echo "Install the ALSA headers first: sudo apt-get install -y libasound2-dev pkg-config"
    exit 1
  fi
fi

# What this branch changed since it left main (committed or not).
git fetch -q origin main 2>/dev/null || echo "note: could not fetch origin/main, using the copy already here"
if ! git rev-parse -q --verify origin/main >/dev/null; then
  echo "No origin/main here: fetch it first (git fetch origin main)."
  exit 1
fi
base=$(git merge-base origin/main HEAD)
changed=$( { git diff --name-only "$base"; git ls-files --others --exclude-standard; } | sort -u)

step "Format"
run cargo fmt --all --check

step "Rules numbers ahead of main"
run python3 scripts/rules-bump.py --check origin/main HEAD

step "Clippy"
run cargo clippy --workspace --all-targets -- -D warnings
run cargo clippy -p gc_game --features profile -- -D warnings
run cargo clippy -p gc_game --all-targets --features dev -- -D warnings

if grep -qE '^(assets|assets-src|tools/gc_artbake|tools/gc_scenebake|tools/gc_sfx|crates/gc_assets|crates/gc_audio)/' <<<"$changed"; then
  step "Packs match their manifests"
  run cargo run -q --release -p gc_artbake -- --check
  run cargo run -q --release -p gc_scenebake -- --check
  run cargo run -q --release -p gc_artbake -- --detail --check
  for config in assets-src/buildings/*.ron; do
    faction=$(basename "$config" .ron)
    run cargo run -q --release -p gc_artbake -- --config "$config" --out "assets/buildings/$faction.pak" --check
  done
  run cargo run -q --release -p gc_sfx -- --check
else
  step "Packs: skipped (no art, sound or bake tool changes)"
fi

if $quick; then
  crates=$(grep -oE '^(crates|tools)/[^/]+/' <<<"$changed" | cut -d/ -f2 | sort -u \
    | while read -r c; do [ -f "crates/$c/Cargo.toml" ] || [ -f "tools/$c/Cargo.toml" ] && echo "$c"; done || true)
  if [ -n "$crates" ]; then
    step "Tests of the changed crates: $(echo $crates)"
    run cargo test $(printf -- '-p %s ' $crates)
  else
    step "Tests: no crate changed"
  fi
else
  step "Tests (debug, the whole workspace; includes the clearance check)"
  run cargo test --workspace
  step "Data reload tests (dev feature)"
  run cargo test -p gc_game --features dev
fi

step "Determinism in release mode"
run cargo test --release -p gc_sim --test determinism
step "Golden replays"
run cargo run -q --release -p gc_replay -- verify replays
step "Maps are valid and fair"
run cargo run -q --release -p gc_mapcheck -- check

if [ -n "${STRESS:-}" ] || ! $quick; then
  step "Stress scenarios${STRESS:+: $STRESS}"
  run cargo test --release -p gc_game stress_suite -- --ignored --nocapture
fi

if ! $quick; then
  step "Warlord battles play out the same every time"
  run cargo test --release -p gc_ai --test versus a_warlord_battle_plays_out_the_same_every_time
  step "Conquest soak"
  run cargo test --release -p gc_game conquest_soak -- --ignored --nocapture
  step "Conquest AI games in release mode"
  run cargo test --release -p gc_game ai_only -- --ignored
  step "Benchmark budgets (times only warn here: this machine is not CI's)"
  run cargo run -q --release -p gc_bench -- --warn-on-time
  run cargo run -q --release -p gc_bench -- --scenario conquest-6p --warn-on-time
  if command -v valgrind >/dev/null; then
    step "Benchmark instruction counts"
    run python3 scripts/bench-gate.py
  else
    step "Benchmark instruction counts: skipped (valgrind not installed; CI runs it)"
  fi
fi

printf '\n\033[32mAll pre-PR checks passed\033[0m in %dm%02ds%s.\n' \
  $(((SECONDS - start) / 60)) $(((SECONDS - start) % 60)) "$($quick && echo ' (quick: CI still tests every crate)')"
exit
}
