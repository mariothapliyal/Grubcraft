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

## Step 2 as built

*2026-10-31.* Field Battles start on **Measured: Random**.

- **Why it fell short.** The old second search walked a coarse grid
  that guessed the land's cost from its slopes. On the finished land, as
  the nav rules walk it, 4- to 6-player maps at 512 tiles came out 7 to
  35% unequal (worst of every way the lobby hands starts out).
- **Settling on the true walks** (`grown::starts::settle`). One full
  walk from each start, over the finished land as `gc_mapcheck` walks it,
  gives every tile's walk to that start. So any tile within 24 tiles
  (scaled to the map) can be tried for a start at once, scored on every
  way the lobby hands starts out, with a small price for each tile it
  moves. A start that moves is walked from again. The clearings are then
  flattened and the ways cut again, which changes the walks a little, so
  it is measured again, and a second, shorter round of 8 tiles runs when
  the walks are still over 5% apart. A round that makes things worse is
  thrown away.
- **Reroll.** A map still over 10% is grown once more from another seed
  (`seed ^ k * golden ratio`), and the fairer of the two is kept.
- **The default.** A new Field Battle lobby starts on **Measured:
  Random**. Land (mirrored), Classic and Random stay choices. Conquest
  stays mirrored: its shares are placed round the north starts and turned
  for the south, so mirrored land is what keeps them equal.
- **PROTOCOL 42**, because the same Measured map choice now makes other
  land, and machines must agree on it.
- **Measured** (true walks, 5 seeds of each wildness at each size, worst
  over every handout, before → after): see the table below.

| Size | Before | After | Build time (each) |
|---|---|---|---|
| 320 | — | 0.4 to 9.0% | 0.6 to 2.0 s |
| 512 | 7 to 35% | 0.7 to 6.7% | 1.8 to 4.8 s (with a reroll) |
| 768 | — | 0.4 to 6.1%, one Wild map 13.8% | 3.7 to 5.5 s, about 10 s with a reroll |

  The one miss (768, Wild) was still 13.8% after its reroll: the reroll
  keeps the fairer of two maps, it does not promise 10%.

  A 768 Measured map took about 4 s before this change on the same
  machine (mirrored 1.8 s). The 3-second target for 768 is still missed;
  most of the time is the full walks over 590,000 tiles.
- **Tests.** `measured_starts_are_within_the_limit` grows 12 Measured maps
  (every wildness, 256 and 320 tiles) and holds each within 10%.
  `every_grown_map_passes_the_checker` still passes with no extra rerolls.
  The probe (`fairness_probe::probe`, ignored) prints the table.
- **Real use.** Four- to six-player Field Battles (2v2, 3v3, free for
  all, three teams of two) were the unfair case, because a mirrored map
  makes twins equal but not corners and middles. Each start's walk to its
  nearest enemy is now within a tenth of the others', so no team gets to
  the fight a minute sooner. Two-player maps were already equal.

## Step 3 as built

*2026-10-31.* Grown maps get riverside dressing and worn trails. Lairs
guarding rich fields came with step 1.

- **Gravel bars** (`Habitat::GravelBar`). Where the water wraps round more
  than half of a bank tile right by a river (a ring of 32 points, 4 tiles
  out), that is the inside of a bend. The shingle spreads over the low,
  dry bank up to 2 tiles round it. Open ground, drawn with the gravel
  pattern.
- **Lake shores.** On gentle shores within 3 tiles of a lake: sand
  (`Beach`, the ripple pattern) where the sun falls, and reeds
  (`ReedBed`) in patches where it doesn't. Reeds are tall grass, so a
  regiment can hide in them ([20](20-ambushes.md)), and they are drawn
  with dense sedge.
- **Bankside alders.** Open meadow on a river or lake shore gets the odd
  alder (scenery only).
- **Worn trails** (`grown::trails`). Once the sites stand, the first start
  is the network; each other start and each site in turn (villages,
  camps, the old fort, the standing stones, the Heart; not lairs) wears
  the cheapest way to the nearest worn tile. Worn ground costs a third
  of fresh ground, so trails join into a network. A slow wobble in the
  cost (every 9 tiles) makes them wander. They keep off steep ground,
  cross rivers at the fords, and go round woods and bog where they can.
  Each trail is 2 tiles wide. In the open it shows as `Habitat::Trail`
  (a worn brown, colour only so its edges blend) and its tall grass is
  trodden flat; under trees, on bog and in water nothing changes. A trail
  never squeezes diagonally between two tiles that can't be walked. On
  mirrored land only one half's ends wear trails, plus one from the turned
  first start to join the halves, and every worn tile's turned copy is
  worn too, so the trails are their own turned copy.
