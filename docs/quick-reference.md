# Grubcraft: Player Quick Reference

Keep this file current: any PR that changes something a player sees updates it in the same PR, like the in-game guides.

Answer "how do I do X in the game?" from this file first. Facts come from `docs/` and the code (checked 2026-10-31). Section 8 lists what is not built yet; check it before promising a feature. Conflicts between sources are at the end of section 8.

Terms: **Grub** (common, from seeps), **Marrow** (rare, from veins). **Regiment** = a block of soldiers. **Worker** = Hands, Gnawers, Tuners, Ledger Clerks.

## 1. Controls and hotkeys [docs/08-ui-ux.md, crates/gc_ui/src/hud.rs (HELP_GROUPS), crates/gc_game/src/main.rs]

**Command grid** (bottom-right, 4 x 3): `Q W E R / A S D F / Z X C V`.

**Regiments**
- Left-click selects; drag a box; `Shift` adds or removes. Double-click select by type: not built.
- Right-click ground: move. Right-click enemy: attack. Right-drag: draw a line (position, frontage, facing). Alt+right-drag: a deep line (the drag is the front, each regiment keeps its frontage and the rest stack behind, as the Deep shape), to fill a gate or bridge.
- `Shift`+right-click: queue a move or an attack after the regiment's current orders (up to four; more are refused). Each player can have 32 regiments with queued orders or on an attack-move at once; beyond that a queued order is refused and an attack-move marches plainly. Any order without `Shift` replaces the queue. A dotted line runs on through the queued stops (red for advances).
- `Q` then click: move. `W` then click: attack; a click on open ground advances there instead. `A` (Advance) then click: attack-move, which marches but stops to fight any enemy within 12 tiles, then marches on. `Shift` on the click queues it and keeps the tool armed. `Esc` cancels. While a hero panel is shown (`Shift+H` test), `A` is the hero's first ability instead.
- Every move shows a fading plate at once where the regiment will stand.
- A regiment sent back the way it faces (more than 135° round) turns about where it stands, the rear rank becoming the front, and marches off a third of a second later; told to face back the way it came at the end, it turns about again. Up to 90° off it wheels as it marches; between 90° and 135° it turns on the spot first.
- `R` cycles formation: Line, Tight, Loose, Shield wall (shields only). `F` does the same while no hero owns `F`.
- `Z` Brace (spears and pikes; again lifts). `V` Fall back (8 tiles at half pace). `E` Burn (set nearby woods alight). `X` Torches (night only). `C` Plant stakes (Conquest).
- `O` cycles army shape for group moves: Line, Deep, Hammer, Square, Plain, then saved Mine 1 to 3. `Shift+O` saves the selection's shape.
- Control groups: `Ctrl`+`1` to `0` set; `1` to `0` recall; `Shift`+number adds; double-tap centres.
- `Y` rides with the first selected regiment (banner view); `Y` or `Esc` returns.
- `Space` jumps to the latest alert (red `!` losing men, orange `A` ambush, pale `R` routing).

**Workers** (Conquest; workers selected)
- Right-click a seep or vein: gather there. Right-click your unfinished or damaged building: build or mend it.
- `D` lays a new main building. `F` or `B` opens the build menu; pick a building, click ground. `R` or wheel turns it a quarter turn; `Shift+R` turns back. `Shift` keeps the tool; `Esc` puts it down; `Esc` again closes the menu.
- `S` then drag: clear woods, scrub or thicket (area up to 16 x 16).
- Clans workers: `Z` (Fell in) then drag: clear, felling bank woods and thicket into the river.
- `C` (Raft) then click running river along a bank: a worker tips in a raft (12 Grub). Workers only, maps with a river.
- `A` Work plans (build your planned walls). `F1` selects the next idle worker; with none idle, `F1` opens the Marshal's Scroll.

**Wall tool** (armed from the build menu)
- Press and drag to draw a smooth curve; let go to order it. Click a start, click again for each sharp corner; right-click or `Esc` ends. `Shift` keeps lines straight, in 15-degree steps.
- Bridge: drag bank to bank (gentle curve, no corners); `[` and `]` pick narrow, standard or wide (3, 4 or 5 tiles).
- `B` plans a blueprint (paid later). `O` ring tool: click corners, or drag round for a smooth loop; `Enter` or the first corner closes it.

**Selected building** (Conquest card)
- The grid shows what it trains (`Q` first), with prices. A queue holds five; clicking the queue removes the last one.
- Right-click ground: rally point.
- `V` (bottom-right): main building's next tier ("Tier 2" or "Tier 3", with price); "Gatehouse" on a finished gate; "Cancel" (75% back) while going up or upgrading. See section 3.
- Selected gate: `Q` Auto, `W` Locked, `E` Open. Selected wall: `U` raises its stretch a level. No wall selected: `U` opens the Guide.

