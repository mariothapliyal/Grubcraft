# 45 · Prisoners after battles

Idea 14's missing half ([28](28-soldier-combat.md), "Aftermath and wear"), and Mario's step 3 for the heroes thread (2026-11-22: "Do number 2, then 3 then 4 then 1"). Today a regiment that breaks either rallies or is hunted down soldier by soldier. With this change, a broken regiment caught alone gives itself up. In Conquest its men are held at the captor's base, where they work, and their owner can ransom them back.

Reviewed on Opus before building (2026-11-28). The review's ten points are folded in below.

## The rules

### Taking prisoners (every mode)

- **The check:** once a second, after the morale checks, in regiment slot order.
- **Who gives up:** a routing or shattered regiment that has run for at least **5 seconds** surrenders when both of these hold. The 5 seconds are its head start: a regiment that breaks in the middle of a fight runs first and is taken only if it is run down, not where it broke.
  - an enemy fighting regiment's block touches its block at melee reach this tick. Workers can't catch; archers and riders can.
  - no fighting regiment of its own side (its owner's or an ally's) is within 8 tiles of it.
- **The catcher:** the touching enemy regiment with the lowest slot index. The prisoners belong to that regiment's owner.
- **Who never gives up:**
  - regiments that never rout (workers, most of the Concord's undead, the Brood's summoned swarms);
  - non-fighters such as Caravans (morale stat up to 30), which still drop their cargo when cut down;
  - heroes, which fall and are revived ([43](43-heroes.md));
  - summoned regiments, and in Conquest any unit nothing trains (`reinforces_as(unit).trained_at` is none, the same test reinforcement uses).
- **Same rule for every faction:** the Concord gives few prisoners only because its undead don't rout.
- **A surrender is not a death:**
  - It is told as `Event::Surrendered`, not as losses. It plants no torn standard, hands over no trophy, gives no hero experience, no Tithe and no graves, and counts as no kills.
  - The regiment leaves the field at once, as a shattered regiment that goes home does. In Conquest that frees its supply.
- **What you see:**
  - The men stand still for three seconds, then sink away (there is no kneeling move in the fight library yet).
  - The news says, for example, "12 Yari Ashigaru surrendered to Blue".
  - Field Battle results add a *Prisoners taken* line for each side.

### Holding them (Conquest)

- **The record:** each group is one record of captor, owner, unit, rank, stockade and count. Records are kept in the order taken and are in the state hash.
- **The stockade:** the captor's finished main building or outpost hall nearest the place of surrender, by squared distance, then by building slot. There is no march to it in stage 1: the prisoners are counted there at once.
- **Room:**
  - A main building holds **60** and an outpost hall **30**. Only the Clans and the Concord have outpost halls, so the Brood and the Choir hold fewer.
  - Prisoners that don't fit go to the next-nearest stockade with room. Those that fit nowhere are let go and leave the field.
- **Work:** each prisoner held works for **1 Grub a minute** for the captor, cut by upkeep like gathered Grub. For scale, a full seep gives the Clans about 210 a minute, Concord caravans about 170 and the Choir about 109 ([04](04-economy.md)).
- **Escape, when a stockade is destroyed:**
  - Its prisoners stand up there as their owner's regiments, in lots (below).
  - They come back only while their owner has supply and pool room. The rest are let go.
- **When either side is out of the match:** an eliminated owner's prisoners are let go, so escapees never bring a player back. An eliminated captor's prisoners escape, as above.

### Ransom (Conquest)

- **Who ransoms:** the prisoners' owner, paying from their own store, never an ally they handed their army to. It is done with the **Ransom** button in the top bar (shown while an enemy holds some of theirs), and the captor doesn't have to agree.
- **The command:** `Command::Ransom { from, unit }` ransoms the oldest record held by player `from` of that unit, one lot at a time.
  - It is refused if `from` isn't hostile, or if the owner has no finished main building, the supply, or the price.
- **Lots:** a lot is at most one full regiment of that unit (its soldiers at full strength). For example, 40 Yari held at 16 a regiment are lots of 16, 16 and 8.
- **The price:** the full reinforcement share of those soldiers, what reinforcing them would cost ([04](04-economy.md), "Reinforcement"). For a Brood evolution it is `reinforces_as`, both forms' costs. The Grub and Marrow go to the captor.
- **How they come back:** at once, at the owner's first finished main building, as one regiment at the lot's count and keeping its rank. A ransom costs the same as training new men and saves the training building, the queue, the training time and a Brood larva; what it buys is the veterans back. (The plan first had them wait out the training time; that needs a second queue beside the buildings' own, so stage 1 brings them back at once, at full price, and the balance runs below show what it does.)

### Computer players

- **Chasing:** generals already run down routers, so they take prisoners with no change.
- **Ransom:** a computer player ransoms a lot only when all of these hold, so it can't be farmed with cheap levies:
  - the regiment is veteran (rank 1 or more), or the player is short of army;
  - it holds that lot's price twice over, and the supply free;
  - it has paid no ransom in the last 60 seconds.

## Numbers (first pass)

| Rule | Number |
|---|---|
| No friend within | 8 tiles |
| Stockade holds | 60 (main building), 30 (outpost hall) |
| Work | 1 Grub a minute per prisoner |
| Run before giving up | 5 s |
| Ransom | the full reinforcement share, back at once |
| Computer players ransom | veterans or when short, at twice the price in hand, at most every 60 s |

## Balance

A caught router now leaves whole instead of rallying later, and a hunted one leaves at once instead of man by man. That shifts who wins fights, so this is a balance change and is measured before it merges:

- the balance tournament (`BALANCE=1`), with and without surrender;
- the Field Battle table;
- AI Conquest games.

Mario sees the numbers before the pull request merges (2026-11-27: no balance changes without his OK).

## Stages

1. **The simulation, the screen and computer players** (built 2026-11-28). The capture rule, records, ransom and escape (`gc_sim::prisoners`, `Command::Ransom`: replay tag 56, wire tag 54; new rules number, replay format and net protocol); the Ransom button, the news, the stand and fade, and the Field Battle line; computer players' ransoms (`gc_ai` steward). Checks:
   - sim tests (`gc_sim/src/prisoner_tests.rs`: caught alone, a friend near, a summoned unit, ransom at rank, work, escape, a full hall, Field Battles);
   - the stress scenario `prisoners`: Skitterlings run down by Clan horse give up, are held at the Clan Hall, ransomed back at the Hive, and freed when the hall falls;
   - `ai_only_generals_take_prisoners`: Generals take prisoners in a Conquest match.
2. **Drawn stockades.** A pen of prisoners beside each stockade, drawn only, with no simulation change.
3. **Later, only if Mario wants it:** the march to the stockade, as a column the enemy can free.

## Open call for Mario

What prisoners are for. The recommendation is **ransom and work**, as above. The other options are ransom only (no work), or also letting the captor *recruit* them into their own army. Recruiting would put another faction's units in an army, which the factions' tech and twists aren't built for.
