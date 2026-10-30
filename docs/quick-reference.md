# Grubcraft: Player Quick Reference

Keep this file current: any PR that changes something a player sees updates it in the same PR, like the in-game guides.

Answer "how do I do X in the game?" from this file first. Facts come from `docs/` and the code (checked 2026-10-30). Section 8 lists what is not built yet; check it before promising a feature. Conflicts between sources are at the end of section 8.

Terms: **Grub** (common, from seeps), **Marrow** (rare, from veins). **Regiment** = a block of soldiers. **Worker** = Hands, Gnawers, Tuners, Ledger Clerks.

## 1. Controls and hotkeys [docs/08-ui-ux.md, crates/gc_ui/src/hud.rs (HELP_GROUPS), crates/gc_game/src/main.rs]

**Command grid** (bottom-right, 4 x 3): `Q W E R / A S D F / Z X C V`.

**Regiments**
- Left-click selects; drag a box; `Shift` adds or removes. Double-click select by type: not built.
- Right-click ground: move. Right-click enemy: attack. Right-drag: draw a line (position, frontage, facing).
- `Q` then click: move. `W` then click: attack. `Esc` cancels.
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
- `A` Work plans (build your planned walls). `F1` selects the next idle worker; with none idle, `F1` opens the Marshal's Scroll.

**Wall tool** (armed from the build menu)
- Click a start, click again for each piece; right-click or `Esc` ends. `Shift` locks 15-degree steps.
- `B` plans a blueprint (paid later). `O` ring tool: click corners; `Enter` or the first corner closes it.

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
- `--sandbox`; `--conquest ribcage_pass|four_teeth`; `--battle recruit|veteran|general|warlord`; `--host`; `--join ADDRESS`; `--password PW`; `--menu`/`--no-menu`; `--map NAME-or-FILE`; `--seed N`.
- `--showroom all|buildings|NAME[,NAME]`; `--showroom-clip idle|walk|attack|shoot|cheer|die|march|look|kill|finish|blows|reel`; `--viewer NAME|start`; `--siege`; `--watch REPLAY`.
- Look and display: `--time-of-day`, `--sky`, `--look`, `--weather`, `--air`, `--no-audio`, `--no-shadows`, `--no-aa`, `--no-glow`, `--draw`, `--lod`, `--render-scale`, `--latency`, `--overlay`.

**Not in the game:** key rebinding, ping wheel (`G` is blocked-ground view), text chat, Hold (`E` is Burn), Run, Column and Wedge formations, `Tab` sub-groups.

## 2. Economy [docs/04-economy.md, docs/03-factions.md, docs/21-buildings.md]

