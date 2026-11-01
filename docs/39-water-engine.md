# 39 · The water engine

*Design note, 2026-10-30 (reviewed on Opus the same day; its fixes are in), idea 25 (Mario, 2026-10-25: "truly dynamic ... varying probabilities, flow rates, volume from season to season, flash floods e.g. could be minor to catastrophic. Almost like a weather engine ... trees felled upstream could fall in and become weaponised debris downstream or the enemy could dump debris in. Think it out and do it properly, dynamically." And 2026-10-30: "debris going downstream should be implemented to interact with the bridges"). It builds on living waterways ([38](38-living-waterways.md): seasonal water, currents, flash floods, computer players reading the water) and is the engine the bridge plan ([29](29-restart-plan.md), "Bridges and the water engine") and faction dams (idea 24) sit on.*

## The idea in one paragraph

Every river becomes a living thing with its own water in it. Rain falling on a river's land fills it, snow piles up on it in winter and comes down in spring, summer drinks it away. That water travels: a storm in the hills raises the river at the hills first and at the lowland ford a minute later, so a commander who watches the upper river knows what is coming. Floods are no longer one size. Most are a nuisance, some shut the fords, a few burst the banks, and once in a long while a catastrophe sweeps the valley floor and takes the weak bridges with it. And the river carries things: logs from woods felled or burnt on its banks, rafts an enemy tips in on purpose, the wreck of a bridge broken upstream. They ride the current, hit troops wading the fords, strike and jam bridge piers, back the water up behind them, and can bring a bridge down so its pieces go on to hit the next. A river is ground you fight over and a weapon you can use.

## What there is today (docs/38)

- One water line for the whole map (`Terrain::water_line`): the map's line plus `rise` (a rain flood or the spring melt) minus `low` (a dry summer). Every river rises and falls together.
- Fords shut all at once when the rise is full; summer fords open when the water is low.
- A flash flood is one fixed-size wave on one river, set off by a storm roll (`gc_sim::flash`). It is the only time water differs along a river.
- Currents push wading troops (`Ground::flow`, `Terrain::drift`). Rivers and their reach come from the flow layer on grown maps; other maps have still water.
- Computer players avoid fords that are shutting (`gc_ai::terrain::Water`).

What is missing is the river itself: an amount of water that comes from somewhere, takes time to arrive, and differs by size.

## 1. Rivers with water in them

### Stretches

Each river (from the flow layer, as `flash` finds them today) is cut into **stretches** of 16 tiles of reach, from its top to where it ends. A stretch is the engine's unit: it has its own water, and everything the engine does is per stretch, once a second. A big map has a few rivers of a few hundred tiles, so a few dozen stretches in all. Joined rivers are one tree: a stretch can have two above it (a fork), never two below.

Each stretch holds:

| Field | What it is |
|---|---|
| `store` | The water in it beyond its normal flow, in fixed-point units. 0 is the river at its line. |
| `stage` | How far that lifts the water there, in height units, from `store` and the stretch's width (a narrow gorge rises faster than a wide flat). Negative in a dry summer. |
| `land` | How much land drains into it: its own tiles plus everything above it (worked out once). A big river's lower stretches take far more rain than a stream's head. |
| `snow` | The snowpack on its land. |
| `choke` | How much of its outflow a jam or a dam holds back (section 4 and idea 24), 0 to 100%. |

### What fills and drains it

Once a second, for every stretch, top to bottom:

