# 46 · First-person hero (Take command)

Mario, 2026-11-28: "a first-person kind of takeover mode where you can be the hero in first person, … fully comprehensive and threshed out, plan everything, do this perfectly." This is the plan. **Nothing is built yet.** Revision 2 (2026-11-28, same day): revised after a four-lens Opus 5.5 review (sim and determinism, net and prediction, game feel and balance, render and input), each claim checked against the code; the review's corrections are folded in, and the list of what changed is in [Review log](#review-log).

Heroes are built ([43](43-heroes.md), [44](44-hero-abilities.md)): a hero is a one-model regiment that fights, casts two abilities, levels and leads. This mode lets a player step into that one regiment: a first-person camera, walking and looking with the mouse, aimed swings, a guard, aimed abilities, and a way to call the rest of the army from the field. The simulation stays the only authority, so online play, replays and golden replays keep working.

## 1. Goals and non-goals

**Goals**

1. **Be the hero.** WASD and mouse look at the hero's own eye, your own arm and weapon in view, swing and guard, the two abilities aimed with a crosshair, in the middle of the battle the game already simulates.
2. **Still a strategy game.** Leaving first person is one key, the army can be called from inside it, and the army never waits for you.
3. **Deterministic and online-safe.** Everything that changes the sim goes through `Command`s in the lockstep stream and is recorded in replays. Camera, mouse look, bob and sound are presentation only.
4. **Not a free power-up.** Taking command gives control and aim, not a hidden stat boost. Perfect play beats the autopilot hero by a small, measured margin (6.8). Mario's heroes rule stands: "strategic assets, not invincible; three regiments should kill one in about 30 s".
5. **Cheap.** No new sim cost beyond one hero's steering; a frame no slower than the existing ride view within 10%.

**Non-goals for the first release**

- Possessing ordinary regiments or single soldiers.
- Field Battles (heroes reach them in H3). Conquest only; `Steer`, `Strike` and `Guard` are refused unless `hero_of(regiment)` is a hero.
- Controller support (the input layer is written so a pad can plug in), key rebinding (the game has none), VR, split screen, observers (the game has none).
- Climbing walls, manning towers, hitting structures with a swing. Heroes do not man walls; a structure is attacked with an ordinary Attack order from the army wheel or the commander overlay.
- Reversed-Z or other renderer-wide depth changes.

## 2. How it works on one page

- **Taking command is a client mode, not sim state.** The sim has no "possessed" flag. A player in first person has the camera on the hero, the mouse grabbed, and the keyboard sending **steering commands** for that hero. The sim keeps a small *steering side table* for heroes that have been steered (hashed only while it is non-empty, 6.1). Leave first person, or lose the connection, and the commands stop; the hero stops and fights on its own like any unordered regiment.
- **Three new commands, one sim PR:** `Steer` (movement, facing, sprint and dodge), `Strike`, `Guard`. Abilities use the existing `Cast`.
- **Prediction is visual only, by input replay** (section 5): your camera, your hero's drawn body and your arm move at once from your own input, and the sim's authoritative hero pulls them home.
- **The army** is called with a radial wheel and a map overlay, which send ordinary `MoveTo`, `AttackMove`, `Hold`, `FallBack` commands (section 7).

## 3. Controls

**Entering and leaving.** Select the hero and press **F6**, or press the new **Take command** button on the hero panel, or double-click the portrait. From the ride (**Y**), **F6** steps in. **F6** is the only way out of first person (Esc does not leave it: the ride's "Esc leaves" rule is deliberately not copied, because Esc is the menu and the wheel's cancel). F6 is free today. The hero panel's F6 comment in `gc_game/src/hud.rs` is stale (the preview is Shift+H); PR A fixes it.

**The whole key map** (every key `main.rs` handles today has a rule in first person; a stray key must never become an order):

| Input | In first person |
|---|---|
| W A S D | Move forward, left, back, right, relative to where you look. Back and sideways pace are 60% and 75% of forward. |
| Shift (hold) | Sprint: 150% pace, drains stamina (6.4). Hold or toggle in settings. Not available in melee contact. |
| Mouse | Look. Yaw turns the camera at once, body follows; pitch is camera only (clamped 80 degrees). |
| Left mouse | Tap: light swing. Hold and release after 0.6 s: heavy swing. Ranged heroes: aimed shot (6.5). |
| Right mouse (hold) | Guard (melee heroes). |
| Space | Sidestep (a dodge, 6.4). |
| 1, 2 | Hero ability 1 and 2 (the A and S slots of the panel), aimed with the crosshair (6.6). |
| Q (hold) | Warlord's wheel: release on a slice to order the army (section 7). Digits 1 to 9 while it is open pick the control group. |
| E | "To me!": the active group marches to you. |
| Tab (hold or tap) | Commander overlay (section 7). Takes the key before `switch_side`. |
| F6 | Leave first person. |
| F2 to F5 (faction powers) | Powers that need a click on the ground or a shrine are aimed with the crosshair; the ones that need neither work as now; any that cannot be aimed say so in the status line. |
| Esc | Cancels in this order: the wheel or overlay, an armed cast, the help/guide/build screens, then the menu. Esc never leaves first person. The menu releases the mouse. |
| P, Enter, H, U, M, N, F1, F7 to F12 | As now; any screen that takes the mouse releases the grab while it is open. |
| Delete, Backspace, Y, Backquote, Numpad + - Enter, digits 0 and 3 to 9 outside the wheel | **Swallowed** (Delete would send an order, Y would toggle the ride, digits would recall a group and jump the RTS camera); Pause works as now. |
| R F Z X C V B O L G K T I, `,` `.` `-` `=` `[` `]`, Home, arrows, wheel, middle-drag, gestures | **Swallowed.** None sends an order or moves the RTS camera; pressing a grid or tool key shows "Leave first person (F6) for orders". |

Other rules: the RTS camera is not moved by edge scroll (the cursor position is cleared when the grab starts), arrow keys, middle-drag, gestures or the wheel while in first person, and `camera_mut()` no longer exits a mode here (it still exits the ride). The grab is released, and the mode kept, by the menu, chat, focus loss, alt-tab, pause overlays, the Guide, and the match end; it is re-taken by a click after focus returns. Death ends the mode (4.4).