**Camera**
- Arrow keys pan. Wheel zooms. `Alt`+middle-drag rotates. `Home` resets rotation. `,` and `.` turn the view.
- `F8` director camera (any camera move turns it off). Far zoom switches to Commander View (automatic).

**Help, menus, sound**
- `H` or `F1`: Marshal's Scroll (key list and live notes). `Esc`: menu (Resume, New sandbox during a match).
- `M` mute all; `N` music only. `L` AI lens. `G` shows ground no regiment can cross. `C` credits (outside Conquest, or when no selected unit can plant stakes).

**Faction power buttons** (row under the top bar; press, then click a target)
- Choir: `F2` Overclock (20 Resonance). `F3` Phase Shift (60 Resonance; teleports selected regiments to one of your Resonators).
- Ember Clans Edicts (Honor): `F2` Rice Tax (25, +300 Grub). `F3` Forced March (20; press with regiments selected). `F4` Banner of the Ancestors (35; click ground; nearby regiments cannot rout 20 s). `F5` Ancestral Return (80, tier 3; click a finished Shrine). Needs a finished Ancestor Shrine.

**Sandbox** (main menu Sandbox, or `grubcraft --sandbox`)
- `B` Place panel (click a troop to place). `[` `]` choose type. `K` places buildings. `Delete` removes under the mouse; `Shift+Delete` clears the field.
- `Tab` switches side. `I` hands that side to the computer (cycles levels). `P` pause. `-` `=` speed (0.1x to 4x).
- `T` tunes the chosen unit (arrows; `Shift` for ten steps; `H` hides). `Shift+R` reloads `data/units.ron` (dev build only).
- `Shift+V` shows hidden regiments. `Shift+H` test hero panel.

**Debug and look keys** (anywhere)
- `F9` light presets. `F12` weather (Clear, Rain, Snow, Fog). `F10` light-tuning panel (developer tool). `F11` or backquote: dev overlay.

**Command-line flags** [crates/gc_game/src/main.rs]
- `--sandbox`; `--conquest ribcage_pass|four_teeth|land-random|land-mixed|...`; `--battle recruit|veteran|general|warlord`; `--host`; `--join ADDRESS`; `--password PW`; `--menu`/`--no-menu`; `--map NAME-or-FILE`; `--seed N`.
- `--showroom all|buildings|NAME[,NAME]`; `--showroom-clip idle|walk|attack|shoot|cheer|die|march|look|kill|finish|blows|reel`; `--viewer NAME|start`; `--siege`; `--gate-march N` (a regiment through each gate, looking at the Nth); `--watch REPLAY`.
- Look and display: `--time-of-day`, `--sky`, `--look`, `--weather`, `--air`, `--no-audio`, `--no-shadows`, `--no-aa`, `--no-glow`, `--draw`, `--lod`, `--render-scale`, `--latency`, `--overlay`.

**Not in the game:** key rebinding, ping wheel (`G` is blocked-ground view), text chat, Hold (`E` is Burn), Run, Column and Wedge formations, `Tab` sub-groups.

## 2. Economy [docs/04-economy.md, docs/03-factions.md, docs/21-buildings.md]