- **Grub:** from seeps. A seep holds 7,500; then dregs at 25% of the rate, for good. Pays for most things.
- **Marrow:** from veins (3,000 each, no regrowth, dregs at 25%). Pays for tier upgrades, elite units, monsters, hero revival and most research.
- **Top bar:** Grub, Marrow, Supply used/cap, clock, and a line "Upkeep 80%" in amber once upkeep bites. A faction meter sits beside them.
- **Harvest:** a full seep (5 workers) gives about 200 Grub a minute; a vein (3 workers) about 50 Marrow a minute at Standard tempo. Hands carry 10 Grub in 9 s; a Marrow load (5) takes 12 s.
- **Upkeep** (Grub only): supply used 0 to 60 = 100%; 61 to 100 = 75%; 101 to 150 = 55%.
- **Supply:** every faction starts at a cap of 12, raised by housing up to a hard maximum of 150. Housing: Farmstead +10 and Tenement +10 (Clans and Concord), Pustule +8 (Brood), Resonator or Siphon +4 (Choir), Outpost Hall +6 (Clans). Each main-building tier adds +10.
- **Supply per unit:** worker 1, light 3, line 4, elite or cavalry 5, artillery 4, air 3, monster 6, hero 5. Training waits in the queue while there is no room.
- **Workers:** Ember Hands 50 Grub; Brood Gnawers 50 Grub + 1 larva; Choir Tuners 60 (build and repair only); Concord Ledger Clerks 50 (Marrow only); Concord Caravan 75. Repair is free (Hands and Clerks 8 HP/s; Tuners' 10 not built).
- **Starting state:** Clans Clan Hall, 5 Hands, 300 Grub. Brood Hive, 4 Gnawers + 3 larvae, 300 Grub. Choir Spire + 2 Resonators, 3 Tuners, 300 Grub. Concord Counting House, 4 Clerks + 1 Caravan, 400 Grub.
- **Tempo:** Standard (100%); Swift (gather 150%, build and train 75%); Epic (gather 80%, build and train 120%).
- **Tier costs:** tier 2 = 300 Grub + 100 Marrow, 60 s; tier 3 = 600 Grub + 250 Marrow, 90 s. Tier 1 needs no Marrow.

## 3. Buildings [docs/21-buildings.md, docs/04-economy.md, crates/gc_game/src/conquest_card.rs]

**How to build**
- Select workers; `F` or `B` opens the build menu; pick, then click ground. `D` lays a new main building.
- A foundation starts at 10% HP. Up to four workers build it (the first full speed, each extra half). Nothing works until finished.
- Cancel refunds 75%. A destroyed building refunds nothing. Overgrown ground is cleared first and paid when ordered.
- Choir buildings (except Spire, Resonator, Siphon) must stand in a Choir field. Tier: the main-building tier needed to place it.

**Main building tiers**
- Clans: Clan Hall, Castle Keep, Daimyo's Citadel. Brood: Hive, Great Hive, Throne Hive. Choir: Spire, Cathedral, Grand Organ. Concord: Counting House, Exchange, Grand Bourse.
- Upgrade: select the finished main building, press `V`. It keeps working while upgrading. While upgrading the same slot reads "Cancel" (75%). Gates upgrade with `V` to "Gatehouse" (Yagura Gatehouse, Gate of Debts, Chitin Maw). The Choir builds no gates.

**Buildings per faction** (tier, cost Grub/Marrow)
- **Ember Clans:** Clan Hall T1 400/0; Castle Keep T2 300/100; Daimyo's Citadel T3 600/250; Outpost Hall T1 250/0 (cheap, trains nothing); Farmstead T1 80/0; Granary T1 100/0; Dojo T1 150/0; Stables T2 180/40; Ember Workshop T1 160/0 (rams; rocket carts at T2); Kite Loft T2 160/60; Ancestor Shrine T1 150/0; Forge T1 175/0; Watchtower T1 125/0; Palisade T1 10/0 per tile; Gate T1 60/0.
- **Brood:** Hive T1 300/0; Great Hive T2 300/100; Throne Hive T3 600/250; Pustule T1 40/0; Hatchery Pit T1 120/0; Chrysalis Chamber T2 150/50 (evolutions); Moth Roost T2 150/40; Gorge T2 180/60; Spine Mound T1 100/0; Queen's Chamber T1 140/0; Resin Ridge T1 8/0 per tile; Maw Gate T1 55/0.
- **Shard Choir:** Spire T1 400/0; Cathedral T2 300/100; Grand Organ T3 600/250; Resonator T1 150/0; Siphon T1 175/0; Forge of Facets T1 160/0; Harmonics Lab T1 175/0; Skyglass Loom T2 160/50; Bell Tower T1 150/0; Sanctum of Echoes T1 150/0; Light Curtain T1 10/0 per tile.
- **Carrion Concord:** Counting House T1 400/0; Exchange T2 300/100; Grand Bourse T3 600/250; Waystation T1 200/0; Tenement T1 80/0; Reliquary T1 75/0; Ossuary T1 150/0; Guildhall T1 160/0; Lantern Spire T2 160/50; Tollgate T1 140/0; Vault of Names T1 150/0; Bone Fence T1 12/0 per tile; Iron Gate T1 70/0.

**Walls** [docs/26-fortifications.md]
- Level 1 cheap; level 2 adds a walkway (about 15 Grub a tile more); level 3 adds battlements and the faction twist (20 Grub + 5 Marrow a tile more, needs tier 2).
- Ember: Palisade, Stone Wall, Castle Wall. Concord: Bone Fence, Ossuary Wall, Mausoleum Wall. Choir: Light Curtain, Glass Wall, Singing Wall. Brood: Resin Ridge, Chitin Wall, Thorn Bulwark.
- Raise a stretch with `U`; a worker raises it tile by tile.

## 4. Factions [docs/03-factions.md]

- **Ember Clans** (Shogun): Hands carry Grub to the hall or a Granary. **Honor** meter: +5 when an enemy regiment routs against yours, +10 for a win while outnumbered, +2 a minute per Shrine held, -5 when yours routs. Spent on Edicts. Tight formation gives +15% defence when still.
- **Brood** (vein network): Gnawers burrow into seeps and send Grub home through root-veins; cut a Pustule and seeps beyond it stop. **Larvae** meter: each Hive or Hatchery Pit holds 3, makes one every 12 s; every regiment hatches from one. Tier-1 regiments evolve on the field (15 s cocoon).
- **Shard Choir** (chords): Resonators tune seeps with no workers. Three Resonators linked within 14 tiles (not in a line) form a Chord, adding +10% to +60% Grub to tuned seeps there. Chords charge **Resonance** (cap 100). Most Choir buildings need power from a Spire (8-tile field) or Resonator (6 tiles).
- **Carrion Concord** (trade and undeath): Caravans carry Grub from the Counting House to a trade post: a neutral hamlet (full), your Waystation (half), or an allied main building (full, ally gets 20%). Pay: 40 Grub + 1.2 per tile of straight-line distance. **Tithe** meter: each death within 12 tiles of a Reliquary pays 1. The dead do not rout.
- **Heroes:** up to three per faction, not in the game yet (section 8).
- **Field Battle generals:** each side's general rides with one regiment. Select it to see "Your general rides here: guard him". His death panics nearby regiments. [docs/37-commander.md]

## 5. Combat and fortifications [docs/05-combat.md, docs/26-fortifications.md, docs/21-buildings.md]

- A regiment is a block of models; the back rank steps up as the front falls, so it hits less as it shrinks.
- Hits from the flank do x1.4 damage; from the rear x1.8.
- **Frontage:** only soldiers touching the enemy strike: the front rank (spears also the second, at half), rank ends on a flank, half the front from the rear. Each side of a block has room for as many strikers as its length, shared by every attacker, so a swarm can't pile six regiments onto one front. Gates and bridges hold: only the front can be reached. Attackers who can't reach go round to a side with fewer on it, if their block fits there (not in a gate or on a bridge).
- Charges need about a second of marching (about 3 tiles for infantry). A charge into braced spears' front fails.
- Brace (`Z`): stops the regiment; spears and pikes only. Fall back (`V`): each locked enemy gets one parting blow; routing regiments cannot fall back.
- Woods and tall grass hide regiments. Your hidden regiments show a moss-green flag.
- Melee hits buildings with the front rank; blows on buildings always land. Fortified buildings take half from regiments. Siege (rams, Mausoleum Engine) take x3.0 vs Fortified. Right-click an enemy building to attack it.
- Right-click an enemy with a tower selected to focus it.
- **Drawn walls:** wall tool; built tile by tile; snaps to wall ends, towers and gates within 1.5 tiles.
- **Gates:** Auto (opens for you and allies), Locked (shut to all), Open (held). Enemy routes treat a shut gate as wall.
- **Manning:** with foot or archers, right-click your raised (level 2 or 3) wall to man it. Right-click an enemy raised wall to climb ladders; `Ctrl`+right-click sieges it instead.
- **Bridges** (straight, over water, two tiles wide, 6 Grub a tile, so 12 Grub a step along the span): Timber Bridge (Clans), Bone Causeway (Concord), Glass Span (Choir), Chitin Span (Brood). All four are drawn as the same planked timber bridge with lanterns and your team's cloth for now.
- **Field works:** Stakes and Barricades, drawn with the wall tool. Spearmen and archers plant stakes with `C` (about 10 s).
- **Hidden traps** (one per faction; only its owner sees it): Spike Pit (Clans), Grave Hands (Concord), Shard Mine (Choir), Burrow Nest (Brood).
- **Towers that fire:** Watchtower, Tollgate, Spine Mound, Bell Tower (range about 30 to 32). Bolt and Blast towers are not built.

## 6. World: weather, seasons, fire, floods, rivers [docs/32-changing-land.md, docs/38-living-waterways.md]

- **Time of day** (lobby): Golden hour (17:00), Noon, Overcast (12:00), Dusk (18:00), Night (23:00).
- **Weather** (lobby): Fair, Changing (a new spell every 3 to 6 min), Rain, Snow, Fog. Fog cuts sight up to 60%; snow 30%; rain 20%; night 45%. Snow slows marching up to 15%. The top bar shows the cut (for example "Fog: sight 40%").
- **Night:** an unlit regiment in the open is spotted only within 6 tiles (12 when marching). Torches (`X`) light 10 tiles and make a regiment visible.
- **Seasons** (Conquest only; lobby Off, Short = 8 min a season (default), Long = 15 min; starts in spring): spring showers; summer dry (fire spreads 1.5x, ground dries 2x); autumn harvest (Grub gathered 1.25x); winter (Grub 0.75x, fire half, shallows freeze and can be crossed).
- **Fire** (`E` Burn): scrub burns 10 s, woods 25 s, thicket 40 s; then open ground. Spreads with the wind; wet ground and rain stop it. Burns 4 HP a second per tile touched. Burning buildings are not modelled (flames only).
- **Mud and floods:** long rain soaks ground into mud (slow, no charges). Long heavy rain swells rivers and shuts the fords; troops caught on a shut ford lose 12 HP a second and are moved to the bank.
- **Rivers carry their own water [docs/39]:** each river is cut into 16-tile stretches. Rain on a river fills them and runs on downstream, so a long river's lower fords flood most and drain last.
- **Flash floods:** in Changing weather, a storm (rain strength 220 or more) can send a flood front down a river at 6 tiles a second; each ford shuts as it arrives (upper fords first) and opens once the water falls. Sizes: Minor (no ford shuts; 35 in 100), Flood (45), Major (16), Catastrophic (4); wet ground and rain on spring snow make them bigger, a dry summer smaller. Warnings: 8 s before the first ford ("A flash flood is coming down the river"); a Major is told to everyone 15 s ahead and a Catastrophic one 30 s ahead. Troops in a ford the front shuts are swept 10 tiles downstream and 30% drown.
- **Currents:** wading troops drift downstream; going upstream is slower.
- **Clearing** (`S`): scrub 3 s, woods 8 s (3 Grub of timber), thicket 15 s (5 Grub).

## 7. Game modes and how to start them [docs/06-game-modes.md, docs/08-ui-ux.md, crates/gc_game/src/main.rs]

**Main menu:** Play (Host a game, Join a game), Sandbox, Replays, Unit viewer, Settings, Guide, Exit.

- **Conquest** (primary; 20 to 60 min). Play > Host > Conquest, or `--conquest ribcage_pass` (2 players) / `four_teeth` (4). Made maps **Classic: Random**, **Plains**, **Mixed** and **Wild** are sized by the lobby's **Size**: Small 320 tiles with four starts (up to 4 players), Medium 448 with four (the default, up to 4), Large 640 with six (up to 6). Two players start across the map from each other. Ribcage Pass and The Four Teeth keep their own size, and Size is hidden for them. Fog of war on in Conquest matches played to a victory.
  - Victory (default **Both**): Annihilation (destroy every opponent's main buildings; a player with none has 90 s to build one). Titan's Heart (hold the map centre for 3 minutes with an unbroken regiment within 6 tiles; unlocks at minute 20 Standard, 15 Swift, 24 Epic).
- **Field Battle** (5 to 20 min; no economy). Play > Host > Field Battle, or `--battle recruit|veteran|general|warlord` for a 1v1 against the AI.
  - Size Small, Medium (the default) or Large, set in the lobby header; the side grows with the player count.
  - Points budget 2,000, 4,000 (default) or 8,000; build your army in the lobby (Pick for me is available). 90 s deployment, then Ready. Win by rout or victory points. Champions before battle is on by default.
- **Sandbox:** main menu Sandbox or `--sandbox`. Place any troops and buildings for any side (section 1).
- **Online:** Host a game (map, mode, Tempo, victory), add AI in any slot, then Start. Join lists games on your network, or type an address or join code. Version mismatches are shown in the lobby.
- **Replays:** main menu. **Unit viewer:** main menu or `--viewer`. **Guide:** `U` or main-menu button (Quick Start, Buildings, Troops tabs).

## 8. Built vs not yet built

**Built** (confirmed in code or marked "Built" in the docs)
- Conquest economy: workers, buildings, upgrades, supply, upkeep, tempo, Annihilation, Titan's Heart, fog of war, seasons, weather.
- Battle: formations, Brace, Fall back, army shapes, control groups, ride view, Commander View, sieges, fire, floods and flash floods, currents, summer fords, ambushes, night torches.
- Fortifications: towers fire; drawn walls for all factions; blueprints and ring tool; wall levels 1 to 3; manning; ladders; gates and gatehouses; bridges; field works; Plant stakes; hidden traps.
- Factions: Honor and its Edicts (Ancestral Return needs a Shrine), Brood larvae and root-veins, Choir chords and Resonance (Overclock, Phase Shift), Concord caravans.
- Building looks: the 46 faction buildings use the stronger 2026-10-23 bake with surface detail (docs/31); gatehouses keep their first bake.
- Interface: top bar, command grid, build menu, tooltips, Marshal's Scroll, Guide, minimap, idle-worker button, hand-over to an ally.

**Not built yet** (design only, or no code found)
- Heroes in every mode (Shrine, Queen's Chamber, Sanctum, Vault exist as buildings; no hero data). Hero items, creep camps.
- Orders: Hold, Skirmish, Run, Column and Wedge; double-click select by type; `Tab` sub-groups.
- Concord Tithe contracts (Raise the Bound, Indenture, Foreclose, Hire Militia); Choir Silence the Chord.
- Research at Granary, Forge and Harmonics Lab (costs not designed yet).
- Bolt and Blast towers; tower levels; Walled Town defence bonus (no "built" marker).
- Tuners' repair (10 HP/s).
- Titan Events (Tremor, Marrow Bloom, Heartbeat, Gut Fog): no code found for Heartbeat, Marrow Bloom or Gut Fog; no lobby toggle found.
- Grubfolk market, rumours and mercenaries; allied resource trading (10% tax).
- Regicide, score limit, surrender votes, multiplayer pause limit (3 per match): no code found.
- Parley panel (planned, M4). Campaign (planned, after M4). Co-op Siege, Naval maps, King of the Corpse (later). Drowned Tide and Feathered Republic (set aside).
- Tutorial "First Harvest", faction primers, Codex, contextual hints.
- Ping wheel, text chat, key rebinding, alternate grid layouts.
- Settings saved but not acted on: V-sync, frame cap, effects, unit voices, interface volumes, colour-blind palette, high-contrast banners, hints, most of the graphics preset.
- World: lightning, burning buildings, computer players setting fires or planning for seasons, snow on the Highland.
- Commander: a command to move the general to another regiment.

**Conflicts in the sources** (the code is the authority)
- Field Battle time limit: docs/06 says 20 minutes by default; docs/08 says "There is no time limit yet"; no time-limit control found in `crates/gc_ui/src/lobby.rs`.
- Ping wheel: docs/08 says `G` plus the mouse; in the code `G` shows blocked ground and no ping wheel was found.
- The main-building slot reads "Cancel" in code and docs/08, not "Call off".
- Camera: docs/08 says middle-click resets rotation; the code's `Home` does (also in the Marshal's Scroll).
- docs/26's status header is dated 2026-10-22, older than its own "Built" markers (stages 1 to 8). Treat the markers as current.
- docs/38's header says "Plan only", but stages 1 to 4 are marked "Built 2026-10-25".
- docs/37 (commander) has no "Built" marker, but the code exists (`gc_sim` generals, battle messages).