**Settings (new, all `#[serde(default)]`, so old files load, with a round-trip test):** mouse sensitivity (per platform scale), invert Y, field of view (default 75 degrees vertical, 55 to 100), head bob and weapon sway (default low; "reduced shake" turns shake, bob, sway and kicks off), hold-or-toggle for Guard and Sprint, double-click-portrait to take command, "Take command (preview)" in PR A only.

**Mouse grab and platforms (winit 0.30.13; the repo has no cursor-grab code today and the winit source is not in the local registry, so this is the documented 0.30 API, proven in F1):** try `CursorGrabMode::Locked`, fall back to `Confined`, hide the cursor, always read raw `DeviceEvent::MouseMotion` deltas, and recentre the hidden cursor only on X11 and Windows when confined (never on Wayland, where `set_cursor_position` is unsupported but `Locked` is). macOS raw deltas arrive accelerated and Windows ones do not, so sensitivity is degrees per count with a per-platform factor measured in F1, not one number. F1 checks X11 (Xvfb with xdotool), Wayland (a Weston or GNOME session) and, through Mario's Mac builds, macOS.

## 4. The camera and how it looks

### 4.1 A view-mode enum, not a third camera variant

`ActiveCamera` has `Rts` and `Ride`, and the code often asks `self.ride.is_some()` or reads `self.camera` directly. About sixteen places do (shadow range, the Commander View switch, mist, tilt-shift and sun shafts, `view_focus`, the selection-plate skip, `body_under_cursor`, the order tabs and chevrons, `hud_cover`). A `Hero` camera variant alone would miss them (in first person with the RTS camera zoomed out, soldiers would turn into banner blocks; mist would puff around the eye). So PR A replaces `Option<Ride>` with a **view-mode enum** `{ Rts, Ride, Hero }` with predicates (`low_view()`, `embodied_regiment()`), and the PR lists every site and the behaviour each has in Hero mode: no Commander View (soldiers are never banner blocks), mist from the eye, tilt-shift and shafts as in the ride, no plate on your own regiment, centre-ray picking, the RTS order tabs hidden, the world drawn under the (hidden) bar.

- **Eye:** at the hero's head, from a **head-joint track** (4.2), plus the predicted position, plus the look direction from mouse yaw and pitch. Heading is not damped from the sim's facing (the mouse owns it).
- **Lens:** vertical field of view 75 degrees by default; near plane chosen by measurement (0.12 to 0.15 tiles is expected to be enough once the head is hidden); **far plane stays 1300** like the ride (fog is `1 - exp(-d * density * air)`, so it does not hide everything past 600 tiles; at 600 tiles it hides only about 37 to 62%). The depth buffer is a shared standard-Z `Depth32Float`; reversed-Z would be a renderer-wide change and is not planned.
- **Picking:** a screen-centre ray. `body_under_cursor` excludes the embodied regiment.
- **Sound:** the listener sits at the eye with the ride's balance trick. Footsteps, breath and the swing whoosh are new cues (4.5).

### 4.2 Your own body (the eye, the head and the arm)

