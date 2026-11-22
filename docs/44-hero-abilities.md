# 44 · Hero abilities

Heroes stage H2 ([43 · Heroes](43-heroes.md#stages)). Stage H1 merged in #419: heroes are hired, level, lead, fall and are revived, and their abilities are listed but do nothing. This stage makes the 48 roster abilities work. Reviewed on Opus before building (2026-11-22); its corrections are in.

## The rules

- **Four abilities a hero:** three basic ones and an ultimate, from the roster. Each level gives one skill point. A basic ability has three ranks, learned at levels 1, 3 and 5 at the earliest (Warcraft III's rule: a rank every other level). The ultimate has one rank, from level 6. Ten levels give ten points: nine basic ranks and the ultimate. An ability not built yet can't be learned, and its points stay banked.
- **Learn** with `Command::Learn { regiment, ability }`. A point is spent and can't be taken back.
- **Kinds:**
  - **Active:** cast with `Command::Cast { regiment, ability, target }`.
  - **Aura:** always on while the hero stands.
  - **Passive:** always on.
  - **Ultimate:** an active one, with the long cooldown.
- **Casting:** an ability has a cooldown and, where the roster gives one, a range from the hero.
  - **Target:** nothing, a point, or a regiment. A regiment target must be one the caster's side can see (`Vision::sees`), as for an attack order.
  - **Out of range:** the hero is ordered to walk into range and casts when it gets there, as an attack order closes in. Another order cancels the cast. In lockstep a target moves while the command waits out the input delay, so refusing out-of-range casts would make casts fail at random.
  - **Timing:** a cast takes effect on the tick it happens. There is no cast time and no mana: the roster has neither, and cooldowns pace the abilities.
  - **When refused:** a dead, stunned, silenced or cocooned hero can't cast. A refusal raises an event, so the screen can say why.
- **Numbers:**
  - A roster's "40/70/100" is ranks 1, 2 and 3. Radii, ranges and durations are the roster's.
  - Damage takes soldiers from the back ranks first, through one ability damage path. Like volleys, it carries leftover damage and steps the next soldier up, but it never draws on the combat dice, so a cast can't shift other fights' rolls.
  - Abilities ignore armour: they are rare and aimed.
  - Damage shared among several regiments is shared evenly, and any remainder goes in regiment order.
- **Experience:** soldiers killed by abilities count for the heroes near them like any other dead (H1). Summoned and raised regiments give no experience when they die, so free regiments can't feed enemy heroes.
- **Silence and leadership:** a silenced hero can't cast and its auras stop, but it still leads. Leadership is not an ability (docs/43).

## The effect engine

Every ability is one or two of these effects, baked from a structured `built:` field in the roster, as faction powers are. The plain-language `effect:` text stays the tooltip.

| Effect | Does | Abilities |
|---|---|---|
| `Damage` | Hit points to every enemy regiment in a shape: one target, a circle, a cone ahead or a line. At once, or each second for a duration; optionally morale lost each second; optionally x1.5 to structures | Wind Cut, Burrow Charge, Maw of the Titan, Corpse Explosion, Prismatic Lance, Thousand Cuts, Kitsune Storm, Iaijutsu Dash |
| `Drain` | Damage to one regiment each second, healing the hero by as much | Leech Debt |
| `Cull` | One regiment loses soldiers outright: a share of them, or one. Never the last soldier (`kill_last`), so heroes and one-soldier monsters can't be culled | Foreclosure, Gorge, Maw of the Titan |
| `Heal` | Hit points, and lost soldiers back, to one friendly regiment or the hero | Royal Jelly, Last Rites, Wanderer, Gorge |
| `Morale` | Morale up or down in an area or on the whole map; routing friends rally | Rally, Deafening Peal, Shogun's Banner, Kitsune Storm |
| `Buff` | A timed change to friends or enemies in an area, on the map or on one regiment: melee attack, melee defence, reload, damage taken, missile damage taken or dealt, no morale lost to casualties | Crescendo, Swarm Tide, Command Aura, Lien, Ward Talisman, Mirror Shield, Lens, Split Beam |
| `March` | The regiments' existing march bonus (`march_left`), the larger of a power's and an ability's | Pheromone Trail, Shogun's Banner |
| `Steadfast` | The existing can't-rout timer (`steadfast_left`) | Shogun's Banner |
| `Stun` | One regiment is held fast (the existing `trapped_left`, which brigades already handle) and can't strike or shoot | Toll, Mind-Spore, Confusion Cloud, Puppet Army |
| `Untouchable` | Takes no damage and can't be targeted, except by one regiment if one is named | Thousand Cuts, Challenge |
| `Silence` | Heroes in an area can't cast and their auras stop | Silence |
| `Summon` | Regiments of a unit type beside the hero, for a time | Spawn Brood, Swarm Tide, Fire Spirit |
| `Raise` | A regiment of a unit type rises from the corpses near the hero, for a time, through the Tithe's raising (`crate::tithe`). At least `MIN_RAISED` (4) corpses are needed | Raise Dead, Mass Grave |
| `Blink` | The hero moves to a point in range at once | Echo Step, Iaijutsu Dash, Burrow Charge, Retune |
| `Reveal` | A circle is seen through fog, burrowing and hiding, for a time | Divination, Hive Sight |
| `Income` | A timed or standing change to a player's Grub: their own, or every enemy's, at `Economy::deposit_grub` | Market Crash, Symphony, Harmony, Compound Interest |
| `Grow` | The hero gains hit points as enemy soldiers die beside it (the tick's `deaths`) | Grow |

Where the state lives:
- **On the hero** (at most 3 a player): ranks, cooldowns, Split Beam's volleys left, Grow's count.
- **On each regiment:**
  - Four timed change slots, `buffs`. The same ability cast again refreshes its slot. A new change takes a free slot, or else the slot with the least time left.
  - The aura values, worked out afresh each second as `led` is. Auras take no slots, and two copies of the same aura don't stack.
  - The stun's no-strike flag, the silence timer and the untouchable partner.
  - Together that is about 40 bytes a regiment, about 60 KB for the 1536-slot pool, inside the 8 MB budget.
- **Zones** (Kitsune Storm, Ward Talisman, Reveal, Mass Grave): a fixed list of at most 32, in the state hash.

## The 48 abilities

**Ember Clans.**
- *The Daimyo:*
  - **Rally:** routing regiments within 12 tiles rally, and all of them gain 15/25/35 morale.
  - **Command Aura:** +5/10/15 melee defence within 12 tiles.
  - **Challenge:** the Daimyo and an enemy hero within 8 tiles can be hurt only by each other for 6/8/10 s. Melee, volleys, towers and abilities from anyone else pass them by. If the Daimyo kills the other hero during the duel, his player gains 20 Honor. A champions' duel already under way is abandoned when either side's hero is challenged.
  - **Shogun's Banner:** every Clan regiment on the map gains 25 morale, +20% speed and can't rout, for 20 s.
- *The Ronin:*
  - **Iaijutsu Dash:** blinks up to 10 tiles and deals 60/100/140, spread, to each enemy regiment on the line crossed. The line is worked out from where the Ronin was and where it lands.
  - **Wind Cut:** 40/70/100 to each enemy regiment in a 90° cone 6 tiles ahead.
  - **Wanderer:** regains 2/4/6 hit points a second with no friend within 12 tiles.
  - **Thousand Cuts:** untouchable for 3 s while it deals 45 eight times to enemies within 4 tiles (360 in all, shared).
- *The Onmyoji:*
  - **Fire Spirit:** a Fire Spirit regiment for 30/40/50 s.
  - **Ward Talisman:** friends inside a 5-tile circle within 20 tiles take no missile damage for 8/10/12 s.
  - **Divination,** cut down: reveals 12/16/20 tiles within 30 tiles of the Onmyoji for 10 s, burrowed and hidden units included. "Anywhere explored" can't be checked in the simulation, because exploring is kept on each screen, not in the shared game.
  - **Kitsune Storm:** 20 damage and 3 morale a second to every enemy regiment in an 8-tile circle within 24 tiles, for 10 s.

**The Brood.**
- *The Queen-in-Waiting:*
  - **Spawn Brood:** 1/2/3 Spawnlings regiments beside her for 30 s.
  - **Royal Jelly:** heals a friendly regiment within 10 tiles 60/100/140.
  - **Pheromone Trail:** friends within 10 tiles +15/25/35% speed for 8 s.
  - **Swarm Tide:** Brood regiments within 20 tiles +20 melee attack and no morale lost to casualties for 15 s, and 4 Spawnlings regiments hatch.
- *The Devourer:*
  - **Gorge:** one soldier of a regiment it is fighting dies, and it heals 80/120/160.
  - **Grow:** +4/6/8 hit points for each enemy soldier who dies within 3 tiles of it, up to 30 soldiers.
  - **Burrow Charge:** blinks to a point within 16 tiles and deals 60/90/120 to each enemy regiment within 3 tiles of where it lands, disordering them as a charge does. It tunnels, but not under walls: it obeys the blink rule below.
  - **Maw of the Titan:** within 5 tiles of a point within 16, up to 12 soldiers die, taken from regiments of non-monster units, and monsters and heroes there take 200.
- *The Whisperer:*
  - **Mind-Spore,** cut down: an enemy regiment of at most 6/10/14 soldiers within 12 tiles drops its orders and can't be given new ones for 20/25/30 s. It holds its ground and fights back. Taking control of another player's regiment would change owners mid-match, which the game can't do safely yet.
  - **Confusion Cloud,** cut down: enemy regiments in a 5-tile circle within 16 tiles are stunned and disordered for 4/5/6 s. Making them attack anyone near would need a new target rule.
  - **Hive Sight:** each of the Brood's Pustules sees 10/14/18 tiles (`vision.rs` posts).
  - **Puppet Army,** cut down: every routing enemy regiment within 20 tiles is stunned for 10 s. They can't run, so they are cut down where they stand. Turning them would change owners, as with Mind-Spore.

**The Choir.**
- *The Maestro:*
  - **Crescendo:** friends within 12 tiles reload 15/25/35% faster and gain +10/15/20 melee attack for 10 s. This is not the power `Haste`, which also speeds strikes and building.
  - **Retune,** cut down: the Maestro blinks to beside one of its player's Resonators within 20 tiles (the power `Shift`'s spot search). Moving a building would re-cut the paths and the power fields mid-match.
  - **Harmony:** the player's chords within 20 tiles of the Maestro make +10/20/30% Grub (`chords::harmony_at`).
  - **Symphony:** the player's chords make +100% Grub for 30 s, and Resonance charges twice as fast.
- *The Refractor:*
  - **Split Beam,** cut down: its next 3 volleys deal +50/100/150% damage. Splitting a volley between targets needs a second aim in the volley code.
  - **Mirror Shield,** cut down: a friendly regiment within 12 tiles takes 30/45/60% less missile damage for 6 s. Sending the volleys back needs the same change as Split Beam.
  - **Lens,** cut down: a friendly non-Crystal regiment within 12 tiles gains +20/30/40% missile damage for 15/20/25 s. Refracting beams is not modelled.
  - **Prismatic Lance:** 250 damage shared by every enemy regiment on a 2-tile-wide line up to 40 tiles long, x1.5 to structures.
- *The Hollow Bell:*
  - **Toll:** stuns a regiment within 10 tiles for 1.5/2.5/3.5 s.
  - **Silence:** heroes within 6 tiles of a point within 14 can't cast, and their auras stop, for 4/6/8 s.
  - **Echo Step:** blinks 10/14/18 tiles.
  - **Deafening Peal:** every enemy regiment within 14 tiles loses 30 morale.

**The Carrion Concord.**
- *The Magnate:*
  - **Hostile Takeover,** cut down: for 120/180/240 s a neutral trade post within 8 tiles pays only the Magnate's player's caravans (`trade.rs`).
  - **Bribe:** not built until creep camps are, and not learnable until then. The Magnate tops out at seven points spent.
  - **Compound Interest:** every minute, 1/2/3% of banked Grub, at most 15/30/45.
  - **Market Crash:** every enemy's Grub income -50% for 30 s.
- *The Undertaker:*
  - **Raise Dead:** a Bound Dead regiment of as many soldiers as there are corpses within 8 tiles: at least 4, up to 6/10/14. It lasts 45 s.
  - **Corpse Explosion:** up to 4/6/8 corpses within 10 tiles burst, 30 damage to each enemy regiment within 2 tiles of each.
  - **Last Rites:** an undead regiment within 10 tiles heals 80/130/180 and gets 2/3/4 lost soldiers back.
  - **Mass Grave,** cut down: one Bound Dead regiment rises on its side and grows by one soldier for each soldier who dies within 10 tiles over 15 s, up to 20. One growing regiment rather than one for each death, so a big fight can't flood the regiment pool.
- *The Usurer:*
  - **Leech Debt:** drains 15/25/35 a second from a regiment within 8 tiles for 3 s.
  - **Lien:** a regiment within 12 tiles takes +15/20/25% damage for 10 s.
  - **Collateral,** cut down: an enemy regiment within 10 tiles loses its timed changes, and the Usurer gains the first good one for the time it had left.
  - **Foreclosure:** an enemy regiment within 10 tiles below half strength loses a third of its soldiers, back ranks first.

Cut down, listed again for the Guide: Challenge (only each other, not a locked duel), Divination, Mind-Spore, Confusion Cloud, Puppet Army, Retune, Split Beam, Mirror Shield, Lens, Hostile Takeover, Collateral, Mass Grave. Not built: Bribe (no creep camps).

## Stages

- **H2a, the engine and almost every ability:**
  - **The engine:** learning, casting with walking into range, cooldowns, ranks, auras, passives, the `buffs` slots and zones.
  - **Effects:** Damage, Drain, Cull, Heal, Morale, Buff, March, Steadfast, Stun, Untouchable, Summon, Raise, Blink, Income, Reveal for Hive Sight, and Grow.
  - **On screen:** the hero panel and the command grid's `A S D F` (click, then click the target), with tooltips from the roster.
  - **Computer players** learn in order and cast by simple rules.
  - **Stress scenario** `abilities`.
  - **What's left for H2b:** Silence, Collateral, Hostile Takeover, Retune, Divination, Mass Grave and Challenge (it needs untouchability tied to the champions' duel). They can't be learned until then, their points stay banked, and the computer players skip them.
- **H2b, the rest:** those seven. The golden replays are re-blessed again then.

## Real use

- **Blinks through passages:** a blink needs a clear line on the caster's own movement grid (`Nav::clear_line`), and ground at the point the hero fits on (`settle`, `fits`). That grid has the player's own gates open and enemy gates shut. So Echo Step can't skip a wall, an enemy gate, or a river without a ford, though a hero can blink through its own open gate. Burrow Charge obeys the same rule.
- **Summons and raisings at gates and in crowds:**
  - A summoned regiment is placed like a trained one, at the nearest free ground beside the hero (the spot search `raise_trained` uses), up to 6 tiles out.
  - The spot must also have a clear line from the hero, so a hero in a gate can't summon outside its walls.
  - With no spot, the cast is refused and its cooldown isn't spent.
- **Areas and allies:** harmful areas hit enemies only (`stances.hostile`), never allies or the caster's own side. Friendly effects reach only the caster's and allies' regiments.
- **On the march:** auras use the hero's position each second, so a brigade on the march keeps its aura while the hero rides in it. A stunned regiment in a brigade is held fast, as a trapped one is, and the brigade marches on without it.
- **Stuns and orders:** a stunned regiment keeps its orders and carries on when the stun ends (Mind-Spore excepted).
- **Determinism:**
  - Every effect is fixed-point and ordered by regiment index.
  - Ability damage never draws on the combat dice.
  - Everything is in the state hash: cooldowns, ranks, buffs, stuns, zones and casts waiting on range.
- **Computer players:**
  - **Learning:** in a fixed order per hero: the ultimate when they can, otherwise the first basic ability with ranks left, skipping ones not built.
  - **Casting**, checked each second:
    - heal the most hurt friend in reach;
    - hit the thickest enemy in reach;
    - buff when the lines meet;
    - stun the enemy hero in reach;
    - the ultimate when 6 or more regiments are fighting within its reach;
    - blink only away from a losing fight.
