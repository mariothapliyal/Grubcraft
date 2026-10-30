# 41. Finish Living Land, then make the weather show

*Plan, 2026-10-30. Mario: "Plan and build, then choose whichever one you want first."*

## Why

A headless Conquest match on 2026-10-30 showed almost none of the land and
weather work from the last week:

- **Conquest never sees grown land.** It still plays the Classic maps
  (`wilds.rs`, straight lines and discs). They have no flow layer, so the
  rivers' currents, flash floods and the water engine (docs/39) can't happen
  in the main mode.
- **Fair placement was never made the default.** Mario chose Measured by
  default on 2026-10-22 ([27](27-living-land.md#fairness)). It is built
  only as an extra Field Battle choice, because with four to six players its
  spreads reached about 39% against the 3 to 16% the plan promised.
- **The biomes are thin.** The 19 habitats are on the map and the ground
  reads them ([36](36-habitat-ground.md)), but riverside features, worn
  trails and lairs guarding rich fields were never built.
- **Weather is off by default** (lobby Weather: Fair), so a normal match
  never rains.
- **Seasons can't be seen.** Only the top bar says the season.
- **Few alerts.** Floods and seasons are announced; a storm, fog or a fire
  near your troops is not.
- **The computer players never burn or clear** as a tactic. Lightning and
  burning buildings aren't built.

Water levels, dams and debris belong to the water engine (docs/39) and are
left out here.

## Order

Each step is one pull request, merged on green before the next starts.

| # | Step | What changes for the player |
|---|---|---|
| 1 | **Conquest on grown land (Mirrored)** | Conquest gets **Land: Random, Plains, Mixed, Wild**, the default. Rivers, fords, currents, flash floods, mud and the water engine all reach Conquest. Classic maps stay as **Classic: ...**. |
| 2 | **Fair placement for four to six players, then the default** | Measured meets its limits (walk to the nearest enemy within 10%, own fields' walks within 10%) on 4 and 6 starts by rerolling the start search, in Field Battles and Conquest. Then **Measured** becomes the default land in both lobbies, as Mario chose. |
| 3 | **Places inside each biome** | Lairs that guard rich fields on grown Conquest maps; riverside dressing (gravel bars on the inside of bends, reeds, beaches, alders along banks); worn trails between the starts, villages and fords. |
| 4 | **Weather on by default, with alerts** | Lobby Weather starts on **Changing**. The news line says when a storm, fog or snow comes in and clears, and when woods catch fire near your troops or base. |
| 5 | **Seasons you can see** | Snow lying on the ground in winter, autumn colour in the woods, dry yellow grass in summer, ice drawn on frozen water. Render only. |
| 6 | **Fire and lightning** | Storms throw lightning that can set dry woods alight. The computer players burn woods an enemy army stands in or hides in, and clear thicket that blocks their way. |

Then one playtest: full matches against the AI with all of it on, before and
after clips, and a list of what feels clunky.

## Step 1: Conquest on grown land

`grown::describe_conquest(tiles, seed, wildness)` makes a Conquest map on
grown land, as `conquest::describe_wilds` does on the Classic land, and
`gc_game::conquest::make_map` offers both.

- **Starts.** Four under 512 tiles (the corners), six from 512, the same
  slots and player order as the Classic Conquest maps (north west to east,
  then their turned copies). Only the starts in play get clearings.
- **The Heart.** The middle is kept a flat open clearing, as each start is.
- **Shares.** Each north start gets the Classic Conquest share (its base's
  seeps and vein, the natural expansion, a hamlet, a Shrine, a Bloodseep
  site, a frontier slot and three fields with creeps). The grown land isn't
  flat, so each piece goes to the nearest spot with room to its planned
  place (standable, no thicket, no water). The loader turns the north half
  for the south.
- **Sites.** Villages, camps, the old fort, lairs and standing stones by
  reason, as on Field Battle grown maps; each lair guards a rich field.
- **What doesn't change.** Field Battle grown maps stay byte for byte the
  same (their pinned fingerprints hold). Classic Conquest maps are
  untouched.
- **Checks.** `gc_mapcheck` sweeps grown Conquest maps at every size with
  the Conquest checks (twins fair, every start reaches the Heart and each
  other, every share in reach).

## Real use

How armies use the grown Conquest land, and how it works with the rest of
the game:

- **Leaving the base.** A start must open onto at least two ways out (the
  passes carved in [27](27-living-land.md) stage 2 help), so a 200-soldier
  army in column isn't queued behind a single gap. The map sweep counts
  them.
- **Rivers.** A river between two players is crossed at fords (wide,
  wadeable) or at a bridge the player builds. In spring melt and storms,
  fords shut and armies must go round or bridge. A river is now a front
  line that moves with the weather, which the computer players read
  (`gc_ai::terrain::Water`).
- **Woods, scrub and thicket.** Woods hide ambushes and burn; thicket walls
  are short knots (never longer than 11 tiles), so armies go round, or
  workers clear them. A base grows by clearing the scrub round it.
- **Fighting places.** Heights and fords near the middle are listed as set
  pieces the computer players fight over.
- **The Heart.** Its clearing is flat and open, so a Titan's Heart fight is
  a field battle, not a scramble through woods.
- **Stress check.** A new `grown-conquest` scenario plays two Generals on
  Land: Mixed at 448 tiles and checks they meet and fight (armies cross the
  land between the bases) within a time limit.
- **AI uses it.** The Conquest AI soak runs on grown land; a computer player
  must finish 1v1 matches there as on the Classic maps.

## Not in this plan

Water levels per tile, dams and debris (docs/39). Three- and five-player
layouts. A new art pass for the habitats.