- **The eye follows the head joint.** Skeletons are posed only on the GPU (`cs_pose`), so the CPU cannot ask where the head is, and fight-library clips (lunges, staggers, the Daimyo's rearing and slashing) move the head well away from a fixed fraction of the model's height; `height` also includes hats and crests (the Onmyoji's tall eboshi, the Daimyo's horse and rider). So the bake (`gc_artbake`) writes an **eye track** per model and clip (the Head-role joint position in model space at the clip's 12 frames a second), and the camera samples the track of the clip the hero's body is playing, smoothed with a critically damped follow (about 0.08 s). Heroes that stand and walk have small tracks; staggers and deaths use theirs, so the camera is never left behind a falling body.
- **The head is hidden by a mask, not by back-face culling.** Back faces are culled only on the textured pipeline, which a model uses only on GPUs with BC texture compression and a material pack; elsewhere `cull_mode` is `None` and the inside of a head would show; some heroes' heads are loose parts (the Maestro's tuning-fork crown, the Refractor's prism and lens discs). So the **hide mask is the primary design**: `SoldierInstance` is exactly 64 bytes (a compile-time assert), so the mask is a **uniform naming the one hidden instance** (primary; the tip and fall bytes of `flags` are written for falls and slope tilt, which is exactly when the eye stays on a falling body, so they are not free), with the top bit of `look_weight` or the tone byte as the alternative; `cs_pose` collapses every vertex weighted to the Head-role joint (`rig_of` already finds `Role::Head`) and to its children, so the eboshi and crown go with the head. The horse of the mounted Daimyo is drawn; its neck is below the line of sight at the default pitch (checked in the screenshot).
- **The drawn body is the predicted body.** Your hero's drawn instance (position and facing) uses the same predicted pose as the eye, and the decaying correction applies to both together, so the eye never runs ahead of its own body (on a joiner's link that gap would be about half a tile). Every other regiment is drawn from the sim as now. In PR A (no predictor) the eye rides the interpolated sim body exactly, so there is no gap.
- **The upper body is driven locally.** Your arm and weapon poses start from your input (a local timeline starting on the click, and on the guard press), not from sim events, so they respond at once. The sim's `Blow` events only **confirm** a hit (hit-stop, sparks, sound), **cancel** a local swing the sim refused (cooldown, stamina, stun, no contact: the arm eases back and a "refused" cue plays), and drive the **automatic** blows the player did not ask for. The legs and the rest of the body follow the predicted movement.
- **Your hero is never cut by the skin budget.** The skinned-vertex budget (`SKIN_VERTICES`, 4M) coarsens LOD0 runs first and then drops soldiers from the last batch; in a crowded battle your own body could be coarsened or lost. The embodied hero has its own first run, pinned at LOD0, outside the budget.
- Your own shadow is drawn (4.6); no view-model pass, because the body is a real object.

### 4.3 Body and view

The sim's hero turns at its `turn_rate` stat (160 to 240 degrees a second; 160 for the Queen, Magnate and Hollow Bell), and a regiment locked in melee turns at half that toward its foe. A mouse flick is faster than both.

- The camera is exact: it turns with the mouse every frame.
- The body follows through `face` in `Steer` (6.1): the possessed hero turns at **1.5 times** its stat when free and at **1.0 times** its stat in melee contact (two times the engaged default). The predicted body turns at the same rate, so what you see is what the sim does. The view warns when the body lags by more than 30 degrees (the crosshair drifts open).
- Blows use the aim carried by `Strike`, not the body, so a flick-and-swing is not wasted.
- The sim's side rules (front 100%, flank 140%, rear 180% damage) use body facing, so facing matters honestly, and so does turning your back.

### 4.4 Death, stun, cocoon, revive

- **Killed:** the camera stays on the eye track as the body falls, then lifts to the RTS view after 3 s with the revive timer on the hero panel. The mode ends at death; the panel button re-enters after the revive.
- **Stunned, trapped, cocooned** (`stunned_left`, `trapped_left`, `cannot_act`): movement and actions are locked, look stays free, with a short shake (not under reduced shake) and a vignette; the HUD says why (the sim raises refusal events with reasons).

### 4.5 Feel layer (presentation only)

Sprint widens the field of view by 6 degrees over 0.2 s. Footfalls bob the eye by 0.03 tiles. A swing sways the view and the arm; a confirmed blow gives 50 ms hit-stop, a small kick, and the existing blow sparks and sounds; being struck kicks the view toward the blow and flashes a directional marker; low hit points desaturate the screen edge. Guard raises the weapon edge into view. All driven by `Blow` events and the hero's state. Under reduced shake: no bob, sway, kick or shake.

### 4.6 Shadows and the near view

First person gets its own shadow range of about 35 tiles (the ride's 100 with one 2048 map gives texels of about 0.1 tile, too blocky for your own arm and weapon), and the shadow fit is extended toward the sun so casters near and behind the eye (your body) are kept. A screenshot looking down at your own shadow is part of F1. Non-embodied friendly soldiers within 0.6 tile of the eye are not drawn.

## 5. Latency and prediction

Measured order lag (docs/11, 2026-11-22), click to simulating frame: host 73 to 77 ms, joiner on the same computer 64 ms, home Wi-Fi 111 ms, Tailscale across NZ 140 ms (p95 169), Sydney relay 229 ms (257), a poor link 372 ms (424). The input delay is 1 to 8 turns (a steady link runs at 1 turn, 50 ms; a match starts at 2), chosen from the jitter of the most uneven link, so one player's bad connection raises everyone's. For a joiner the lag is mostly the round trip, not the delay. That is fine for RTS clicks and bad for steering a first-person body.

1. **Look is never delayed.** Mouse yaw and pitch go to the camera at once.
2. **The body is predicted by input replay.** The client keeps a FIFO of the `Steer`s it sent. When a played bundle arrives (`next_turn` returns the commands, tagged with the player), it reads the highest `seq` among its own `Steer`s in that bundle and drops every unacknowledged `Steer` at or below it (the link is reliable and ordered, and the host keeps each player's order). Acknowledging by sequence number, not by counting, keeps the FIFO right when the host coalesces two sends into one (5.6) or drops a `Steer` from a catching-up peer (section 8). The authoritative hero position plus a replay of the *unacknowledged* `Steer`s, run through a small presentation predictor (a hero's pace through read-only terrain walkability, no crowd model), is the predicted pose. This follows the real horizon (round trip plus buffer plus delay for a joiner; the delay for the host), so a delay step needs no special code. It is **not** "authoritative advanced by the delay", which would be wrong for joiners.
3. **The sim pulls the view home.** The difference between the predicted pose and the replayed authoritative pose is a correction that decays at 6 a second (12 a second in melee contact), with a hard snap above 2.0 tiles and across a teleport (a Dash). The predictor stays on in melee at the 60% contact pace with hysteresis on the engaged state; no mode switch at the contact boundary.
4. **Blows hide lag in the wind-up.** A light `Strike` has a 4-tick (200 ms) wind-up and a heavy one 6 ticks (300 ms) in the sim, resolved at apply tick plus the wind-up. The local arm starts at the click and its wind-up is stretched by the measured lag, capped at 0.25 s, latched per swing at the click (it is not re-read mid-swing).
5. **Measured lag does not exist yet.** `Lan` exposes only `input_delay()`; link RTT and jitter are private. F2 adds `Lan::order_lag()` (click-to-apply per own command, an exponentially weighted mean and a p90, from the same FIFO as the prediction) and shows delay and lag on the first-person HUD, as `status()` already does elsewhere.
6. **The stream (budget in bytes per turn and per hour):**
   - `Steer` is sent when *movement intent* changes (a key, the gait, or the move direction by 8 of 256 steps), plus `face` updates at most every 5 ticks (4 a second) and only while a movement key is held, or within 2 s after a movement key, a `Strike` or a `Guard` (an engaged hero whose player is only looking sends nothing). Looking around while standing still sends nothing, so a player who only looks around has the existing autopilot hero.
   - It is about 8 bytes on the wire (player, tag, varint handle, two quantised angles, gait). Worst case about 6 a second with keepalives is about 50 bytes a second, 1 to 3 bytes a turn on average, and about 180 KB an hour of history and replay (the bundle cap is 900 bytes a turn). The F2 measurement is an assert in `crates/gc_net/tests/order_lag.rs`.
   - **Expiry and keepalive:** a steer lasts **40 ticks (2 s)** after it is applied; the client resends a keepalive every 10 ticks (wall clock online, sim ticks offline, so sandbox speed changes do not starve it) and sends an explicit **Stop** the moment keys are released (never waiting for expiry). 40 ticks outlasts a 250 ms late turn, an alt-tab hitch and the poor-link jitter, and the host catches up from hitches (`MAX_HOST_LAG`).
   - **No Steer while the match is on hold:** the client sends none during a kit pause, a "Pause and wait" hold (up to 90 s), or catch-up. The host **coalesces**: a newer `Steer` for the same regiment from the same player replaces an unclosed older one in the inbox, and a coalesced `Steer` does not count against `MAX_QUEUED_ORDERS` (16 times the army cap, so real orders are not silently dropped). Stale `Steer`s therefore never burst in on resume.
7. **No sim rollback is added, ever.**

**Honest limits.** A joiner on a slow link has a heavy, late hero. The Guide says so. **Host advantage in duels:** the host's `Guard` lands about 75 ms after the click and a joiner's lands 111 to 372 ms after, while a light swing's wind-up is 200 ms, so on most links a host can raise a guard against a joiner's swing it has seen and a joiner cannot. Computer-player and soldier blows have no telegraph, so this matters only in hero against hero fights (rare in Conquest). The plan does not hide it: the F2 measurement includes a host-versus-joiner duel on the relay profile, and the options (host possession commands delayed by the worst joiner lag, or longer wind-ups) are a decision for Mario (section 13). The default is no change.

## 6. Simulation changes (stage F2 and F3, one sim PR)

All three commands are player orders (listed in `Command::is_player_order`, or the host drops a joiner's), checked by `commands()`/`stances.can_command` like every hero order, and **refused unless `hero_of(regiment)` is Some**. All math is integer or `Fx`; angles are `Angle(u16)` quantised by the client. A scripted `Steer` that the sim refuses is a golden failure (the hero goldens assert zero rejected commands).

### 6.1 `Command::Steer { regiment, move_heading: Angle, face: Angle, gait: Gait, seq: u8 }`

`Gait` is Stop, Walk, Sprint and Dodge (a one-shot, refused while on cooldown or without stamina; `Guard { up }` on a ranged hero means the steadying crouch, 6.5). `seq` is a client sequence number (wraps), stored with the steer; the client acknowledges by the latest `seq` of its own commands it sees applied (5.2). `move_heading` is where the legs go, `face` is where the body turns, independent (strafing and backpedalling keep the front to the foe; back and side paces are 60% and 75%). Applying it stores a record in the **steering side table**.

- **State is a side table, not a field-list entry.** Every field in the `regiment_fields!` list is hashed for every slot, so adding fields there would change every golden's hash from tick 0. The record (`move_heading`, `face`, `gait`, `until`, stamina, guard, pending strike, last-hit-time, break-out cooldown) lives in a `Steering` table beside `Heroes` (at most `MAX_HEROES` = 3 per player), hashed Stances-style (writes nothing while empty, else a count plus entries), copied in `fork` and `copy_from` for scratch worlds. A hero never steered costs nothing and hashes as today.
- **Proof the legacy path is untouched:** before `scripts/rules-bump.py` blesses the goldens, F2 runs `cargo test -p gc_replay --test goldens` against the *unblessed* goldens and requires it to pass (the bump blesses, which would hide a broken legacy path).
- **A `steered(i)` predicate** (true for a hero with a live steer whose gait is not Stop, or a Stop steer sent within 2 s of a `Strike` or `Guard`) is threaded through the places that read "no target" as "idle":
  - `combat.rs` `before_move`: the press-back guard (`take_on` when a regiment with no target and no attack target is out of reach) must not fire for a steered hero, which would chase the foe it just walked away from, and combat's `target_facing` writes for engaged regiments must not override the steer's `face`;
  - `regiments.rs` `step_movement`: the steered branch goes **before** the engaged early-exit; it moves through the same `terrain.block_going`, crowd and spacing path as a marching regiment (cliffs, water, walls, woods, terrain speed), without `order()`, `leave_off`, `stand_back` or re-pathing;
  - `Brigades`: a new `pub(crate) fn drop_member` that wraps the existing private `leave` (which clears `step_with` and `files_floor`) and adds `step_arc`, called on the first `Steer` with a gait other than Stop (today leaving is detected lazily, and a member with no target is given its brigade target back each tick); a test asserts a marching hero, once steered, has no brigade after one tick and is not given a target again;
  - ranged auto-fire (6.5), plan and queue: the first `Steer` with a gait other than Stop clears the Shift queue and any attack-move hunt once (not on keepalives, not on a face-only Stop); `advance_orders` must not start queued steps under the player.
- **Casts:** `Steer` is **not** in `redirects()` (that would call `call_off_cast`, which clears every pending cast, including in-range casts that go off later the same tick). `Steer` cancels only a walking pending cast (`pending.toward.is_some()`), and only when it changes the move direction. A steered hero never walks to cast: an out-of-range `Cast` while a steer is live is refused with the normal refusal, and a test applies `Cast` then `Steer` in one tick and asserts the cast goes off.
- **Gates:** gates open for a side's marching regiments within `GATE_REACH` (4 tiles) "with somewhere to go"; the steer's move heading counts as somewhere to go (`steer-gate` stress).
- **Stun, trap, cocoon, `held_fast`:** held as for any regiment (turn only); the steer is kept and resumes if still within `until`.
- **Walls and towers:** heroes do not man walls; `on_wall` regiments are skipped by `step_movement`, so none applies.
- **Leadership, the hero's fall, experience:** unchanged.

### 6.2 Melee while steering: free movement, honest costs

Today an engaged regiment cannot move. For a steered hero that would make first person unplayable.

- A steered engaged hero **moves at 60% pace** and turns at 1.0 times its stat (4.3). The foe regiment keeps its lock while the gap is under `LET_GO` (2 tiles) and keeps striking.
- **Walking out of a lock costs what a break-out costs.** Moving out beyond `LET_GO` from a lock calls a **steered break-out**: `breaking_out` and `falling_back` set for the parting blows and relock immunity (up to 8 s or until `CLEAR_TILES` clear), **without setting `target`**, so the legacy march path never takes over (the existing `break_out` needs a target and would put the hero back on the `target` march for 8 s); the steered branch carries the movement, so the hero follows the steer heading during the break-out, and a turn is followed. After a break-out ends there is a 4 s cooldown before another can start, and `Strike` is refused while `breaking_out`.
- **Sprint is refused in melee contact** (as `run()` is refused today when engaged); you leave by walking out (with the break-out cost) or dodging.
- **Strike needs contact:** it is refused unless the target soldier's regiment is engaged with the hero or touching it (within `REACH`), so a hero cannot hit and run.
- These are **changes to the hero rules Mario retuned on 2026-11-22**, so they are listed in section 13 as decisions for him, with the exploit check "strike, disengage, strike" in the gate (6.8).

### 6.3 `Command::Strike { regiment, aim: Angle, power: Power }` (melee heroes)

**The aimed blow replaces the automatic blow while the hero is steered; it does not add to it.** The first blow rate is the same one a second the hero has now, so the player's skill shows in aim, timing and target choice, not in extra damage by mashing. (A first draft added aimed blows on top; the arithmetic put perfect play near 190 to 300% of autopilot damage and was dropped.)

- A hero that is steered and in contact **stops striking automatically**; its blows come from `Strike`, and if the player sends no `Strike` for 2 s while engaged and in contact it resumes the automatic blow (a floor, so a player who freezes mid-fight is the autopilot hero, not a statue; face-only steers do not count as doing something).
- **Light:** wind-up 4 ticks, then one blow at the enemy soldier whose derived slot position (`slot_pos`) is nearest the aim line within reach plus 0.5 tile and within 40 degrees of `aim`, through a new function `aimed_blow(i, j, slot, power)` that reuses `hit_percent`, side percent (that soldier's regiment facing against the hero), the hero level bonus, veterancy, `HERO_EXPOSED_PERCENT` on the receiving side, the winded factor, and `combat_rng` in **one fixed place**: inside `after_move`, after the regular melee loop, in hero-slot order (so other fights' rolls do not shift). Interval 16 ticks (0.8 s): at most 125% of the automatic rate if every swing connects. Stamina cost 0 (stamina is for sprint, guard and dodge, so swing speed is not capped by mashing and the 125% is the cap).
- **Heavy:** hold, release after at least 12 ticks, wind-up 6 ticks, 1.6 times the damage, interval 28 ticks (1.4 s): at most 114% of the automatic rate, plus `HERO_SHOCK` doubled (8 morale) on the regiment struck. Its value is the finishing blow on tougher men and the morale shock, not a higher rate.
- **No stagger or disorder in the first release** (a regiment-wide effect cannot come from one soldier's blow; `trapped_left` is per regiment and would freeze a hundred men).
- **Whiff:** no soldier in the arc: no damage; the interval still runs; a new **`Event::Whiff { striker, aim }`** in `events.rs` (`Event` is `Copy + Eq`; `Blow` is not an `Event` but `combat::Blow`, read through `World::blows()`, and its `target` is required) lets the presentation play it.
- The pending strike (aim, power, resolve tick) is in the side table.
- Soldier slot positions are derived from the regiment block and formation, deterministic, no new state.

### 6.4 `Command::Guard { regiment, up: bool }`, stamina, sprint, sidestep

- **Stamina** is a hashed counter in the side table, 0 to 100, regenerating 12 a second while the hero is not sprinting or guarding. It is spent by sprint (15 a second), blocking (half the damage a blocked blow would have done) and dodging (20 a dodge).
- **Sprint** (possessed): `Gait::Sprint` is 150% pace and drains stamina; at 0 the hero is forced to walk until stamina is 20. It does **not** use `Stride` or the winded state, which are one-shot 8 s bursts with a 10 s penalty and would collide with a second fatigue system; `Stride` stays for RTS `Run` orders.
- **Guard:** while up, **front** melee blows land at 50% damage (a hero's usual 130% exposure becomes 65%) and front missile hits at 70%. The arc is its own integer angle test: the striker is within **plus or minus 60 degrees** of the body facing (`facing.delta_to(Angle::of_vec(striker - hero))`), independent of `Side`, which is a rectangle test that gives plus or minus 45 degrees for a 1.0 by 1.0 hero and about 35 for the 1.75 by 2.5 Devourer. Flank and rear blows are not blocked. Spells, fire and poison are not blocked.
- A guard **break** (stamina 0) holds the hero 1 s (turn only) and drops the guard. While up: movement 50%, no `Strike`, no stamina regeneration, abilities allowed, sprint not allowed.
- Readability: the sim's enemy regiments strike on a fixed turn once a second with no visible telegraph, so "guard when struck" is a hold, not a read. The presentation shows the next strike of each engaged foe (it is deterministic sim state) as a small wind-up cue on the striking file. A **parry window** (a guard raised within 3 ticks before a blow blocks 75%) is a stage F5 option if playtests ask.
- **Sidestep (Space):** a dodge, not an escape. The hero moves 2.5 tiles in 0.3 s along the current move direction (back if none), stamina 20, cooldown 2 s. It does **not** call `break_out` and gives no lock immunity: foes within `LET_GO` keep their lock. Its use is positioning (to a flank, around a blow), and it cannot cross blocked cells or a bridge's edge.

### 6.5 Ranged heroes

Only two heroes have a missile weapon: the Onmyoji (range 22, damage 14, reload 2.5 s, accuracy 70) and the Refractor (range 26, damage 18, reload 2.0 s, accuracy 80; the other numbers in older notes, 24, 16, 10 and 8, are ability ranges). The Daimyo, Ronin, Queen, Devourer, Whisperer, Maestro, Hollow Bell, Magnate, Undertaker and Usurer are melee fighters in first person, with their abilities as their reach. Today a ranged regiment that is not engaged and has no `target` looses a volley at `pick_target` when `reload` is zero (a volley targets a *regiment*, `loose(regs, j, shot, tick)`).

- **`Strike` for these two** looses one volley at the **regiment under the aim** (the nearest visible enemy regiment whose soldiers lie near the aim ray within range), with `shots = models (1)`, the roster's damage and accuracy, gated by `reload` and `cannot_act`; light and heavy are the same shot (heavy waits 0.6 s for 10 more accuracy points, capped at 95). Cover, walls and the existing impact path apply. No Guard for them (right mouse is a steadying crouch that adds accuracy and halves pace).
- Auto-fire is suppressed while the gait is not Stop (a moving steered hero does not shoot at its own choice), as a marching regiment does not; a standing hero fires as now at the nearest.
- `Strike` for a ranged hero in melee contact is a melee blow like any hero's.

### 6.6 Aiming abilities

`Command::Cast` is unchanged. The client converts the crosshair: a point ability gets the ground point under the centre ray, clamped to the ability's range; a regiment ability gets the visible enemy regiment under the crosshair (the sim re-checks vision); `Aim::Hero` casts at once. Movement abilities (the Ronin's Iaijutsu Dash, the Devourer's Burrow Charge) blink along the aimed line, and the camera snaps (no smoothing across a teleport). Out of range: refused (6.1), with the status line saying how far.

### 6.7 Wire, replay and version numbers

- Wire tags **55, 56, 57** in `gc_net/src/wire.rs` (tag 54 is Ransom) with `command_size`; replay tags **57, 58, 59** in `gc_replay/src/codec.rs` (the replay codec numbers separately: Cast is 55 and Ransom 56 there), behind a `FORMAT` bump; three entries in `KINDS` (47 to 50) in `gc_replay/tests/goldens.rs`; a wire test that every `Command` variant has a unique tag. Take the numbers fresh at PR time (SIM_VERSION is 103, FORMAT 60, PROTOCOL 51 at main now) with the `sim-next` merge turn and `scripts/rules-bump.py`.
- Golden: `conquest-heroes-possessed`, recorded from a new `gc_game` script modelled on `hero_golden.rs` (its `Script` asserts zero rejected commands), covering `Steer`, `Strike`, `Guard`, a break-out and a cast.

### 6.8 Balance gate (Mario's rule: no stat tweaks without his OK)

Nothing here ships without the gate shape approved by Mario **before PR B starts**, and the final numbers approved before it merges.

- **Method.** Extend `stress/heroes.rs`, which measures only hit points taken and kill time for a Daimyo in four fights. The new `possessed` scenario adds a faction-generic hero helper (the current `fresh_daimyo` is Clans against Brood only) and measures, per fight, **damage dealt** (enemy hit points lost, men killed) and the **outcome** (hero dead, survived, or the foes broke), at least **16 seeds per hero per script**, gating on **medians**, with survived fights counted at the 120 s cap. Four fights are below the noise of a plus or minus 25% threshold.
- **Scripts:** autopilot (no commands); possessed-idle (face-only steers, mouse moving, no keys: must be within 10% of autopilot, since the first `Steer` of a looker never fires); perfect play (face the nearest, light swing on cooldown, heavy when a regiment is bloodied, guard against the front, walk out with the break-out cost at 40% hit points); poor play (walk into the middle, never guard); and the **exploit** script (strike, disengage, strike).
- **Targets:** perfect play deals at most **125%** of autopilot damage and survives at most **150%** as long; poor play survives at least **75%** as long; the exploit script must not beat perfect play. The arithmetic behind 125% is 6.3 (light 0.8 s interval, heavy 1.4 s); stamina and the guard numbers are derived from the target before the prototype, then adjusted only toward the target.
- Full AI-versus-AI games do not change, because computer players never send these commands (a test asserts it).

## 7. The army while you are the hero

You are one body; the army is still yours and marches on its last orders. It does not stand still, which is what makes first person a choice.

- **Warlord's wheel (hold Q).** Eight slices, each ordering the **active group** (the control group last recalled, else every regiment of yours within 30 tiles): **Follow me** (move to you and keep brigade), **Hold here**, **Advance to my sight** (attack-move to the ground point under the crosshair), **Charge** (attack the enemy regiment under the crosshair), **Fall back**, **Skirmish**, **Brace**, **Run to me**. All are existing commands; no new sim code. At most one wheel order per 0.5 s.
- **E, "To me!":** the active group's `MoveTo` to your position with a fresh brigade number from the human number space (as `Army::join` does), not a command storm.
- **Commander overlay (Tab).** The map and minimap take over the screen with the same fog as the RTS view; click and drag selects, right-click orders, the grab is released while it is open; time keeps running. The hero is drawn with its look cone. An alert in the top strip is a click target that opens the overlay there (Space is the sidestep; the RTS camera's "jump to alert" is not in first person).
- **Army status strip:** up to nine group tiles with health and morale, under the compass.
- **Leaving** (F6) leaves the hero standing and selected; the wheel's Follow me or a click brings it back to the army.
- **Leadership** works as now (16 tiles); the hero in the field keeps led regiments steady, and E is how a possessed hero keeps them close.

## 8. AI, drop cover, rejoin, replays

- **Computer players never send these commands**: `gc_ai` is untouched, and AI-versus-AI games and every existing golden are byte-identical, asserted by a test.
- **Drop cover (corrected).** In play-on mode a peer is declared gone only after `PLAYER_TIMEOUT` (3 s) of silence, so a dropped player's hero idles for about a second after its last steer expires (keepalives go every 0.5 s, a steer lasts 2 s after it is applied, the peer is declared gone at 3 s); in "Pause and wait" mode the whole match holds for up to 90 s and nothing expires. The mind's `Orders::decide` re-sends only orders it sent itself, so a newly seated mind has nothing to re-send. Handover therefore is explicit: when `Lan::uncommanded` first reports the player, **the host injects a `Steer` Stop for that player's heroes** (as `cover.rs` injects `Ready` for a dropped player; the stop goes through the turn stream, so it is deterministic, and one that arrives after the steer expired does nothing), then seats the cover mind, which commands the hero from its next think. `Concede` is the precedent for a host-injected command outside `is_player_order`.
- **Rejoin.** While a returning client is catching up (`catching_up()` / `rejoin_progress()`; `uncommanded()` still lists the player until it has caught up), first person is unavailable, F6 is disabled and the mode drops, and the host **drops `Steer`, `Strike` and `Guard` from a peer whose `catch_up_to` is set** (a catching-up client's steering is computed from a world seconds behind and would fight the mind). The stress scenario `dropped-player` is extended with a possessed hero, a drop and a rejoin, asserting no `Steer` applies while the side is covered.
- **Replays** record `Steer`, `Strike`, `Guard` as commands. Mouse look, camera, prediction and the wheel are not recorded and not needed. The catch-up history grows with the stream (6, about 180 KB an hour worst case).
- **Hash:** the steering side table is hashed only while non-empty (6.1), so rejoin and desync checks cover it.
- **Pause, speed:** pause freezes the camera; the keepalive runs on sim ticks offline.

## 9. Performance

- **Budget (low tier, 60 fps, docs/12):** the mode's CPU (predictor, HUD, camera) is taken from the 4.1 ms of slack, split in the PR (about 0.2 ms predictor and camera, 0.2 ms HUD), not added to the 0.5 ms "input, UI, camera" line. It must render no slower than the existing ride view in the same scene within 10%. GPU main is a target, not measured on a real GPU; lavapipe gives relative changes only, and the real-GPU check is Mario's Mac.
- **Benchmark (to be built in F1, because the existing one does not fit):** docs/12 says the 8000-point battle's frame half cannot yet be played in the game (it needs a 384-tile map) and `GC_HOLD_CAMERA` holds the director camera, not a hero eye. F1 adds a scripted fixed eye (`GC_HERO_EYE=x,z,yaw`) on the real-match screenshot harness on a map the game loads, with `GC_STILL_AT` and `--gpu-times`, plus a crowded melee screenshot from the eye.
- **Sim:** one hero's steering is a handful of integer operations a tick; each stress scenario carries a time budget.
- **LOD:** soldier LOD is by projected pixel height (`LOD_PIXELS = [100, 40, 16]`). At 75 degrees and 1080p (pixel scale about 704) a 1.5-tile model is LOD0 within about 10 tiles, LOD1 to about 26 and LOD2 to about 66. Head-look detail fades in above 100 pixels. The embodied hero is pinned at LOD0 outside the skin budget (4.2).
- **Scenery:** trees, bushes and grass reach are tuned for first person (the ride fakes a focus 40 tiles ahead and a distance of 60), with screenshots checking tree pop and grass reach near 90 tiles.
- **Terrain:** the long horizon strip passes more chunks through frustum culling; measured.

## 10. UI and guides

- **First-person HUD** (the bar hides and the world draws beneath; a new mode on `hud_drawn()`): crosshair (opens at a confirmed hit), the hero panel reduced to name, level, hit points, experience and the two abilities with a **sweep** (cooldowns are whole seconds today; the sweep needs a fractional value), a stamina bar, a guard indicator, a compass strip with army and enemy contacts, a corner minimap, the alert strip, a damage-direction marker, delay and order lag (5.5), and the status line. It scales with the UI scale and larger-text settings.
- **Settings:** a "Take command" group (section 3).
- **Guides, in the same PR as the feature** (project rule): `HELP_GROUPS` (a fixed three-group array with a 720p fit test, so the keys get their own group or the test changes), Quick Start (`STEPS` is a six-step array with drawn pictures), the Guide's hero pages (`guide_data.rs`; ASCII and fit tests), the hero panel tooltip (`ability_tips`), `docs/08-ui-ux.md` (its ride far plane still says 420, the code is 1300), `docs/quick-reference.md`, and docs 43 and 44 (43 gives the Daimyo footprint 1.4 by 2.0; the roster has the Devourer at 1.75 by 2.5 as the largest). The Guide says plainly that a joiner on a slow link has a late hero.
- **Accessibility:** reduced shake removes bob, sway, kicks and shake; high contrast for crosshair and bars; hold-or-toggle for Guard and Sprint; larger text; F6, the wheel and the overlay are all one-handed.

## 11. Real use (project rule: how it plays in practice)

**Big battles.** The 8000-point scenario has about 2,700 soldiers in melee. At the hero's one-man block the contact frontage is a few men, so first person is a duel in front of you and a crowd behind them. Soldiers at one to three tiles fill the view, so the near-eye cull, the eye on the head joint and a 75-degree lens keep it readable. In a crush the hero is hit from all sides: guard covers 120 degrees, so you must keep turning (which is the point), and `HERO_EXPOSED_PERCENT` 130 still applies, so three regiments still kill a hero in about 30 s unless the player fights well, which the gate (6.8) measures.

**Gates, bridges, fords, walls.** The steered hero uses the same nav and clearance code as a regiment (hero footprints run from 1.0 by 1.0 to 1.75 by 2.5 tiles, under every passage width, as the clearance check proves for the whole roster). Gates open for a hero steering toward them (6.1). On a bridge the nav grid keeps the hero on the deck; sidestep cannot cross blocked cells. Fords slow it as they slow others. A closed gate or wall blocks it; heroes do not man walls and a swing does not hit structures, so a first-person hero meets a wall as a regiment of one would. Dams, woods and fire apply through the same movement step and are in the stress checks.

**Traffic.** A marching column flows around a stopped hero and the hero around it. `friend_in_the_way` stops a block behind a friendly regiment, and first person must not turn that into being stuck behind your own line: a steered hero may **shoulder through** a friendly block that is not fighting at 50% pace, using the push-through the jostling work (#392) gave regiments in traffic (confirmed in F2 by reading `brigades.rs` and `combat.rs`; if it does not fit, the hero passes friends only while walking and shoving, at the shove cost). Stress scenario `steer-traffic` puts a steered hero across a column of three brigades and checks it neither stalls nor splits them.

**With other systems.**
- *Fog and vision:* rendering uses the sim's vision; the hero's eye is not a new vision source; regiment casts need a target your side sees.
- *Powers and abilities:* faction powers work (aimed with the crosshair where they need a click); hero slots are 1 and 2.
- *Fire, poison, traps, mines, cocoons:* applied as to any regiment, with the stun rules.
- *Reinforcements, hamlets, prisoners:* unchanged; kills near a hamlet earn experience as now.
- *Leadership:* unchanged.
- *Pause, speed, replays:* the camera freezes with pause; sandbox speed changes scale steering by tick; a replay shows the hero fighting from the RTS camera, and in F5 from the hero camera.
- *`HandOver`:* an ally who commands the hero may steer it (the owner check allows it); two steer streams on one tick: the later in the stream wins; not worth special code.
- *Disconnect while possessing:* section 8.

## 12. Staged build

Three PRs (the sim work is bundled into one, by Mario's bundle rule).

- **PR A, F1 "Eyes" (no sim change).** The view-mode enum and its ~16 sites; the **eye-track bake in `gc_artbake`** (a pack-format change, so it runs the packs check; the track covers the base clips and the fight-library moves, and the camera adds each active pose layer's head offset by its weight); a **minimal Tab overlay** (click to select, right-click to move, attack and attack-move, so the hero can be ordered in F1); cursor grab and raw mouse, proven on X11 (Xvfb with xdotool), Wayland, and Mario's Mac; the first-person camera on the eye track, the hide-head mask, the pinned LOD0 hero, the shadow range; the first-person HUD; the new Settings fields; enter and leave (F6, the panel button); the full key map with every key swallowed or assigned; the fixed-eye benchmark and the screenshot matrix (BC forced off as well, your own shadow, a crowded fight from the eye). **There is no movement in F1:** the hero is driven by ordinary orders from that minimal Tab overlay, so first person is a mouse-look view on the hero while it carries out orders, behind a "Take command (preview)" setting. F1 cannot judge how steering feels; that arrives with PR B. (An earlier draft walked the hero with `MoveTo` previews; the sim turns any `MoveTo` on an engaged hero into a break-out, so the preview would change battles; it was dropped.)
- **PR B, F2 and F3 "Legs and arms" (one sim PR).** `Steer` (with `Gait::Dodge`), `Strike`, `Guard`, the side table, stamina, sprint, sidestep, melee freedom and its costs, the steered predicates, `drop_member`, gates and shoulder-through, hashing, codecs, `is_player_order`, host coalescing and catch-up rejection, `Lan::order_lag()`, version bumps, goldens, stress `steer-gate`, `steer-traffic`, `possessed` and an extended `dropped-player`, the input-replay predictor with sequence numbers, the predicted body and local arm, the wind-up stretch, the order-lag re-measure (including a host-versus-joiner duel). Opus review (`/simplify` and `/code-review`, required for `gc_sim` and `gc_net`); gate shape approved by Mario before it starts, numbers before it merges.
- **PR C, F4 "Voice and polish".** The wheel, E, the full overlay (box select, groups, alerts), the status strip, ranged-hero shots, per-hero pass (the Daimyo mounted, the Ronin's Dash, the Devourer's bulk), death and revive flow, audio, Guide, Quick Start, tooltips, `HELP_GROUPS`, the optional parry window and replay hero camera if cheap.
- **F5:** a playtest thread plays it with all systems on and brings before and after clips of each feature (project rule), the perf-log entry, fixes.

Each stage runs `scripts/pre-pr-check.sh` before every push, updates the guides in the same PR, and ends with a one-line status and the Mac build link at merge.

## 13. Decisions for Mario

The plan takes the recommended option unless he says otherwise. Because PR B changes hero rules he retuned on 2026-11-22, each is listed on its own:

1. **Gate shape (before PR B starts):** perfect play at most 125% damage and 150% survival of autopilot, poor play at least 75%, 16 seeds per hero, medians, plus the exploit check (6.8).
2. **Rule changes to heroes while steered:** moves in melee at 60% pace; turns at 1.0 times its stat in melee (autopilot: half); walking out of a lock costs a break-out (parting blows) and has a 4 s cooldown; sprint uses stamina instead of the 8 s burst and 10 s winded; sprint is refused in contact; the aimed blow replaces the automatic blow; the guard halves front melee blows and costs stamina.
3. **Keys:** F6 to take and leave, hold-Q wheel, E "To me!", Tab overlay, Space sidestep, 1 and 2 abilities.
4. **Duel fairness online:** accept the host's reaction-time edge in hero-against-hero fights (default), or delay host possession commands by the worst joiner's lag, or lengthen wind-ups (5, measured).
5. **PR A ships behind a "Take command (preview)" setting; Conquest only.**

## Review log

Revision 2 changed the following after the Opus 5.5 review (the review checked each claim against the code):

- State moved from the hashed field list to a side table, and the "goldens stay byte-identical" claim gets a real proof step (unblessed goldens must pass before the bump).
- Wire tags 55 to 57 and replay tags 57 to 59 (not 54); `KINDS` and a unique-tag test.
- `Steer` is not in `redirects()`; out-of-range casts under a steer are refused; same-tick test.
- `steered(i)` predicate in combat's press-back and `target_facing`, `step_movement`'s engaged exit, `Stride`, ranged auto-fire; `Brigades::drop_member`; a steered break-out without `target`.
- `Steer` has `move_heading`, `face` and `gait`; the stream is intent-based; the keepalive and a 40-tick expiry run on wall clock online; host coalescing and hold, pause and catch-up rules; the host injects a Stop on drop; rejoin drops steering.
- Prediction is input replay, not "advance by the delay"; the predictor stays on in melee; `Lan::order_lag()` is built; wind-up latched per swing; the host-advantage is stated and put to Mario.
- Combat model: aimed blows replace the automatic blow (the first draft's arithmetic put perfect play near 190 to 300%); strike needs contact; sidestep is a dodge; walking out costs a break-out; guard has its own 60 degree angle test; sprint uses stamina; no stagger; `Event::Whiff`; the gate gets damage dealt, 16 seeds, medians and an exploit script.
- Roster facts: two ranged heroes with real ranges; turn rates 160 to 240; hero goldens use their own script.
- Render and input: a view-mode enum replacing about sixteen `ride` checks; head-joint eye track baked at pack time; the head mask is primary (the culling trick fails without BC textures and with loose-part heads); `SoldierInstance` has no spare bytes (steal a flag bit); the drawn body follows the predictor; pinned LOD0 hero; far plane 1300, no reversed-Z; LOD distances corrected; own shadow range; F1 has no movement; Wayland and the winit fallback order; the full key map, Esc order, Space no longer double-booked, Tab before `switch_side`; the benchmark is defined; the CPU budget is split from slack.
- Second Opus pass (same day): `Steer` carries a sequence number so coalescing cannot break the predictor; `Gait::Dodge` gives the sidestep a wire path; face-only steers no longer make a looker "steered"; the eye-track bake and a minimal Tab overlay move into PR A; more keys swallowed (Delete, Backspace, Y, Backquote, numpad, stray digits); the Refractor's accuracy is 80; the hide mask is a uniform (the flag bytes are used by falls); the drop-cover idle gap is about a second; `Event::Whiff` and `drop_member` placement corrected.
