# 40. Real-play checks

Mario's rule (2026-10-30): looking right in Blender is not enough; what
counts is how things work in play. Past misses were gates units couldn't
walk through and bridges too narrow to cross properly. Two cheap checks run
on every pull request, and failures block the merge.

## 1. Clearance check

`crates/gc_game/src/clearance.rs`, run by `cargo test --workspace`.

Every passage troops walk through or over is measured twice: as the
simulation walks it (its footprint) and as the player sees it (its finished
model, measured from the baked triangles at chest height, 0.1 to 1.2 tiles
above its floor: the threshold or deck under the middle of the way through). Each is held against:

- **the widest walking unit**: the widest model across (flyers left out),
  plus 0.1 tiles each side. Today the Broodmother Bulk, 2.7 tiles;
- **a column of line troops two abreast**: twice the widest file spacing of
  any foot regiment of nine or more soldiers. Today the Mantid Reapers,
  2.0 tiles;
- and the model must leave open what the footprint lets through.

Passages today: every gate and gatehouse (footprint: its long side) and
every bridge (two lanes, 2 tiles; the model is one lane, mirrored).

Problems already found are listed in `KNOWN` with what fixes them. A new
problem fails the check; so does a listed one that has been fixed, so the
list only shrinks. Found when the check was built:

| Passage | Problem | Fix |
|---|---|---|
| ~~All six gate and gatehouse models~~ | ~~the door leaves were modelled shut across the doorway: 0 tiles open~~ | fixed 2026-10-30: doorways 2.84 to 2.93 open, doors are separate swinging leaf models ([26](26-fortifications.md), "How gates are drawn") |
| Timber Bridge model (every faction's bridge shows it) | its rails leave 1.4 of the 2 tiles open at chest height | rails further out |
| Every bridge | the Broodmother Bulk (2.7 tiles) is wider than the 2-tile deck | wider bridges, or a rule for giants |

A new kind of passage (a dam's walkway) adds itself in `passages()`.

## 2. Stress scenarios

`crates/gc_game/src/stress/`, run in CI on Linux in release:

```text
cargo test --release -p gc_game stress_suite -- --ignored --nocapture
STRESS=gate cargo test --release -p gc_game stress_suite -- --ignored --nocapture
```

Each scenario plays armies of the real game data through a hard place,
checks what must hold, and reports what it measured. The runner times every
tick (p50, p99, max) and the whole scenario. The report is printed and
saved to `target/stress/report.txt` (CI keeps it as the `stress-report`
artifact). The whole suite takes about two seconds.

| Scenario | What it plays | Checks (first run) |
|---|---|---|
| `gate` | 200 Ember soldiers (4 spear, 3 sword, 2 bow regiments and horse) march through one Gate in a Palisade across the map | all through and standing within 75 s (52 s), no soldier lost |
| `bridge` | a column of 6 regiments crosses a two-lane Timber Bridge over a deep river while 3 archer regiments shoot from the far bank | all across within 70 s (49 s; a routed regiment, running from the archers, no longer counts as part of the column), no regiment strays off the deck into the river |
| `queued-orders` | 4 Ember regiments get, in one tick, a march up to their Gate, a Shift-queued march through it and a Shift-queued attack on an enemy regiment beyond | the enemy beaten within 100 s (26 s), every regiment that fought did so through the Gate (3 of 4 fought), none lost |
| `forest-retreat` | 6 regiments fall back west through 16 tiles of woods broken by thicket, 6 foot regiments chasing from 24 tiles behind; the player re-sends any that stop | every regiment out, fallen or caught within 85 s (55 s), at least half get out (6 of 6, 95 of 132 soldiers). Since regiments hold until bloodied (2026-10-30), one the pursuers catch fights it out instead of running at the first blows, so a caught regiment no longer has to get out in time: 20 s, 5 of 6 out, 85 soldiers |
| `ai-walls` | a General (Ember) on Ribcage Pass against an Ember base walled all round with a Palisade and a Gate | it hits the wall and gets inside within 10 minutes (5:42, 5:55) |

Checks are on what the simulation decides (ticks, positions, losses), the
same on every machine, so they don't flake. Time is held only to each
scenario's generous budget, which catches a slowdown of several times.
A change that moves a scenario's numbers on purpose (better pathfinding, new
morale) updates its limit in the same pull request and says why.

### Adding a scenario

1. Write `crates/gc_game/src/stress/<name>.rs` with a
   `pub(super) const SCENARIO: Scenario` (name, one line about it, time
   budget, and `run(&Real, &mut Report)`; the game data is loaded once for
   the suite) and list it in `SCENARIOS` in `stress/mod.rs`.
2. Build the world with `arena::Arena` (a flat map: paint `river`, `woods`,
   `thicket`; `world()` gives a two-player Conquest world with every
   faction's units and buildings) or any gc_map map, as `ai-walls` does.
   `Real` looks units and buildings up by name; `spawn` and `put_up` set
   the field up (untimed), `move_to`, `attack_nearest` and `grid` give
   the usual orders and places, `of` and `soldiers` read regiments back.
3. Play only through `report.tick` so the time is measured.
4. `report.metric` what is worth seeing; `report.check` what must hold.
   Set limits about one and a half times what it measures today.

Every new feature units interact with (gates, bridges, dams, walls,
terrain, buildings, water) adds its scenario, and one where a computer
player uses it (crosses it, opens it, clears it, repairs it).

## 3. Batched playtests

Not code: when a plan stage or a batch of threads has merged, one playtest
thread plays everything together in the real game against the AI, records
short before/after clips of each new feature and reports what is clunky.
See the project instructions.