- **Grub:** from seeps. A seep holds 7,500; then dregs at 25% of the rate, for good. Pays for most things.
- **Marrow:** from veins (3,000 each, no regrowth, dregs at 25%). Pays for tier upgrades, elite units, monsters, hero revival and most research.
- **Top bar:** Grub, Marrow, Supply used/cap, clock, and a line "Upkeep 80%" in amber once upkeep bites. A faction meter sits beside them.
- **Harvest:** five Hands on a seep near the hall give about 210 Grub a minute; a vein (3 workers) about 50 Marrow a minute at Standard tempo. Hands fill 10 Grub in about 7.2 s; a Marrow load (5) takes about 9.6 s. Choir: one Resonator (175 Grub) tunes one seep for about 109 Grub a minute, and a Siphon one vein for about 40 Marrow; chords add 10% to 30%.
- **Upkeep** (Grub only): supply used 0 to 60 = 100%; 61 to 100 = 75%; 101 to 150 = 55%.
- **Supply:** every faction starts at a cap of 12, raised by housing up to a hard maximum of 150. Housing: Farmstead +10 and Tenement +10 (Clans and Concord), Pustule +8 (Brood), Resonator or Siphon +4 (Choir), Outpost Hall +6 (Clans). Each main-building tier adds +10.
- **Supply per unit:** worker 1, light 3, line 4, elite or cavalry 5, artillery 4, air 3, monster 6, hero 5. Training waits in the queue while there is no room.
- **Workers:** Ember Hands 50 Grub; Brood Gnawers 50 Grub + 1 larva; Choir Tuners 60 (build and repair only); Concord Ledger Clerks 50 (Marrow only); Concord Caravan 75. Repair is free (Hands and Clerks 8 HP/s; Tuners' 10 not built).
- **Starting state:** Clans Clan Hall, 5 Hands, 250 Grub. Brood Hive, 4 Gnawers + 3 larvae, 300 Grub. Choir Spire + 1 Resonator, 3 Tuners, 300 Grub. Concord Counting House, 4 Clerks + 1 Caravan, 350 Grub.
- **Tempo:** Standard (100%); Swift (gather 150%, build and train 75%); Epic (gather 80%, build and train 120%).
- **Tier costs:** tier 2 = 300 Grub + 100 Marrow, 60 s; tier 3 = 600 Grub + 250 Marrow, 90 s. Tier 1 needs no Marrow.

## 3. Buildings [docs/21-buildings.md, docs/04-economy.md, crates/gc_game/src/conquest_card.rs]

**How to build**
- Select workers; `F` or `B` opens the build menu; pick, then click ground. `D` lays a new main building.
- A foundation starts at 10% HP. Up to four workers build it (the first full speed, each extra half). Nothing works until finished.
- Cancel refunds 75%. A destroyed building refunds nothing. Overgrown ground is cleared first and paid when ordered.
- Choir buildings (except Spire, Resonator, Siphon) must stand in a Choir field. Tier: the main-building tier needed to place it.
- Hover a build-menu button for its tooltip: a one-line blurb (what it does, what it unlocks or trains, when to build it), then its key. Blurbs live in `data/guide/*.ron` as `blurb`.

**Main building tiers**
- Clans: Clan Hall, Castle Keep, Daimyo's Citadel. Brood: Hive, Great Hive, Throne Hive. Choir: Spire, Cathedral, Grand Organ. Concord: Counting House, Exchange, Grand Bourse.
- Upgrade: select the finished main building, press `V`. It keeps working while upgrading. While upgrading the same slot reads "Cancel" (75%). Gates upgrade with `V` to "Gatehouse" (Yagura Gatehouse, Gate of Debts, Chitin Maw). The Choir builds no gates.

**Buildings per faction** (tier, cost Grub/Marrow)
- **Ember Clans:** Clan Hall T1 400/0; Castle Keep T2 300/100; Daimyo's Citadel T3 600/250; Outpost Hall T1 250/0 (cheap, trains nothing); Farmstead T1 80/0; Granary T1 100/0; Dojo T1 150/0; Stables T2 180/40; Ember Workshop T1 160/0 (rams; rocket carts at T2); Kite Loft T2 160/60; Ancestor Shrine T1 150/0; Forge T1 175/0; Watchtower T1 125/0; Fire-Arrow Watchtower T2 75/50 (Watchtower level 2); Ballista Yagura T1 175/0 (bolt tower); Great Ballista Yagura T2 105/50 (Ballista Yagura level 2); Horoku Tower T2 200/50 (blast tower); Great Horoku Tower T2 120/75 (Horoku Tower level 2); Palisade T1 10/0 per tile; Gate T1 60/0.
- **Brood:** Hive T1 300/0; Great Hive T2 300/100; Throne Hive T3 600/250; Pustule T1 40/0; Hatchery Pit T1 120/0; Chrysalis Chamber T2 150/50 (evolutions); Moth Roost T2 150/40; Gorge T2 180/60; Spine Mound T1 100/0; Spine Bastion T2 60/50 (Spine Mound level 2); Harpoon Maw T1 160/0 (bolt tower); Harpoon Gullet T2 95/50 (Harpoon Maw level 2); Acid Bloater T2 190/50 (blast tower); Acid Gorger T2 115/75 (Acid Bloater level 2); Queen's Chamber T1 140/0; Resin Ridge T1 8/0 per tile; Maw Gate T1 55/0.
- **Shard Choir:** Spire T1 400/0; Cathedral T2 300/100; Grand Organ T3 600/250; Resonator T1 175/0; Siphon T1 175/0; Forge of Facets T1 160/0; Harmonics Lab T1 175/0; Skyglass Loom T2 160/50; Bell Tower T1 150/0; Great Bell Tower T2 90/50 (Bell Tower level 2); Prism Lance T1 190/0 (bolt tower, needs power); Prism Spire T2 115/50 (Prism Lance level 2); Chord Obelisk T2 200/50 (blast tower, needs power); Choral Obelisk T2 120/75 (Chord Obelisk level 2); Sanctum of Echoes T1 150/0; Light Curtain T1 10/0 per tile.
- **Carrion Concord:** Counting House T1 400/0; Exchange T2 300/100; Grand Bourse T3 600/250; Waystation T1 200/0; Tenement T1 80/0; Reliquary T1 75/0; Ossuary T1 150/0; Guildhall T1 160/0; Lantern Spire T2 160/50; Tollgate T1 140/0; Toll House T2 85/50 (Tollgate level 2); Collector's Bolt T1 185/0 (bolt tower); Debt Collector's Bolt T2 110/50 (Collector's Bolt level 2); Corpse Catapult T2 210/50 (blast tower); Ossuary Catapult T2 125/75 (Corpse Catapult level 2); Vault of Names T1 150/0; Bone Fence T1 12/0 per tile; Iron Gate T1 70/0.

**Walls** [docs/26-fortifications.md]
- Level 1 cheap; level 2 adds a walkway (about 15 Grub a tile more); level 3 adds battlements and the faction twist (20 Grub + 5 Marrow a tile more, needs tier 2).
- Ember: Palisade, Stone Wall, Castle Wall. Concord: Bone Fence, Ossuary Wall, Mausoleum Wall. Choir: Light Curtain, Glass Wall, Singing Wall. Brood: Resin Ridge, Chitin Wall, Thorn Bulwark.
- Wall looks: the Palisade, the Stone Wall and the Concord's three walls are built from kits along the drawn curve (log or bone runs, posts at the joints, tall end posts where the wall stops, broken runs below half hit points, charred Palisade logs below a fifth, braces behind the Palisade). The Stone Wall is the palisade on a stone footing with iron bands and a walkway behind, where men manning it stand. Fronts face away from your nearest hall. Other walls are still plain slabs.
- Drawing walls: drag for a curve; turn sharply mid-drag (over 40 degrees) for a corner; click for a straight piece from the last end. Bridges bend gently only.
- Raise a stretch with `U`; a worker raises it tile by tile.

## 4. Factions [docs/03-factions.md]

- **Ember Clans** (Shogun): Hands carry Grub to the hall or a Granary. **Honor** meter: +5 when an enemy regiment routs against yours, +10 for a win while outnumbered, +2 a minute per Shrine held, -5 when yours routs. Spent on Edicts. Tight formation gives +15% defence when still.
- **Brood** (vein network): Gnawers burrow into seeps and send Grub home through root-veins; cut a Pustule and seeps beyond it stop. **Larvae** meter: each Hive or Hatchery Pit holds 3, makes one every 12 s; every regiment hatches from one. Gnawers fill a load in 87% of the time (`gather_percent: 115`). A swarm: Brood regiments are cheap and each soldier is weak, so a Brood army has about twice the soldiers of an equal-worth one. Tier-1 regiments evolve on the field (15 s cocoon).
- **Shard Choir** (chords): Each Resonator tunes one seep with no workers. Three Resonators linked within 14 tiles (not in a line) form a Chord, adding +10% to +30% Grub to the seeps they tune there. Chords charge **Resonance** (cap 100). Most Choir buildings need power from a Spire (8-tile field) or Resonator (6 tiles).
- **Carrion Concord** (trade and undeath): Caravans carry Grub from the Counting House to a trade post: a neutral hamlet (full), your Waystation (half), or an allied main building (full, ally gets 20%). Pay: 37 Grub + 1.1 per tile of straight-line distance (about 81 at 40 tiles). **Tithe** meter: each death within 12 tiles of a Reliquary pays 1. The dead do not rout.
- **Heroes:** up to three per faction, not in the game yet (section 8).
- **Field Battle generals:** each side's general rides with one regiment. Select it to see "Your general rides here: guard him". His death panics nearby regiments. [docs/37-commander.md]

## 5. Combat and fortifications [docs/05-combat.md, docs/26-fortifications.md, docs/21-buildings.md]

- A regiment is a block of models; the back rank steps up as the front falls, so it hits less as it shrinks.
- Hits from the flank do x1.4 damage; from the rear x1.8.
- **Frontage:** only soldiers touching the enemy strike: the front rank (spears also the second, at half), rank ends on a flank, half the front from the rear. Each side of a block has room for as many strikers as its length, shared by every attacker, so a swarm can't pile six regiments onto one front. Gates and bridges hold: only the front can be reached. Attackers who can't reach go round to a side with fewer on it, if their block fits there (not in a gate or on a bridge).
- **Routing:** a regiment can't waver or rout until it is down to its breaking point: about 45% strength for levies (morale under 55), 40% for line troops, 35% for elite (morale 70+), give or take 5, and 5 lower with its general near. Workers can break at any strength. Charges and flank attacks break regiments by killing men faster. The tick on a regiment card's health bar marks it.
- **Veteran ranks:** a regiment earns ranks by killing enemy soldiers, weighed by their hit points against its own: rank 1 at its own worth killed, rank 2 at twice, rank 3 at four times; volley kills count half. Each rank: +10% damage, breaking point 5 lower. Ranks never go. Gold chevrons over the banner, on the card and on the selection banner ("Veteran rank 2").
- **Orders and why:** the selection panel says what each regiment is doing (Holding, Marching, Forced march, Attacking, Advancing on an attack-move, Fighting, Braced, Falling back, Running), how many orders are queued behind it, with a note when it is held up (pinned, driven back, re-forming ranks, filing through, trapped). Mixed selections say "Mixed orders". A wavering or routing regiment adds a "Why:" line with up to two reasons, most serious first. Hover any regiment for a card with its order, morale and why (your own also shows its route).
- Charges need about a second of marching (about 3 tiles for infantry). A charge into braced spears' front fails.
- Brace (`Z`): stops the regiment; spears and pikes only. Fall back (`V`): each locked enemy gets one parting blow; routing regiments cannot fall back.
- Woods and tall grass hide regiments. Your hidden regiments show a moss-green flag.
- Marching pace: woods 85% (heavy units 70%), soft ground 75% (heavy 60%), wading 60%. Routes go round woods and soft ground when that is quicker. Friendly blocks keep about half a tile apart and step round a friend standing in the way; only overlapping a friend ahead slows you, and overlapping past a tile throws both into disorder.
- Melee hits buildings with the front rank; blows on buildings always land. Fortified buildings take half from regiments. Siege (rams, Mausoleum Engine) take x3.0 vs Fortified. Right-click an enemy building to attack it.
- Right-click an enemy with a tower selected to focus it.
- **Drawn walls:** wall tool; built tile by tile; snaps to wall ends, towers and gates within 1.5 tiles.
- **Gates:** Auto (opens for you and allies), Locked (shut to all), Open (held). Enemy routes treat a shut gate as wall.
- **Manning:** with foot or archers, right-click your raised (level 2 or 3) wall to man it. Right-click an enemy raised wall to climb ladders; `Ctrl`+right-click sieges it instead.
- **Bridges** (drawn bank to bank over water along a gentle curve, three to five tiles wide, 3 Grub a tile; the land remembers every line): Timber Bridge (Clans), Bone Causeway (Concord), Glass Span (Choir), Chitin Span (Brood). All four are built from the timber kit, tile by tile: a planked deck (patched, weathered, broken when badly hurt), rope rails and posts on its outer edges, log cribs on the banks and timber piers in the water reaching the river bed; level 2 adds braced piers, timber rails and lantern posts; level 3 stone piers with cutwaters. Raise a finished bridge with `U` like a wall: level 2 braced piers (3 Grub, 3 s a tile, 600 hp; tier 1), level 3 reinforced piers (4 Grub + 1 Marrow, 4 s a tile, 1000 hp; tier 2). Names: Braced Timber Bridge / Stone-Footed Bridge, Braced Bone / Iron-Shod Causeway, Braced / Anchored Glass Span, Ribbed / Rooted Chitin Span. The AI raises its own bridges when rich.
- **Field works:** Stakes and Barricades, drawn with the wall tool. Spearmen and archers plant stakes with `C` (about 10 s).
- **Hidden traps** (one per faction; only its owner sees it): Spike Pit (Clans), Grave Hands (Concord), Shard Mine (Choir), Burrow Nest (Brood).
- **Towers that fire:** Watchtower, Tollgate, Spine Mound, Bell Tower (range about 30 to 32).
- **Bolt towers** (tier 1, no Marrow; range 32 to 34): one heavy bolt every 5 s (6 s for Harpoon Maw) that runs down a single file of soldiers, never killing deeper than the target's ranks. Twice as hard on big (heavy) soldiers: monsters, riders, engines. Shoot siege first, then big soldiers, then whoever attacks your buildings, then the nearest. A shield wall does not stop a bolt. Weak against wide lines of small soldiers; strong against columns, monsters and cavalry.
  - Ember Clans: Ballista Yagura (175), no quirk.
  - Carrion Concord: Collector's Bolt (185): each soldier it kills pays you 3 Grub.
  - Shard Choir: Prism Lance (190, needs power): the beam runs on through up to 3 more regiments in line behind.
  - Brood: Harpoon Maw (160, range 32): drags the regiment it hits 3 tiles toward it and halts it, unless that regiment is in melee or on a wall.
- **Blast towers** (tier 2, 50 Marrow; range 36): six shots every 6 s (Chord Obelisk 5 s) into the fullest enemy block in range; the more packed, the harder (up to 1.75x). Half splashes on up to 4 other enemy regiments within 2 tiles of the target. Siege first. They cannot hit anything within 8 tiles of them, so guard them with troops. A shield wall does not stop them.
  - Ember Clans: Horoku Tower (200 + 50): fire-pots leave burning ground (4 tiles across) for 8 s and set woods and scrub alight.
  - Carrion Concord: Corpse Catapult (210 + 50): every 10 soldiers it kills rise beside it as 10 Bone Pikes, yours for 30 s.
  - Shard Choir: Chord Obelisk (200 + 50, needs power): every regiment its burst hits is thrown into disorder.
  - Brood: Acid Bloater (190 + 50): leaves an acid pool (4 tiles across) for 6 s.
- **Tower levels** (tier 2): select a finished tower and press `V` to raise it to its level 2. It takes 30 seconds and keeps shooting while it upgrades. The level 2 has 1.5 times the hit points, about 20 to 30% more damage a shot and 4 tiles more range, with the same reload, shots, shot and quirk. It costs 60% of the tower's Grub plus 50 Marrow (75 Marrow for a blast tower). The Fire-Arrow Watchtower's fire arrows set woods and scrub alight where they land. Computer players of General level and up raise their towers once they reach tier 2 with Grub and Marrow to spare. Until a level 2 has its own model, it looks like its level 1.
- **Walled Town** (docs/26 stage 8): when your walls, gates, cliffs and your own buildings in the line close all the way round your main building (within 40 tiles of it), your base is a Walled Town. A gate set Open, a wall piece down or an unpowered Choir curtain breaks the ring. The ring is checked every 2 seconds. While it holds, your buildings inside take a quarter less from ordinary regiments' blows (siege engines, artillery and heroes hit as before). Your regiments inside lose 1 point less morale a second, as if a general were near, and each wounded soldier of a regiment resting inside (not fighting) gets 1 hit point back a second. The top bar shows "Town: Walled", a message says when the ring closes or is breached, and a selected regiment inside says "In its walled town". Computer players of General level and up carry their front wall on round home as a rectangle at tier 2, always leaving their front gate as the way out, and once it has closed mend a breach before any other work. A bridge deck in the line is a gap.
- Burning ground and acid hurt only the owner's enemies, once a second, and shake morale like fire.

## 6. World: weather, seasons, fire, floods, rivers [docs/32-changing-land.md, docs/38-living-waterways.md]

- **Time of day** (lobby): Golden hour (17:00), Noon, Overcast (12:00), Dusk (18:00), Night (23:00).
- **Weather** (lobby): Fair, Changing (a new spell every 3 to 6 min; the default since 2026-10-30), Rain, Snow, Fog. Fog cuts sight up to 60%; snow 30%; rain 20%; night 45%. Snow slows marching up to 15%. The top bar shows the cut (for example "Fog: sight 40%").
- **Lightning:** a storm (rain 192+; the lobby's Rain is 200) throws a bolt about every 10 s somewhere on the map, likeliest on woods or thicket. It sets the tile alight only while the ground is dry (the first ~30 s of a storm over dry land), so it burns small patches, mostly in summer. Flash and thunder within 160 tiles of the camera; bolt drawn where it struck if seen. Code: `Fires::lightning` (gc_sim/src/fire.rs), `Event::Lightning`. Docs 41 step 6.
- **AI and fire:** in Field Battles, Veteran+ burns woods an enemy stands or hides in (seen or remembered, within 16 tiles, woods unbroken from where its torches catch, dry ground, wind not blowing back, no friends in the way), one fire per 20 s; and smokes out an army it knows is hiding in woods within 40 tiles (no deeper than 12 from the edge): keeps its troops out of those woods, marches its nearest regiment to the edge and burns them, for up to 45 s (`gc_ai/src/firing.rs`). Conquest Engineer (Veteran+) cuts a 3-wide path through thicket on the cheapest way to the nearest other start when crossing beats going round (thicket tile = 12 tiles of march; thicket on rock or deep water skipped), every 90 s, one worker at a time. Conquest minds don't flee fires, so they don't set them either. Docs 41 step 6.
- **Weather and fire news:** the news line says when rain (strength 80+), a storm (rain 192+, where lightning starts; rain again under 152), snow or fog comes in, and when it clears (under 40), once each way. Woods or scrub burning within 10 tiles of your buildings or regiments: "Fire! It is spreading near your base" (or "troops"), each once as it comes near, then not again for 45 s; "Lightning! It set the woods alight near ..." when a bolt lit a tile near the same place in the last 2 s. Not told while catching up online. The weather a match starts in isn't announced. Code: `gc_game/src/sky_news.rs`.
- **Night:** an unlit regiment in the open is spotted only within 6 tiles (12 when marching). Torches (`X`) light 10 tiles and make a regiment visible.
- **Seasons** (Conquest only; lobby Off, Short = 8 min a season (default), Long = 15 min; starts in spring): spring showers; summer dry (fire spreads 1.5x, ground dries 2x); autumn harvest (Grub gathered 1.25x); winter (Grub 0.75x, fire half, shallows freeze and can be crossed). The land shows it: straw-coloured grass in summer, gold, orange and red broadleaf woods in autumn, a thin snow and ice on the frozen water in winter (`--season NAME` previews one).
- **Fire** (`E` Burn): scrub burns 10 s, woods 25 s, thicket 40 s; then open ground. Spreads with the wind; wet ground and rain stop it. Burns 4 HP a second per tile touched. Burning buildings are not modelled (flames only).
- **Mud and floods:** long rain soaks ground into mud (slow, no charges). Long heavy rain swells rivers and shuts the fords; troops caught on a shut ford lose 12 HP a second and are moved to the bank.
- **Rivers carry their own water [docs/39]:** each river is cut into 16-tile stretches. Rain on a river fills them and runs on downstream, so a long river's lower fords flood most and drain last.
- **Reading the rivers [docs/39]:** on grown maps every river has a name ("the Brackwater"). The top bar shows the worst river you can see: up (wading in it is a quarter slower), in flood (its fords shut) or bursting its banks, and whether it is rising or falling. When you see a river rising in flood upstream, the river watch tells you when its crest reaches a ford ("The Brackwater is rising upriver: its crest reaches a ford in about 40 s"). The river's water is drawn as high as it stands.
- **Flash floods:** in Changing weather, a storm (rain strength 220 or more) can send a flood front down a river at 6 tiles a second; each ford shuts as it arrives (upper fords first) and opens once the water falls. Sizes: Minor (no ford shuts; 35 in 100), Flood (45), Major (16), Catastrophic (4); wet ground and rain on spring snow make them bigger, a dry summer smaller. Warnings: 8 s before the first ford ("A flash flood is coming down the river"); a Major is told to everyone 15 s ahead and a Catastrophic one 30 s ahead. Troops in a ford the front shuts are swept 10 tiles downstream and 30% drown.
- **Floodplains [docs/39, stage 2]:** land within 6 tiles of a river and under a tile above its line is the river's floodplain, and the river's own water rises over it. Once the river stands over it, it is wading ground (slow, no charges; a quarter slower again once the water is 0.1 of a tile deep over it; the selection panel says "In flood water"). Water doesn't cross higher ground, so low land behind a ridge stays dry. Where the water stands 0.35 of a tile over it, it shuts like a flooded ford: troops there are hurt each second and sent to dry ground, swept 10 tiles downstream (30% drown) while a flood front is over it, and buildings there lose 2% of their hit points a second and fall to rubble at none (bridges don't). Troops manning a wall there are safe. It opens again once the water is 0.1 of a tile lower. Low meadows go under in a Flood; higher land only in a Major or Catastrophic flood, or a long rain on a full river.
- **Debris on the rivers [docs/39, stage 3a]:** logs ride the rivers. A Major or catastrophic flood tears them off wooded banks (one a stretch at once, then one every 30 s, or 10 s in catastrophic water; at most 8 float at once), and a wood burning out on a bank topples one in one time in three. They drift down at the river's pace (0.5, 1 or 2 tiles a second, up to twice that in a flood) and leave where the river does. A log passing through troops wading a ford or flooded ground hurts them (3 HP per tile a second of its pace), throws them into disorder and shakes them, once per log; troops on a bridge or wall are safe. In quiet water logs ground on fords that aren't fast; 4 or more on one ford is a snag, slow to wade like deep water, until a flood (0.15 of a tile over the line) floats them off. Computer players keep off a ford a log is coming down on.
- **Felling and rafts [docs/39, stage 3b]:** Clans workers can Fell in (`Z`, then drag): bank woods and thicket beside running river fall into it, a log a woods tile and two a thicket tile, for no timber and half as long again; tiles away from the water are cleared as usual. Any faction's worker can tip in a Raft (`C`, then click running river along a bank; the Fell in and Raft keys show when only workers are selected, on maps with a river): it walks to the bank, works 6 s and pays 12 Grub as the raft goes in (mass 3: three times a log's blow, and a snag on its own). Each player may have at most 16 pieces on the water. The Concord's burning clears topple bank logs in as a fire does (one in three).
- **Currents:** wading troops drift downstream; going upstream is slower.
- **Trails and shores [docs/41 step 3]:** grown maps have worn trails between the bases, villages, camps and standing stones. They are open ground and cross rivers at the fords. River bends have gravel bars and sunny lake shores have sand (both open). Shaded lake shores have reed beds, which are tall grass to hide in.
- **Clearing** (`S`): scrub 3 s, woods 8 s (3 Grub of timber), thicket 15 s (5 Grub).

## 7. Game modes and how to start them [docs/06-game-modes.md, docs/08-ui-ux.md, crates/gc_game/src/main.rs]

**Main menu:** Play (Host a game, Join a game), Sandbox, Replays, Unit viewer, Settings, Guide, Exit.

- **Conquest** (primary; 20 to 60 min). Play > Host > Conquest, or `--conquest ribcage_pass` (2 players) / `four_teeth` (4) / `land-mixed` (any lobby map's name). Made maps: **Land: Random** (the default), **Plains**, **Mixed** and **Wild** on grown land with rivers, fords and woods (docs/41), and the older **Classic: Random**, **Plains**, **Mixed** and **Wild**. Both are sized by the lobby's **Size**: Small 320 tiles with four starts (up to 4 players), Medium 448 with four (the default, up to 4), Large 640 with six (up to 6). Two players start across the map from each other. Ribcage Pass and The Four Teeth keep their own size, and Size is hidden for them. Fog of war on in Conquest matches played to a victory.
  - Victory (default **Both**): Annihilation (destroy every opponent's main buildings; a player with none has 90 s to build one). Titan's Heart (hold the map centre for 3 minutes with an unbroken regiment within 6 tiles; unlocks at minute 20 Standard, 15 Swift, 24 Epic).
- **Field Battle** (5 to 20 min; no economy). Play > Host > Field Battle, or `--battle recruit|veteran|general|warlord` for a 1v1 against the AI. New lobbies start on **Measured: Random**: grown land with no mirrored half, starts placed so each one's walk to its nearest enemy is within 10% of the others' (any way the lobby hands starts out). Also offered: Land (mirrored grown land), Classic and Random maps. [docs/41]
  - Size Small, Medium (the default) or Large, set in the lobby header; the side grows with the player count.
  - Points budget 2,000, 4,000 (default) or 8,000; build your army in the lobby (Pick for me is available). 90 s deployment, then Ready. Win by rout or victory points. Champions before battle is on by default.
- **Sandbox:** main menu Sandbox or `--sandbox`. Place any troops and buildings for any side (section 1).
- **Online:** Host a game (map, mode, Tempo, victory), add AI in any slot, then Start. Join lists games on your network, or type an address or join code. Version mismatches are shown in the lobby.
- **Replays:** main menu. **Unit viewer:** main menu or `--viewer`. **Guide:** `U` or main-menu button (Quick Start, Buildings, Troops tabs).

## 8. Built vs not yet built

**Built** (confirmed in code or marked "Built" in the docs)
- Conquest economy: workers, buildings, upgrades, supply, upkeep, tempo, Annihilation, Titan's Heart, fog of war, seasons, weather.
- Battle: formations, Brace, Fall back, army shapes, control groups, ride view, Commander View, sieges, fire, floods and flash floods, currents, summer fords, ambushes, night torches.
- Fortifications: towers fire; Bolt and Blast towers (eight, two per faction; see section 5); drawn walls for all factions; blueprints and ring tool; wall levels 1 to 3; manning; ladders; gates and gatehouses; bridges; field works; Plant stakes; hidden traps.
- Factions: Honor and its Edicts (Ancestral Return needs a Shrine), Brood larvae and root-veins, Choir chords and Resonance (Overclock, Phase Shift), Concord caravans.
- Building looks: the 46 faction buildings use the stronger 2026-10-23 bake with surface detail (docs/31); gatehouses keep their first bake.
- Interface: top bar, command grid, build menu, tooltips, Marshal's Scroll, Guide, minimap, idle-worker button, hand-over to an ally.

**Not built yet** (design only, or no code found)
- Heroes in every mode (Shrine, Queen's Chamber, Sanctum, Vault exist as buildings; no hero data). Hero items, creep camps.
- Orders: Hold, Skirmish, Run, Column and Wedge; double-click select by type; `Tab` sub-groups.
- Concord Tithe contracts (Raise the Bound, Indenture, Foreclose, Hire Militia); Choir Silence the Chord.
- Research at Granary, Forge and Harmonics Lab (costs not designed yet).
- Tuners' repair (10 HP/s).
- Titan Events (Tremor, Marrow Bloom, Heartbeat, Gut Fog): no code found for Heartbeat, Marrow Bloom or Gut Fog; no lobby toggle found.
- Grubfolk market, rumours and mercenaries; allied resource trading (10% tax).
- Regicide, score limit, surrender votes, multiplayer pause limit (3 per match): no code found.
- Parley panel (planned, M4). Campaign (planned, after M4). Co-op Siege, Naval maps, King of the Corpse (later). Drowned Tide and Feathered Republic (set aside).
- Tutorial "First Harvest", faction primers, Codex, contextual hints.
- Ping wheel, text chat, key rebinding, alternate grid layouts.
- Settings saved but not acted on: V-sync, frame cap, effects, unit voices, interface volumes, colour-blind palette, high-contrast banners, hints, most of the graphics preset.
- World: burning buildings, computer players planning for seasons, snow on the Highland.
- Commander: a command to move the general to another regiment.

**Conflicts in the sources** (the code is the authority)
- Field Battle time limit: docs/06 says 20 minutes by default; docs/08 says "There is no time limit yet"; no time-limit control found in `crates/gc_ui/src/lobby.rs`.
- Ping wheel: docs/08 says `G` plus the mouse; in the code `G` shows blocked ground and no ping wheel was found.
- The main-building slot reads "Cancel" in code and docs/08, not "Call off".
- Camera: docs/08 says middle-click resets rotation; the code's `Home` does (also in the Marshal's Scroll).
- docs/26's status header is dated 2026-10-22, older than its own "Built" markers (stages 1 to 8). Treat the markers as current.
- docs/38's header says "Plan only", but stages 1 to 4 are marked "Built 2026-10-25".
- docs/37 (commander) has no "Built" marker, but the code exists (`gc_sim` generals, battle messages).
