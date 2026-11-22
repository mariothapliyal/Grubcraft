# 42 · Armies that march as armies

Mario (2026-11-21): look at drastic improvements to pathfinding, unit
organisation and unit coherence. Broad scope, drastic reforms where they
are justified, thorough and creative.

This is the proposal. It starts from what armies do in the game today,
measured and drawn, names the one cause behind most of it, and lists
twenty changes in five stages (and a sixth stage Mario added the same
day, changes 21 to 25). Stage 1 is the reform everything else
hangs on, and it is specified in full here, with its real-use section.

**Status (2026-11-21):** proposal, reviewed by Opus the same day (verdict:
build stage 1, with changes; they are folded in below and listed under
"What the review changed"). Stage 0 (the measuring tools) and stage 1 are
built. "Stage 1 in full" below describes what was built, "What it
measured" has the numbers, and "What a brigade does not do yet" the
places where it lets go and regiments march as they did before.
Stage 6 (woods, 22 November) and stage 2 (drilled movement) are built
since; each has its own "in full" section at the end.

## What armies do today

Four earlier passes (#136, #193, #333 and #390, then #392 and #400) made
single regiments good at finding their way: a regiment alone walks at 88%
of the best possible pace on grown maps and is never stuck. All of those
passes were scored on **time**: how soon the last regiment arrives.
Nothing measured whether an army still looks and works like an army on
the way. So this time the marches were recorded tick by tick and drawn
(`gc_map/tests/marches.rs`, `TRACE_JSON`), and five new numbers were
taken from the recordings.

Mixed armies ordered across grown maps with one group order, 50 to 90
tiles (2026-11-21, main at `c5e731a`, SIM 88):

| March | Regiments | First regiment there | Last one there | Marching time spent in disorder | Friends standing in each other |
|---|---|---|---|---|---|
| Rolling land, one ford on the way | 6 | 37 s | 73 s | 28% | 15 pair-seconds |
| Between two lakes | 6 | 30 s | 53 s | 29% | 1 |
| Along a river to a ford | 6 | 39 s | 65 s | 27% | 16 |
| Open plain, then one narrow pass | 12 | 39 s | 75 s | 48% | 86 |
| Open plain with woods | 12 | 36 s | 60 s | 20% | 12 |

What the drawings show, in every march:

1. **The army is not an army after the first second.** A group order is
   turned into one separate march per regiment on the ordering player's
   machine (`formation::group_move`, `arrange::arranged_move`). The
   simulation never knows the regiments belong together. All it is given
   to hold them together is one shared pace, the slowest member's
   (`formation::pace_of`), and the first route found being lent to the
   others while they wait for their own (`Nav::lend`). Each then walks
   its own route to its own spot, sets off facing its own way, and the
   shape the player chose exists only at the far end.
2. **It arrives in pieces.** The first regiment stands at the goal 23 to
   36 seconds before the last, although all walk at one pace: the routes
   differ in length and the queue at every narrow place stretches the
   army out. Against a waiting enemy that is the whole battle: the head
   fights alone.
3. **It jostles itself.** A fifth to a half of all marching time is spent
   in disorder (fighting 20% worse) from pushing through friends of the
   same army, on an ordinary march with no enemy in sight.
4. **It weaves on open ground.** Twelve regiments on an empty plain walk
   wavy lines, each stepping aside from its neighbours and back again
   (`Combat::keep_apart`), because nothing tells them they are in step.
5. **A narrow place is a heap.** At a ford or a pass every regiment heads
   for the gap at once. Whoever gets there first goes first; the rest
   stand in each other at the mouth (the `army-gate` scenario, 48
   regiments each sent through one gate: 1,193 regiment-seconds in
   disorder).
6. **Spots are handed out blind.** A place in the line can land on a strip
   of bank where the block doesn't fit: one regiment ended its march as a
   single file fifteen tiles long and stayed that way.
7. **Computer players march one line abreast.** The Marshal sends its
   whole army as one row, 7 tiles apart, however many regiments
   (`gc_ai` `marshal::march`): twenty regiments make a line 140 tiles
   wide that then has to get through a pass.

## The cause

The simulation knows regiments and nothing above them. Every rule added so
far is one regiment looking at its neighbours and guessing: step aside,
wait behind, trail, squeeze, make room, march through after two seconds.
There are eleven such rules now. They made the time better. They cannot
make an army keep its shape, take turns at a gap or arrive together,
because no regiment knows what the others are about to do.

Real armies solved this with organisation, not with cleverer soldiers: an
order of march, a column for the road, a drill for passing a narrow place
and forming up beyond it. The reform is to give the simulation the same
thing.

## The twenty changes

In build order. Each stage is one pull request (the bundle rule for
simulation work), each with its stress scenarios and its measured before
and after.

**Stage 0. Measure cohesion** (no rule change)

1. **A cohesion report and a recorder.** The five numbers above for any
   march, and a recording that plays back as a top-down picture. Every
   later stage is judged with them, and scenario limits hold what is won.

**Stage 1. Marching together** (simulation; specified below)

2. **The simulation knows a brigade.** Regiments ordered together stay
   one body from the first step to the last.
3. **One route for the brigade**, searched once and kept by all, instead
   of one each with the first lent out meanwhile.
4. **The shape is kept on the way.** The army marches in the order it
   will stand in, facing the way it goes, and wheels round corners.
5. **It closes up and opens out.** Where the ground is narrower than the
   army, rows re-form into as many abreast as fit, in a fixed order,
   before they reach the narrow place, and open out again beyond it.
6. **It keeps together.** The head slows for the tail; stragglers hurry;
   one that is stuck, cut off or far away is left to come on alone.
   Everyone else arrives within a few seconds of each other.
7. **Places that fit the ground.** The final line is laid on ground where
   every block fits.
8. **Computer players march this way too**, in rows that fit, not one
   line abreast.

**Stage 2. Regiments that move like drilled men** (simulation)

9. **Small moves without turning.** A block a few tiles from its place
   sidesteps or backs into it facing the same way, instead of turning
   round, walking and turning again.
10. **Make way.** Friends standing at ease in the path of a brigade step
    aside and step back afterwards, so pushing through (and its disorder)
    becomes the rare case.
11. **Real footprints, checked.** A test holds every unit's block against
    its model, so big bodies and deep cavalry take the room they are
    drawn with. (The Bulk already does: 3.0 tiles for a 2.7-tile model.)
12. **Prune the guessing rules.** Stage 1 already switches them off
    between regiments in step in one brigade. Here each of the eleven is
    measured switched off everywhere; those that no longer earn their
    cost go.

Stage 2 is built, `SIM_VERSION` 94: see "Stage 2 in full" below. All
eleven guessing rules were measured off and all eleven stay.

**Stage 3. Narrow places and the roads to them** (simulation)

13. **Passages.** Gates, bridges, fords and passes become things the
    simulation knows, each with a width and a flow. One direction at a
    time through a single-lane passage, keep right on a wider one; a
    second army waits back from the mouth, drawn up, instead of meeting
    the first head on in the middle.
14. **Routes that suit the army.** A big army prefers the wide valley to
    the footpath even when the footpath is shorter, because filing
    through costs more than walking round. The search prices a narrow
    place by how long this army takes to file through it.
15. **Faster route search**, measured after 3 and 14 have cut the number
    of searches (a group of twelve asks once, not twelve times).

Stage 1 left two things here (see "What a brigade does not do yet"): a
big army in wide rows through a gate, and a route that doubles back
round a corner. Both need a place that is never laid on ground the block
cannot stand on (so a flank comes in to the file along the wall instead
of through it) and a turn at the passage for each row.

**Stage 4. Organisation the player can use** (controls, simulation, AI)

16. **Advance in line.** An attack-move by a brigade goes forward dressed
    in line and each regiment takes the enemy opposite it, instead of
    all converging on the nearest. (Stage 1 brings the first half: a
    brigade on an attack-move halts as a body when one of its regiments
    turns to fight.)
17. **Standing armies.** A control group can be locked as an army: it
    keeps its arrangement between orders, shows one banner and one
    route, and regiments raised later can be sent to join it (this is
    where the reinforcements work meets this one).
18. **Draw the road.** Hold a key and draw the way the army should go;
    the brigade follows the drawn line instead of the shortest one.
19. **Column of march.** A brigade can be told to go in road column the
    whole way (faster on roads and through country full of narrow
    places) and to deploy on arrival or when an enemy comes near.

**Stage 5. Every soldier on the ground** (the option C call)

20. **Soldiers that walk the ground, then soldiers in the battle maths.**
    First on screen only: each soldier steers along his regiment's way
    round trees, corners and gateposts instead of holding a slot in a
    sliding rectangle. Then the measured cost of making soldiers bodies
    in the simulation (option C) is put to Mario with numbers.

**Stage 6. Through and round the woods** (simulation; Mario
2026-11-21, built straight after stage 1 because it sits on the brigade's
shared track)

Measured on the 42 recorded marches: regiments spend 36 to 40% of their
marching time in woods where the quickest path for a single soldier
spends about 20%, with and without brigades. Stage 6 found that the
routes are not the cause, and that the woods themselves were too easy
on a block at their edge; see [Stage 6 in full](#stage-6-in-full-woods).
Built (Mario chose it, 22 November 2026, 6:44 am NZDT):

21. **Woods slow the whole block.** A regiment keeps 85% of its pace in
    woods (70% heavy) by the share of its block under the trees, not by
    its middle alone (soft ground the same way), and its ranks loosen on
    screen by that share. Routes price the ground under the whole block.
    Built, `SIM_VERSION` 93.

Not built, and not needed on what was measured:

22. **Close up beside a wood.** Rows close up on the side nearest the
    trees instead of marching a flank through them.
23. **One line through a wood.** Where the way must cross a wood, the
    track takes the thinnest crossing and the body goes through in
    column on one line.
24. **Priced for the slowest.** The track is priced for the slowest
    troops in the body (heavy troops keep 70% of their pace in woods,
    foot 85%), so a body with horse goes round.
25. **Held to a number.** The march report gives time in woods and the
    share of soldiers under trees; no limit is set on them, since the
    quickest single-soldier path is not a reachable mark for a block.

## Stage 1 in full: marching together

### What the player sees

Select six regiments and right-click across the map. They step off
together as they stand, nobody pushing past anybody to form up first,
and over the first stretch grow into the shape they will stand in (melee
in front, bows behind, horse on the wings, as the army shapes already
arrange them). On open ground they hold that shape and walk straight, at
the pace of the slowest. Coming to a ford two regiments wide, the rows
split in good time: the front row goes through two abreast, the rest
follow two by two, nobody stands in anybody. Beyond the ford the rows
come up and widen again, and the army walks on. At the goal each
regiment walks the last few tiles to its own place, turns to the facing
ordered and stands, the whole army within a few seconds of each other
on open ground.

### How it works

As built (`gc_sim::brigades`; the numbers are its constants).

- **A brigade** is a new thing in the simulation: its members, where each
  was sent, one track, and how far along the track its middle (the
  anchor) has come. Every move order carries one more number, the
  brigade it belongs to (0 for none), chosen by whoever gives the order:
  a player's group orders count up from 1, a computer player's above
  32,767, so the two never share one. Moves of one player with the same
  number are one brigade. The arranging (roles, presets, saved shapes, a
  dragged width) stays where it is, on the ordering player's machine:
  the simulation reads the shape off the spots the moves name.
- **Who is in it.** The regiments standing together when the order is
  given: those within 20 tiles of the middle of them all, those within
  20 tiles of one of those, and so on, so a big army standing together
  is one body however wide it stands. One further off walks to its spot
  alone, as before. So does one whose spot the ground cuts off from the
  rest of the body's (the far side of a cliff or a wall from them).
  Workers and caravans never join. Two regiments make a brigade; one is
  a plain move. A march shorter than 14 tiles is each regiment's own:
  there is nothing to keep together. Up to 64 regiments, and eight
  brigades a player at once; more march as before.
- **The track.** One route, searched once, from the member nearest the
  middle of the body to the middle of where they were sent. It keeps a
  wide berth. A long way is searched in pieces as the body nears the end
  of what it has. Behind where it was searched from, the track runs 30
  tiles straight back, so members standing behind that one are on it
  too.
- **The shape, seen along the way in.** Each member's spot in the final
  shape is read as so far behind the shape's middle and so far left or
  right, looking along the way the track comes in over its last 8 tiles.
  Members whose spots are level (within a tile and a half) make a row.
  The order of march is row by row from the front.
- **Rows on the track.** A row marches abreast, as it will stand
  (shifted across to fit, if it must be), where the ground either side
  of the track is wide enough for it from 8 tiles ahead of it (up to 24
  for a wide body, whose flanks have further to come in) back to its
  rear. Where it is not, the row files: as many as fit side by side go
  first, those furthest along before the others, and the rest fall in
  behind, as many abreast again. Nobody's place is put closer behind a
  member in its way than their blocks and the gap between regiments
  allow, so rows that file string out and close up again beyond the
  narrow place. The room is measured across the track each way
  (`Nav::room_sides`) every 2 tiles along it, afresh at every look, four
  looks a second, so a gate shutting or a ford flooding is seen at once.
  Where even one block is too wide, it narrows by files inside its
  place, to as many as the gap has room for.
- **Each regiment's place** is so far behind the anchor along the track
  and so far left or right of it. A brigade sets off with every place
  where its regiment stands, and each place then moves to where the rows
  put it: up the body no faster than its regiment could gain on the
  anchor, back and across at a walk. A place never moves up past a
  member marching in front of where it stands, nor closer behind
  another's place in its way than their blocks allow: a rear rank closes
  up as those before it move aside, not through them. As the anchor
  moves, the places move with it and bend round corners with the track
  (its direction is smoothed over a few tiles so places sweep round a
  corner rather than jump). A regiment is steered at a point six tiles
  ahead of its place, never at the place itself: faster when it is
  behind, slower when ahead, waiting when well ahead (but never waiting
  inside a narrow place, which would stop the way for all behind it), at
  most its own speed. It never turns back for its place. It comes no
  nearer behind the block of the member before it than they march.
- **Out of sight of its place.** Five times a second each member looks
  whether it can see the point it walks at. If it cannot (a cliff or a
  wall between), it walks at the middle of the track ahead of its place,
  failing that at the track just ahead of where it is, and after five
  looks with nothing of the track in sight it finds a route of its own
  to the body.
- **Keeping together.** The anchor moves at the pace ordered (the
  slowest member's), slower as the member furthest behind its place
  falls back: full pace up to a tile behind, down to three eighths of it
  by five tiles, never a stop (a stopped body would block the exit of a
  narrow place). Mud, woods and wading slow it this way too: the member
  in the worst going falls behind and the body slows for it.
- **Left behind.** A member in a melee, in a trap, more than 45 tiles
  behind its place, making no headway for five seconds, or held up by
  the member in front of it no longer slows the anchor. It stays in the
  brigade and takes its place again if it catches up.
- **Between members in step the guessing rules are off.** Two members
  both at their places (within a tile and a half along the track and a
  tile and a quarter across it) don't step aside from each other, queue
  behind each other or wait out their patience on each other: their
  places are already clear of one another. While a brigade is still
  forming up, and toward everyone outside it, the rules work as before.
  Pushing through a friend still slows and disorders, whoever it is.
- **Arriving.** The anchor stops at the end of the track. Each member
  leaves the brigade for its own spot when it is within six tiles of
  coming level with it along the track; once the anchor is in, also as
  soon as it stands on open ground in plain sight of its spot. Those still filing through a narrow place behind keep their
  turns until they are through. When fewer than two are left the brigade
  is over.
- **Leaving a brigade.** Any other order to a regiment takes it out. So
  does routing. The same order given again (the same number, the same
  place within three tiles, facing within ten degrees, the same pace)
  changes nothing, so a computer player repeating itself doesn't
  restart anything. A player's group order is
  always a new brigade with the next number, so clicking twice on the
  same spot forms the body up again from where it stands (it sets off
  as it stands, so this costs little).
- **Orders that land one by one.** A computer player's orders for one
  army land over several ticks, not all at once as a player's do. The
  first regiment of a brigade marches by its own order for up to three
  seconds while the rest join.
- **Fighting on the way.** On an attack-move, a member that turns
  aside to fight leaves the brigade for the fight. While one of them
  fights within 25 tiles of the anchor, the body halts and the rest
  stand in their places, each turning on any enemy that comes within
  its own reach. When its fight is over it takes up the march again,
  joins the brigade again and takes its place, and the brigade goes on.
  The body does not wait for one standing with no way to its quarry (a
  foe behind a wall or over deep water): only for one fighting,
  shooting or on its way. On a plain move the brigade keeps marching.
- **Letting go.** Where a brigade would be slower than its regiments
  each by its own way, it is over on the spot and they march on as they
  did before there were brigades. See "What a brigade does not do yet".

### Real use

*Sizes.* One regiment: unchanged, a plain move. Two to six: one or two
rows, as the shape says. Twelve: two or three rows; through a ford they
go two or three abreast. Forty-eight sent as one group march as one
brigade over open ground, in rows of eight; at a gate they let go and go
through as they did before (see "What a brigade does not do yet": in
single file with clear gaps they took 97 s against 50 s pushing through
each other, and in no less disorder, because rows 88 tiles wide are a
long time coming in to the file).

*Shapes.* Line, Deep, Hammer, Square, Plain and saved shapes all march as
themselves, since the order of march and the rows come from the shape.
A dragged width sets how wide the rows may be on open ground. Regiments
in the Column order (#411) keep their column inside their place; a brigade
that is all columns is a column of columns.

*Through, over and round.* A gate, a bridge deck and a ford are narrow
ground to the track like any other: the rows that fit go through abreast
(two on a two-lane bridge, three on a five-tile deck, one through a
gate), and a single block still narrows by files inside its place where
even one is too wide; rows are spaced by the depth blocks have when
narrowed, so a column doesn't grow back into the row behind. Round a
wood or a lake the whole brigade takes one side, the track's, instead of
splitting. A lone rock or a copse in a field narrows the rows on that
side for as long as they pass it (a known roughness: stage 3 lets a row
pass either side). The track is checked 16 tiles ahead of the anchor
once a second: a gate that shuts, a ford that floods or a bridge that
falls re-routes the brigade as one, or ends it where there is no way
on; members caught on the far side find their own way.

*In traffic.* Two brigades of the same player crossing, or a brigade
meeting workers and caravans, treat each other as regiments do today
(keep apart, push through with disorder as the last resort); stage 3
gives passages turns. A friend standing on the track is walked round by
the row it blocks; stage 2 makes it step aside.

*In combat.* A plain move that is shot at keeps marching, together.
A member charged or caught in melee fights; the rest go on (the player
chose to march, and can turn them), and it follows when free. An
attack-move halts as a body when one member turns to fight, as above.
Run (#411) is still each regiment's own: a member told to run runs to
its place and no further ahead; a brigade does not run as one yet
(stage 4). Hold, Brace, Skirmish's step back, falling back and a change
of formation take a regiment out, as any order does; the Column and
Wedge a regiment is already in, it keeps inside its place. The Forced
March power quickens the members it is cast on up to their places; the
brigade as a whole goes faster only when every member has it.

*With other systems.* Shift-queued legs carry their own brigade number:
as each member finishes one leg it joins the brigade of the next. Since
a brigade's members finish a leg within a few seconds of each other,
the first marches on by its own order for up to three seconds while the
rest join; one later than that marches the leg alone. Mud, woods and
wading slow the body through its hindmost member, as above.
Regiments of an ally under a player's command walk their own side's
ground and don't join that player's brigades. Fog of war, morale,
generals and walls are untouched. Replays and online play carry the one
extra number per move; the brigade itself is ordinary simulation state:
fixed in size, made when the match starts (nothing is allocated during
play), hashed, saved, and copied for the computer players' rehearsals
with the regiments it holds.

*Computer players.* The Marshal's marches are brigades, in rows of at
most six, so an army of twenty is four rows deep instead of 140 tiles
wide. Its check that an order is being carried out looks at the brigade,
not at one regiment waiting its turn at a gate, so it doesn't pull
members out by ordering them again. In a whole Veteran match (Ember
Clans against Carrion Concord, 30 minutes) one side marched 12
regiments as one brigade; the check
`ai_only_armies_march_as_brigades` holds that at six or more.

### What it measured

The proposal set targets (first to last regiment under 6 s, disorder
under 5% of marching time, arrival no later). Stage 1 reaches the third,
and a good part of the first two, not all of it.

Forty-two recorded marches on grown maps (`crates/gc_map/tests/marches.rs`,
`BRIGADE=1` for brigades, `ARMY=big` for twelve regiments), the mean of
each number, as separate moves and as one brigade:

| | Apart | As a brigade |
|---|---|---|
| Six regiments, 24 marches: last regiment arrives | 50.7 s | 50.1 s |
| ... first to last | 15.6 s | 11.4 s |
| ... marching time in disorder | 12% | 6% |
| Twelve regiments, 18 marches: last regiment arrives | 61.2 s | 61.5 s |
| ... first to last | 25.8 s | 21.6 s |
| ... marching time in disorder | 25% | 17% |

With six, 16 of the 24 marches arrive more than 2 s closer together and
4 further apart; with twelve, 9 of 18 and 2. Five of the 42 marches
arrive more than 5 s later than apart, the worst by 21 s (twelve
regiments 82 s against 61 s, in under half the disorder). Why those
five are slower is not yet found.

The stress scenarios play each march twice on the same ground:

| Scenario | Apart | As a brigade |
|---|---|---|
| `brigade-ford`: 8 regiments wade a ford 8 tiles wide | last 40.6 s, first to last 12.5 s, 129 regiment-seconds in disorder | 42.3 s, 11.6 s, 18 |
| `brigade-gate`: 8 regiments, four abreast, through one Gate | 38.0 s, 10.4 s, 119 | 42.3 s, 2.2 s, 26 |
| `brigade-column-gate`: 12 regiments, two abreast, through one Gate | 36.1 s, 13.2 s, 161 | 30.8 s, 5.6 s, 64 |
| `brigade-corner`: 8 regiments back round the corner of a thicket (the brigade lets go) | 58.4 s, 20.8 s, 78 | 58.0 s, 20.4 s, 82 |
| `brigade-big-gate`: 24 regiments, eight abreast, through one Gate (the brigade lets go) | 45.3 s, 17.0 s, 469 | 46.4 s, 17.2 s, 510 |

The first three are held to arriving no more than 15% later than apart,
no further apart and in no more disorder; the two where the brigade lets
go, to within 10% of the time apart and 25% of the spread and the
disorder. `army-gate` (48 regiments through one Gate) is unchanged at
50.0 s whether they are sent as one brigade or not.

### What a brigade does not do yet

A brigade lets go, and its regiments march on each by its own way as
they did before, where it was measured slower than that:

- **A route that doubles back.** When the track sets out more than a
  third of a turn from the way it comes in, the body would have to turn
  its whole order about on the way (those in front at the start stand at
  the back at the end). Tried as a brigade, eight regiments back round
  the corner of a thicket took 74 s against 58 s, in nearly twice the
  disorder. Two cures were tried and set aside: telling off the rows
  from how the body stands at the start (better round the corner, worse
  over the 42 marches), and keeping the shape's facing fixed all the way
  (no better).
- **Wide rows at a gate.** A file through a gap that squeezes its blocks
  passes about 12 soldiers a second where regiments pushing through each
  other pass 40, and rows that reach far to the side are long coming in
  to the file. A body reaching more than 24 tiles to either side of the
  track lets go wherever a row must file; one of more than eight
  regiments reaching more than 10 tiles to either side lets go at a gap
  that squeezes. (Twelve in rows of four were 37% later through a Gate
  than apart; 24 in rows of eight 66% later; 48, twice as long. Twelve
  in rows of two are 15% sooner than apart, in 40% of the disorder, and
  stay a brigade.)
- **Drawn up close before a gate.** A body ordered through a gap that
  squeezes, standing so near it that the ground closes in on a member
  before it could come in to the file, lets go as it sets off. (Eight
  regiments four abreast, 12 tiles before a Gate: 29 s against 19 s.)

Over the 42 marches these rules cost the brigades a little (twelve
regiments would otherwise arrive 19.1 s apart, not 21.6 s, in 12%
disorder, not 17%): they let go at some natural narrows where filing was
in fact no slower. Stage 3 (passages) is where a brigade learns to take
a gate and a corner well, and these rules go.

Regiments of two owners ordered together (an ally's army handed over,
with your own) march as two bodies, one for each owner: a brigade is
its owner's. Each player's brigade numbers are its own, so two players
ordering one owner's regiments never share a brigade.

Not yet measured: brigades of one player crossing each other, and a
brigade under fire.

### Checks

Stress scenarios `brigade-ford`, `brigade-gate`, `brigade-column-gate`,
`brigade-corner` and `brigade-big-gate` (above; [40](40-real-play-checks.md)),
beside `army-gate`, which keeps sending 48 separate moves. Simulation
tests in `crates/gc_sim/tests/brigades.rs`: a brigade arrives together
where separate moves straggle, keeps its shape, files through a gap and
forms again, the same order again changes nothing, an attack-move brigade does not
wait on a foe nobody can reach, a member given
another order leaves and the rest march on, a short move is each
regiment's own, an attack-move brigade halts while one of its own
fights, and a copy made for a computer player's rehearsal marches on
exactly like the original. The golden replay `field-brigade-1v1` has a
brigade march and fight. `ai_only_armies_march_as_brigades` plays a
whole match between computer players.

### What the review changed

Opus read the proposal against the code (2026-11-21) and agreed with
building stage 1, with these changes, all taken:

- Regiments are steered at a point ahead of their place, never at the
  place (a place behind a regiment made it face about); places follow a
  smoothed track.
- Rows re-form by a regiment falling in behind or moving up beside, with
  a two-second hold, measured across the track each way.
- The spacing rules stay on while a brigade forms; disorder from pushing
  through friends is kept honest.
- A member stuck, cut off or far away no longer holds the head.
- The same order given again changes nothing; the computer players'
  order checks know brigades.
- The brigade is named by a number on each move (also on queued steps),
  not by a separate order naming a head regiment.
- An attack-move brigade halts as a body when one member turns to fight.
- The gate target is measured, not promised; engineering limits (no
  allocation in play, heap, instruction counts, rehearsal copies) are
  named.

## Considered and not chosen

- **Flow fields** (one field of arrows per destination that every unit
  follows, as in Supreme Commander 2 and Age of Empires IV). Right for
  hundreds of small independent units. Wrong shape for this game: our
  units are wide blocks in a chosen formation, and a flow field has no
  idea of order or of who stands beside whom. The one thing it gives,
  one search for many units, stage 1 gets from the shared track.
- **Stop, form up, then move** (the classic Age of Empires answer). It
  keeps formations but makes every order start with a wait. Here the
  army forms on the move: the head sets off slowly and the rest fall in.
- **More neighbour rules.** Four passes of them bought time, not order;
  the drawings above are the result.
- **Every soldier a body in the simulation first** (option C now). It
  would make gaps flow naturally, but it multiplies what the simulation
  holds and hashes, what computer players copy when they rehearse a
  battle, and what can drift apart online, and it rewrites melee. An
  army of soldier-bodies with no organisation would still arrive in
  pieces. So organisation first (stages 1 to 4), soldiers on screen next,
  and option C as a call with its cost measured.

## Sources

- Dave Pottinger, "Coordinated Unit Movement" and "Implementing
  Coordinated Movement" (1999): groups, formations with a forming state,
  ordered avoidance and splitting round obstacles.
- Stardock, "Ashes II Dev Journal: Pathfinding for Armies": an army
  routes once and passes formation points down to its units.
- "Pathing in Age of Empires IV: Flow Fields and Steering Behaviors"
  (GDC talk, abstract) and Elijah Emerson, "Crowd Pathfinding and
  Steering Using Flow Field Tiles" (Game AI Pro): the flow-field family.
- Drill of the horse-and-musket age (order of march, passing a defile,
  forming line from column): the model for rows that close up and open
  out.

## Stage 6 in full: woods

### What was wrong

Watched in a real Field Battle on a wooded map (veteran, seed 7), an
army ordered into a forest marched in cleanly and disappeared under the
canopy, its flags marking where it stood. Nothing broke.

On the recorded marches, even regiments that start and end in the open
have their middle in woods about twice as long as a single soldier's
quickest path (six regiments 20% against 10%, twelve 28% against 13%).
Drawn out, the cause is plain: the open strip beside a wood is often
narrower than a block, and routes keep a block's berth off rocks and
cliffs (up to four times the cost of open ground) while woods cost 1.2
times, so the block's middle runs a few steps inside the trees. That is
about the best a block of that width can do; the single-soldier path
threads gaps no block fits through.

Six route changes were tried, each over all 42 marches with and without
brigades, and none moved arrival times or time in woods beyond the noise
(about 1.5 s and 1 point): portals split where the ground changes,
links from a regiment to the portals in sight priced by the ground
rather than by distance, plain-sight walks only over ground no slower
than their ends, a lower cost near rocks, no route tightening, and a
cost added for trees beside a cell while the march still went by the
block's middle. None was kept; the routes' share of what was built
below only prices what the march now really costs.

What was missing was the woods themselves: a block was slowed only when
its middle stood in a wood, so a flank through the trees cost nothing,
and the trees never touched its order.

### What was built

- **Slowed by the share under the trees.** `Terrain::block_going`
  looks at a block's middle and its four corners (three eighths of the
  way out, where the outermost soldiers stand; `block_points`) and keeps
  85% of the pace (70% heavy) by the share of them in woods, and 75%
  (60% heavy) by the share on soft ground. A block with one flank in a
  wood keeps about 94%, and one entering a wood slows as its ranks go
  in. Giving ground in a fight still goes by its middle.
- **Routes priced for the block.** Each cell's stored cost is its cost
  at the mean pace kept at it and at four cells `BODY_REACH` (two tiles,
  about where a line regiment's outer files stand) away
  (`Grid::block_going_at`), so every route stage reads the one cost.
  Only the going is read round a cell, so burning or clearing woods
  reprices every cell within `BODY_REACH` of the changed tiles (a test
  checks the result against a grid built afresh). Rain-soaked and
  rubbled ground slows the march but routes still price it by the map's
  footing, as before.
- **Ranks loosen under trees.** On screen, a block's ranks open out by
  up to 15% and each soldier stands up to 0.18 tiles off his slot, by
  the share of the block in woods (`Block::wooded`).

### What it measured

The 42 recorded marches (last arrival / first to last / time in
disorder / soldiers under trees):

| | before | after |
|---|---|---|
| Six regiments, apart | 50.7 s / 15.6 s / 12% / 35.4% | 49.7 s / 14.7 s / 10% / 35.2% |
| Six, as a brigade | 50.1 s / 11.4 s / 6% / 34.8% | 49.5 s / 10.9 s / 7% / 35.3% |
| Twelve, apart | 61.2 s / 25.8 s / 25% / 39.2% | 62.1 s / 26.9 s / 24% / 38.6% |
| Twelve, as a brigade | 61.5 s / 21.6 s / 17% / 39.4% | 59.7 s / 19.6 s / 16% / 38.8% |

The changes are within about 2 s either way, near the noise, as expected
when routes were already about right. The change
is in the rules: a flank in the trees now costs what it should, in
pace and on the route.

## Stage 2 in full: drilled movement

### What was built

- **Small moves keep the facing** (change 9, `Regiments::order`). A block
  sent less than four tiles (`SHUFFLE_DISTANCE`) with no facing given, no
  attack and no target keeps facing the way it faces and sidesteps or
  backs into its place (`drilled_pace`: full pace within 45 degrees of
  ahead, three quarters sideways, half backing), instead of turning
  round, walking and turning back. The last steps into place at the end of any other march keep
  half pace (`Regiments::drill`). Single bodies (heroes, workers, big
  beasts) walk as before: drilled pace made workers faster to their
  fields, which moved the economy.
- **Make way** (change 10, `Combat::make_way`). A regiment at ease (no
  orders, not braced, fighting, working, running or held, not on a wall,
  no enemy within 20 tiles, `WAY_FOE`) looks each crowd pass for a friend
  marching right through it and on past (a friend coming to stand beside
  it or where it stands is placed by `own_place` instead). Within six
  tiles of that friend (`WAY_AHEAD`) it steps out of the friend's lane
  (the way the friend walks now: its route's next leg, or the point its
  brigade steers it at, as far as the end of its march), to the near
  side, at three quarters of its pace (`way_pace`), facing as it faced,
  and remembers where it stood (`Regiments::home`). It never steps into
  a friend that won't give way (holding its ground, fighting, marching
  by); then it is pushed through as one standing its ground is. When
  no lane is within a step of it, it steps back: straight back, or
  squared along its front or flank when a friend is in the way of that,
  and only when the step makes no overlap deeper than now
  (`clear_to_step`). The one earlier in the army goes back first; a
  friend at ease in the way is shouldered aside (and steps back in
  turn). A friend that has settled where it stood ends it: it stands
  where it is. Still not back 12 s after the last friend marched by
  (`WAY_PATIENCE`; two that were shoved past each other can wedge), it
  marches back as on any order, round friends in the way, drawing up
  beside the place if one has settled on it.
  Around it: a regiment it shoves while stepping out goes the whole way
  and remembers its own place too (`make_room`); `own_place` counts a
  friend that made way as standing where it stood, so nobody is sent to
  the gap; marchers step round a friend making way by half as much
  (`keep_apart`); and a regiment making way counts as moving.
- **Real footprints, checked** (change 11,
  `crates/gc_game/src/footprints.rs`). A test measures every model's
  lowest 40% (where bodies meet in a block) against its file and rank
  spacing and fails when a body is more than 10% bigger than its room.
  It found the Broodmother Bulk (rank 3.0 to 3.9), Colossus Maw (3.5 to
  3.9) and Reclaimer (1.5 to 1.75) too big for their room, now widened
  (Mario's choice, 22 November 2026, 9:20 am NZDT: big bodies only).
  Listed as known and left as they are: levelled pikes (drawn reaching
  ahead of the block on purpose) and the Brood line troops (Chitin
  Guard, Mantid Reapers, Shellbacks, Spitters, Skitterlings, Burrowers),
  drawn up to 40% bigger than their room; widening those changes their
  frontage and is a balance change for Mario to decide with the balance
  tables.

### What it measured

The 42 recorded marches (last arrival / first to last / time in
disorder):

| | stage 6 | stage 2 |
|---|---|---|
| Six regiments, apart | 49.7 s / 14.7 s / 10% | 49.5 s / 14.5 s / 10% |
| Six, as a brigade | 49.5 s / 10.9 s / 7% | 49.3 s / 10.9 s / 6% |
| Twelve, apart | 62.1 s / 26.9 s / 24% | 60.6 s / 25.4 s / 22% |
| Twelve, as a brigade | 59.7 s / 19.6 s / 16% | 58.8 s / 18.7 s / 15% |

Twelve regiments arrive a second or so sooner and a little less
disordered: those that arrive first and stand at ease no longer hold up
those still coming in past them. Six barely change; they seldom march
through each other. Two things on the way were not kept:

- Drilled pace for every regiment's last steps into place (full pace
  ahead instead of half) made all four rows about 2 s quicker, but the
  computer held a ford in 33 of 64 battles instead of 56 (its line came
  forward into the ford faster); drilled pace is now for small moves
  only (53 of 64).
- A measuring slip showed spreads 2 to 3 s tighter: regiments stepping
  aside after they had arrived were counted as not arrived yet. A column of three marching through a line
of six at ease (`crates/gc_sim/tests/make_way.rs`) spent about 700
regiment-ticks in disorder before and none now; the line stands where
it stood within a twentieth of a tile.

The stress scenario `make-way` marches eight regiments ordered as one
group through twelve at ease in two lines. The last arrives in 28.3 s
(34 s with nobody making way), and neither the marchers nor the army
at ease spend any time in disorder (71 and 77 regiment-seconds with
nobody making way). The army at ease steps up to 6.6 tiles aside, one
shoving the next, and stands where it stood again by the time the last
marcher arrives.

### What make way does not do yet

- **Stepping back is its own small solver.** It tries a straight step,
  then squared ones, then shoulders friends, and only after 12 s
  marches back as on any order. Marching back that way from the start
  would be one rule fewer, but after only 4 s it already pushed
  through neighbours still stepping back themselves (130
  regiment-ticks of disorder in the crossing test, none at 12 s);
  worth another look before stage 3 adds more crossings.
- **Orders given a tick or two apart.** A regiment not yet given its
  order may make way for a moment for a friend ordered just before it
  (a computer player's orders for one army land over a few ticks). It
  steps aside a little and then marches with its own order; the
  pathfinding test of three blocks filing through a gap gives its orders
  in one tick for that reason.

### The guessing rules, each switched off

Each of the eleven rules was switched off on its own, everywhere, over
the 42 marches and the whole stress suite (twelve regiments apart, last
arrival / disorder; baseline 58.6 s / 22%):

| Rule off | Twelve apart | Stress suite |
|---|---|---|
| Step aside from friends (`keep_apart`) | 56.3 s / 17% | a column misses the bridge span it should be on as it goes; the fight in the Tithe scenario goes another way |
| Queue at a narrow place | 56.1 s / 23% | 48 regiments no longer all through a Gate in 240 s |
| Trail a friend marching ahead | 56.4 s / 23% | the bridge column again |
| Wait its turn at a ford | 58.2 s / 22% | the Tithe fight again |
| March through after waiting (ghosting) | 58.6 s / 22% | all pass |
| Slow when pushing a friend | 51.1 s / 28% | three brigade gate and ford scenarios fail on disorder |
| Squeeze through a narrow place | 59.0 s / 21% | the bridge column; the brigade ford |
| Square up before wheeling in tight ground | 57.3 s / 21% | 48 through a Gate; 24 eight abreast through a Gate |
| Make room for a friend standing in it | 58.6 s / 22% | the rally point; make way |
| Draw up beside a friend going to the same spot | 58.5 s / 22% | the rally point |
| Lend a route to friends ordered together | 57.6 s / 22% | all pass |

None was taken out. Eight break a check that guards what they are for.
Stepping aside from friends is quicker and less disordered off on
twelve regiments, but on six it is slower, and it is what keeps a
column off a line standing its ground (braced, held, fighting), which
make way leaves alone. Ghosting and lending never come into play on
these runs, which is why they show nothing: ghosting is the last way
out for a regiment stuck for good, and lending lets a hundred
regiments ordered at once set off on the next tick while the route
search catches up; both cost little.

### Checks

`crates/gc_sim/tests/make_way.rs` (a column through a line at ease, four
offsets), `crowding.rs` (a friend at ease makes way and steps back; one
told to Hold is pushed through as before; one making way never steps
into a friend holding its ground), `formations.rs` (a short
move sidesteps or backs into place; a longer one still turns and
walks), the stress scenario `make-way`, and
`crates/gc_game/src/footprints.rs`.

