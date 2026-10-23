# 32 · Animation engine

*Stage 3 of [28 · Soldier combat](28-soldier-combat.md), the engine half of the "Materials, animation engine and fight library" thread of the [restart plan](29-restart-plan.md). This is the plan; each part is marked built when it lands.*

## Where it stands

A soldier plays one clip at a time. `gc_artbake` samples each clip at 12 frames a second into the model's own rows of the bone animation texture (a 3×4 matrix per bone, bind pose already folded in), the instance names two rows and a blend, and `cs_skin` blends each vertex's bones from those two rows. The game picks a clip by `ClipKind` (ten kinds) and `gc_game::models::Clips` turns an action into a `Pose` (two rows and a blend). That is why every model carries its own copy of the stock clips, why a soldier cannot swing his arms while his legs walk, and why a flinch cannot lie on top of a swing.

## What the engine adds

- **Moves, not kinds.** A clip carries tags: a *move* (guard, attack, parry, hit, stagger, knockdown, death, get up, footwork, matched kill...), a weapon type, a direction (front, left, right, back, high, low), a mirror flag and a layer role (full body, legs, arms, additive). The ten old kinds stay as the default for each move, so every model already in the pack still works.
- **A move picker.** `Clips::pick(model, want)` takes what the fight wants (a move, the weapon, the side a blow came from, how it ended, a seed) and returns the best clip the model has, falling back along the old candidates list. T4 keeps `puppets.rs` and its `Move` enum; the picker is the one call that grows underneath it.
- **Shared clips per skeleton.** Models built on the same skeleton (the Quaternius 62-bone rig under most humanoids; each Brood creature has its own) share one set of clips, stored as joint poses, not as finished matrices. Each model keeps a small table of its bind-pose differences, which is what the bake folded in before. A fifty-move library for a weapon type costs about a megabyte per skeleton instead of per model.
- **A pose pass.** A compute pass before skinning (`cs_pose`) builds each soldier's final bone matrices once, a thread per bone, into a palette buffer, and `cs_skin` only reads the palette. Today every vertex re-blends its bones' frames itself; the palette makes skinning cheaper as well as richer. The pass mixes, in order: the base move, a blend from the previous move, a second move on masked bones (legs from one, arms from another), additive flinches scaled by weight, then the post fixes: look-at on the head and chest bones, two-bone foot IK on legs against the ground height.
- **Mixed speeds and mirrors.** Each soldier plays a move a few percent faster or slower and, where the rig is symmetric, mirrored.

## Format: model schema 5, packs stay readable

The loader accepts schemas 4 and 5. Schema 5 adds optional tables (skeletons, joint-pose clips, move tags, bind differences, bone masks and roles); a pack without them loads as before and its clips become one-clip-per-kind moves. Building packs (one bone, no clips) and the scenery pack stay at 4 and are not re-baked. `models.pak` is baked once, together with the units' detail re-bake of [31 · Surface detail](31-surface-detail.md), so the 86 MB pack is written once.

## Stages

1. **Pose pass and shared skeletons** (no visible change). Schema 5 tables, palette pass, `cs_skin` reading the palette, a parity test that poses the same soldiers both ways. Sonnet, high.
2. **Layers and the picker.** Move tags in the baker (from glTF animation names such as `Attack_High.Spear`), masks, additive layers, blends, speed and mirror, `Clips::pick`. T4 is told when the picker lands.
3. **Look-at and foot IK.**
4. **Units re-bake** with detail (`detail_tile` about 0.15) and the new tables.
5. **Fight library**, Ember Clans first (about 50 moves for sword and shield and for spear), from Mixamo sets Mario downloads and Blender fitting; then the other human weapon types, Brood and Choir bodies.

Open call for Mario, as in doc 28: where the moves come from (default: Mixamo plus Blender).
