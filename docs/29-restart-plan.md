# 29 · Restart plan

*Plan, 2026-10-23; progress updated 2026-10-25 08:30 UTC. Mario asked for an in-depth plan to restart development after the pause, at high quality and token efficient. Wave 1 started at 01:00 NZDT on Saturday 24 October (2026-10-23 12:00 UTC). About 120 pull requests (#184 to #304) were built in the two days after; see [Progress](#progress). Work pauses for the week from 2026-10-25.* The illustrated version is the "Grubcraft Restart Plan" artifact (https://claude.ai/artifact/Qw4kBb6YPYZeq6ckdjJytB).

## Progress

*As of 2026-10-25 08:30 UTC, counting two pieces still finishing as done: hidden traps (#304) and the computer players' half of fortifications stage 8 (walls, bridges, stakes, traps and crossing ice; plan in `design/fortifications/stage8-plan.md`). The waves below are the original plan; threads ran ahead of them wherever nothing blocked them, so waves 1 to 5 are built except for the items under Left.*

| # | Thread | Built (pull requests) | Left |
|---|---|---|---|
| T1 | Pathfinding overhaul | Overhaul #193; attacks on buildings you can see #200 | Nothing. Nav doors (#203) and the reopen hook (#199) were built by T6 and T9 |
| T2 | Living Land maps | Stage 1 grown land #189; stage 2 ways through (passes, fords, walkable mountains) #292; stage 3 Measured maps #293; stage 4 sites by reason and five habitat scenery models #295 (habitat ground by T8, #221) | Stage 5, water that runs downhill, is a later call for Mario |
| T3 | Cinematic lighting | Stage 1 #188; particles #201; clean edges and motes #209; looks per land and F10 panel #217; camera craft #222 #228 (director camera, replay films) #233; weather and night look #235 #239 #245 #247 #250 #254 #255 #257 #261 #262 #265; F10 knobs #268 | Tuning on Mario's Mac with F10 |
| T4 | Every soldier fights | Scrum #187; duels #191 #192 #195; charges that plough (idea 11) #212; commander on the field (idea 12) #226; matched kills #243 #263; tumbling #266; finishers #270 #280; motion warping and champion kills #272; head turns #275; general as hero #277 | Soldiers drawn on their feet with duel spacing kept (2026-10-30); option C is a later call |
| T5 | Materials, animation engine, fight library | Surface detail #198; engine stages 1 to 4 #213 #224 #229; fight library #238 #241 #260 #267 #274 #279; kills and finishers allow for where models are drawn #282; tipping models #285; blows, reels and kills for the Brood, riders and other rigs #291 #299 #301 | The rest of the fight library ([33](33-animation-engine.md) stage 5): more blows and kills per weapon, kanabo and sword-and-shield moves, kills for the other factions' weapons |
| T6 | Fortifications and bases | Towers #190; turning #196; gates #203; drawn walls #215 #230 #236; ring tool and blueprints #244; wall levels #251; manning #264; ladders #273; faction wall twists #278; gatehouses #290; bridges #302; stakes and barricades #303; hidden traps #304; computer players fortify, assault walls and cross ice (stage 8, AI side) | Stage 5, the eight new towers (Bolt and Blast for each faction) and tower levels; stage 8's walled-town bonus and town life |
| T7 | Battle effects and aftermath | Sieges on particles #207; sparks and wounds #211; standards, crows and grime #219; Blood setting #286; toppling walls and towers #294; rubble as slow ground #296; trophies #298 | Prisoners, a later call (idea 14 needs a design) |
| T8 | Living ground | Ground textures #194; scenery detail #197 #202; Brood veins (19) #205; faction lands (15) #208; ground marks (2) #216; habitat ground #221; stumps #225; scorched ground #232; mud #237; scenery re-bake and Conquest map marks #289 | Nothing |
| T9 | Weather and changing land | Clearing land (22) #199 #206 #210; reading the ground (13) #227; weather, mud and night rules #242 #249 #252 #258; woods that burn #271; fords that flood (6) #283; seasons in Conquest (7) #288 | Small later stages in [32](32-changing-land.md): lightning as a game event that can set woods alight, burning buildings lighting the ground, Highland snow |
| T10 | Battle soundscape | Positional sound and beds #184; blows, weather and habitat sounds #287 (thunder came with T3, #262) | Tuning by ear on Mario's Mac |
| T11 | Ambient life and busy bases | Clearing on screen #214 #234; living touches #218; busy bases #220; walkers #231; log piles #240; deer #246; doves #248; houses in Grubfolk villages #253; villagers #256; crow model #259; caravans #269; insects #276; fish rises #281; crows on standards, wheel dust, carts on slopes #300 | Polish from [34](34-ambient-life.md)'s "not yet" lists: supply carts in bases, forge sparks, other lands' fauna, crows on corpses, ruined hamlets |

**Ideas 1 to 22:** all are built: 1 weather and time of day, 2 a world that remembers, 3 wildlife and villagers, 4 sound, 5 director camera and replays, 6 usable rivers (floods and bridges), 7 seasons, 8 wear, trophies and torn banners, 9 night battles, 10 busy bases, 11 physical formations, 12 commander on the field, 13 terrain tactics, 14 aftermath (prisoners are a later call), 15 faction lands, 16 smarter formations, 17 pathfinding, 18 better guides, 19 Brood veins, 20 option B, 21 less-flat textures and 22 clearing land.

Outside the eleven threads: the Hand over button to give your army to an ally (#186). CI and docs housekeeping: #185 #204 #223 #284.

**What remains, in one list:**
1. Fortifications stage 5: the eight new towers (Bolt and Blast for each faction) and tower levels ([26](26-fortifications.md)).
2. Fortifications stage 8, the town half: the walled-town bonus and town life.
3. ~~The building re-bake with the stronger bake~~ (done 2026-10-27 at Mario's word, see [31](31-surface-detail.md#built-so-far)).
4. Soldiers drawn where they stand, with the duel spacing kept (done by T4, 2026-10-30, at draw time rather than in the bake); the rest of the fight library (more kills per weapon, kanabo, sword and shield, the other factions' weapons).
5. Smaller extras: lightning as a game event, burning buildings lighting the ground, Highland snow, and ambient polish (supply carts, forge sparks, more fauna).
6. Tuning on Mario's Mac: lighting with F10, and sound by ear.
7. Mario's later calls: option C (every soldier a body in the simulation), Living Land stage 5, prisoners after battles, faction depth stages 7 to 9.
8. Dynamic bridges, first wave at the restart: drawn curved paths, a choice of width, upgrades to braced and reinforced piers, and models generated from a timber kit; walls, fences and palisades drawn on the same curves; once the water engine (idea 25) is built, debris and floods strike, jam and break bridges ([in brief](#dynamic-bridges-in-brief)).
9. Holding until bloodied, first wave at the restart: regiments can't rout or waver until they are down to a breaking point below half strength (about 35 to 45%, lower for veterans, varied a little per regiment); shock works by killing men ([in brief](#holding-until-bloodied-in-brief)).
10. The world remembers the new features, first wave at the restart: every bridge, curved wall, dam, jam and flood scar the restart adds stays on the land for the rest of a Conquest match, so later battles are fought on ground the earlier ones shaped ([in brief](#the-world-remembers-the-new-features)).
11. Eight changes to how battles play, approved 2026-10-30: unit spacing, frontage, a balance tournament and an even early game in the first wave; charges by weight and speed, responsive orders and clear feedback in the second; smarter computer generals in the third ([in brief](#how-battles-play-eight-changes-in-brief)).
12. Faction rebalance, first wave at the restart: fair economies (the Choir earns about three and a half times what the Clans do), the Clans gathering 20% faster, and the Brood as a true swarm of cheap, weak units ([in brief](#faction-rebalance-in-brief)).

## What it covers

- **Living Land**, organic world generation ([27](27-living-land.md)), stages 1 to 4, with stage 5 as a later call. It absorbs natural mountains stage 2.
- **Cinematic lighting and effects** (the "Grubcraft Lighting Study" page), stages 1 to 5 and the seven small wins Mario approved on 2026-10-22.
- **Every soldier fights** ([28](28-soldier-combat.md)): option B first, option C later.
- **Fortifications and bases** ([26](26-fortifications.md)), all eight stages.
- **Ideas 1 to 19** from 2026-10-22. Ideas 16 (smarter formations, PR #178) and 18 (better guides, PR #177) are built. Idea 20 is soldier combat.
- **Idea 21, less-flat textures** (Mario, 2026-10-23, at minimal cost): (1) a small shared set of repeating detail textures (stone, wood, plaster, cloth, tiles) layered by material over every building and unit; (2) real grass and soil textures with normal maps blended across the ground, from the Grass001 and Ground054 sets in the project files (built 2026-10-23, see [10](10-architecture.md)); (3) lighting stage 1, already planned; (4) a stronger bake with crevice darkening, colour varied from block to block and stronger normals, then every building and unit re-baked; (5) the same surface detail on scenery (Mario, 2026-10-23): trees, bushes, grass, rocks, stumps and the other scenery models get bark, leaf, stone and moss detail textures with surface relief, rocks share the Rock035 cliff textures so they match the mountains, tree and rock bases blend into the ground, and the whole scenery pack is re-baked with the stronger bake. (Built 2026-10-23, render only: see [25](25-scenery-models.md#surface-detail); the scenery pack was re-baked with the stronger bake's colour variation on 2026-10-24.)
- **Idea 22, clearing land to expand** (Mario, 2026-10-23): trees and scrub stand in the way of a growing base, with the full mechanics, animations and planning. See [Idea 22 in brief](#idea-22-in-brief).

- **Idea 23, living waterways** (Mario, 2026-10-25): rivers that vary with the seasons, currents and flash floods. Planned in [38](38-living-waterways.md), to be built later.
- **Dynamic bridges** (Mario, 2026-10-30, for the restart): drawn timber bridges with gentle curves and a choice of width, generated from a kit of pieces. See [Dynamic bridges in brief](#dynamic-bridges-in-brief).
- **Holding until bloodied** (Mario, 2026-10-30, for the restart): a morale redesign so regiments hold until they have lost most of their men. See [Holding until bloodied in brief](#holding-until-bloodied-in-brief).
- **The world remembers the new features** (Mario, 2026-10-30): the "living campaign map" idea folded into idea 2, a world that remembers. See [The world remembers the new features](#the-world-remembers-the-new-features).
- **How battles play: eight changes** (Mario, 2026-10-30, for the restart): unit spacing, frontage, charges by weight and speed with braced spears, an even early game, an automatic balance tournament, responsive orders, clear feedback and smarter computer generals. See [How battles play](#how-battles-play-eight-changes-in-brief).
- **Faction rebalance** (Mario, 2026-10-30, for the restart): fair resources for every faction, the Clans gathering faster, and Brood units cheap and weak one by one but many. See [Faction rebalance in brief](#faction-rebalance-in-brief).

Not in it: faction depth stages 7 to 9 (Tithe, Brood metamorphosis, heroes). They fit in any wave from 2 on without clashing with this plan if Mario wants them.

## Idea 22 in brief

Mario (2026-10-23): "Make it so that tree/scrub etc clearance is a thing for expanding bases", planned and built in full, with animations. The plan, with first-pass numbers that T9's design note settles:

- **Ground has to be cleared to build on.** Buildings can't go on scrub, woods or thicket. Workers get a **Clear** order (drag an area): scrub goes quickly, woods slower, thicket slowest. Laying a building on overgrown ground queues the clearing first, so a blueprint just takes longer.
- **Cleared ground changes the battlefield.** It becomes open ground with stumps: nobody hides there any more, troops no longer slow down in it, and cleared thicket stops blocking the way. Felled timber pays a little Grub.
- **Each faction clears its own way**, to confirm with Mario in the design note. For example, the Clans fell and haul timber, the Concord burns it (fast, leaving scorched ground), the Choir shatters it with resonance, and Brood Gnawers eat it.
- **Maps and computer players.** Living Land grows woods and scrub round every start, with equal open and clearable room for each player (T2's fairness work). Computer players clear ground as they expand.
- **Settled (2026-10-24, T9's design note, [32](32-changing-land.md)):** scrub 3 s, woods 8 s, thicket 15 s a tile for one worker; woods pay 3 Grub, thicket 5. Shared rules built first; Mario chose all four faction twists as proposed, built next.
- **On screen.** Workers chop with their own moves, trees shake and fall, logs pile up, and stumps and bare earth stay behind. The moves and models are Sonnet 3D work.
- **How it works.** One runtime ground-change command updates a tile's cover and, for thicket, the walking grid through T1's reopen hook. It goes through the lockstep like every other order, and burning woods, flooded fords and siege rubble reuse it later.

## Dynamic bridges in brief

Mario (2026-10-30), for the restart: bridges must be **truly dynamic**. Wood only for now, but drawn almost freely, with gentle curves, a little variation in width, and models more sophisticated than today's straight line of one-tile timber tiles (#302, #318, and two lanes in #319). Built from **a kit of pieces put together by a generator**, the way the grown terrain is: nothing complex, but honest generation.

**What the player does**
- **Draw the path.** Drag from bank to bank; the bridge follows the drag as a smooth curve (a few control points, smoothed). Bends are gentle, about 15 degrees per two tiles at most, so it reads like a real timber bridge and never zig-zags. The preview shows the curved deck, its cost and any tile it can't use before you confirm.
- **Pick a width.** Narrow, standard or wide (3, 4 and 5 tiles across), standard by default. *Built 2026-10-30: the first plan said about 1.5, 2 and 3 tiles, but the clearance check (#328) found the Brood's Broodmother Bulk is 2.7 tiles wide, wider than every bridge then. So the narrowest bridge now clears the widest walking unit, and the price per tile was halved (3 Grub a tile) to keep spans affordable.* The deck also varies a little on its own (a few percent wider at the piers, narrower mid-span), seeded so every player sees the same bridge.
- **Upgrade it (Mario, 2026-10-30).** A finished bridge can be upgraded, like walls (the same raise order, U key), a span at a time. Level 2, **braced piers**: cross-bracing and doubled pilings, more hit points against blows and debris. Level 3, **reinforced piers**: stone-footed piers with pointed cutwaters facing upstream, so debris glances off and jams less, and the bridge stands through all but catastrophic floods. Each level costs Grub and build time by the tile, and the generator draws the new pieces onto the same bridge (bracing, then stone footings and cutwaters), so you can see which bridges are strong. *Built 2026-10-30: each faction's bridge has two levels (Ember: Braced Timber Bridge, Stone-Footed Bridge; Choir: Braced and Anchored Glass Span; Brood: Ribbed and Rooted Chitin Span; Concord: Braced Bone and Iron-Shod Causeway); level 2 is 3 Grub and 3 seconds a tile for 600 hit points, level 3 is 4 Grub and 1 Marrow and 4 seconds a tile for 1000. The whole bridge is raised at once and stays open while it goes up. `Structures::pier_level` gives 1 to 3 for the water engine. Computer players raise their own bridges when rich.*
- **Everything else stays as now:** workers build it out from the bank, it costs Grub by the tile covered, and tiles burn and fall one by one, dropping whoever stands on them into the river.

**How it works (simulation)**
- The order carries the path (control points in fixed-point) and the width, so it goes through the lockstep like any order and replays stay exact.
- The sim turns the curve into **deck tiles**: every tile the deck covers, joined edge to edge (no corner-only steps) so soldiers can always walk it. Each tile is still a bridge building slot, so building, fire, falling and the walking grid (`Nav::set_bridged_all`) work as today. This replaces #319's fixed second lane with width from the drawn path.
- Rules: both ends on land, every tile over water or shore, a length cap, and the bend limit. Computer players keep drawing straight paths (a curve with no bend) and may bend round rocks later.
- Bumps SIM_VERSION, FORMAT and PROTOCOL; goldens re-blessed.

**How it looks (generation from a kit)**
- **The kit** (Sonnet 5.5 alone, 3D, one timber material with the stronger bake): bank abutments (log cribs that meet the ground), trestle piers in three heights, stringer beams, plank modules in a few lengths and wear levels, rail posts, rails, rope, lantern posts, team cloth, and burnt and broken versions of the deck and rails. *Built 2026-10-31: 23 pieces (three deck patterns plus broken and burnt, stringer, rail post, timber and rope rails, lantern post, log-crib abutment, piers at three levels and three heights, stone cutwaters), laid tile by tile by `gc_game::bridge_kit`: piers reach the river bed, the look follows the bridge level (see docs/26, Look).*
- **The generator** walks the curve: a pier every few metres, its height from the water depth under it (tall in the channel, short in shallows, none on the banks); planks laid across the direction of travel with small seeded changes in length, angle and colour; rails and rope following the curve along both outer edges; a slight rise over the middle of long spans; abutments sunk into the banks at the ground height. Seeded from the bridge's id, so it is the same on every machine.
- Pieces appear in order as tiles finish, and burn or fall with the tile they sit on.

**Who and when**
- Owner: T6 (fortifications), Opus lead for the sim, the order and the drawing tool. Sonnet 5.5 alone for the kit models; a Sonnet helper for the generator and its drawing code; Haiku for docs, guides and the quick reference. Size M to L, three PRs: (1) curved path and width in the sim, with the drawing tool and preview; (2) the kit and the generator; (3) upgrades (braced and reinforced piers) and polish: wear, broken pieces, lanterns lit at night, computer players bending round rocks.
- Starts in the first wave after the restart, once #319 is settled. Runs alongside the dedicated pathfinding thread: deck tiles are ordinary walkable tiles, so it does not touch route search. Dams (idea 24) reuse the kit and piers.

**Walls, fences and palisades drawn the same way (Mario, 2026-10-30)**
Today a wall is drawn as straight lines from click to click (any angle, chained at corners). At the restart, walls get the same drawing as bridges, so palisades, fences and walls follow the land:
- **One shared drawn path.** The curve, the bend limit, the smoothing and the turning of the curve into tiles are one piece of sim code that bridges, walls and blueprints all use (bridges join their tiles edge to edge so soldiers can walk them; walls keep today's corner-joined steps, which already seal the way). The order carries control points instead of two end tiles. Each tile stays its own building, so breaches, wall levels, manning, ladders, gates and fire work as today. Walls get the bend limit too, but a click still makes a sharp corner, so square forts stay easy.
- **The tool.** Drag to draw a smooth curve, click for a corner, Shift to keep straight lines and 15 degree steps as now. The ring tool closes into a smooth loop when dragged, and blueprints are drawn the same way. Ends still snap to your walls, gates, towers and bridges.
- **Gates on a curve** sit on the tile under them and turn to the curve's direction there; the generator straightens the wall for a tile on each side so the gate looks right.
- **Generated looks.** Like the bridge, a wall's look is laid along the curve from a kit, seeded per wall. First the Ember Palisade (logs of slightly different heights, thicknesses and lean, sharpened tops, bracing and a walkway behind on higher levels) and the Concord Bone Fence; other walls keep today's models bent to follow the curve until their kits come.
- Computer players keep drawing straight walls and may follow ridges and rivers later. This is part of the same T6 work: the shared path lands in the first bridge PR, wall drawing in the second, wall kits with the third.

**Bridges and the water engine (Mario, 2026-10-30)**
Once the dynamic water engine (idea 25: seasonal flow and volume, minor to catastrophic flash floods) and its debris are built, bridges must react to them. This is a fourth bridge PR, built with or right after idea 25, owned by whichever thread builds the water engine, with T6 reviewing:
- **Debris hits the piers.** Logs from trees felled or burnt upstream, rubble and loads an enemy dumps in ride the current and strike the piers in their path. Each hit damages the bridge tiles over that pier, scaled by the debris size and the current's speed.
- **Debris jams.** Pieces caught on the piers pile up into a jam. A jam narrows the channel, so water backs up and rises upstream, and pushes harder on the bridge the bigger it gets. Workers can clear a jam from the deck, and a jam can be set alight.
- **Bridges fail under it.** When the damage is enough, the tiles over the pier fall: soldiers on them drop into the river as today, and the broken pieces join the debris heading downstream. That can take out the next bridge down, so one dump upstream can cascade.
- **Floods and high water.** Deep, fast water strains the piers, and a catastrophic flood can sweep away a weak or damaged span outright. Tall piers in the channel stand up best, which gives the width and pier layout a reason to matter.
- **Upgrades pay off here.** Braced piers take less damage from each hit; reinforced piers with cutwaters shed most debris, rarely jam and survive all but catastrophic floods. Upgrading the bridges on a river an enemy can dump debris into is the counter.
- **Readable and fair.** Strain and jams show on the bridge (creaking, logs piling up, water foaming at the piers), and computer players clear jams, repair their own bridges and use dumped debris against yours.
- All of it runs in the sim from seeded, fixed-point state, so it stays deterministic in multiplayer and replays.
- Other materials (Concord bone, Choir glass, Brood chitin) are kit variants for later.

## Faction rebalance in brief

Mario (2026-10-30): resources aren't fair. The Choir (Shard) piles them up fast while the Ember Clans struggle; the Brood are "absurdly strong". Brood units should stay cheap, with lots of attacks, like a swarm of ants, but each one should be weak; today they are almost a match for other factions' units, and they arrive wave after wave before an Ember player has built much. He suggests about 20% faster Ember gathering.

**Measured (2026-10-30, before any change).** Headless Conquest, computer against computer at Veteran on Ribcage Pass: every pair of factions, each on both starts, 8 seeds, two seed sets (1 to 8 and 101 to 108), 192 games. The tool is a new `conquest_balance` test that prints wins, income, army and losses (kept as a patch until the build; see below). Both seed sets agree:

| Pair | Wins (both sets, 32 games) | Worth of fighters killed (set 2) |
|---|---|---|
| Clans v Brood | 7 to 25 | 39k v 39k |
| Clans v Choir | 0 to 32 | 14k v 15k |
| Clans v Concord | 1 to 31 | 35k v 34k |
| Brood v Choir | 0 to 32 | 9k v 16k |
| Brood v Concord | 2 to 30 | 26k v 33k |
| Choir v Concord | 31 to 0 (1 undecided) | 24k v 35k |

| Faction | Grub/Marrow in by 5 min | by 10 min | Army worth at 10 min | Soldiers at 10 min | Worth killed per worth lost |
|---|---|---|---|---|---|
| Ember Clans | 1,130 / 150 | 2,160 / 330 | 350 | 57 | 1.01 |
| Brood | 810 / 240 | 2,020 / 460 | 400 | 119 | 0.84 |
| Choir | 3,880 / 385 | 9,830 / 1,050 | 3,210 | 234 | 0.92 |
| Carrion Concord | 1,310 / 195 | 3,020 / 390 | 740 | 100 | 1.21 |

(Worth is Grub plus twice the Marrow a fighter cost; set 2 shown, set 1 within a few percent.) What this says:

- **The Choir's economy is the big break.** It brings in about 780 Grub a minute by minute 5, three and a half times the Clans (about 225). A tuned seep gives 200 Grub a minute with no workers and no supply; the Choir starts with two Resonators already tuning; one Resonator tunes every seep within 4 tiles, not one; a new Resonator costs 150 Grub and pays for itself in under a minute; chords add up to 60% more. The Clans pay 250 Grub, 5 supply and a minute of training for the same 200 a minute from five Hands. The Choir wins every game, by about 11 minutes, without killing more per fight: it simply has far more.
- **The Clans are the weakest economy and the weakest side** (8 wins in 96 games). The Clans AI also leaves Marrow unspent early (200+ banked at 6 minutes) because nothing at tier 1 costs Marrow, so Hands on the vein are wasted then.
- **The Brood is strong only against the Clans** (25 of 32) and loses to the Choir and the Concord, but its numbers explain what Mario sees: about twice the soldiers of an equal army, hatching several at once from larvae while other factions train one at a time. Skitterlings cost 100 Grub for 32 soldiers with the highest tier 1 attack in the game (50) and 192 hit points a regiment, as much as an Ember spear regiment costing 120. One Brood soldier is as good as others', and there are more of them.
- **The Concord** wins most of its fights (1.21 worth killed per worth lost) on an ordinary income, so it will lead once the Choir is fixed. Watch it.

**The plan (first-pass numbers, settled by measurement in the build).**

1. **Choir economy, brought into line.** A Resonator tunes only its nearest seep (a Siphon its nearest vein); a tuned seep gives about 130 Grub a minute instead of 200 (a load every 4.5 s, not 3 s) and a tuned vein about 40 Marrow instead of 50; chords add at most 30%, not 60%; a Resonator costs 200 Grub, not 150; the Choir starts with the Spire and one Resonator, not two. A Mobile Resonator tunes the same way. The Choir keeps its identity: no workers to protect, but every pylon is a target and expanding costs a building per seep.
2. **Clans gather 20% faster (Mario's ask).** Hands fill a Grub load in 7.5 s instead of 9, and Marrow in 10 s instead of 12. Done as data: a gathering speed per unit in the roster (`gather_percent: 120` on the Hand, 100 by default), so other factions can be tuned the same way later. The Clans AI keeps one Hand on Marrow, not more, until a tier 2 upgrade or a Marrow unit is in sight.
3. **Brood as a real swarm.** Cheaper, more of them, more attacks, each one weak:
   - Skitterlings: 100 to 70 Grub, supply 3 to 2, 32 to 36 soldiers, 6 to 4 hit points each, attack 50 to 36, defence 28 to 20, damage 3 to 2, train 18 to 13 s.
   - Chitin Guard: 130 to 100 Grub, 16 to 12 hit points each, attack 30 to 24, defence 55 to 45.
   - Tier 2 evolutions (Spitters, Burrowers, Mantid Reapers, Shellbacks) and Drone-Moths: about 25% fewer hit points and 15% less attack per soldier, and about 15% cheaper. Monsters and heroes stay as they are.
   - Swarm Instinct stays (30% less morale lost to casualties), so swarms keep coming, but they melt under archers, cavalry charges and spear lines.
   - Larvae: no change to start with. If the first Brood wave still hits Clans players too early in play (see the real-use check), the Hive's first larvae come every 16 s for the first 5 minutes instead of 12.
   - Rule of thumb for the tuning: per Grub spent a Brood army has 1.5 to 2 times the soldiers of anyone else's, but loses one-on-one per soldier, and even per Grub it ends level (worth killed per worth lost between 0.9 and 1.1 against each faction).
4. **Then the Concord.** Re-measure after 1 to 3. If the Concord leads, trim its combat edge first (undead Bone Pikes or the Ledger's tithe), not its economy.
5. **Field Battles too.** Field Battle points come from costs (3 a Grub, 6 a Marrow), so cheaper Brood units mean bigger Brood armies there. Re-run `faction_table` on both seed sets and retune points or stats until every pair is 40 to 60%.

**Targets (how we know it's done).**
- Conquest, Veteran and General, both seed sets, both starts: every faction wins 40 to 60% of its games, every pair 35 to 65% (single pairs swing with the AI).
- Income: by minute 5 and minute 10 every faction is within 15% of the others, with the Clans allowed at the top after their 20%.
- Brood: 1.5 to 2 times the soldiers of an equal-worth army; worth killed per worth lost 0.9 to 1.1 against each faction.
- Mario's case, as a real-play scenario: a Clans side against a Brood Veteran. By the first Brood attack the Clans can have at least two fighting regiments and their tier 1 production up, and hold that wave at their base with a small loss. The scenario is added to the stress suite with its numbers.
- Field Battles: `faction_table`, every pair 40 to 60%.

**Real use.** Nothing here changes how units walk, fit through gates or cross bridges, except that bigger Brood swarms mean more regiments in traffic: the stress suite's 200 units through a gate and the column over a bridge run with a Brood swarm (twelve regiments of Skitterlings) as well. The Choir AI places one Resonator per seep and the Clans AI moves Hands off Marrow (AI-uses-it). The batched playtest after the first wave plays a full match for each faction against each, records a Brood wave against a Clans base, and Opus judges whether it feels like a swarm.

**Fits with the eight changes to how battles play (Mario, 2026-10-30).** That plan gives a "rebalance thread" the balance tournament and the even early game; this is the same thread, and this rebalance is its first job. The `conquest_balance` test below is the Conquest half of the tournament ([How battles play](#how-battles-play-eight-changes-in-brief), item 3). The Brood early-wave target joins the even early game. Frontage (only soldiers touching the enemy strike) also weakens swarms against a held line, so the Brood stat cut must not stack blindly on it: the economy PR (1 and 2) goes first; the Brood PR (3) is measured with frontage if it has merged, and cuts less if frontage already brings the Brood into the band.

**Who and when.** A new thread, "Faction rebalance", Opus lead at high effort, first wave after the restart. The sim rules (tuning reach and rate, gather speed per unit) are the lead's; a Sonnet helper does the AI changes (Choir placement, Clans Marrow), Haiku helpers run the balance tables and update the guides, quick reference, docs/04, docs/17 and the rosters. One PR for the economy (1 and 2), one for the Brood (3), then the Concord and Field Battle passes if needed. Sim changes bump SIM_VERSION, re-bless the goldens and go through /simplify and /code-review on Opus. The `conquest_balance` test lands in the first PR (its patch is in the project files, `balance/conquest_balance_test.patch`): `AI_GAMES=8 AI_FROM=1 cargo test --release -p gc_game conquest_balance -- --ignored --nocapture`, then `AI_FROM=101`; about two minutes a set on 4 cores. It touches the economy code, the Conquest AI and the data, not route search or structures, so it runs alongside the bridge and pathfinding threads.
**Progress.** PR 1 (economies, 2026-10-30): the Choir tunes one seep a Resonator (about 109 Grub a minute, chords up to +30%, one starting Resonator at 175 Grub), Clans Hands fill loads in two thirds of the time (+22% income measured), the Concord starts with 300 Grub with caravans at 37 plus 1.1 a tile and Bone Pikes at 110, and the `conquest_balance` tournament test is in. After, over 192 games: Ember Clans 71%, Choir 54%, Concord 39%, Brood 36% (before: Clans 8%, Choir 100%). Income by minute 10: Clans 3,260, Choir 3,160, Concord 2,710, Brood 2,090 (the Choir was 9,830). Out of the 40 to 60% band: Clans over Brood 26-6 and the Clans overall, because their first army is ready at about 3:40 against 4:30 to 5:30 for the others. Next: the even early game (first armies within about 30 seconds of each other), then the Brood cut measured on top of frontage (#335).

PR 2 (even early game, 2026-10-30): the Clans start with 250 Grub and the Concord with 350 (were 300). First armies (450 Grub's worth): Clans about 3:50 (was 3:40), Concord 4:10 (was 4:45), Choir 4:30; the Brood's first wave stays at about 5:25 until the swarm cut. Wins over 192 games: Clans 62%, Choir 55%, Concord 49%, Brood 34%. Start stores move the AI a lot (Choir at 325 or the Brood at 350 broke other pairs), so the Brood's timing is left to its own pass with frontage.

PR 3 (Brood swarm, 2026-10-31, on its own branch so it waits on neither #340 nor frontage #335): Skitterlings 70 Grub, 2 supply, 13 s, 32 × 5 HP, 45 / 22 / 3, morale 60 (Field Battle points 280, so 14 regiments to others' 10); Chitin Guard 100 Grub, 12 × 12 HP, 24 / 45; tier 2 evolutions and Drone-Moths about a quarter fewer hit points, 15% less attack, 15% cheaper; Gnawers gather 15% faster (`gather_percent: 115`). The plan's first numbers lost every Field Battle, so the cut is softer. Conquest on the holding-until-bloodied morale rules (96 games a Brood pair, seeds 1 to 8 and 101 to 108): the Brood has about 2.3 times the Clans' soldiers by minute 10, trades 0.97 to 0.99 worth killed per worth lost (target 0.9 to 1.1), and wins 8 of 31 decided games against the Clans, 7 of 23 against the Choir and 7 of 31 against the Concord (5, 3 and 5 without the cut). Its first army is ready at about 6:15 (was 5:25). Field Battles: Clans v Brood 33.5% (was 41.5%), Brood v Choir 22.5% (26%), Brood v Concord 47% (41%). Left: the new morale rules put the Clans far ahead in Conquest (about three wins in four), the Brood against the Choir in Field Battles (item 5), and the Clans-v-Brood first-wave scenario for the stress suite. A six-player Veteran game on seed 1 runs past two hours with no winner (the six-player test plays seed 2), more input for smarter AI generals. So did a Clans v Brood one-on-one on a grown Medium map (seed 7) on the maps before Living Land step 3: both sides at the supply cap fought over the Heart for the whole late game, the Clans sure of winning but never pushing on to the Brood base while the cheap swarm refilled its losses; on today's maps that game ends. A General winning at the supply cap should leave the Heart and go for the base. When frontage merges, re-run both tables, since it weakens swarms against a held line.

## Holding until bloodied in brief

Mario (2026-10-30), for the restart: routing is far too easy. Regiments waver and break at around 80% strength, and that is absurd. A regiment should hold, even after heavy shock like a cavalry charge, until it has lost a lot of men: it breaks somewhere below half strength (30 to 50%), later for seasoned and veteran troops, with some variation. Shock should work mostly by **killing men**: a charge cuts a regiment down fast, so it reaches its breaking point sooner, but it does not break a regiment that still has most of its men.

**Why it happens today (`gc_sim/src/morale.rs`)**
- Morale is a pool of points (the unit's stat, 50 to 80) that routs at zero, whatever the strength. Most of the drain is not casualties but steady pressure: 4.2 points a second hit in the rear, 2.1 hit in the flank, 2.1 pinned, 1.4 or 2.8 outnumbered two or three to one, plus one-off knocks (a friend routing nearby 8, an ambush 20, the general falling 10, a flash flood 25). Casualties cost only 0.9 points per percent of the regiment lost.
- So a few seconds of flank or rear pressure, or a nearby rout, empties a regiment that has barely lost anyone, and it wavers below 25 points no matter how many men it has.
- Measured on 2026-10-30 (Ember stats, flat ground, no generals, `gc_sim` probe): Yari Ashigaru in an even frontal fight wavered at 68% and broke at 46%; against two Yari regiments they wavered at **82%** and broke at **64%**; held in front with Mounted Samurai in the rear they wavered at 79% and broke at 46% after only 9 s; Katana Samurai (morale 80) caught front and rear wavered at **88%** and broke at **69%**. In real battles the knocks stack (nearby routs, the general falling, ambushes), which is how regiments end up breaking near 80%.

**The new rule: nerve and a breaking point**
- Each regiment keeps its morale points, renamed **nerve** in the docs, drained and restored by the same things as today. What changes is what nerve can do: **a regiment cannot rout until it is down to its breaking point**, a share of its full strength. Above it, nerve can run out, but the regiment holds (steady, maybe grim), and it routs the moment it reaches its breaking point with no nerve left. A regiment that still has nerve at its breaking point fights on below it until its nerve runs out. So pressure decides *how soon after* the breaking point a regiment goes, and casualties decide *when it can*.
- **Breaking points** by grade, from the unit's morale stat (first pass, tuned with the faction table):
  - Levy and light troops (morale under 55, workers aside): 45%.
  - Line troops (55 to 69): 40%.
  - Elite troops (70 and up, such as Katana and Mounted Samurai): 35%.
  - **Veterancy** (docs/05, three ranks from kills): each rank takes 5 points off, so a rank 3 elite regiment can only break at 20%. **Built 2026-10-31** ([05 · Veterancy](05-combat.md#veterancy)): kills weighed by hit points against the regiment's own (rank 1 at its worth, 2 at twice, 3 at four times; volley kills count half), +10% damage per rank as docs/05 says (its +10 morale is the lower breaking point), gold chevrons over the banner and on the card.
  - **Variation:** each regiment rolls up to 5 points either way when it is raised, seeded from the match seed and its handle, so the same regiment breaks at the same point on every machine and in replays. Its general near it takes 5 points off while he lives.
  - A hard cap: **no regiment ever routs above 50% strength.** Concord undead still never rout, and Steadfast and summoned regiments still hold at the edge.
- **Wavering** follows the breaking point too: a regiment wavers (15% less damage, the flickering banner) only when its nerve is low **and** it is within 10 points of its breaking point. A regiment at 80% is never seen wavering.
- **Below the breaking point,** casualties cost twice the nerve they do now, so a badly cut-down regiment does not fight to the last man unless it is winning.
- **Rallying, shattering and rout chains** work as today. A rallied regiment is by definition under its breaking point, so it breaks again when its rallied nerve runs out. A friend routing nearby still costs nerve, but can only start a chain among regiments already down to their breaking points, so one rout no longer topples a healthy line.
- **Shock works by killing men.** Charges keep their weight and ploughing (docs/05), flank and rear blows still land 1.4 and 1.8 times as hard, and pinned regiments still fall into disorder and take more damage, so a cavalry charge in the rear still wrecks a regiment fast. The flank, rear, pinned and outnumbered drains stay, so a hard-pressed regiment arrives at its breaking point with no nerve left and goes at once, while a fresh one fighting a fair fight can hold well below it.

**Real use (what changes in play)**
- **Cavalry charge on spears, rear:** the spears lose men quickly to the 1.8 rear blows and the plough, and break when they reach about 40% (35 to 45 with the roll), not at 80% after a few seconds of shock. A charge that only grazes them leaves them fighting.
- **Line against line:** both lines grind down to around 30 to 40% before one breaks. Fights last longer and cost more men, so the battle pace (`BATTLE_PACE_PERCENT` 70, docs/05) may need raising back to keep fights near 105 to 140 s; measured, not guessed.
- **Flanking still wins:** a flanked regiment loses men faster and breaks sooner after its breaking point, and the regiments beside it are no longer knocked over by its rout unless they are bloodied too. Envelopment becomes about killing, which is what Mario asked for.
- **Ambushes, floods, a fallen general:** they still cost nerve and men, but they only break regiments that are already down to their breaking points. The ambush's main bite becomes its volley and charge damage.
- **AI:** the computer players read wavering and morale points today (`gc_ai` captain roles, helpers and the battle estimator). They must learn that strength is now the main signal: pull back a regiment near its breaking point, not one at 80%, and the estimator must stop expecting early routs. Their AI-only battles are rerun before merging.
- **Readable:** the regiment card shows strength with a small tick at the breaking point, and the banner flickers only when it is close; the guide and quick reference explain the rule in one line.

**Checks (the project's real-play rules)**
- A new headless stress test in `gc_sim/tests/morale.rs` with real roster stats prints the strength at which each side breaks in the same cases as above, plus a nearby rout and a lone cavalry charge, and fails if any regiment routs above 50% or a fresh line wavers above 60%.
- The faction table (both seed sets), battle length, the golden replays and the AI-only Conquest games are rerun; `SIM_VERSION` bumped and goldens re-blessed.
- The batched playtest records a cavalry charge into a line and an even line fight, before and after, and reports the rout points as numbers.

**Who and when**
- First wave at the restart. Owner: the soldier combat area (T4 owns `morale.rs`, `combat.rs` and `regiments.rs`), Opus lead at high effort since it moves balance; Haiku for the faction tables, goldens, guides and the quick reference. Two PRs: (1) the breaking point, wavering, variation, the AI and the stress test; (2) veterancy ranks with chevrons. `/simplify` and `/code-review` on Opus before each merge.
- It touches combat balance, so it merges before or after the dynamic bridges and pathfinding work, never alongside a faction table someone else is running.

## How battles play: eight changes in brief

Mario (2026-10-30), for the restart: eight changes to how the game plays, not how it looks, each aimed at a cause of what he keeps hitting (clumping, swarms winning everywhere, early routs, clunky orders, one faction too strong) rather than a symptom. He approved ten and then dropped two (a travel-distance economy and fatigue, 2026-10-30), so eight remain. The code was checked on 2026-10-30 for what each already has, so every item below starts from what is built. Nothing here is built before Mario says go for the restart.

**Order.** Four start in the first wave because the rest lean on them: **unit spacing** (it is the root of the clumping, gate and bridge trouble), **frontage** (it is what makes chokepoints and held lines work), the **balance tournament** (every later change is measured with it) and an **even early game** (measured by the tournament). The combat items go one after another in the soldier combat thread, after the morale redesign ([Holding until bloodied](#holding-until-bloodied-in-brief)), because they all change `combat.rs` and move balance.

| # | Change | Owner | Wave | Already built (checked 2026-10-30) |
|---|---|---|---|---|
| 1 | Unit spacing and collision | The dedicated pathfinding thread (Opus) | First | Regiments are blocks; friends overlapping past a margin slow and fall into disorder (`CROWDING`), blocks squeeze into columns in tight ground and queue behind friends (#136). Soldiers are drawn in slots, not simulated bodies (option C is later) |
| 2 | Frontage: only the soldiers touching the enemy fight | Soldier combat (T4, Opus) | First, after morale PR 1 | Option B duels: the front rank strikes, spears' second rank too, flank and rear men strike at flanks and rears. **Missing:** an attacker strikes with its whole front however little of the enemy it touches, and any number of regiments can pile onto one |
| 3 | Balance tournament | Faction rebalance thread | First | Field Battle `faction_table` in `gc_match` (pairs, two seed sets) and AI-only Conquest tests that end in a win. **Missing:** Conquest pairs, timings, deciding units, a report and a gate |
| 4 | Even early game across factions | Faction rebalance thread | First, with 3 | Starting stores and workers per faction (docs/04). Nobody has measured when each faction's first army is ready |
| 5 | Charges from weight and speed, braced spears | T4 | Second | Charges plough in by mass ("heft") against the ranks they hit (#212); a charge bonus stat that fades; braced spears (Z) stop a frontal charge and throw the charger into disorder; flank and rear blows land 1.4 and 1.8 times as hard. **Missing:** speed at impact counts for nothing beyond "at the run", and nothing about the angle |
| 6 | Responsive orders | New controls thread (Sonnet lead, Opus for the sim command) | Second | Right-drag draws a line (frontage and facing), army shapes (O), input delay of 2 turns (100 ms) on LAN. **Missing:** Shift-queued orders, a depth choice on the drag, attack-move, a measured click-to-move delay |
| 7 | Clear orders and feedback | The controls thread, with T4 for the reasons | Second | Banners blink amber when wavering, white routing, grey shattered; cards show morale points; the AI lens shows what computer players think. **Missing:** order and stance markers, why a unit wavers |
| 8 | Smarter computer generals | The AI (gc_ai owner, Opus) | Third | Scouts, ground reading (cover, high ground, water), flanks, rehearsed plans, raiders, walls and traps. **Missing:** using the restart's new systems, pulling back by strength under the new morale, frontage in the battle estimator |

### 1. Unit spacing and collision

Bigger units take up real room, and regiments keep their spacing while they march, wheel and squeeze, instead of bunching up and spreading out again. Merged into the **dedicated pathfinding thread**, whose scope (Mario, 2026-10-28) already covers crowding, clumping and flow through gaps: one owner for how regiments move.
- **What changes.** A regiment's block keeps its own size for its unit's real footprint (a Maw or a Bulk is as wide as its model, cavalry deeper than infantry). Blocks push gently apart instead of overlapping when marching side by side, so nobody slides through a friend and falls into disorder for no reason. In a gap narrower than the block, the regiment files into a column at its marching spacing, goes through and re-forms on the far side, in order, rather than every soldier racing for the gap. Soldiers on screen hold their slots while the block turns, so it wheels like a block, not a swarm.
- **Real use.** 200 soldiers through a gate: they form a column, go through at a steady rate and re-form; the stress scenario times it. A column on a two-lane bridge: the block narrows to the deck and keeps its spacing, nobody is pushed off the edge. Two friendly regiments ordered to the same spot: they stand side by side. A big monster in a line: it takes the room of three files, so the line is longer. In combat, regiments pushed back keep their shape, and a regiment marching past a melee waits or goes round instead of shoving into it.
- **Checks.** The clearance check uses these footprints (largest unit and a formation width, per the project rules). Stress scenarios: gate, bridge, a retreat through forest, two armies crossing paths.
- **Built so far (2026-10-30, first PR).** Blocks keep their spacing while marching (step aside from friends beside them or stopped ahead, follow friends marching on ahead, never stuck for good), and routes are priced by how fast the ground is walked, so armies go round a forest when that is quicker. Measured on grown maps (`marches.rs`): armies of 3, 6 and 12 regiments march at 84%, 82% and 69% of the best possible pace (73%, 75% and 61% before), none stuck. Still to come: real footprints for big units, gap flow for big armies, wheeling in slots, going round a melee, and the stress scenarios.

### 2. Frontage: only the front ranks fight

A regiment can only strike with the soldiers who physically touch the enemy; the rest wait behind to step up. Swarms then only beat a line where they can wrap round it, so gates, bridges and a held line become strong against the Brood. This is what makes chokepoints and Total War-style battles work.
- **The gap in the code.** Each attacker strikes with its front rank, capped only at a ladder head (`width` in `combat.rs`). So a 20-wide regiment hitting a 4-deep flank strikes with 20 men, and six Brood regiments round one Clans regiment each strike with their whole front.
- **The rule.** Who strikes is limited by where the two blocks actually touch: an attacker's strikers are the files whose stretch of front overlaps the side they hit, and the total striking any one side is capped by that side's length (one striker per soldier on it, two with spears behind). Extra regiments queued behind wait, push or go round to an open side. Ladders keep their few.
- **Real use.** A Clans line of 20 files in a gate 6 tiles wide fights only 6 to 8 Skitterlings at once, so the gate holds far longer; outside, the same swarm wraps it and wins. On a bridge, the lead files fight, the rest push behind. Flanking still pays because it opens a new side to strike. Deep blocks are not "wasted": they replace the fallen and hold against a charge (plough depth, item 5).
- **Balance and AI.** It shifts Brood against everyone, so it lands with the tournament (item 3) and a faction table on both seed sets. It is measured together with the faction rebalance's Brood stat cut ([Faction rebalance](#faction-rebalance-in-brief)), so the two nerfs don't stack: whichever lands second retunes against the other. The AI's battle estimator already counts frontage beyond the enemy's ("hit flanks for 40%"); it must use the same contact rule, and the AI must hold chokepoints and wrap lines on purpose.
- **Checks.** Stress scenarios: a swarm against a line in a gate and in the open (the win rate must differ sharply), and a column fighting off a bridge head.

### 3. Automatic balance tournament

After every balance or unit change, computer players fight each pair of factions dozens of times and report the result, so a change that makes one faction too strong is caught before Mario plays it. It belongs to the **Faction rebalance thread**, which owns balance, and is its tooling. Its Conquest half starts from that thread's `conquest_balance` test ([Faction rebalance](#faction-rebalance-in-brief)); this item grows it into the full report and gate.
- **What it runs.** Every pair of the four factions (and the Oathbound if it lands), Conquest 1v1 on two or three maps, both seed sets, at General, a few dozen games a pair; and the existing Field Battle table. Headless, in release, on the CI runner overnight or on demand, not on every PR (too slow).
- **What it reports.** Win rates per pair with the noise band; when each faction's **first proper army** is ready (first time it fields a set amount of supply in fighting units); when its first attack lands; Grub and Marrow per minute at 5 and 10 minutes; and **which units decide games** (share of kills and of supply in the winner's army at the end). A short Markdown table goes in the PR.
- **The gate.** A PR that changes stats, rosters, combat or the economy runs it and must keep every pair within a band (first pass 40 to 60%, both seed sets) unless the PR says why. Cheap per PR: Haiku runs it and posts the table; Opus judges.
- **Real use.** The Brood swarm problem: if a change pushes Brood above 60% on two maps, or their first army comes a minute before everyone else's, the PR stops there.

### 4. Even early game across factions

Every faction fields its first proper army at about the same time, so no faction is swarmed before it has built anything. Measured with the tournament (item 3), owned by the **Faction rebalance thread**, which also sets the Brood early-wave target.
- **What changes.** Target: every faction's first army ready within 30 s of the others at Standard tempo (first pass; the tournament sets the real figure). Knobs: starting stores and workers, first barracks cost and time, first unit cost and train time, Brood larvae rate. No new mechanics.
- **Real use.** A Brood rush at minute 3 meets a Clans or Choir army that exists, standing in a gate or behind a palisade (items 2 and the fortifications), not workers. Rushing stays possible but costs economy, so it is a choice, not the default.

### 5. Charges from weight and speed, with braced spears

A charge's punch comes from how heavy the unit is and how fast it is going when it hits. Spears braced and facing the charge stop it dead; a hit to the flank or rear wrecks a unit. With the new morale rule, charges break units by killing men.
- **What exists.** Ploughing by heft (#212), a fading charge bonus, braced spears that turn a frontal charge, and flank and rear multipliers.
- **What changes.** Impact = mass × speed at contact (the regiment's real speed over its last second, so a charge from a standstill is weak and one down a slope is strong). It sets both the plough depth and the charge bonus, replacing the fixed `PLOUGH_SPEED`. Braced spears stop a charge whose angle hits their front (within about 45 degrees of facing), and the faster the charger the more it loses on the points. A charge into the flank or rear ploughs deeper (no one braces a side). A unit that has charged must pull out and re-form to charge again at full weight (cycle charging). Mud, rough ground, woods and uphill cut speed, so they cut the charge.
- **Real use.** Cavalry into the rear of spears locked with infantry: deep plough, many dead in seconds, the spears reach their breaking point. The same cavalry into braced spears' front: stopped, disordered and bleeding. A charge through woods or up a hill hits soft. Charging into a gate or onto a bridge: only the touching files hit (item 2), so a narrow charge does little.
- **AI.** Computer generals brace spears when they see cavalry coming at their front, charge flanks and rears, and pull cavalry out to charge again.
- **Checks.** Stress scenarios: a cavalry charge into the front of braced spears, into a flank, and up a hill, with casualties printed; the faction table.

### 6. Orders that respond instantly

A lot of what feels clunky is the controls, not the pathfinding. A **new controls thread** (Sonnet lead for `gc_game` input and HUD; the Opus lead of whichever sim thread owns `regiments.rs` writes the sim side of the queue).
- **Measure the delay first.** Time from click to the first step on screen in single player and on LAN (Tailscale), split into input, lockstep delay (2 turns, 100 ms), route search and the regiment's turn to face. Fix the biggest part: show the order at once (route line and ghost on click), start the turn immediately, and cut route-search waits (with the pathfinding thread).
- **Shift-queued orders.** Shift-right-click adds a waypoint or an attack after the current order; the route line shows the whole queue. In the sim, a short queue of orders per regiment (a handful, hashed), so it goes through the lockstep and replays.
- **Drag a formation.** The right-drag already sets frontage and facing; add depth by dragging further (or a modifier), with a ghost of every regiment's block before letting go. Units in a group turn and move off together and arrive together (matching pace to the slowest).
- **Attack-move.** Move, but stop and fight anything met on the way (A then click).
- **Real use.** Queue a march over a bridge, through a gate, then an attack: the regiments file through in order (item 1) and attack on arrival. Drag a deep line across a gate: it fills the gate wide and stacks the rest behind (item 2).
- **Checks.** A headless test measures ticks from command to first movement and fails over a budget; LAN timing in the batched playtest.

**Progress (2026-10-30, built).**
- **Delay measured.** Lockstep delay is 2 turns (100 ms). Route search answers on the order's tick. The biggest part was the turn: a block stood and turned on the spot first.
- **Wheeling (#330).** Blocks now wheel as they march up to 90° off. `order_response.rs` checks the ticks from command to first step.
- **Queued orders and attack-move (#339):**
  - Shift queues up to four moves or attacks.
  - `A` attack-moves.
  - Every order shows a ghost plate and a dotted line at once.
  - The AI attack-moves when it defends.
  - Stress scenario `queued-orders`: through a gate, then an attack.
- **Deep lines and about-face (#341):**
  - Alt+right-drag draws a deep line, each regiment keeping its frontage and the rest stacked behind.
  - A block sent more than 135° behind faces about where it stands, its rear rank becoming the front, and marches off 0.3 s later.
- **Not done:** LAN timing waits for the batched playtest.

### 7. Clear orders and feedback

Show on screen what each unit is doing and why. Many "this feels wrong" moments happen because you can't see what a unit is thinking. Controls thread for the screen side, T4 for the reasons the sim exposes.
- **What it shows.** Order and stance icons on the banner and card (marching, attack-move, braced, hold, falling back, forced march, queued orders); morale as a bar with a tick at the breaking point (from the morale redesign); and **why** a unit is wavering, as one or two short reasons from the sim (flanked, rear, outnumbered, general fell, ally broke nearby). Hovering a unit shows its order queue and route.
- **Real use.** In a big fight you can see which regiment is about to break and why, and send a reserve there. A regiment that won't move shows "waiting: friend in the way" or "pinned".
- **Sim side.** The reasons come from the morale drains the sim already sums, kept as a small bit set per regiment in the view (not hashed, it is derived).
- **Built (2026-10-30).** Order line with icon (Holding, Marching, Forced march, Attacking, Advancing, Fighting, Braced, Falling back, Running), held-up note (pinned, driven back, re-forming ranks, filing through, trapped) and queued-order count on the panel and hover card; up to two "Why" reasons while wavering or running; order icons on tray cards and in a tab over selected banners; hover card for any regiment, with the route for your own. The sim keeps the reasons as unhashed bits beside the regiment pool and gives them out through `Regiments::feedback`. "Bloodied" means below half strength until the morale redesign lands, then down to its breaking point.

### 8. Smarter computer generals

Computer players scout, pick favourable ground, flank, pull back damaged units and use the new systems. A strong opponent makes every other system worth having. Owned by the AI (`gc_ai`), third wave, once the systems they must use are in.
- **What exists.** Scouts that find the enemy start, ground reading (cover, high ground, water), flanking roles, rehearsed plans (the Warlord tries plans in a scratch copy), raiders, walls and traps for Veterans and up, reading fords and floods.
- **What changes.** Each restart system teaches the AI in its own PR (rule (c): a feature isn't done until the AI uses it). This item adds what no single system owns: scouting **before** each battle and choosing to fight where the ground favours them (a gate, a bridge head, a hill, woods for an ambush); pulling back regiments near their breaking point (new morale) and bringing up fresh reserves; using frontage (hold a chokepoint against a bigger army, wrap a line in the open); and using bridges, debris and walls on purpose (dump debris on an enemy bridge, burn a bridge behind a retreat, clear jams).
- **Real use.** Outnumbered, the AI falls back to a bridge or gate and holds it; ahead, it meets you in the open and wraps your line.
- **Checks.** The AI-only Conquest games and the tournament; new tests that the AI holds a chokepoint when outnumbered and pulls back a bloodied regiment.

### Who and when

- **First wave at the restart:** spacing (pathfinding thread); frontage (T4, after morale PR 1); in the Faction rebalance thread, after its economy and Brood passes (which use its `conquest_balance` test), the full tournament, then the early-game pass, so frontage and morale are measured with it.
- **Second wave:** charges (T4), the controls thread (orders, then feedback).
- **Third wave:** the generals (AI).
- Every sim change bumps `SIM_VERSION` and re-blesses goldens through the merge train; `/simplify` and `/code-review` on Opus; guides and the quick reference updated in the same PR; each adds its stress scenario. The batched playtest after each wave records before and after clips of each change.
- Helpers: Haiku runs the tournament and tables, goldens, docs and guides; Sonnet writes the HUD and input code; Opus leads every sim, balance and AI change.

## How the work is split

Work is grouped into **eleven threads, one per area of code**. A thread keeps its area for all of its stages, so it never has to relearn the code, and two threads never edit the same files at once. Since M3 the project has read [15](15-staffing-plan.md#hard-rules) rule 3 ("one writer per crate") as one writer per module; this plan makes that explicit for `gc_sim`, `gc_render` and `gc_game`.

| # | Thread | Covers | Owns | Lead · effort | Helpers | Waves | Cost |
|---|---|---|---|---|---|---|---|
| T1 | Pathfinding overhaul | Idea 17; nav doors for gates; the hook that reopens cleared ground (idea 22) | `gc_sim` `nav.rs`, `nav_reblock.rs` | Opus · high | Haiku: benches, ford test, goldens, docs. Sonnet: nav debug overlay | 1–2 | L |
| T2 | Living Land maps | Doc 27 stages 1–4; scrub, woods and thicket as separate cover, and the clearable room near each start counted in fairness (idea 22) | `gc_map`, `gc_mapcheck`, map dressing in `gc_game/scenery.rs` | Opus · medium | Sonnet: port each layer behind hash tests, and the five new scenery models. Haiku: mapcheck sweeps, load times, screenshots, docs | 1–4 | L |
| T3 | Cinematic lighting | Lighting stages 1–5 (stage 1 is idea 21 part 3); small wins: camera tilt, backlit foliage, film grain, light motes; idea 5 director camera and replays; look of ideas 1 and 9 | `gc_render` frame, post, shadows, sky, particles; `world.wgsl` lighting; `gc_game` `camera.rs`, `replays.rs` | Sonnet · high for stage 1, then medium | Haiku: screenshots, bench runs, docs | 1–4 | L |
| T4 | Every soldier fights | Doc 28 stages 1, 2, 5; small win: soldier variation; hit flash; idea 11 physical formations; idea 12 commander on the field | `gc_game/puppets.rs`; `gc_sim` melee in `combat.rs`, `morale.rs`, `regiments.rs`, `champions.rs` | Opus · medium for stage 1, high from stage 2 | Sonnet: scrum and matched-kill screen code. Haiku: balance tables, benches, goldens, docs | 1–4 (C after) | XL |
| T5 | Materials, animation engine and fight library | Idea 21 parts 1 and 4 (detail textures by material, the stronger bake, re-baking buildings and units); doc 28 stages 3–4 | `gc_render/soldiers.rs` (it draws buildings too) and skinning, `gc_assets` model format, `gc_artbake`, the building and unit bake scripts | Sonnet · high for the engine, medium for bakes and moves | Sonnet: re-bakes, retargeting and tagging moves in Blender, one faction at a time. Haiku: screenshots, docs | 2–4 | XL |
| T6 | Fortifications and bases | Doc 26 stages 1–6 and 8; bridges that are built and burned (idea 6) | `gc_sim/structures.rs`, tower volleys, new wall modules; placing tool; fortification AI | Opus · medium | Sonnet: placing tool and wall interface. Sonnet: wall and tower models. Haiku: goldens, docs | 1–5 | XL |
| T7 | Battle effects and aftermath | Doc 26 stage 7; doc 28 stage 6; idea 8 wear, trophies, torn banners; idea 14 crows and lingering dead | Effect modules in `gc_game` and the particle users in `gc_render` | Sonnet · medium | Sonnet: collapse chunks, banner and trophy models. Haiku: sound list, screenshots, docs | 3–4 | L |
| T8 | Living ground | Idea 21 part 2 (blended ground textures with normals, its first pull request); ideas 15 and 19; idea 2 trampled grass and scars (also on the Conquest map, idea 14); small win: trampled ground; doc 27 habitat ground materials; cleared ground and stumps (idea 22); idea 21 part 5, surface detail on scenery (its second pull request) | Ground surface in `world.wgsl`, new ground-mask module; scenery drawing (`gc_render/scenery.rs`, `scenery.wgsl`), the scenery model format (`gc_assets::scenery`) and `gc_scenebake` | Sonnet · high | Sonnet: the scenery re-bake in Blender. Haiku: texture sourcing and baking, screenshots, docs | 2–3 | L |
| T9 | Weather and changing land | Idea 22 first: its design note, then clearing trees and scrub to expand (orders, rules, computer players); idea 13 terrain tactics; ideas 1 and 9 rules (rain, mud, night vision by light); idea 2 woods that burn and are cut; idea 6 fords that flood; idea 7 seasons in Conquest | `gc_sim/terrain.rs`, `vision.rs`, new `weather.rs`, a runtime ground-change command | Opus · medium | Haiku: balance tables, goldens, docs | 2–5 | L |
| T10 | Battle soundscape | Idea 4, later hit, weather and habitat sounds | `gc_audio`, `gc_game/sounds.rs` | Sonnet · medium | Haiku: sound sourcing and licence checks, credits | 1–2, 4 | M |
| T11 | Ambient life and busy bases | Ideas 3 and 10; idea 22 on screen (workers chopping, trees falling, log piles); doc 27 living touches (mist, fireflies, birds) | New `gc_game` ambient module | Sonnet · medium | Sonnet: animal and villager models. Haiku: screenshots, docs | 3–4 | L |

**Cost tiers** are relative, for planning: **M** is two or three pull requests, mostly Sonnet. **L** is four to six pull requests, or fewer with an Opus lead on hard code. **XL** is seven or more pull requests, long-lived.

**Models and effort** follow the project rules: Opus leads anything that changes the simulation, netcode or replays; Sonnet leads render, audio and tools. **3D modelling runs on Sonnet 5.5 alone** (Mario, 2026-10-23): models, rigs, moves, bakes and re-bakes, with no Opus supervisor and no Haiku builders. Low effort is never used. Extra-high or max needs Mario's go first; option C may be worth suggesting it.

## The world remembers the new features

Mario (2026-10-30): the "living campaign map" idea (regions that keep their history, battles that start on ground shaped by the past) is not a new system. It folds into idea 2, **a world that remembers**, which is already built. Every restart feature that changes the land must be remembered the same way.

**What is remembered today (checked 2026-10-30).** A Conquest match is one long game on one map, so its "battles" are the fights within it, and the world's memory is the match's own state:
- **In the simulation, for the whole match:** cleared and burnt woods, scrub and thicket (cover and growth, through the one ground-change path in [32](32-changing-land.md)); rubble of fallen buildings until it settles; bridge decks, stakes, barricades, walls, gates and their levels and damage; flood height, summer low water and frozen water; soaked ground. Every change mid-match feeds the ground's `changes` digest in the state hash, so every machine and every replay agrees on it.
- **On screen only, rebuilt from the simulation's events:** felled, scorched, trodden and soaked ground and stumps ([35](35-ground-marks.md), on the Conquest map since #289), the fallen and spent arrows, trophies and torn banners. Trodden tracks heal over minutes; the rest stays.
- **Not built yet:** save games and "Restage this battle" (the copying pieces are in `gc_sim::fork`), the Campaign ([19](19-campaign.md), design only, whose regions would carry this memory from battle to battle), and enemy buildings remembered as last seen in fog.

**The requirement.** Each new feature below keeps its state in the simulation (the terrain or the building it belongs to), changes it only through the ground-change path or a building's own state, so it reaches the `changes` digest, and keeps it until something in the game undoes it, never on a timer the player can't see:

| Feature | What the land remembers | Until |
|---|---|---|
| Drawn bridges | The drawn path, width, each span's level (plain, braced, reinforced), damage, burnt and fallen tiles | Repaired, rebuilt or upgraded |
| Curved walls and palisades | The drawn path and each tile's level, damage and breaches | Repaired or rebuilt |
| Dams (idea 24) | Built, open or shut spillway, damage; a **broken dam** stays broken, with its rubble in the river and the river running at its new level downstream and its old one upstream | Rebuilt |
| Debris and jams (idea 25) | Each jam where it caught, its size, and the water backed up behind it; logs and wreckage stranded on banks and shallows | Cleared by workers, burnt, or carried on by the next high water |
| Water-engine flood damage (idea 25) | Spans swept away, palisade and field works torn out, banks scoured (shore tiles lost to the river), mud and silt left on the floodplain (soft ground), drowned woods knocked flat | Rebuilt; silt dries and woods regrow only on the slow season clock |

**Real use.** The point is the next fight on the same ground. A column that crossed a braced bridge in spring finds the same bridge, damaged or upgraded, in autumn. An army that broke a dam to flood the valley meets a river it can wade below the dam and a lake it can't cross above it, and both sides' routes, fords and pathfinding see the change straight away (the nav reopen hook). A jam left on a pier keeps the water high upstream until someone clears it, so it is both a hazard and a ford-closer either side can use. Computer players read the remembered state as they read the land today: they cross the bridges that stand, clear jams in their way, rebuild what they need and avoid ground the flood has made soft.

**How each feature proves it.** Its pull requests add one remembered-state test (make the change, play on, check the state is still there and its digest is in the hash) and extend a golden replay so a later fight in the same match uses the changed ground. Save games, "Restage this battle" and the Campaign, when they are built, take this same state through `gc_sim::fork` rather than a second record, so nothing has to be remembered twice. The on-screen marks for the new features (broken dam rubble, stranded logs, flood silt) are added to [35](35-ground-marks.md)'s grid by whichever thread builds the feature.

**Who and when.** No thread of its own: each feature's owner (T6 for bridges and walls, the dam and water-engine threads for ideas 24 and 25) builds its part in its own pull requests, at the restart. Haiku helpers update the docs, guides and quick reference (the guide line: "the land remembers what was done to it").

## Waves

A wave starts when the work it waits on has merged, not on a date. At most seven threads run at once.

**Wave 1 (Saturday 24 October, 01:00 NZDT): six threads.**

| Thread | First pull request |
|---|---|
| T1 Pathfinding overhaul | Routes that match building footprints and terrain exactly, find clear gaps between buildings and replan fast; the open nav follow-ups (sector heuristic near walls, `Grid::number_areas`, local portal updates). It must also win back route-search headroom under the 2 ms p99 budget, because gates (T6 stage 2) spend it |
| T2 Living Land maps | Stage 1, grown land, with the old Wilds kept as Classic and a 768 map built in under 3 seconds |
| T3 Cinematic lighting | Stage 1, Light and Air, plus camera tilt, backlit foliage and film grain, starting from the prototype in `design/cinematic/` (built 2026-10-23, see [30](30-cinematic-lighting.md); the looks per land, lobby Time of day, Clarity and tuning panel followed on 2026-10-24) |
| T4 Every soldier fights | Stage 1, the scrum, plus per-soldier tint and animation phase variation |
| T6 Fortifications and bases | Stage 1, towers fire |
| T10 Battle soundscape | Positional sound and ambient beds per land |

Only T1 and T6 change the simulation in wave 1, in different files.

**Wave 2:** T1 builds nav doors (cells open to one team) and the hook that reopens cleared ground, then finishes. T9 starts in its place with idea 22: a short design note for Mario, then clearing. T2 stage 2, once T1's overhaul is in. T3 builds particles first (stage 3, built 2026-10-23: see [30](30-cinematic-lighting.md#built-stage-3)) because four threads wait on them, then clean edges (stage 2, built 2026-10-24: see [30](30-cinematic-lighting.md#built-stage-2)) and light motes, with hit flash left to T4 and trampled ground to T8. T4 stage 2, duels in the simulation. T6 stage 2, gates and turning, once the nav doors are in. New, both once T3 stage 1 is in: T5 starts with idea 21's surface detail (the stronger bake, detail textures by material, then the re-bakes), carried in the same model-format change as the animation engine; T8 starts with real ground textures (idea 21 part 2), then puts the same detail on scenery (idea 21 part 5), using T5's shared detail textures and stronger bake, and re-bakes the scenery pack.

**Wave 3:** T2 stage 3 (measured fairness) and stage 4. T3 stage 4, camera craft and the director camera. T4 physical formations. T5 fight library, Ember Clans first. T6 stages 3 and 4. T8 habitat ground materials, once Living Land stage 1 is in. T9 terrain tactics, once duels are in. New: T7 and T11, once particles are in; T11 starts with idea 22 on screen.

**Wave 4:** T3 stage 5, the look of weather and night. T4 stage 5, matched kills and impacts, and the commander on the field. T5 the other three factions' moves. T6 stages 5 and 6 (with bridges). T7 sparks, blood, wear and aftermath, using T4's blow events. T9 weather, night and changing land. T10 hit, weather and habitat sounds.

**Wave 5:** T9 seasons in Conquest. T6 stage 8, walled towns and smarter sieges. Then Mario's calls: option C, Living Land stage 5 (water that runs downhill), prisoners.

## Shared files

Some files every thread touches. Rules for them:

- **Merge train.** `SIM_VERSION`, the replay `FORMAT`, the net `PROTOCOL`, the golden replays, the `PINNED` table and `tools/gc_bench/baseline.json` are taken in merge order. A thread rebases on main, takes the next free numbers, re-blesses goldens and the pinned table, and merges as soon as it is green. The others rebase after it.
- **Interfaces between neighbours.** T5's detail textures and stronger bake are described in [31](31-surface-detail.md): T8 reads `assets/detail.pak` (`gc_render/src/detail.rs` uploads it) and the same bake method for scenery. T5 gives T4 a move picker; T4 keeps `puppets.rs`. T3 gives T7, T8 and T11 the particle system and ground-mask hooks; they add modules and only one-line hooks in `gc_render/lib.rs`. T5 makes the shared detail textures and the stronger-bake method; T8 uses both for scenery, and T2's new scenery models are made to T8's format (any made earlier are re-baked with the rest). T6 touches only tower targeting in the volley code; T4 does not touch volleys. T9 reaches combat only through the terrain modifier.
- **The AI keeps up.** Each simulation thread updates the computer players for its own feature in the same pull request, and runs the AI-only Conquest games before it pushes.
- **Guides, docs, status.** Every pull request updates the guides it affects (Guide menu, Quick Start, building tips, tooltips), the docs it affects, and the status page after merging, and the merge reply ends with the Mac build link.
- **Quality.** `/simplify` then `/code-review` on every code pull request, at high effort for the simulation, netcode, replays and AI, medium elsewhere.

## Saving tokens without losing quality

- Start **fresh build threads** briefed by the merged docs (26, 27, 28 and this one). The planning threads carry large prototype contexts that every later turn would read again; they stay as references.
- **Long-lived threads**, one per area of code, carry what they learn through all their stages.
- **Opus writes a short spec and reviews**; Sonnet helpers write render, interface and tool code; Haiku helpers re-bless goldens, run benches, balance tables and map sweeps, take screenshots, write docs and guide text, and source textures and sounds.
- **Medium effort by default.** High only where a slip is costly: route-search speed, duels and balance, the gates in route search, and the first look pass of the lighting and the ground.
- **Unblock first.** The pathfinding overhaul lands before gates and grown mountains, and particles land before every effect, so nobody builds twice.
- **Reuse what exists:** the lighting prototype and its presets, the Python map generator, the fight toy, the Mount & Blade research.

## Mario's to-dos

- **When you play next:** tune the lighting on the Mac build with F10 (wet, snow, stars, cloud banks, thunder, torches, Brood glow) and copy the Enter log to the lighting thread. Tune the sound beds, blows and weather sounds by ear.
- **Decide what's next:** Bolt and Blast towers and walled towns are ready to build; option C, Living Land stage 5, prisoners and faction depth stages 7 to 9 wait on your call.
- **Open calls with recommendations** (work follows the recommendation unless Mario says otherwise): the three calls in [26](26-fortifications.md#calls-for-mario). Blood is settled: stylised by default, Full as a setting (#286).
