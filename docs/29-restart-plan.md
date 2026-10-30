# 29 · Restart plan

*Plan, 2026-10-23; progress updated 2026-10-25 08:30 UTC. Mario asked for an in-depth plan to restart development after the pause, at high quality and token efficient. Wave 1 started at 01:00 NZDT on Saturday 24 October (2026-10-23 12:00 UTC). About 120 pull requests (#184 to #304) were built in the two days after; see [Progress](#progress). Work pauses for the week from 2026-10-25.* The illustrated version is the "Grubcraft Restart Plan" artifact (https://claude.ai/artifact/Qw4kBb6YPYZeq6ckdjJytB).

## Progress

*As of 2026-10-25 08:30 UTC, counting two pieces still finishing as done: hidden traps (#304) and the computer players' half of fortifications stage 8 (walls, bridges, stakes, traps and crossing ice; plan in `design/fortifications/stage8-plan.md`). The waves below are the original plan; threads ran ahead of them wherever nothing blocked them, so waves 1 to 5 are built except for the items under Left.*

| # | Thread | Built (pull requests) | Left |
|---|---|---|---|
| T1 | Pathfinding overhaul | Overhaul #193; attacks on buildings you can see #200 | Nothing. Nav doors (#203) and the reopen hook (#199) were built by T6 and T9 |
| T2 | Living Land maps | Stage 1 grown land #189; stage 2 ways through (passes, fords, walkable mountains) #292; stage 3 Measured maps #293; stage 4 sites by reason and five habitat scenery models #295 (habitat ground by T8, #221) | Stage 5, water that runs downhill, is a later call for Mario |
| T3 | Cinematic lighting | Stage 1 #188; particles #201; clean edges and motes #209; looks per land and F10 panel #217; camera craft #222 #228 (director camera, replay films) #233; weather and night look #235 #239 #245 #247 #250 #254 #255 #257 #261 #262 #265; F10 knobs #268 | Tuning on Mario's Mac with F10 |
| T4 | Every soldier fights | Scrum #187; duels #191 #192 #195; charges that plough (idea 11) #212; commander on the field (idea 12) #226; matched kills #243 #263; tumbling #266; finishers #270 #280; motion warping and champion kills #272; head turns #275; general as hero #277 | A duel spacing retune with T5 (below); option C is a later call |
| T5 | Materials, animation engine, fight library | Surface detail #198; engine stages 1 to 4 #213 #224 #229; fight library #238 #241 #260 #267 #274 #279; kills and finishers allow for where models are drawn #282; tipping models #285; blows, reels and kills for the Brood, riders and other rigs #291 #299 #301 | Fixing the bake itself so models are drawn where they stand, with T4's duel spacing retune; the rest of the fight library ([33](33-animation-engine.md) stage 5): more blows and kills per weapon, kanabo and sword-and-shield moves, kills for the other factions' weapons |
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
4. The bake fix so models are drawn where they stand, together with a duel spacing retune (T5 with T4); the rest of the fight library (more kills per weapon, kanabo, sword and shield, the other factions' weapons).
5. Smaller extras: lightning as a game event, burning buildings lighting the ground, Highland snow, and ambient polish (supply carts, forge sparks, more fauna).
6. Tuning on Mario's Mac: lighting with F10, and sound by ear.
7. Mario's later calls: option C (every soldier a body in the simulation), Living Land stage 5, prisoners after battles, faction depth stages 7 to 9.
8. Dynamic bridges, first wave at the restart: drawn curved paths, a choice of width, upgrades to braced and reinforced piers, and models generated from a timber kit; walls, fences and palisades drawn on the same curves; once the water engine (idea 25) is built, debris and floods strike, jam and break bridges ([in brief](#dynamic-bridges-in-brief)).
9. Faction rebalance, first wave at the restart: fair economies (the Choir earns about three and a half times what the Clans do), the Clans gathering 20% faster, and the Brood as a true swarm of cheap, weak units ([in brief](#faction-rebalance-in-brief)).

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
- **Pick a width.** Narrow, standard or wide (about 1.5, 2 and 3 tiles across), standard by default. The deck also varies a little on its own (a few percent wider at the piers, narrower mid-span), seeded so every player sees the same bridge.
- **Upgrade it (Mario, 2026-10-30).** A finished bridge can be upgraded, like walls (the same raise order, U key), a span at a time. Level 2, **braced piers**: cross-bracing and doubled pilings, more hit points against blows and debris. Level 3, **reinforced piers**: stone-footed piers with pointed cutwaters facing upstream, so debris glances off and jams less, and the bridge stands through all but catastrophic floods. Each level costs Grub and build time by the tile, and the generator draws the new pieces onto the same bridge (bracing, then stone footings and cutwaters), so you can see which bridges are strong.
- **Everything else stays as now:** workers build it out from the bank, it costs Grub by the tile covered, and tiles burn and fall one by one, dropping whoever stands on them into the river.

**How it works (simulation)**
- The order carries the path (control points in fixed-point) and the width, so it goes through the lockstep like any order and replays stay exact.
- The sim turns the curve into **deck tiles**: every tile the deck covers, joined edge to edge (no corner-only steps) so soldiers can always walk it. Each tile is still a bridge building slot, so building, fire, falling and the walking grid (`Nav::set_bridged_all`) work as today. This replaces #319's fixed second lane with width from the drawn path.
- Rules: both ends on land, every tile over water or shore, a length cap, and the bend limit. Computer players keep drawing straight paths (a curve with no bend) and may bend round rocks later.
- Bumps SIM_VERSION, FORMAT and PROTOCOL; goldens re-blessed.

**How it looks (generation from a kit)**
- **The kit** (Sonnet 5.5 alone, 3D, one timber material with the stronger bake): bank abutments (log cribs that meet the ground), trestle piers in three heights, stringer beams, plank modules in a few lengths and wear levels, rail posts, rails, rope, lantern posts, team cloth, and burnt and broken versions of the deck and rails.
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

**Fits with the ten changes to how battles play (Mario, 2026-10-30).** That plan gives a "rebalance thread" the balance tournament and the even early game; this is the same thread, and this rebalance is its first job. The `conquest_balance` test below is the Conquest half of the tournament. The Brood early-wave target joins the even early game. Frontage (only soldiers touching the enemy strike) also weakens swarms against a held line, so the Brood stat cut must not stack blindly on it: the economy PR (1 and 2) goes first; the Brood PR (3) is measured with frontage if it has merged, and cuts less if frontage already brings the Brood into the band.

**Who and when.** A new thread, "Faction rebalance", Opus lead at high effort, first wave after the restart. The sim rules (tuning reach and rate, gather speed per unit) are the lead's; a Sonnet helper does the AI changes (Choir placement, Clans Marrow), Haiku helpers run the balance tables and update the guides, quick reference, docs/04, docs/17 and the rosters. One PR for the economy (1 and 2), one for the Brood (3), then the Concord and Field Battle passes if needed. Sim changes bump SIM_VERSION, re-bless the goldens and go through /simplify and /code-review on Opus. The `conquest_balance` test lands in the first PR (its patch is in the project files, `balance/conquest_balance_test.patch`): `AI_GAMES=8 AI_FROM=1 cargo test --release -p gc_game conquest_balance -- --ignored --nocapture`, then `AI_FROM=101`; about two minutes a set on 4 cores. It touches the economy code, the Conquest AI and the data, not route search or structures, so it runs alongside the bridge and pathfinding threads.

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
