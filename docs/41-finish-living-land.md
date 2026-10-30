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
- **AI uses it.** Two computer players must finish one-on-ones on a Medium
  grown map (`ai_only_grown_land_1v1_ends_in_a_win`, run in CI with the
  other `ai_only` checks): they scout, expand, cross the land between the
  bases and fight it out.

## Step 1 as built

*Built 2026-10-30.* `gc_map::grown::describe_conquest` and the lobby's
**Land: Random, Plains, Mixed, Wild** (`gc_ui::lobby::CONQUEST_MAPS`, after
the Classic choices so old choices keep their numbers;
`DEFAULT_CONQUEST_MAP` is Land: Random). Network protocol 40.

- **The land.** Grown as the Field Battle's mirrored land, with one change
  (`Grown::heart`): the middle is flattened and kept open like a start's
  clearing. Field Battle maps are unchanged (their fingerprints hold).
  Four-start maps keep the two middle start clearings as open meadow.
- **Shares by walk.** Each north start's share is the Classic one, but each
  piece stands where the walk to it from its start is what it would be on
  open ground (within 1%), as near its open-ground place as the land
  allows. Where the land makes every start go round, all of them get the
  same longer walk (up to 30% longer), so the starts stay equal. A start's
  own seeps and vein stay inside its base area (24 tiles) and its natural
  expansion between 24 and 60 tiles.
- **Counter-routes.** Before the shares, every start's way to its nearest
  enemy is walled off in the map maker's own walk; the way round must be at
  most twice as long (the checker allows 2.2).
- **Checked again as shipped.** The sites' open ground and any knoll change
  the land a little, so the finished map is walked again: every start's
  walks to its share must still agree, and the counter-routes must hold.
- **Rerolls.** A seed whose land fails any of these grows another seed's
  land (up to 16; seeds far apart, so neighbouring lobby seeds don't share
  land); if all fail, the Classic map of the seed is made, so the game never
  stops on a bad seed. In a sweep of 240 maps (20 seeds of each wildness at
  320, 448 and 640 tiles) every map passed the checker's Conquest checks,
  none fell back to Classic, and about one in five was rerolled (a reroll
  grows the land again: about a second more at 640 tiles).
- **Lairs.** Each lair guards a rich field (a seep and a vein beside it,
  off the fords). A knoll is never raised in the Heart's clearing.
- **Dev flag.** `--conquest land-mixed` (any lobby map's name) starts a
  quick Conquest on it.
- **Checks.** `gc_mapcheck` test `every_grown_conquest_map_passes_the_checker`
  (`SWEEP=N` for more seeds), and the AI one-on-one above (first run: wins
  by Annihilation at 25:53 and 32:42, first contact at 7:22 and 6:34).

## Not in this plan

Water levels per tile, dams and debris (docs/39). Three- and five-player
layouts. A new art pass for the habitats.

## Step 4 as built

*2026-10-30.*

- **Changing by default.** The lobby's Weather starts on **Changing** in
  both modes (`WeatherSetting::default`). Fair, Rain, Snow and Fog stay as
  choices. Nothing in the simulation changed: the spells were already there,
  only the default was Fair.
- **Weather news.** `gc_game/src/sky_news.rs` reads `world.weather()` after
  each tick and says on the news line when rain, a storm, snow or fog comes
  in, and when it clears: "Fog is rolling in: troops see much less far.
  Watch for ambushes", "A storm is coming in: ...", "The fog has lifted".
  Weather comes in at strength 80 (the sim's spells are 90 to 255) and has
  cleared under 40; rain is a storm from 192 (where the picture's lightning
  starts) and rain again under 152. The give either way means the sim's
  slow fades and small wobbles are told once. The weather a match starts in
  isn't told, nor is a new match told the last one's weather.
- **Fire news.** Once a second, woods or scrub burning within 10 tiles of
  one of the viewer's buildings or regiments are told: "Fire! It is
  spreading near your base" or "... near your troops". Base and troops are
  told apart: each once as a fire comes near, then not again for 45
  seconds, so a blaze spreading tile by tile or flickering at the edge is
  told once, and a fire that spreads from the troops to the base is told
  again. The sim keeps its fires sorted by row, so only the rows in reach of
  each building or regiment are looked at.
- **Catching up.** While an online player catches up, the watcher keeps up
  but says nothing, so a rejoin doesn't replay old news.
- **Real use.** The alerts matter most in Conquest on grown land, where
  storms send flash floods down the rivers (already told) and fog hides
  ambushes in the woods. The computer players read the weather and fire
  directly from the sim, so they need no alerts.
- **Tests.** `sky_news::tests`: a fade in and out of rain through a storm is
  told once each way, a wobble about the thresholds says nothing, the
  opening weather is quiet, a new kind is told after the old fades, and a
  fire near troops then the base is told once each, with the quiet time,
  and a fire spreading from the troops to the base is told again.
