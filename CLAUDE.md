# Grubcraft: notes for contributors and Claude

## Before a pull request

- Run `scripts/pre-pr-check.sh` before opening a pull request and before every
  push to one. It runs the Linux checks GitHub CI runs on a PR: formatting,
  rules numbers, clippy, packs (when art or the bake tools changed), tests
  (including the clearance check), golden replays, the map check, benchmark
  budgets and instruction counts. A full run takes about 30 minutes from a
  cold build on 4 cores, much less with a warm build.
- While iterating, use `scripts/pre-pr-check.sh --quick`, which tests only the
  crates the branch changed. Run the full script before the final push.
- Fix failures locally. Do not push to see whether CI finds them.
- CI still runs what the script leaves out: the Mac and Windows test jobs, the
  app builds and smoke runs, and the oldest Rust version in `Cargo.toml`.
- Golden replays in `replays/` must stay green. A change that alters how
  battles play out re-blesses them in the same PR, as `replays/README.md`
  describes.
- To run one stress scenario: `STRESS=name scripts/pre-pr-check.sh` (for
  example `STRESS=gate`); CI runs the whole suite on every PR.

## Batched checks

- The slow runs are not per PR: the balance runs (Warlord contest, Field
  Battle and roster tables, Conquest balance), the full map sweeps (`SWEEP=1`;
  a PR checks a quarter of the grown and lobby maps) and real-match
  screenshots run
  in `.github/workflows/batch.yml`, every six hours when main has moved, and
  by hand from the Actions tab. The stress scenarios and the Conquest soak
  take seconds, so they stay on every PR.
- Their reports are the `balance-report` and `match-screenshots` artifacts.
  A failure there is fixed before the next batch merges.
