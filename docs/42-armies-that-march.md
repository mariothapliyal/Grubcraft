# 42 · Armies that march as armies

Mario (2026-11-21): look at drastic improvements to pathfinding, unit
organisation and unit coherence. Broad scope, drastic reforms where they
are justified, thorough and creative.

This is the proposal. It starts from what armies do in the game today,
measured and drawn, names the one cause behind most of it, and lists
twenty changes in five stages. Stage 1 is the reform everything else
hangs on, and it is specified in full here, with its real-use section.

**Status (2026-11-21):** proposal, reviewed by Opus the same day (verdict:
build stage 1, with changes; they are folded in below and listed under
"What the review changed"). Stage 0 (the measuring tools) and stage 1 are
being built in the same thread.

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

## Stage 1 in full: marching together

### What the player sees

Select six regiments and right-click across the map. They step off
together, already in the shape they will stand in (melee in front, bows
behind, horse on the wings, as the army shapes already arrange them),
facing the way they are going. On open ground they hold that shape and
walk straight. Coming to a ford two regiments wide, the rows split in
good time: the front row goes through two abreast, the rest follow two
by two, nobody stands in anybody. Beyond the ford the head slows, the
rows come up and widen again, and the army walks on. At the goal the
whole army wheels to the facing ordered and stands, all within a few
seconds.

### How it works

- **A brigade** is a new thing in the simulation (`gc_sim::brigades`): its
  members, the shape they will stand in at the end, one track, and how
  far along the track its head has come. Every move order carries one
  more number, the brigade it belongs to (0 for none), chosen by whoever
  gives the order. Moves of one player with the same number are one
  brigade. The arranging (roles, presets, saved shapes, a dragged width)
  stays where it is, on the ordering player's machine: the simulation
  reads the shape off the spots the moves name.
- **Who is in it.** The regiments standing together when the order is
  given. One far from the rest (more than 30 tiles from the main body)
  walks to its spot alone, as today. Workers and caravans never join.
  Two regiments make a brigade; one is a plain move. A move shorter than
  the brigade is wide is a plain move too: there is nothing to keep
  together.
- **The track.** One route, searched once, from the member nearest the
  middle of the body to a point a short run behind where the shape will
  stand, then straight in along the shape's own front-to-back line, so
  the brigade arrives square to its shape. It keeps a wide berth.
- **The shape, seen along the way in.** Each member's spot in the final
  shape is read as so far behind the shape's head and so far left or
  right, looking along the direction of arrival (the ordered facing
  turned by quarter turns to match it, so a line told to face sideways
  arrives as a column and turns into line). Members whose spots are level
  make a row. The order of march is row by row from the head, left to
  right.
- **Rows on the track.** Rows are laid from the head. A regiment stands
  beside the one before it if the ground is wide enough for the widened
  row all the way from that row's place, and eight tiles beyond, back to
  where the regiment would walk if it fell in behind instead. Else it
  falls in behind. On open ground this gives the shape itself, gaps and
  all. Coming to a narrow place it gives a funnel: single file (or two
  or three abreast, as fit) nearest the gap and wider rows behind,
  formed before anyone reaches the mouth. Beyond it each regiment moves
  up beside the one ahead only when it is itself through. The room is
  measured across the track each way (`Nav::room_each_side`) afresh at
  every look, four looks a second, so a gate shutting or a ford flooding
  is seen at once. A row that has re-formed keeps its new form for at
  least two seconds.
- **Each regiment's place** is a lane: so far behind the head along the
  track, so far left or right of it. As the head moves, the places move
  with it and bend round corners with the track (the track's direction
  is smoothed over a few tiles so places sweep round a corner rather
  than jump). A regiment is steered at a point six tiles ahead of its
  place, never at the place itself: faster when it is behind, slower
  when ahead, waiting when well ahead, at most its own speed. It never
  turns back for its place.
