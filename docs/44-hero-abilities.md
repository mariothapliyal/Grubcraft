# 44 · Hero abilities

Heroes stage H2 ([43 · Heroes](43-heroes.md#stages)). Stage H1 merged in #419: heroes are hired, level, lead, fall and are revived, and their abilities are listed but do nothing. This stage makes them work. Reviewed on Opus before building (2026-11-22). Cut down the same day (Mario: "simplify the abilities, nothing wrong with the ability engine"): **two abilities a hero, six a faction, 24 in all**, and no skill points.

## The rules

- **Two abilities a hero:** one basic ability and one ultimate, from the roster. The roster's other abilities are dropped.
- **No skill points:** abilities grow with the hero's level by themselves.
  - The basic ability has three ranks: rank 1 from level 1, rank 2 from level 4, rank 3 from level 7.
  - The ultimate opens at level 6 and has one rank.
  - H1's banked skill points go: nothing spends them.
- **Kinds:**
  - **Active:** cast with `Command::Cast { regiment, ability, target }`, where `ability` is 0 (the basic one) or 1 (the ultimate).
  - **Passive:** always on (only Compound Interest).
  - **Ultimate:** an active one, with the long cooldown.
- **Casting:** an ability has a cooldown and, where the roster gives one, a range from the hero.
  - **Target:** nothing, a point, or a regiment. A regiment target must be one the caster's side can see (`Vision::sees`), as for an attack order.
  - **Out of range:** the hero is ordered to walk into range and casts when it gets there, as an attack order closes in. Another order cancels the cast. In lockstep a target moves while the command waits out the input delay, so refusing out-of-range casts would make casts fail at random.
  - **Timing:** a cast takes effect on the tick it happens. There is no cast time and no mana: the roster has neither, and cooldowns pace the abilities.
  - **When refused:** a dead, stunned or cocooned hero can't cast. A refusal raises an event, so the screen can say why.
- **Numbers:**
  - A roster's "40/70/100" is ranks 1, 2 and 3. Radii, ranges and durations are the roster's.
  - Damage takes soldiers from the back ranks first, through one ability damage path. Like volleys, it carries leftover damage and steps the next soldier up, but it never draws on the combat dice, so a cast can't shift other fights' rolls.
  - Abilities ignore armour: they are rare and aimed.
  - Damage shared among several regiments is shared evenly, and any remainder goes in regiment order.
- **Experience:** soldiers killed by abilities count for the heroes near them like any other dead (H1). Summoned and raised regiments give no experience when they die, so free regiments can't feed enemy heroes.
- **Where the numbers live:** the ability table is in the simulation (`gc_sim::abilities`), keyed by hero and slot, as other rules numbers are; a test checks every roster hero's two abilities match it by name. The roster's `effect:` text stays the tooltip.

## The effect engine

Every ability is one or more of these effects:

| Effect | Does | Abilities |
|---|---|---|
| `Damage` | Hit points to every enemy regiment in a shape: one target, a circle, or a line. At once, or each pulse for a duration; optionally x1.5 to structures | Iaijutsu Dash, Thousand Cuts, Kitsune Storm, Burrow Charge, Maw of the Titan, Prismatic Lance |
| `Drain` | Damage to one regiment each second, healing the hero by as much | Leech Debt |
| `Cull` | One regiment loses soldiers outright: a share of them, or a number. Never the last soldier (`kill_last`), so heroes and one-soldier monsters can't be culled | Maw of the Titan, Foreclosure |
| `Morale` | Morale up or down in an area or on the whole map; routing friends rally | Rally, Shogun's Banner, Kitsune Storm, Deafening Peal |
| `Buff` | A timed change to friends in an area: melee attack, reload, no morale lost to casualties | Crescendo, Swarm Tide |
| `Volleys` | The hero's next volleys deal more damage | Split Beam |
| `March` | The regiments' existing march bonus (`march_left`), the larger of a power's and an ability's | Shogun's Banner |
| `Steadfast` | The existing can't-rout timer (`steadfast_left`) | Shogun's Banner |
| `Stun` | A regiment is held fast (the existing `trapped_left`, which brigades already handle) and can't strike or shoot | Toll, Confusion Cloud, Puppet Army |
| `Untouchable` | Takes no damage | Thousand Cuts |
| `Summon` | Regiments of a unit type beside the hero, for a time | Spawn Brood, Swarm Tide, Fire Spirit |
| `Raise` | A regiment of a unit type rises from the corpses near the hero, for a time, through the Tithe's raising (`crate::tithe`). At least `MIN_RAISED` (4) corpses are needed | Raise Dead, Mass Grave |
| `Blink` | The hero moves to a point in range at once | Iaijutsu Dash, Burrow Charge |
| `Income` | A timed or standing change to a player's Grub: their own, or every enemy's, at `Economy::deposit_grub` | Compound Interest, Market Crash, Symphony |

Where the state lives:
- **On the hero** (at most 3 a player): its two cooldowns and Split Beam's volleys left.
- **On each regiment:** the timed changes (melee attack bonus, reload bonus, no casualty morale, the stun's no-strike flag, untouchable), each with its ticks left. The same change again takes the larger and the longer. About 12 bytes a regiment, under 20 KB for the 1536-slot pool, inside the 8 MB budget.
- **Zones** (Thousand Cuts, Kitsune Storm, Leech Debt, Mass Grave's growth): a fixed list of at most 32, each an ability pulsing at a point, on the hero or on a target, in the state hash.

## The 24 abilities

**Ember Clans.**
- *The Daimyo:*
  - **Rally:** routing regiments within 12 tiles rally, and all of them gain 15/25/35 morale.
  - **Shogun's Banner** (ultimate): every Clan regiment on the map gains 25 morale, +20% speed and can't rout, for 20 s.
- *The Ronin:*
  - **Iaijutsu Dash:** blinks up to 10 tiles and deals 60/100/140, spread, to each enemy regiment on the line crossed. The line is worked out from where the Ronin was and where it lands.
  - **Thousand Cuts** (ultimate): untouchable for 3 s while it deals 45 eight times to enemies within 4 tiles (360 in all, shared).
- *The Onmyoji:*
  - **Fire Spirit:** a Fire Spirit regiment for 30/40/50 s.
  - **Kitsune Storm** (ultimate): 20 damage and 3 morale a second to every enemy regiment in an 8-tile circle within 24 tiles, for 10 s.

**The Brood.**
- *The Queen-in-Waiting:*
  - **Spawn Brood:** 1/2/3 Spawnlings regiments beside her for 30 s.
  - **Swarm Tide** (ultimate): Brood regiments within 20 tiles +20 melee attack and no morale lost to casualties for 15 s, and 4 Spawnlings regiments hatch.
- *The Devourer:*
  - **Burrow Charge:** blinks to a point within 16 tiles and deals 60/90/120 to each enemy regiment within 3 tiles of where it lands, disordering them as a charge does. It tunnels, but not under walls: it obeys the blink rule below.
  - **Maw of the Titan** (ultimate): within 5 tiles of a point within 16, up to 12 soldiers die, taken from regiments of non-monster units, and monsters and heroes there take 200.
- *The Whisperer:*
  - **Confusion Cloud,** cut down: enemy regiments in a 5-tile circle within 16 tiles are stunned and disordered for 4/5/6 s. Making them attack anyone near would need a new target rule.
  - **Puppet Army** (ultimate), cut down: every routing enemy regiment within 20 tiles is stunned for 10 s. They can't run, so they are cut down where they stand. Turning them would change owners mid-match, which the game can't do safely yet.

**The Choir.**
- *The Maestro:*
  - **Crescendo:** friends within 12 tiles reload 15/25/35% faster and gain +10/15/20 melee attack for 10 s. This is not the power `Haste`, which also speeds strikes and building.
  - **Symphony** (ultimate): the player's chords make +100% Grub for 30 s, and Resonance charges twice as fast.
- *The Refractor:*
  - **Split Beam,** cut down: its next 3 volleys deal +50/100/150% damage. Splitting a volley between targets needs a second aim in the volley code.
  - **Prismatic Lance** (ultimate): 250 damage shared by every enemy regiment on a 2-tile-wide line up to 40 tiles long, x1.5 to structures.
- *The Hollow Bell:*
  - **Toll:** stuns a regiment within 10 tiles for 1.5/2.5/3.5 s.
  - **Deafening Peal** (ultimate): every enemy regiment within 14 tiles loses 30 morale.

**The Carrion Concord.**
- *The Magnate:*
  - **Compound Interest** (passive): every minute, 1/2/3% of banked Grub, at most 15/30/45.
  - **Market Crash** (ultimate): every enemy's Grub income -50% for 30 s.
- *The Undertaker:*
  - **Raise Dead:** a Bound Dead regiment of as many soldiers as there are corpses within 8 tiles: at least 4, up to 6/10/14. It lasts 45 s.
  - **Mass Grave** (ultimate), cut down: one Bound Dead regiment rises on its side and grows by one soldier for each soldier who dies within 10 tiles over 15 s, up to its full strength (16 soldiers). One growing regiment rather than one for each death, so a big fight can't flood the regiment pool.
- *The Usurer:*
  - **Leech Debt:** drains 15/25/35 a second from a regiment within 8 tiles for 3 s.
  - **Foreclosure** (ultimate): an enemy regiment within 10 tiles below half strength loses a third of its soldiers, back ranks first.

Dropped (2026-11-22): Command Aura, Challenge, Wind Cut, Wanderer, Ward Talisman, Divination, Royal Jelly, Pheromone Trail, Gorge, Grow, Mind-Spore, Hive Sight, Retune, Harmony, Mirror Shield, Lens, Silence, Echo Step, Hostile Takeover, Bribe, Corpse Explosion, Last Rites, Lien, Collateral. Cut down, listed again for the Guide: Confusion Cloud, Puppet Army, Split Beam, Mass Grave.

## Stages

- **H2, all 24 in one stage (built 2026-11-22):**
  - **The engine:** casting with walking into range, cooldowns, ranks by level, the timed changes and zones (`gc_sim::abilities`, tested in `ability_tests.rs`, one test or more for each ability).
  - **On screen:** the hero panel's two abilities and the command grid's `A` (basic) and `S` (ultimate). One cast on the hero goes off at once; one aimed at a place or a regiment asks for a click on it ("click a place", "click an enemy regiment"), and Esc, a right-click or pressing the key again puts it down. A locked, cooling or stunned ability says why at once; a cast that a hero walked to and then couldn't make says why when it fails. A cast throws up a burst of dust where it went off, when that is seen. Tooltips from the roster.
  - **Computer players** cast by simple rules (the Steward, `gc_ai/src/conquest/steward/heroes.rs`).
  - **Stress scenario** `abilities`, and the golden replay `conquest-heroes-1v1`, which gives `Cast` at the hero and at a regiment.
- **H3:** heroes in Field Battles.

## Real use

- **Blinks through passages:** a blink needs a clear line on the caster's own movement grid (`Nav::clear_line`), and ground at the point the hero fits on (`settle`, `fits`). That grid has the player's own gates open and enemy gates shut. So Iaijutsu Dash can't skip a wall, an enemy gate, or a river without a ford, though a hero can blink through its own open gate. Burrow Charge obeys the same rule.
- **Summons and raisings at gates and in crowds:**
  - A summoned regiment is placed like a trained one, at the nearest free ground beside the hero (the spot search `raise_trained` uses), up to 6 tiles out.
  - The spot must also have a clear line from the hero, so a hero in a gate can't summon outside its walls.
  - With no spot, the cast is refused and its cooldown isn't spent.
- **Areas and allies:** harmful areas hit enemies only (`stances.hostile`), never allies or the caster's own side. Friendly effects reach only the caster's and allies' regiments.
- **On the march:** a stunned regiment in a brigade is held fast, as a trapped one is, and the brigade marches on without it.
- **Stuns and orders:** a stunned regiment keeps its orders and carries on when the stun ends.
- **Determinism:**
  - Every effect is fixed-point and ordered by regiment index.
  - Ability damage never draws on the combat dice.
  - Everything is in the state hash: cooldowns, timed changes, stuns, zones and casts waiting on range.
- **Walking to cast:** a hero that gets as near as it can and is still out of reach gives the cast up and says so. A new cast replaces the one it was walking to, and ends that walk. A hero that falls on its way forgets the cast, so a revived hero never walks off on an old order. An army handed to an ally casts for the ally.
- **Within the tick:** casts that summon or raise (Fire Spirit, Spawn Brood, Swarm Tide, Raise Dead, Mass Grave) go off after the tick's dead are counted, so a summoned regiment never takes the slot of one just wiped out before its Tithe, corpses and experience are paid, and raising uses this tick's corpses. Mass Grave's regiment grows by every death in the tick, towers' and traps' too. The other casts go off before, so their kills count. A zone's pulses keep their time from the first.
- **The field's 32 zones full:** an ability that leaves a zone (Thousand Cuts, Kitsune Storm, Leech Debt, Mass Grave) is refused before anything happens, with no cooldown spent.
- **Bell Towers:** the Choir's Bell Tower silences faction powers near it, not hero abilities. A hero's ability is its own, as a regiment's attack is.
- **Locked, cooling or stunned:** a cast order is still taken (in lockstep it has already left the player), and the hero does nothing; the game asks before sending, so a player only sees this when the state changed on the way.
- **Computer players** (built, simpler than planned): every half second, each of their heroes casts the first of its two abilities, the ultimate first, that is ready and has a foe to go at.
  - One cast on the hero (a rally, a summons, an aura) needs a standing foe within 8 tiles.
  - One aimed at a place or a regiment goes at the nearest standing foe in its reach, so the hero never walks off its army's line to cast.
  - Foreclosure waits for a foe below half strength.
  - After a cast, a hero is given no other for 3 s past its reaction time, while the order is on its way.
  - The `abilities` stress scenario checks a General's Daimyo casts within seconds of foes coming near (4.7 s). In the Conquest AI test's Veteran games, heroes come at tier 2 and rarely meet a foe before the game is decided, so few casts happen there yet. The planned finer rules (the thickest enemy, the enemy hero for a stun, the ultimate when 6 regiments fight) are left for the smarter AI generals.
