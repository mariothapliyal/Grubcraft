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
every bridge, measured at the narrowest width a player can draw (3 tiles, `gc_sim::drawn::BRIDGE_WIDTHS`); drawn bridges are built from the timber kit's pieces (`bridge_kit`).

Problems already found are listed in `KNOWN` with what fixes them. A new
problem fails the check; so does a listed one that has been fixed, so the
list only shrinks. It is empty today (2026-10-31): all three problems found when the check was built are fixed.

| Passage | Problem | Fix |
|---|---|---|
| ~~All six gate and gatehouse models~~ | ~~the door leaves were modelled shut across the doorway: 0 tiles open~~ | fixed 2026-10-30: doorways 2.84 to 2.93 open, doors are separate swinging leaf models ([26](26-fortifications.md), "How gates are drawn") |
| ~~Timber Bridge model (every faction's bridge shows it)~~ | ~~its rails leave 1.4 of the 2 tiles open at chest height~~ | fixed 2026-10-31: drawn bridges are 3 to 5 tiles wide and built from the timber kit, rails on the outer edges (#334, #362) |
| ~~Every bridge~~ | ~~the Broodmother Bulk (2.7 tiles) is wider than the 2-tile deck~~ | fixed 2026-10-30: the narrowest bridge is 3 tiles (#334) |

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
artifact). The whole suite takes a few seconds.

| Scenario | What it plays | Checks (last measured) |
|---|---|---|
| `gate` | 200 Ember soldiers (4 spear, 3 sword, 2 bow regiments and horse) march through one Gate in a Palisade across the map | all through and standing within 75 s (49 s), no soldier lost |
| `bridge` | a column of 6 regiments crosses a two-lane Timber Bridge over a deep river while 3 archer regiments shoot from the far bank | all across within 70 s (44 s; a routed regiment, running from the archers, no longer counts as part of the column), no regiment strays off the deck into the river |
| `queued-orders` | 4 Ember regiments get, in one tick, a march up to their Gate, a Shift-queued march through it and a Shift-queued attack on an enemy regiment beyond | the enemy beaten within 100 s (26 s), every regiment that fought did so through the Gate (3 of 4 fought), none lost |
| `forest-retreat` | 6 regiments fall back west through 16 tiles of woods broken by thicket, 6 foot regiments chasing from 24 tiles behind; the player re-sends any that stop | every regiment out, fallen or caught within 85 s, at least half get out (before regiments held until bloodied: 43 s, 6 of 6 out, 108 of 132 soldiers). Since regiments hold until bloodied (2026-10-30), one the pursuers catch fights it out instead of running at the first blows, so a caught regiment no longer has to get out in time: 20 s, 5 of 6 out, 104 soldiers |
| `ai-walls` | a General (Ember) on Ribcage Pass against an Ember base walled all round with a Palisade and a Gate | it hits the wall and gets inside within 10 minutes (5:11, 5:23) |
| `forest-pass` | 12 Ember regiments march 86 tiles east past a 40 by 56 tile forest packed with staggered thicket clumps, lying right across the straight way, with open fields north and south | all arrived and standing within 65 s (43 s), at least half the pace of a straight march on open ground (66%), under a tenth of regiment-ticks in the forest (0%: the army goes round), no soldier lost |
| `armies-crossing` | two armies of the same player, 6 regiments each, swap places head on along the same rows, 54 tiles apart | all arrived and standing within 30 s (19 s), at least 70% of a straight march's pace (94%), no soldier lost |
| `town-gaps` | 8 regiments march north through their own town of 76 Farmsteads built right across the map in 4 rows, streets and lanes 2 tiles wide, every other row shifted so no street runs straight through | all through and standing within 40 s (26 s, first through at 14 s), at least half a straight march's pace (70%), no soldier lost |
| `bridge-flood` | a column of 6 regiments crosses a Timber Bridge 3 tiles wide over a flowing river (`Arena::flowing`) as a squall rolls a catastrophic flood down it, with logs torn off bank woods above; enemy archers first wear the bridge's first span over the water to 58% (under the strain line, over the sweep line); a second bridge 28 tiles upstream; the player re-orders any regiment that halts short (washed up, or routed and rallied) | a catastrophic flood comes; the weakened span is swept away while it runs (55.9, 58.9 and 59.9 s, row by row); every sound pier stands; a regiment is on the span when it goes and washes up out of the water (2); none walks off the deck into the river; every regiment still short of the gap is on its way round by the upstream bridge within 10 s (4.8 s); all across and standing within 150 s (93 s) |
| `bridge-flood-armies` | two armies of the same player, 11 regiments and over 200 soldiers a side, cross one Timber Bridge 3 tiles wide opposite ways after a catastrophic flood has shut the ford upstream | at least 200 a side (212); the flood has shut the ford; no regiment stuck (under a quarter tile of headway) more than 15 s (10.6 s, at the bridgehead; docs/39's mark is 10 s); all arrived and standing within 180 s (59 s), at least 20% of a straight march's pace (31%), no soldier lost |