- **Keeping together.** The head moves at the pace ordered (the slowest
  member's) over the worst going any member is on, and slower in
  proportion as the member furthest behind its place falls back, down to
  a fifth of its pace, never a stop (a stopped head would block the exit
  of a narrow place).
- **Left behind.** A member in a melee, in a trap, more than 20 tiles
  behind its place, or making no headway for five seconds no longer
  slows the head. It stays in the brigade and takes its place again if
  it catches up; else it comes on to its final spot alone.
- **Between members in step the guessing rules are off.** Two members
  both near their places don't step aside from each other, queue behind
  each other or wait out their patience on each other: their places are
  already clear of one another. While a brigade is still forming up, and
  toward everyone outside it, the rules work as today. Pushing through a
  friend still slows and disorders, whoever it is.
- **Arriving.** When the head reaches the end of the track each member
  is sent the last few tiles to its place as it then stands (rows that
  don't fit the ground there have already re-formed to fit), turns to
  the facing ordered, and the brigade is over.
- **Leaving a brigade.** Any other order to a regiment takes it out. So
  does routing. The same order given again (the same number, the same
  place within three tiles) changes nothing, so a player clicking twice
  and a computer player repeating itself don't restart anything.
- **Fighting on the way.** On an attack-move, when a member turns aside
  to fight, the head halts and the rest stand in their places, ready,
  each turning on any enemy that comes within its own reach. When the
  fighting is over the members take their places again and the brigade
  goes on. On a plain move the brigade keeps marching.

### Real use

*Sizes.* One regiment: unchanged, a plain move. Two to six: one or two
rows, as the shape says. Twelve: two or three rows; through a ford they
go two or three abreast. Forty-eight sent as one brigade: rows of eight
on the open ground, single file through a gate in the order of march,
with the rows behind drawn up and waiting their turn, each a row's
depth back from the one in front. (In single file with clear gaps 48
regiments cannot pass a gate as fast as 48 pushing through each other
do today in 50 s; how much slower is measured, and reported with the
disorder saved.)

*Shapes.* Line, Deep, Hammer, Square, Plain and saved shapes all march as
themselves, since the order of march and the rows come from the shape.
A dragged width sets how wide the rows may be on open ground. Regiments
in the Column order (#411) keep their column inside their place; a brigade
that is all columns is a column of columns.

*Through, over and round.* A gate, a bridge deck and a ford are narrow
ground to the track like any other: the rows that fit go through abreast
(two on a two-lane bridge, three on a five-tile deck, one through a
gate), and a single block still narrows by files inside its place
(`squeeze`) where even one is too wide; rows are spaced by the depth
blocks have when narrowed, so a column doesn't grow back into the row
behind. Round a wood or a lake the whole brigade takes one side, the
track's, instead of splitting. A lone rock or a copse in a field narrows
the rows on that side for as long as they pass it (a known roughness:
stage 3 lets a row pass either side). A gate that shuts, a ford that
floods or a bridge that falls re-routes the brigade as one; members
caught on the far side are left behind and find their own way.

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
as each member finishes one leg it joins the brigade of the next, which
waits at a crawl until most are in. Mud, woods and wading slow the head
with the worst going any member has, so the brigade slows as one.
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
members out by ordering them again.

### What it should measure

| | Today | After stage 1 |
|---|---|---|
| First to last regiment arriving (6 and 12 regiments, the five marches above) | 23 to 36 s | under 6 s |
| Marching time in disorder | 20 to 48% | under 5% |
| Time for the whole army to arrive | 53 to 75 s | no more than today |
| 48 regiments through one gate as a brigade: regiment-seconds in disorder | 1,193 (sent one by one) | under 300 |
| The same: all through | 50 s | measured and reported (single file with gaps is slower than pushing through) |
| Simulation tick, heap and instruction counts | budgets in docs/12 | inside the same budgets |

Holding the army together should not cost the last man much time:
today's last regiment sets the army's arrival already, and the brigade
only brings the others in with it. Also counted, so the cure is not
worse than the illness: times a place jumps, times a regiment faces
about, how long the head crawls, and members left behind.

### Checks

New stress scenarios: `brigade-ford` (twelve regiments across a
two-regiment ford: shape kept before, rows through in order, re-formed
beyond, spread on arrival), `brigade-corner` (rows round a right-angle
bend between cliffs), `brigade-gate` (48 regiments through one gate as
one brigade, beside today's `army-gate`, which keeps sending 48 separate
moves so the old way stays measured). The cohesion report runs on grown
maps for armies of 3, 6 and 12. A golden replay with a brigade in it.
The computer players' walling and battle tables are re-run, since their
armies now arrive together.

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