- **Bare ground stays bare.** Trails get no scenery at all; gravel bars
  and beaches get a few pebbles instead of the land's grass, bushes and
  lone trees. A trail or bar inside a river valley keeps the valley's
  land (its scenery set and sounds).
- **PROTOCOL 43**: the same map choice now makes slightly different land.

### Real use

- **Trails are the roads.** They run from each base to its neighbours,
  the villages and the camps, and they cross rivers where the fords are.
  A player can read where the enemy will come from, and where to wait
  for them. Trails are open ground, so an army marching on one is in the
  open.
- **Reeds hide.** A lake shore with reeds is tall grass: a place for an
  ambush beside the water.
- **Gravel and sand are open.** The bends of a river and the sunny lake
  shores are open ground to form up on before a crossing.
- **Fairness.** Trails change a few tiles of tall grass to open ground,
  which walking costs don't read, so fairness can't change. `gc_mapcheck`
  runs on the map with its trails and still passes; the Conquest share
  check (`still_fair`) skips wearing them to save time.

### Checks

- `trails_reach_the_starts_and_sites`, `trails_lie_on_open_dry_ground`,
  `mirrored_trails_are_their_own_turned_copy`; the habitat tests cover
  gravel bars; `every_grown_map_passes_the_checker` still passes.
- In-game screenshots are in project files under
  `screenshots/living-land-step3/`: trails leaving a Conquest base, and
  gravel bars and reeds on a river pool. Beaches are only shown in the
  map-data picture there. They are small and I did not get one in a
  screenshot.

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

## Step 5 as built

*2026-10-31.* The season shows on the land in a Conquest match with
seasons. Render only: nothing in the simulation changed.

- **Summer** (`Look::dry`): the grass turns to straw, in drifts so a
  field is never one colour, with the land's and habitat's own shades
  still under it. Grass clumps and bushes go the same way.
- **Autumn** (`Look::autumn`): broadleaf trees turn gold, orange or red,
  each tree its own colour (picked from where it stands), and a few turn
  late and stay green. Conifers keep their needles. The grass browns a
  little and bushes go brown.
- **Winter**: a thin snow lies whatever the weather (`WINTER_SNOW`, 0.35
  of a full cover: patches on flat ground, a dusting on trees and roofs).
  It is the least the lying snow melts to: it settles in as winter comes,
  real snowfall builds on it as before, and it melts away in the spring. The grass is dead and pale.
  Frozen water (`Look::ice`, from the simulation's frozen water, the same
  state that lets armies cross the shallows) is drawn as a pale, cloudy,
  cracked sheet of ice that hides the ripples and the bed, with snow
  lying on it in the same patches as on the bank. Only the shallows and
  fords freeze, as in the simulation where only they can be crossed:
  deep water stays open inside an icy rim. The ice comes and goes over
  about 2 seconds (`ICE_SETTLE`), and a match opened or joined in winter
  starts frozen and frosted.
- **Crossing over.** Each season is full through its middle and crosses
  to the next over a fifth of a season either side of its edge
  (`atmosphere::Year`), so nothing changes at a stroke: at Short seasons
  the woods take about three minutes to turn.
- **Where it lives.** `SkyView` carries where the year stands and whether
  the water is frozen; `Atmosphere::look` sets the three new `Look`
  values. They reach the shaders in free slots of `Globals` (`haze.w`,
  `air.w`, `sky_top.w`). The tuning panel has knobs for each.
- **`--season spring|summer|autumn|winter`** draws the land in that
  season whatever the match's year (the picture only), to check the look
  without waiting for it.
- **Real use.** In a Conquest match with seasons, a player sees the
  season on the field, not only in the top bar: frozen rivers that can be
  crossed look frozen, the dry summer when fire spreads looks dry, and
  winter's snow is there without a snowstorm.
- **Tests.** `each_season_shows_through_its_middle_and_crosses_over_at_its_edges`
  and `the_season_is_drawn_and_the_ice_comes_in_over_seconds`; the shader
  validation test covers the shaders.

## Not in this plan

Water levels per tile, dams and debris (docs/39). Three- and five-player
layouts. A new art pass for the habitats.