1. **Rain** adds `rain_strength × own land`. Wet ground (the sky's `wet`, already counted) passes more of it on: dry ground soaks up most of the first minutes of rain, so the first shower after a dry spell barely moves the river, and the same rain after a wet week floods it. This is what makes floods depend on what came before.
2. **Snow** in winter piles onto `snow` instead. In spring it melts into `store` at the season's pace, faster in rain (rain on snow is the classic big flood).
3. **Summer** drinks water away: a steady loss that drops a dry river below its line, as low water does today, but stretch by stretch, so small streams dry out first and big rivers last.
4. **It flows on.** A share of `store` moves to the stretch below each second (a linear reservoir: the more water, the faster it goes; the share is always under one), cut by `choke`. A stretch held back by a jam or a dam fills up behind it. The share is set so a slow rise (rain, melt) travels down a river at roughly 1 to 3 tiles a second as a swell, spreading out as it goes.
5. **The mouth** gives its water to the map's edge or a lake, and is gone.

**Bursts run as fronts.** A chain of reservoirs smooths every pulse into a long gentle hump, so it can't make a flash flood's wall of water. So a burst (a storm burst, a jam letting go, a dam broken; section 2) is not poured into the store: it runs on top of it as a **front**, like today's flash wave (`flash::Wave`, `front_ticks`): a crest height, a place it set off and its age. The front runs down at 6 tiles a second, and its crest drops an eighth for every 64 tiles it runs (and where a fork joins, it carries on into the stem). Behind the front the water stays at the crest for 20 seconds and falls over 30, then what is left of it is handed to the stores of the stretches it passed, so the river stays high a while after a big one. At most 4 fronts run at once; a fifth adds to the biggest. Every front is hashed while it runs, as the wave is today.

**The water on a tile.** A stretch's `stage` follows from its `store`, plus any front over it. Once a second, the engine writes the water level of every river tile into one small layer (one number a river tile, eased between neighbouring stretches so there is no step on the water). `Terrain::wet`, which runs for every soldier every tick, reads only that layer, as cheaply as it reads the map's line now. A new `Terrain::depth(p)` reads the same layer for the things that care how deep (the Minor flood's slower wading, debris stranding). Lakes and still water keep the map-wide level, rain floods and low water exactly as now.

**Fords belong to one stretch.** Each ford (a connected group of ford tiles, as the map lays them) is given to the stretch its middle lies in, so a stretch boundary never half-shuts one. Fords on lakes and still water stay with today's map-wide flood. Every ford group that switches in a second is switched in one call on the walkable grid (`set_flooded_all` with all their runs), so a crest passing three fords in the same second is one change, not three. Each group switches with a margin (shut at the full rise, open below half), so it never flickers.

**Backwater.** A jam or a dam chokes the stretch it is in, so the water rises in that stretch and the ones above it, and falls in the ones below. Bridges and dams don't recut the stretches: a bridge in the lower half of a stretch raises the whole stretch, which reads fine at 16 tiles.

Everything is whole-number maths, run in the same order on every machine. No random stream moves: the only rolls are hashes of the seed and a tick, as for storms today.

### What the player sees

- **Every river has a name** (drawn from the seed, by faction land: "the Brackwater", "the Mirrow") and a **gauge** on the tooltip of its water: Low, Normal, Up, In flood, Bursting its banks.
- **The top bar chip** shows the worst river you can see, with an arrow for rising or falling.
- **The floodplain when building.** Placing a building, wall or camp shows the river's floodplain in two shades (reached by a Major flood, reached by a Catastrophic one), so nobody builds a base on it by accident.
- **River watch.** Your units see water rise on any stretch they can see. Messages and the computer players read only what their side can see, plus the map-wide warnings below. When a stretch you can see starts a swell, you get "The Brackwater is rising in the hills: crest at the Old Ford in about 40 s", worked out from the swell's speed. Without eyes upstream you hear it only from the rumble, 8 seconds before it reaches a ford you can see. So a scout posted up the river is an early warning post: a new reason to hold ground that isn't on the front.

## 2. Floods from minor to catastrophic

A flood is simply a lot of water in a stretch. Its size comes from what fell, on what ground, in what season, so it is never the same twice. Four bands, by the crest's stage at the stretch:

| Band | Stage | What it does in play |
|---|---|---|
| **Minor** | over 0.1 tile | Wading is deeper (read from `Terrain::depth`): wading in it is a quarter slower, and summer fords go deep. The current pushes harder. Nobody is swept. |
| **Flood** | over 0.3 tile | As today's flood: the stretch's fords shut, and troops caught in them are swept to the bank (or, in a fast rise, downstream). |
| **Major** | over 0.5 tile | **The banks go under.** Low land along the stretch (the floodplain, below) turns to wading ground: slow, no charges, troops in it fight worse. Damaged bridges on the stretch are strained (section 4). Wooded banks shed a few logs into the water. |
| **Catastrophic** | over 0.8 tile | **The valley floor floods.** Floodplain under a wading depth of water is shut on the walkable grid, and anyone on it is swept downstream as by a flash flood today (up to 10 tiles, some drown). Weak or damaged bridge spans are swept away. Buildings on the floodplain take damage. A catastrophic flood leaves debris all down the river. |

### What sets them off

- **Long rain** on wet ground fills the rivers slowly. Usually Minor, sometimes a Flood, as now.
- **Storms** (rain of strength 220 or more, as now) roll once, from the seed and the spell's start, for a **burst**: a front set off in the upper river (section 1), the flash flood. Its crest is drawn from a **heavy tail**: most are modest, some are big, a very few are huge. The band of a burst is the crest it has when it reaches the river's first ford; it weakens on down the river, so a Major at the first ford may be a Flood at the last. In numbers, of the bursts that happen: about 35 in 100 Minor, 45 Flood (today's flash flood, so flash floods are not weaker on the whole than now), 16 Major and 4 Catastrophic, before the ground and season move them:
  - wet ground and a high river make every burst a band worse more often;
  - spring rain on a deep snowpack doubles a burst;
  - summer on dry ground halves it.
- **The spring melt** after a snowy winter is a slow, long Flood or Major on every river, worst on rivers with a lot of high land. A winter with little snow gives a gentle spring.
- **A dam breaking or opened** (idea 24) and **a jam letting go** (section 4) drop their held water into the stretch below as a burst. This is the same burst, so everything downstream (warnings, fords, the floodplain, debris, bridges) reacts the same way whatever started it.

**A catastrophe is always told.** A burst rolled Catastrophic (at the first ford) is set off far enough up the river that its front takes at least 30 seconds to reach the first ford, and it is told map-wide at once, whatever you can see: thunder, the sky darkening upstream, and "A great storm has broken over the hills of the Brackwater: get off the river". A Major is told map-wide 15 seconds ahead. Minor and Flood bursts keep today's 8-second rumble (or the river watch's longer warning). So the floods that kill come with time to act, and the small ones still reward watching the river.

So a catastrophe is rare but has a story: a storm in a wet spring after a snowy winter, or a dam blown upstream. It is in the forecast for anyone watching the rivers. In a 30-minute Conquest match with seasons, the expected count is a handful of Minor floods, one or two that shut fords, maybe a Major. A Catastrophic flood comes in perhaps one long match in five on a river map, and almost never without warning signs (rain, a high river, a big snowpack).

### The floodplain

When the match starts, every land tile within 6 tiles of a river tile is measured: how far it stands above the river's line, and which stretch is nearest. Tiles that a catastrophic crest (1 tile) could reach are kept as the river's **floodplain**, grouped by stretch and height band. It is a short list on most maps (flat valleys have a lot, gorges almost none), worked out once from the heights the sim already has. Shallow floodplain water is wading ground: it is only `Terrain::wet` (from the water layer), not a change on the walkable grid. Deep floodplain water (over 0.35 of a tile, the wading depth) is shut on the walkable grid. So each floodplain group has only two states on the grid, open and shut, and switches at most twice a flood, with a margin so it doesn't flicker, exactly as ford groups do today; all switches of one second go in one call. The walkable grid changes a handful of times per big flood, never per tick, and only in Major and Catastrophic floods. Buildings on deep floodplain water lose a share of their hit points a second. (Walls holding the water back from what is behind them was considered and dropped: it needs the flooded area worked out again whenever a wall goes up. Dams, idea 24, hold water instead.)

## 3. Debris

### What a piece is

A small, fixed pool of floating things, at most 48 on the map. Each piece:

| Field | What it is |
|---|---|
| kind | **Log** (a felled or burnt tree), **Raft** (a load dumped on purpose), **Wreck** (a fallen bridge tile), **Rubble** (a broken dam or building). |
| mass | 1 to 8: a log 1, a raft 3, a wreck 2, rubble 4. Pieces that jam together add up. |
| where | A point on the water, and its stretch. |
| state | Floating, Stranded (aground on shallows or a bank), or Jammed (caught at a bridge or dam). |
| owner | Who put it in, if anyone, for the score and the messages. Nobody controls it once it floats. |

When the pool is full, the oldest stranded piece rots away first, then the oldest floating one. 8 of the 48 places are kept for wrecks and rubble, so a broken bridge always sends its pieces on. Pieces that float off the map's edge or into a lake or still water are gone (this includes the still middle tile where the two turned halves of a grown map meet).

### Where it comes from

- **Felled woods.** Workers clearing a woods or thicket tile that touches running water can **fell it into the river** (a toggle on the Clear order, F while clearing): no timber is paid, it takes half as long again as plain clearing, and a log (thicket: two) goes in. The plain Clear order still pays timber and drops nothing. With the timber given up and the time, a log costs about as much per mass as a raft, so neither is the cheap way. By faction twist (docs/32): the Clans fell (the toggle, as above); the Concord burn, so their bank clearing topples burnt logs by the burning rule below and they have no toggle; the Choir shatter and the Brood eat, so their clearing drops nothing and they dump rafts instead.
- **Burnt woods.** A burning woods tile on a bank has a 1 in 3 chance of toppling a log into the water when it burns out. Fire upstream feeds the river with logs.
- **Floods.** A Major flood takes a log off one wooded bank tile per stretch every 30 seconds, and a Catastrophic one every 10, but no more than 8 natural pieces float at once on the whole map. Natural debris is a hazard, not a chore.
- **Dumping.** Workers ordered to **Dump into the river** at a bank tile walk there and spend 6 seconds tipping in a raft. It costs 12 Grub (the timber), Conquest only. The pool cap, the share any one player may own (16 pieces), and the workers standing on the bank where the enemy can see them stop it becoming a flood of logs.
- **Broken bridges and dams.** Each run of bridge tiles over running water that falls together becomes one wreck for every 3 tiles (at least one), mass 2; a broken dam becomes rubble. That is the cascade, kept to a size the next bridge can survive if it is braced or cleared.

### How it moves

- A floating piece rides the current **along the river's reach**: its place is its reach (how far up the river, fixed-point) and a small offset across the channel, kept from when it went in. Each tick its reach drops by a speed from the flow there (slow 0.5, steady 1, fast 2 tiles a second) and the water's depth (up to double in a flood), and its drawn point is the river tile at that reach and offset. Moving along the reach means a piece can't wander onto a bank or circle in an eddy. A piece caught by a front rides with the front. 48 of them cost almost nothing.
- Every roll a piece makes (jam or strike, toppling in) is a hash of the match seed, the tick, the piece's number and the tile, so pieces arriving in the same tick don't all do the same thing, and no random stream moves.
- **Low water strands it.** A piece on a stretch below its line, or that drifts onto a ford in slow water, grounds and stays. When the water comes up again it floats off. So debris piles up quietly in a dry summer and comes down all at once in the first big flood of autumn or the spring melt: the **spring flush**.
- Pieces close together in a narrow place can stick to each other (a snag), and a snag of 4 or more mass at a ford makes that ford rough: it costs a third more to cross until cleared or washed away.

### What it hits

- **Troops.** A piece passing through a regiment wading a ford, a summer ford or flooded floodplain hits it: hit points by mass and speed (a log at a fast river's pace costs about 6 HP; a raft in a flood about 40, spread on the soldiers it passes), disorder, and a little morale. A regiment is hit by any one piece once. A regiment being swept away (already carried and washed up by the flood) isn't hit again on top. Troops on a bridge are not hit, but troops on a bridge that falls drop in (as now).
- **Bridges.** Section 4.
- **Dams.** A piece reaching a dam jams against it (idea 24). A dam's spillway passes small pieces.
- **Other pieces** at a jam add to it.

## 4. Bridges in the water (the "fourth bridge PR", docs/29)

This is built by this thread with the bridge thread (T6) reviewing, as docs/29 sets out.

### Piers

The simulation counts a **pier** under every bridge deck tile over running water whose deck is not on a bank, at the bridge's level (1 plain, 2 braced, 3 reinforced; the bridge thread adds the levels). Narrow bridges have fewer piers in the water than wide ones, and a curved bridge that crosses at a slant has more. That gives the drawn layout a reason to matter: cross square and narrow where debris comes down.

### What a pier does with a piece

When a floating piece reaches a deck tile with a pier:

| | Plain (1) | Braced (2) | Reinforced (3, cutwaters) |
|---|---|---|---|
| Chance it jams | 40% | 30% | 10% |
| Otherwise it strikes: hit points lost by the tile, per mass at a steady river's pace | 6% of the tile | 4% | 1.5% (it glances off) |

The strike scales with the square of the piece's speed, so the same log does four times the harm in a fast flood. A piece that strikes and doesn't jam goes on under the bridge.

### Jams

- A jam is a pile at one bridge tile, with a total mass, at most 12 (more pieces pass over or round it). It **chokes** the stretch the bridge is on: each mass point holds back 5% of the outflow, up to 60%. The water backs up and rises in that stretch and above it (and the fords there may shut behind a big jam), and falls below it.
- A jam pushes on its tile every second, but only while the water is up (Minor or more), by its mass times the water's stage: the tile loses hit points (plain 0.1% a mass point a second at Minor, rising to 0.4% in a Major; braced two thirds of that, reinforced a third). A full 12-mass jam on a plain bridge in a Major brings the tile down in about 20 seconds; a 3-mass natural jam at normal water does nothing. So jams matter when the river is up, which is when an enemy will send them.
- **Small jams wash free.** When the water falls back below Minor, a jam of 3 mass or less breaks up and floats on. Bigger jams (almost always someone's doing) stay until cleared, burnt or the bridge goes.
- **Clearing it.** Workers on the deck tile or the bank next to it can clear a jam with the Clear order: 4 seconds per mass point, and each piece pulled out pays 1 Grub of timber. The cleared pieces are drawn ashore.
- **Burning it.** A jam can be set alight with the Burn order, as woods are. It burns off a mass point every 3 seconds and won't light in rain, but the fire spreads to the deck tile above it (timber bridges burn), so it is fast but costly.
- **It lets go.** If the tile falls, the jam's held water drops into the stretch below as a burst, and every piece in the jam plus the tile's wreck floats on down. That can take out the next bridge down: one raft upstream can cascade.

### High water

- From Major upwards, piers that are already hurt are strained: a plain tile under 75% of its hit points loses 0.5% a second, braced a quarter of that, reinforced nothing. A sound bridge stands through any spring melt or Major flood untouched, so nobody has to babysit every bridge; one an enemy has damaged gets worse.
- A catastrophic crest sweeps away any plain span tile below half its hit points, or braced below a quarter; reinforced spans stand.
- **Bridges stand better than fords.** A bridge's deck stays open whatever the water does (as now), until it falls. In a flood the bridges are the only crossings, so they become what both sides fight over, and what an enemy wants to bring down with debris.

### Interface with the bridge thread

- The bridge thread owns deck tiles, their building slots, levels and the generator. This thread needs from it: the deck tiles and their slot (today's `Structures`), each tile's **pier level** (`Structures::pier_level(slot) -> 1..=3`, added in the bridge upgrades PR; a stub returning 1 if this lands first), each tile's `ashore` flag (no pier on a bank tile) and `drawing` id (which bridge it belongs to, for messages and the cascade), and the existing damage, fire and fall paths (`settle_decks`). Agreed with the bridge thread on 2026-10-30.
- This thread adds: a list of the deck tiles that fell this tick, handed out of `settle_decks` (so its signature only gains an out-list, and the debris pool stays outside it), turned into wreck pieces, the strike, jam and strain code (in its own module, `gc_sim::debris`), and the visuals' data (`World::debris()`, `World::jams()`), which the bridge kit draws as logs piled against the piers and foam.
- Drawn, curved and wide bridges change nothing here: piers are counted from deck tiles whatever their shape.

## 5. Computer players

- **They read the rivers.** The gauge, the rise and the forecast crest go into their water reading (`gc_ai::terrain::Water`), stretch by stretch instead of all fords at once. They keep off a ford a crest will reach before they are across, and wait for it to pass.
- **They time crossings.** A mind that sees a crest pass a ford knows it has the falling water's time to cross (when nothing else is coming), and goes then.
- **They keep off the floodplain** in a Major or worse flood, and don't camp armies on it when a big crest is forecast.
- **They clear jams** on their own bridges (a worker regiment is sent when a jam passes 3 mass or the tile drops under 60%), and burn one when the bridge is lost anyway.
- **They repair** bridges hurt by debris, as other buildings.
- **They use the river.** A mind with workers and woods or Grub upstream of an enemy bridge, an enemy army on a ford below, or a dam above an enemy town, can dump or fell into the river, timed for a flood when it sees one coming. Harder minds do it more and time it better. Every AI choice above is visible in the AI journal.
- **They upgrade** bridges on rivers an enemy can reach upstream (once the bridge thread adds levels).

A feature isn't done until they do this (project rule), so each stage carries the minds' half of what it adds: stage 1 moves today's all-fords reading to stretches and forecasts, stage 2 adds the floodplain, stage 3 avoids debris and stage 4 clears jams and repairs. Only the offensive use of the river (dumping and felling against an enemy, upgrading against it) waits for stage 5. Minds read only stretches their side can see, plus the map-wide warnings.

## 6. Real use

How it plays with armies of each size and formation, in traffic and in battle, and with the other systems. These are also the stress scenarios each stage adds to the headless suite.

### Crossing at a ford

- **A small raid (2 to 4 regiments)** in column crosses a 4-tile ford in about 8 to 12 seconds. Against a swell the forecast gives 30 to 60 seconds if anyone is watching upstream, plenty of time. Without a watcher, the rumble gives 8 seconds: enough to finish if already half way, not enough to start. The rule a player learns: watch the river before you wade it.
- **A big army (20 regiments, about 500 soldiers)** takes a minute or more to cross one ford in a column, longer in line. A Flood shutting the ford mid-crossing splits it: the half across fights alone. That is the intended drama, and it is readable: the gauge, the forecast and the rumble all come first. The minds plan around it (they don't start a crossing a forecast crest will cut).
- **Formations.** The ford's width limits frontage as today. A Minor flood makes wading slower (a quarter), so lines lose shape faster in the current; columns are the right way to cross high water. Currents push the whole block, never single soldiers (as built).
- **Shut fords and the walkable grid.** Ford groups are now per stretch, not per river: a flood shuts the fords of the stretches it reaches, in order down the river, each once. A big river with five fords changes the grid five times as a crest passes, and five times as it falls.

### Crossing at a bridge

- A two-tile (standard) bridge passes a column two soldiers wide; a 200-soldier column takes about 25 seconds to cross a 10-tile span (the clearance and stress suite's "column over a bridge" scenario). A log hitting a pier does not stop the column: it damages the tile. A jam does not block the deck either; it only strains the tile under it.
- **Under fire and debris together.** The dangerous case is a column on a plain bridge while a raft arrives in a flood and archers burn the deck. A tile falling drops its soldiers into the river (as now), and they come up on the nearest bank, which may be the enemy's. The suite runs this case (the bridge falls, the swept regiments come ashore without getting stuck, and the column behind halts at the gap and re-routes).
- **Traffic.** In a flood, every ford is shut and the bridges are the only ways over, so armies crowd onto them. Pathfinding treats deck tiles as ordinary ground, so this is a traffic test for the pathfinding overhaul thread: the stress scenario is two armies of 200 trying to use one bridge in a flood, in opposite directions.

### Huge armies and other units

- **An 8000-point battle (40 or more regiments, 6,000 soldiers)** on a river map: no army that size crosses one ford between two crests, so the river splits it into waves, and the commander picks a crossing by the forecast. The engine's own cost doesn't grow with army size; what grows is debris hit tests (48 pieces against the regiments in water, done by stretch, so only regiments wading on a stretch with pieces are tested) and the sweep (as today). The bench runs field-8000pt on a grown river map with a burst to check the tick budget.
- **Big and monster units** (the largest blocks, siege engines, the Brood Maw): they wade as any regiment and are swept the same, but a monster is hit by debris for half (its bulk) and a siege engine can't cross a ford at Minor or above (it would be carried off), so floods can strand a siege train. A bridge carries them as now.
- **Workers and carriers in Conquest**: a flood that shuts the ford between a base and its far fields cuts that Grub until it falls or a bridge goes up. That is the economic side of the river, and a reason to build bridges early.
- **Melee on a bridge.** Fighting on a two-wide deck is as today (the frontage is the deck). A jam straining the tile under a fight doesn't stop it; if the tile falls, both sides drop in. Line and wedge formations reform into a column at the bridgehead as the pathfinding thread's narrow-passage rule does today; the suite checks a line of 8 files reaches the far bank.
- **Archers and wading.** Troops wading a Minor flood shoot as from wading ground (today's penalty); shooters on the bank have the usual edge over troops in the water, which a flood makes bigger by keeping them there longer.

### Pass marks for the stress scenarios

- Retreat through a flooding valley: every regiment reaches dry ground within 60 seconds of the shut, none stuck walking in place, none on shut ground at the end.
- Two armies of 200 on one bridge in a flood, opposite ways: both across within 3 minutes, no regiment stalled more than 10 seconds, no deadlock.
- A raft down a ford of wading troops: each regiment hit at most once per piece, the raft comes out below the ford, nobody left inside deep water.
- A column on a bridge when debris and a flood arrive: the tile falls when the numbers say, the soldiers on it wash up on a bank, the column behind halts at the gap and re-routes to another crossing within 10 seconds.
- Timings of each recorded in CI, as the suite does.

### Fighting on and by the floodplain

- A Major flood turns a riverside battlefield into wading ground: no charges, slow, worse fighting. Cavalry loses its edge. A defender on high ground beside a river can wait for the water. The bands are on the gauge so this is never a surprise to someone looking.
- A catastrophic flood shuts the deep floodplain and sweeps troops on it downstream. Troops on shallow floodplain wade out. Formations break up when swept (disorder, as now). Armies retreating through a flooded valley route over higher ground automatically, because the grid shuts the deep water before they get there. The suite runs "a retreat through a flooding valley": every regiment reaches dry ground, none is left walking in place.
- Fire and water: burnt bank woods drop logs; a jam set alight can burn the bridge; rain puts out fires and fills the rivers, so a wet day is a day of floods, not fires.
- Morale: being swept or hit by debris costs morale (as swept-away does), and the morale redesign's "holding until bloodied" counts debris hits as wounds, not a reason to rout.
- Weather and night: the river watch works at night only with torches or at short range (dark sight); a night flood is far more dangerous.

### Dams (idea 24) on this engine

Dams are idea 24 and get their own stage, but the engine is built for them:

- A dam is drawn across a river like a bridge, with a **walkway** on top that is a crossing (standard width, two tiles, so a column two wide; wide dams three). It chokes its stretch, so water piles up behind it into a reservoir (the stretch's stage rises and its land floods as floodplain, a lake forming above the dam).
- **Opening the spillway** lets the held water out as a controlled swell: the river below rises, fords shut, a Flood or Major comes down on the enemy, timed by you. Closing it again refills.
- **Destroying a dam** releases everything at once as a burst: the reservoir empties, a catastrophic crest goes down the valley with the dam's rubble in it.
- The real-use rules for the walkway (clearance for the largest unit and a formation, traffic both ways, fighting on it, what happens to troops on it when it breaks) go in the dam stage's design, which follows this one.

## Built: stage 1a, rivers with water

Built 2026-10-30 (`gc_sim::rivers`, which replaces `gc_sim::flash`). Simpler than the plan in places, as noted.

- **Stretches.** Worked out when the match starts from the flow layer: each river's tiles side by side in the same 16 tiles of reach are a stretch, each stretch knows the one below and the one above with the most land, and a walk of each river's tree tells in one comparison whether one stretch is downstream of another. A 256-tile grown map has some 10 to 90 fords on its rivers.
- **Stores.** Once a second each stretch takes the rain falling on its tiles (strength times wetness of the ground, so the first rain on dry ground barely moves it), plus the spring melt as rain of strength 120, and passes an eighth of its water on to the stretch below (`OUTFLOW_DIVISOR`). The water lifts the stretch by its store over `STORE_PER_UNIT` a tile, at most half a tile (`MAX_SWELL`). So long rain lifts a river's lower stretches several times more than its head, and they fall last.
  - *Simpler than the plan:* summer low water, the snowpack and the rain flood that shuts every ford stay map-wide as built in docs/38, and the stretches add to them. Small streams don't yet dry first.
- **Fronts.** A storm's roll gives a band from the heavy tail (35 Minor, 45 Flood, 16 Major, 4 Catastrophic in 100), a band bigger half the time on wet ground or a high river, the crest doubled on spring snow and halved in a dry summer. The front sets off up to 96 tiles above the first ford it reaches, runs at 6 tiles a second only down the stretches below where it set off (so a fork's other branch stays dry), and fades an eighth every 128 tiles (at most by half). Up to 4 run at once; a storm while all four run sets none off. A Major waits to set off until it is at least 15 seconds from its first ford, a Catastrophic one 30, and both are told to everyone at once (`Event::GreatFlood`). Flood and bigger give the river's 8-second warning as before.
  - *Simpler than the plan:* what is left of a front isn't handed to the stores as it passes.
- **Fords.** Each connected group of ford tiles on a river (with the summer fords up or down its branch) is one ford, in the stretch of its middle tile. It shuts when the map's rise, its stretch's swell and the highest front over it reach a full flood, and opens below half one. Every ford that shuts or opens in a tick is one change to the walkable grid. A rain flood that has shut every ford still keeps them shut and tells it; fords a river still holds shut stay shut when it goes. `Stage::Here` and `Gone` now tell when the rivers first shut a ford and when the last opens.
- **The water on a tile.** The terrain keeps a small copy of the stretches' swell and the fronts (`RiverWater`), brought up to date once a second and every tick a front runs. `Terrain::wet` asks it only for ground lower than the most it raises anywhere, so dry land costs nothing. The current pushes twice as hard where a river is up.
- **Swept away.** Troops in a ford a front is over when it shuts are carried down the river as before (`World::sweep_away`); troops in a ford a slow swell shut struggle to the bank as in a rain flood.
- **Computer players** read the fords shut or threatened stretch by stretch (`Rivers::threatened`), as they read one river's before.
- **Golden.** `field-river-storm-1v1`: a storm over a grown map's rivers rolls a major flash flood. No older golden moves: none has a flow layer.
- `SIM_VERSION` 58 (grown maps play differently). No new commands, so no format or protocol change.
- *Left for stage 1b:* river names, the gauge and chip, river watch and forecasts, wading slower in a Minor flood (`Terrain::deep`), and the drawn water by stretch (all built, below).

## Built: stage 1b, reading the rivers

Built 2026-10-30.

- **Deeper water in a river that is up.** Where a river's swell and fronts raise its water a tenth of a tile or more (`rivers::DEEP_RISE`, a minor flood), wading there is a quarter slower again (`terrain::DEEP_WADING_PERCENT`). `Terrain::deep` tells whether a tile is in such water. The selection panel says "In a river that is up: slower still, no charges".
- **Readings.** `Rivers::reading` gives each stretch's river, a tile about halfway along it, how far its water stands above the line, and the worst front still to reach it (seconds, and its crest there). `Gauge` reads it as Low, Normal, Up, In flood or Bursting its banks.
- **Names.** Every river (joined rivers are one) is named from the shape of the map's rivers, so a map's rivers keep their names from match to match: "the Brackwater", "the Mirrow"... The rivers of one map never share a name.
  - *Simpler than the plan:* the names don't yet follow the faction lands they run through.
- **The top bar chip** shows the worst river the viewer can see that is out of the ordinary, with whether it is rising or falling: "The Brackwater in flood, rising". Low water and the map-wide flood keep their own words.
  - *Simpler than the plan:* there is no tooltip on the water itself; the chip and the selection panel say it.
- **River watch.** Once a second, if the viewer sees a stretch rising in flood and a front is still to come down on a ford below it, they are told when it gets there: "The Brackwater is rising upriver: its crest reaches a ford in about 40 s" (and that it is a major or catastrophic flood, if it is). Once per crest a river. Without eyes upstream, the river's 8-second warning is all they get, as before.
- **The drawn water.** Each river tile's water is drawn at its own height, the swell and any front over it, and the bank beside it with it, eased between tiles (`Renderer::set_river_water`, a byte a tile, redrawn up to ten times a second while a river is up and not at all otherwise). Soldiers, scenery and particles in the water stand at its raised surface, as they did at the map-wide flood's.
- **Computer players** keep reading the threatened fords (stage 1a), which already covers every ford a warned front will reach until it gets there.
- **Golden.** `field-river-storm-1v1` was recorded again on another seed: the one before rolled a Flood, not the major flash flood stage 1a said it did. Now a Major is told to everyone 15 s before it shuts the first ford, and the water reaches Bursting its banks.
- `SIM_VERSION` 63: wading on grown maps changes. No other golden moves (none wades a swollen river; `water_engine.rs` tests it). No new commands.

## Built: stage 2, floods over the banks

Built 2026-10-30.

- **The floodplain.** When the match starts, dry land within 6 tiles of a river (`rivers::PLAIN_TILES`) and less than a tile above its line, reached from the river without crossing higher ground (water doesn't climb a ridge), becomes that river's floodplain. Each tile joins the stretch of the river tile nearest it and takes that tile's reach, so the river's own water (swell and fronts) rises over it as it rises over the river. `Rivers::floodplain`, `Rivers::on_floodplain`; `Rivers::stretch_at` now answers for floodplain too.
- **Wading ground.** Once the water stands above a floodplain tile it is `Terrain::wet`: slow and no charges, as a ford, and a quarter slower again (deep wading) once the water is a tenth of a tile deep over it. The selection panel says "In flood water: slow, no charges". The drawn water spreads over it, as the river picture covers every tile with a stretch.
- **Shut when deep.** Each stretch's floodplain is grouped in bands a tenth of a tile high (`PLAIN_STEP`). Once a second, a group shuts on the walkable grid where the water stands `WADING_DEPTH` (0.35 tile) over its band's middle, and opens again `PLAIN_MARGIN` (0.1 tile) lower, so a flood switches each group at most twice, all of a second's switches in one call. The floodplain shuts and opens whatever the map-wide flood is doing.
- **Troops on shut floodplain** are hurt and sent to dry ground as in a shut ford, and while a front is over them they are swept 10 tiles down the river from the river tile nearest them (30% drown), as in a ford. Troops on a wall stand above it. The selection panel says "In deep flood water: hurt every second, get to high ground".
- **Buildings** whose middle stands on shut floodplain lose `DROWNED_PERCENT` (2%) of their hit points a second and fall at none, leaving rubble; all that fall in one second come off the walkable grid together. Bridges are spared.
- *Simpler than the plan:* the bands decide nothing themselves; how high the water stands does, so a Flood can wet a low meadow and a long rain on a full river can too. No new map-wide band messages (stage 1a's Major and Catastrophic warnings stand). Siege engines are not yet kept off flooded fords, the floodplain is not yet shown when placing, and the minds keep off shut floodplain only because the walkable grid does (they don't yet stay off floodplain a front is coming down on). Rivers without a ford have no floodplain, as they have no water engine at all yet. Troops sent off shut floodplain go to the nearest open ground, as from a shut ford, which on a wide meadow can be across the river; the pathfinding thread's settling is the place to fix that for both.
- **Stress scenario.** `gc_sim/tests/floodplain.rs` runs a retreat through a flooding valley: three regiments on a low meadow when a catastrophic flood comes over it all reach dry ground within a minute of it shutting, none is left on shut ground, and the meadow opens again after.
- `SIM_VERSION` 68 (main reached 67 first). No golden moves: the river storm's Major wets about 290 floodplain tiles but shuts none, and nobody walks on them.

## Built: stage 3a, debris on the rivers

Built 2026-10-31. Stage 3 is split in two: 3a is the debris itself and the logs nature puts in; 3b adds the players' orders (Dump into the river, and the fell-in toggle on Clear), which need new commands.

- **The pool.** `rivers::debris`: 48 places (`POOL`), 8 kept for the wrecks of stage 4. When the other 40 are full the oldest grounded piece rots away first, then the oldest floating one. Only logs go in for now (mass 1); rafts, wrecks and rubble have their kinds and masses ready.
- **Logs from floods.** While a stretch's water stands Major or more over its line (half a tile, measured at its middle) and a Major or catastrophic front is running, it gives up a log from one of its wooded banks at once, then one every 30 seconds (catastrophic water: every 10). The stretches take turns going first, so when the cap is reached no one stretch always wins. At most 8 natural logs float at once. Each stretch keeps 4 wooded bank places, found when the match starts, one per quarter of its reach; a log comes off one that is still wooded. The woods stay (it is deadfall).
- **Logs from fires.** A wood burning out beside a river topples a log in one time in three.
- **Drift.** A floating piece goes tile by tile down the river's reach (the way the flood fronts run, so it rides the same water), where the flow points if that goes down the reach, else to the neighbour nearest the river's end. Its pace is the river's (half a tile a second slow, one steady, two fast), up to twice that as the water rises to a full flood. It leaves the map where the river ends.
- **Grounding and snags.** A piece checks whether it grounds as it comes onto each tile, and once a second wherever it is: it grounds on frozen water, under the line, or on a ford that isn't fast with under `FLOAT_RISE` (0.15 tile) of water over the line, and floats off once the water is up. Grounded mass of 4 or more on one ford is a snag: wading it is slow as deep water (`Terrain::snagged`; `Terrain::deep` and the walking pace share one rule, the selection panel says "In a ford snagged with logs") until a flood lifts them off. So logs pile up at fords after a flood and come down together in the next.
- **Strikes.** A floating piece passing within a regiment's block (half its side and half a tile) where the regiment is wading, not on a bridge or wall and not being swept away, takes off 3 hit points per mass per tile a second of its pace, throws it into disorder and shakes it (3 morale), once per piece (each piece remembers the last 6 regiments it struck). `Event::DebrisStruck` names the players struck; the news line tells ours at most every 10 seconds.
- **The minds.** A ford with a floating piece coming down on it within 24 tiles of reach joins the fords the computer players keep off (`Rivers::threatened`).
- **Drawn.** Each piece is the bare snag trunk lying on the water along its way, in wet bark, sitting lower once grounded (`gc_game::river_debris`), only where the viewer sees.
- *Simpler than the plan:* felled logs and rafts wait for 3b's orders. Snags slow wading rather than adding path cost (the walkable grid has no per-ford cost yet), and pieces never stick to each other in open water. Natural logs come from Major floods only as the plan says, so most matches see few.
- **Tests.** `gc_sim/tests/debris.rs`: a burnt bank wood topples logs that ride down and strike troops wading the lower ford, then ground there; a snag slows wading until a flood lifts it; a catastrophic flood tears logs off wooded banks with never more than 8 floating; the same seed sends the same logs down.
- `SIM_VERSION` 76 (main reached 75 first). Only the river-storm golden moves: its Major flood now washes logs down the Rushbrook (screenshots in project files, `screenshots/water-engine/stage3a-*`).

## Built: stage 3b, felling and rafts

Built 2026-10-31.

- **Fell in** (`Command::Fell`, `clearing::Work::Fell`): the Clear order, but woods and thicket beside running river (any of the 8 tiles round them) fall into it: a log a woods tile, two a thicket tile, no timber, half as long again (`FELL_PERCENT` 150). Tiles away from the water are cleared and paid as usual. Only factions whose clearing fells (`ClearStyle::fells`): the Clans. The Concord's clearing burns, and a burnt-off bank tile topples a log in one time in three, as a fire's does. The Choir and the Brood drop nothing.
- **Raft** (`Command::Dump`, `clearing::Work::Dump`): a worker walks to a running river tile along a bank (one with land beside it, so a worker on the bank can reach it; `Rivers::bank_river`), works 6 seconds (`DUMP_SECONDS`, at the match's tempo) and the raft goes in, paid then (`RAFT_GRUB` 12; refused when ordered without it, and nothing happens if the Grub is gone by then). Any faction. A raft is mass 3: three times a log's blow, and a snag on its own when it grounds.
- **Share:** each player may have at most 16 pieces they put in on the water (`SHARE`); the pool's 40 open places and its rot rule hold as before. Pieces remember who put them in (hashed).
- **Shown:** workers' card has Fell in (`Z`, Clans only) and Raft (`C`, 12 Grub, greyed without it), on maps with a river and only when the selection is all workers (those keys are Brace and stakes for soldiers). Hints and tooltips say what they do; a refused raft says whether the click missed a bank or the player's 16 pieces are already afloat. A second raft goes to a selected worker not already at work. Each kind of work keeps its own progress on a tile, so a feller and a clearer on one tile don't finish each other's work. A raft is drawn as three paler trunks lashed side by side.
- **Formats:** replay `FORMAT` 51 (tags 46 and 47, and whether a worker fells), network `PROTOCOL` 44 (tags 44 and 45), `SIM_VERSION` 77 (3a has 76). The two Conquest goldens and the river storm move (pieces now carry who put them in); the other field goldens don't.
- **Goldens:** a new one, `conquest-river-1v1` (gc_game `river_golden.rs`): three minutes on grown land, two Clans sides each fell the bank woods nearest their start and tip a raft into the nearest river. The Ribcage golden's Clans clear with Fell in (no river there, so it clears as before).
- **Tests:** `gc_sim/tests/river_work.rs`: felling drops only the bank row and pays only the rest; plain clearing drops nothing and a faction that doesn't fell is refused; a raft costs its Grub and rides the current; no raft without the Grub.
- *Not yet:* the computer players don't fell or dump (stage 5), and the share isn't per team.

## Built: stage 4a, bridges in the water

Built 2026-10-31.

- **Piers** (`gc_sim::rivers::debris::piers`): each second the world hands the rivers every standing, finished bridge tile that isn't on a bank (`Structures::piers`: tile, slot, `pier_level` 1 to 3 and hit points left); those over running river are piers.
- **A piece reaching a pier** jams on it (40%, 30% or 10% by level; a jam holds up to 12 mass, and a piece that would jam on a full one passes over it) or strikes it: 6%, 4% or 1.5% of the tile a mass point at a steady pace, times the square of its pace (up to four times in a full flood), and goes on under. A jammed piece lies on the tile above, against the pier, and is drawn low like one aground.
- **Jams** choke their stretch's outflow, 5% a mass point up to 60% (`Rivers::choke`), so the water backs up above them. While the water is Minor or more a jam pushes on its tile (0.1% a mass point a second at Minor, 0.2% in a Flood, 0.4% from Major; braced two thirds of that, reinforced a third). When the water falls back below Minor a jam of 3 mass or less washes free.
- **High water:** from Major up a tile under 75% of its hit points loses 0.5% a second (braced 0.12%, reinforced nothing); catastrophic water sweeps away a plain tile under half its hit points or a braced one under a quarter.
- **A tile that falls** (worn, struck or swept to nothing; not one taken away when its player is put out) lets its jam go, and if it stood over running river its wreck (mass 2, drawn as two grey deck timbers) floats on down and can strike the rest of its own bridge and the next bridge down. A bank tile leaves no wreck.
- Pieces held in a jam don't count towards a player's 16 on the water, nor towards the 8 logs floods may have afloat at once.
- Harm to bridges goes through the same path as fire and attacks (`wear_down`), so a fallen tile shuts and reopens the river's crossings as before.
- **Formats:** `SIM_VERSION` 78; no new commands, so `FORMAT` and `PROTOCOL` stay. The river Conquest golden and the river storm move; the others don't.
- **Tests:** `gc_sim/tests/river_bridges.rs`: rafts reaching a bridge strike or jam on its piers; a fallen bridge tile floats on as a wreck, a bank tile doesn't; a sound bridge stands through a catastrophic flood with nothing in the water; a catastrophic flood tears logs off the banks that batter the bridge below; rafts in a flood do far more harm than in a steady river.
- *Not yet (stage 4b):* clearing and burning jams, the burst when a jammed tile falls, news of strikes and jams, computer players clearing and repairing, and the stress scenarios.

## Built: stage 4b, clearing jams

Built 2026-10-31.

- **Clearing a jam:** the Clear (or Fell in) order takes jams too: drag over the river tiles just above a bridge where debris lies jammed. A worker on the deck or the bank beside it pulls the pieces out one at a time, 4 seconds a mass point (`UNJAM_SECONDS`, at the match's tempo), and each piece dragged ashore pays 1 Grub of timber (`UNJAM_GRUB`). Overgrown tiles and jams in the area are worked nearest first. Two workers on one jam work the same piece; the second to finish finds it gone and starts on the next. Jammed pieces are never pushed out of the pool to make room for new ones. An area with nothing growing and no jam is refused as before (`World::clearable`).
- **Jams hold in steady water.** A jam of 3 mass or less washes free only when the water at its stretch falls back below a Minor flood after being up (stage 4a let small jams go every second in steady water, so rafts sent one at a time could never build a jam). Each jammed piece remembers whether the water at its pier has been up since it jammed or last settled (hashed); a bigger jam that holds as the water falls settles and waits for the next rise.
- **News:** when debris jams on a player's bridge they are told (`Event::BridgeJammed`), at most once every 30 seconds: "Debris has jammed against one of our bridges: send workers to Clear it before the river rises".
- **The burst:** a jam holds back its stretch's outflow, so the water there stands higher; when the jam lets go (cleared, washed free or its tile falls) the held water runs on down at once. No separate burst front is added.
- **Formats:** `SIM_VERSION` 79; no new commands. No golden moves (none has a bridge over a river, so the new rules never come into play there).
- **Tests:** `river_bridges.rs`: workers clear a jam a piece at a time for its timber (and the owner is told it jammed); small jams wash free when a flood falls back; jams hold in steady water.
- *Not yet (stage 4c):* burning jams, computer players clearing jams and repairing bridges, and the stress scenarios.

## Built: stage 4c, burning jams

Built 2026-10-31.

- **Burning a jam:** the Burn order (E) from a regiment on the deck or bank beside a jam can set the jam alight: it takes the nearest of the woods and the jams in reach (`Fires::tinder` with `Rivers::unlit_jams`), and a jam catching is told as woods catching are (`Event::Kindled`). It won't light in wet weather, as woods won't. The fire belongs to the jam (`Debris::fires`, by pier), so pieces that jam on it later burn too; its pieces burn down one at a time, 3 seconds a mass point (`BURN_SECONDS`). While it burns the timber deck above loses 1% of its hit points a second whatever its level (`DECK_BURN_PER_TEN_THOUSAND`; fire goes for the deck, not the piers), so a full 12-mass jam burnt off costs a sound tile over a third of itself: fast, but costly. Rain puts it out (the same dampness that stops woods catching). A jam that washes free or lets go floats on put out. The deck's own burning stops with the jam (timber decks don't keep burning on their own).
- **Shown:** a low blaze on the water at each burning jam (`Rivers::jam_fires`); the Burn tooltip and hint say it can light a jam beside a bridge.
- **Formats:** `SIM_VERSION` 80; no new commands. The burning jams are hashed after the rest of the debris, and only while there are any, so no golden moves.
- **Tests:** `river_bridges.rs`: a jam set alight burns away and takes a slice of the deck with it.
- *Not yet (stage 4d):* computer players clearing jams and repairing bridges; then the stress scenarios (4e).

## 7. Stages (each its own pull request)

1. **Rivers with water.**
   - **1a, the sim.** Stretches, store and stage, rain, snow and summer by stretch, flow down the river, the per-tile water layer refreshed each second (`Terrain::wet` and `depth` read it), per-stretch ford groups switched in one batch, bursts as fronts with a heavy-tailed crest (replacing the fixed flash wave), bands Minor and Flood, the map-wide warnings. The minds move from all-fords to per-stretch reading. A parity test: a single river with today's numbers shuts and opens its fords as today's wave does. A new golden: changing weather with seasons on a grown river map. `SIM_VERSION` bump; the goldens on old maps don't move (they have no flow layer, so no stretches). No new commands, so no FORMAT or PROTOCOL bump.
   - **1b, reading it.** River names, the gauge, the top bar chip, river watch and the forecast messages, the water drawn per stretch, guides.
2. **Major and catastrophic floods.** The floodplain list and its groups, wading and shut floodplain, buildings hurt, siege engines kept off flooded fords, the floodplain shown when placing, the band messages, minds keeping off the floodplain. Stress scenario: retreat through a flooding valley.
3. **Debris.** The pool, logs from felled and burnt bank woods and floods, rafts dumped (the Dump order, F toggle on Clear), drift along the reach, stranding and the spring flush, snags at fords, hitting troops, minds steering clear. The data for drawing it. New commands, so `FORMAT` and `PROTOCOL` bump, and a golden that uses them (the goldens must cover every kind of command). Stress scenario: a raft down a ford full of wading troops.
4. **Bridges in the water.** Piers, strikes, jams and choke, washing free, clearing and burning jams, wrecks and the cascade, strain on hurt piers and sweeping by catastrophic water, pier levels from the bridge thread, minds clearing and repairing. Stress scenarios: a column on a bridge while debris and a flood arrive; two armies on one bridge in a flood. T6 reviews.
5. **Computer players use the river.** Timing crossings to crests, dumping and felling against the enemy, upgrading threatened bridges. Arena runs.
6. **Dams** (idea 24), with their own design note and real-use section first.

Each stage updates the Guide, Quick Start, tooltips and docs/quick-reference.md in the same PR. The drawn water (stages on the water surface, foam at jams, logs) is generated from the sim data: the water level per stretch goes to the renderer's water, logs reuse the felled-trunk scenery, and the bridge kit adds a jam pile.

## 8. Decisions (Mario's rule: pick the recommended option, with an Opus check)

- **Per stretch, not per tile.** Water per tile (a real fluid grid) would look clever but costs memory and time all match and is hard to keep deterministic and readable. Stretches of 16 tiles give water that travels, backs up and differs along a river, at a cost of a few dozen numbers a second. Lakes stay one level.
- **Floods are emergent, with a heavy-tailed storm burst on top.** Floods come from what fell on what ground in what season, so they vary naturally; the burst roll gives the rare catastrophe without a random stream.
- **Debris is a small pool, not scenery.** 48 pieces at most, owned shares capped. That keeps the tick cost tiny and stops spam, while still letting one well-timed raft bring down a bridge.
- **Dumping costs Grub and needs workers at the bank.** It is a deliberate act with a cost and a body at risk, not a click from across the map.
- **Bridges are what floods fight over.** A deck stays open in any water until it falls, so floods move the battle onto bridges rather than stopping it.
- **Fixed weather and maps without a flow layer don't change.** No golden on today's maps moves; new goldens cover the engine.

## 9. Risks

- **Budgets.** The engine's state is a few dozen stretches, a floodplain list built once and 48 pieces. The tick work is per stretch once a second and per piece per tick. The `field-8000pt` heap is tight (about 7.95 of 8 MB), but that map has no flow layer, so nothing is allocated there. The bench gate and conquest-6p laying budget are checked each stage.
- **Grid churn.** Floodplain and ford groups switch with a margin, at most twice per flood each, as today's groups do.
- **Readability.** Every effect has a message, a gauge or a drawn sign before it hits, and the forecast comes from the same numbers the simulation uses.
- **Balance.** Dumping must not become the best use of Grub. The pool share, the 20 Grub price and the jam chances are the knobs; the arena runs check it once the minds use it.
- **Fairness up and down the river.** Grown maps are turned copies of one half, so every player has the same rivers above them as their opponent. Six-player maps and hand-made maps get checked when stage 3 lands: dumping must not belong to whoever happens to start upstream.
- **The walkable grid's cost.** Each switch re-blocks every side's grid, and cutting an area off can renumber it. Batching each second's switches into one call keeps that to a few per flood, and the no-allocation-in-tick test is run on a flood each stage.
- **Order with the bridge thread.** Stage 4 needs pier levels; until they land, every pier is level 1 and the levels are added in one line.
