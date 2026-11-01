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
| `gate` | 200 Ember soldiers (4 spear, 3 sword, 2 bow regiments and horse) march through one Gate in a Palisade across the map | all through and standing within 75 s (23.0 s; 27.0 s before columns grew forward), no soldier lost |
| `bridge` | a column of 6 regiments crosses a two-lane Timber Bridge over a deep river while 3 archer regiments shoot from the far bank | all across within 70 s (30.6 s; a routed regiment, running from the archers, no longer counts as part of the column), no regiment strays off the deck into the river |
| `queued-orders` | 4 Ember regiments get, in one tick, a march up to their Gate, a Shift-queued march through it and a Shift-queued attack on an enemy regiment beyond | the enemy beaten within 100 s, every regiment that fought did so through the Gate (3 of 4 fought), none lost |
| `forest-retreat` | 6 regiments fall back west through 16 tiles of woods broken by thicket, 6 foot regiments chasing from 24 tiles behind; the player re-sends any that stop | every regiment out, fallen or caught within 85 s, at least half get out (before regiments held until bloodied: 43 s, 6 of 6 out, 108 of 132 soldiers). Since regiments hold until bloodied (2026-10-30), one the pursuers catch fights it out instead of running at the first blows, so a caught regiment no longer has to get out in time: 18.2 s, 6 of 6 out, 104 of 132 soldiers |
| `ai-walls` | a General (Ember) on Ribcage Pass against an Ember base walled all round with a Palisade and a Gate | it hits the wall and gets inside within 10 minutes (5:11, 5:23) |
| `forest-pass` | 12 Ember regiments march 86 tiles east past a 40 by 56 tile forest packed with staggered thicket clumps, lying right across the straight way, with open fields north and south | all arrived and standing within 65 s (41.8 s), at least half the pace of a straight march on open ground (68%), under a tenth of regiment-ticks in the forest (0%: the army goes round), no soldier lost |
| `armies-crossing` | two armies of the same player, 6 regiments each, swap places head on along the same rows, 54 tiles apart | all arrived and standing within 30 s (19 s), at least 70% of a straight march's pace (94%), no soldier lost |
| `town-gaps` | 8 regiments march north through their own town of 76 Farmsteads built right across the map in 4 rows, streets and lanes 2 tiles wide, every other row shifted so no street runs straight through | all through and standing within 40 s (19.0 s, first through at 9 s; 69 regiment-seconds in disorder from crowding, 53 before columns grew forward), at least half a straight march's pace (94%), no soldier lost |
| `army-gate` | 48 Ember regiments (936 soldiers: spears, swords, bows, horse) eight abreast on a 128-tile map march through one Gate in a Palisade across the map | all through and standing within 240 s (62.0 s, 70.3 s before columns grew forward, 79 s before trailing; 26 regiment-seconds stuck before the gap, 296 before trailing; 1,372 in disorder from crowding, 1,568 before), no soldier lost |
| `army-bridge` | 16 regiments (352 soldiers) four abreast cross a two-lane Timber Bridge over a deep river, all ordered at once | all across and standing within 150 s (35.8 s, 41.3 s before columns grew forward, 73 s before trailing; 6 regiment-seconds stuck, 241 before trailing; 288 in disorder, 373 before), no soldier lost |
| `mountain` | 8 regiments ordered onto spots on a hill's steep south side (4 tiles high, sides falling over 8 tiles) | the hill has a steep side, every regiment stops, none stands on steep ground at the end |
| `bridge-flood` | a column of 6 regiments crosses a Timber Bridge 3 tiles wide over a flowing river (`Arena::flowing`) as a squall rolls a catastrophic flood down it, with logs torn off bank woods above; enemy archers first wear the bridge's first span over the water to 58% (under the strain line, over the sweep line); a second bridge 28 tiles upstream; the player re-orders a regiment the fall caught (on the span, or routed by it) if it halts short | a catastrophic flood comes; the weakened span is swept away while it runs (55.8, 58.8 and 59.8 s, row by row); every sound pier stands; a regiment is on the span when it goes and washes up out of the water (1); none walks off the deck into the river; every regiment still short of the gap goes round by the upstream bridge on its own, no order given again, within 10 s (1 regiment, 2.2 s); all across and standing within 150 s (94 s) |
| `bridge-flood-armies` | two armies of the same player, 11 regiments and over 200 soldiers a side, cross one Timber Bridge 3 tiles wide opposite ways after a catastrophic flood has shut the ford upstream | at least 200 a side (212); the flood has shut the ford; no regiment stuck (getting under a quarter tile nearer its goal, `March::stuck`, now reported by every march) more than 10 s (1.9 s); all arrived and standing within 180 s (41 s), at least 20% of a straight march's pace (45%), no soldier lost |

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
   `thicket` on it and the game's own map maker, gc_map, makes it, with
   the same banks, cover and blocking rules as a real map; `world()` gives a two-player Conquest world with every
   faction's units and buildings) or any gc_map map, as `ai-walls` does.
   `Real` looks units and buildings up by name; `spawn` and `put_up` set
   the field up (untimed), `move_to`, `attack_nearest` and `grid` give
   the usual orders and places, `of` and `soldiers` read regiments back,
   and `march` sends an army to its goals and says when every regiment
   stood there and how that compares with a straight march on open ground
   (`March::report` adds those metrics and the usual checks: there in
   time, a pace floor, no soldier lost).
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
