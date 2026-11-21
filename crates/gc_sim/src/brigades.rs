//! Brigades: regiments ordered together march as one body (docs/42).
//!
//! A move order may carry a brigade number (see [`crate::Command::MoveTo`]).
//! The regiments of one player sent with the same number are a brigade
//! until they arrive: they share one way there (the track, searched once),
//! each has a place in the body (how far behind the brigade's middle and
//! how far to its side, from where its order sends it), and the brigade's
//! middle (the anchor) walks the track no faster than its hindmost can
//! keep up. So an army sets off together, keeps its shape on the way,
//! files through a narrows row by row and arrives together.
//!
//! A brigade only steers: each tick it gives every member a point a little
//! ahead of its place to walk at (never the place itself, which it would
//! overshoot and turn back for) and a pace, and the regiment marches there
//! as on any move (see `Regiments::step_movement`). A member given any
//! other order, routed or sent after a foe has a target the brigade did
//! not set, and is a member no more. Where the ground is too narrow for a
//! row abreast, the row marches as several, one behind the other (see
//! [`Brigades::pack`]).
//!
//! Nothing here allocates once a match has started.

use alloc::vec;
use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::Block;
use crate::formation::GROUP_GAP;
use crate::hash::StateHasher;
use crate::nav::{self, Nav};
use crate::orders::Step;
use crate::regiments::Regiments;
use crate::{MAX_PARTIES, PlayerId, Terrain, UnitStats};

/// Brigades each player may have on the march at once; a move sent with a
/// brigade number when none is free marches alone.
pub const PER_PARTY: usize = 8;
/// The most regiments in one brigade; more march alone.
pub const MAX_MEMBERS: usize = 64;
/// Points kept of a brigade's track: the way ahead as far as it has been
/// searched, and the way behind as far back as its tail.
const TRACK: usize = 40;
/// How far ahead of its place a member walks at, in tiles.
const LEAD: Fx = Fx::from_int(6);
/// ... and never nearer ahead of where it is now.
const MIN_LEAD: Fx = Fx::from_int(2);
/// An order to within this of where a member is already going, in tiles,
/// is the same order sent again.
const SAME_PLACE: Fx = Fx::from_int(3);
/// Regiments further than this from the middle of those ordered, in
/// tiles, march alone.
const JOIN_REACH: Fx = Fx::from_int(20);
/// A march shorter than this, in tiles, is each regiment's own.
const SHORT: Fx = Fx::from_int(14);
/// How much faster or slower than the anchor a member goes for each tile
/// it is behind or ahead of its place, in tiles a tick.
const GAIN: Fx = Fx::from_ratio(1, 20);
/// The anchor walks at full pace while its hindmost member is no further
/// behind its place than this, in tiles...
const LAG_FREE: Fx = Fx::ONE;
/// ... slows over this much more...
const LAG_SPAN: Fx = Fx::from_int(6);
/// ... down to this share of its pace, never to a stand: the body creeps
/// on while it forms.
const CRAWL: Fx = Fx::from_ratio(1, 4);
/// A member this far behind its place, in tiles, is left behind: the
/// anchor no longer waits for it.
const LEFT_BEHIND: Fx = Fx::from_int(45);
/// ... as is one that has made no headway for this many ticks.
const STALLED: u8 = 100;
/// Ticks between two looks at how the rows fit the ground.
const REPACK: u32 = 5;
/// The ground's width is measured every this many tiles along the track...
const SAMPLE: Fx = Fx::from_int(2);
/// ... at this many points, from ahead of the foremost row back.
const PROFILE: usize = 48;
/// A row narrows this far before a narrows, in tiles, and widens again
/// only once the ground has been wide for as long.
const LOOK: Fx = Fx::from_int(8);
/// The ground is measured as far either side of the track as the widest
/// row reaches, between these, in tiles.
const REACH: Fx = Fx::from_int(8);
const MAX_REACH: Fx = Fx::from_int(30);
/// Room kept between a row's end and the edge of the ground, in tiles.
const EDGE: Fx = Fx::from_ratio(1, 2);
/// Members whose places lie within this of a row's front, in tiles, are
/// of that row.
const ROW: Fx = Fx::from_ratio(3, 2);
/// A member within this of its place along the track and across it, in
/// tiles, is in step: it and its neighbours in step leave each other be
/// (see [`Regiments::in_step`]).
const IN_STEP_ALONG: Fx = Fx::from_ratio(3, 2);
const IN_STEP_ACROSS: Fx = Fx::from_ratio(5, 4);
/// How fast a place moves up the body when rows close up, in tiles a tick
/// (slower than a regiment marches, so it can keep up), falls back when
/// they file, and moves across.
const SLEW_UP: Fx = Fx::from_ratio(1, 32);
const SLEW_BACK: Fx = Fx::from_ratio(1, 8);
const SLEW_SIDE: Fx = Fx::from_ratio(1, 16);
/// Ticks between a member's looks at whether it can see where it walks.
const SIGHT: u32 = 4;
/// Looks in a row with nothing of the track in sight before a member
/// finds its way to it by a route of its own.
const LOST: u8 = 5;
/// While any regiment of an attack-move brigade has turned aside to fight
/// within this of the anchor, in tiles, the body halts.
const HUNT_NEAR: Fx = Fx::from_int(25);
/// How far ahead of the anchor the track is checked for ground that has
/// changed, in tiles, and how often, in ticks.
const WATCH: Fx = Fx::from_int(16);
const WATCH_EVERY: u32 = 20;
/// The track is searched on from its end when the anchor is this near
/// it, in tiles.
const EXTEND: Fx = Fx::from_int(30);

/// A member ahead of its place folds in to its lane over the last this
/// many tiles before its place comes level.
const FUNNEL: Fx = Fx::from_int(6);

/// Members of a row within this of each other along the track, in tiles,
/// are level when the row files.
const TURN: Fx = Fx::from_int(3);

const QUARTER: Fx = Fx::from_ratio(1, 4);

/// How a regiment stands in its brigade (see [`crate::World::brigade`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BrigadeView {
    /// The number its order gave.
    pub number: u16,
    /// Where its order sent it: where it will stand when the brigade
    /// arrives.
    pub goal: FxVec2,
    /// Its place in the body now, once the brigade has found its way.
    pub place: Option<FxVec2>,
    /// At its place, in step with its neighbours.
    pub in_step: bool,
    /// How far behind its place it is along the brigade's way, in tiles
    /// (ahead of it when negative).
    pub behind: Fx,
    /// How it is steering: 1 at its lane, 2 at the middle of the way, 3
    /// at the way just ahead, 4 finding its own way to the brigade.
    pub steering: u8,
}

/// How a member is steering.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Mode {
    /// Just joined: marching to where its order sent it until the brigade
    /// takes it up.
    Plain,
    /// At a point ahead of its place.
    Lane,
    /// At the middle of the track ahead of its place (its own lane is out
    /// of sight).
    Centre,
    /// At the middle of the track just ahead of where it is.
    Short,
    /// Nothing of the track in sight: on a route of its own to it.
    Astray,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Phase {
    /// No track yet.
    Gathering,
    Marching,
}

#[derive(Clone, Copy)]
struct Brigade {
    /// 0: free.
    number: u16,
    /// An attack-move.
    hunt: bool,
    phase: Phase,
    /// Members have joined or been sent somewhere else since it last
    /// looked.
    changed: bool,
    /// Standing while a member fights (see [`HUNT_NEAR`]).
    halted: bool,
    /// The middle of where its members were sent when the track was
    /// searched for, and now.
    goal: FxVec2,
    centre: FxVec2,
    /// The way it faces as it arrives: places are told off along and
    /// across this.
    ahead: FxVec2,
    /// Tiles a tick: the slowest pace its members were ordered at.
    pace: Fx,
    /// How far along the track the anchor is, and how fast it went last
    /// tick.
    arc: Fx,
    speed: Fx,
    /// How far along the track its hindmost member is.
    tail: Fx,
    len: u8,
    /// The track stops short of the goal.
    partial: bool,
    track: [FxVec2; TRACK],
    /// How far along the track each point is.
    cum: [Fx; TRACK],
}

impl Brigade {
    const FREE: Brigade = Brigade {
        number: 0,
        hunt: false,
        phase: Phase::Gathering,
        changed: false,
        halted: false,
        goal: FxVec2::ZERO,
        centre: FxVec2::ZERO,
        ahead: FxVec2::ZERO,
        pace: Fx::ZERO,
        arc: Fx::ZERO,
        speed: Fx::ZERO,
        tail: Fx::ZERO,
        len: 0,
        partial: false,
        track: [FxVec2::ZERO; TRACK],
        cum: [Fx::ZERO; TRACK],
    };

    fn total(&self) -> Fx {
        self.cum[self.len as usize - 1]
    }

    /// Add `p` to the end of the track (not a point on top of the last);
    /// false when there is no room.
    fn push(&mut self, p: FxVec2) -> bool {
        let n = self.len as usize;
        if n == 0 {
            self.track[0] = p;
            self.cum[0] = Fx::ZERO;
            self.len = 1;
            return true;
        }
        let d = (p - self.track[n - 1]).length();
        if d < QUARTER {
            return true;
        }
        if n == TRACK {
            return false;
        }
        self.track[n] = p;
        self.cum[n] = self.cum[n - 1] + d;
        self.len += 1;
        true
    }

    /// The point `arc` along the track and the way the track runs there;
    /// past either end, straight on.
    fn at(&self, arc: Fx) -> (FxVec2, FxVec2) {
        let n = self.len as usize;
        let mut k = 0;
        while k + 2 < n && self.cum[k + 1] <= arc {
            k += 1;
        }
        let len = self.cum[k + 1] - self.cum[k];
        let d = self.track[k + 1] - self.track[k];
        let dir = FxVec2::new(d.x / len, d.y / len);
        (self.track[k] + dir * (arc - self.cum[k]), dir)
    }

    /// The point `arc` along the track, and the way the track runs about
    /// there (over a few tiles, so a row wheels round a corner instead of
    /// snapping to the new way).
    fn at_smooth(&self, arc: Fx) -> (FxVec2, FxVec2) {
        let (p, dir) = self.at(arc);
        let d = self.at(arc + MIN_LEAD).0 - self.at(arc - MIN_LEAD).0;
        let run = d.normalized();
        (p, if run == FxVec2::ZERO { dir } else { run })
    }

    /// The place `side` to the right of the track at `arc`.
    fn lane(&self, arc: Fx, side: Fx) -> FxVec2 {
        let (p, dir) = self.at_smooth(arc);
        p + FxVec2::new(-dir.y, dir.x) * side
    }

    /// How far along the track the point of it nearest `p` is, looking
    /// only between `lo` and `hi` along it (a track that doubles back
    /// round a river has points near `p` that are a long way on).
    fn project(&self, p: FxVec2, lo: Fx, hi: Fx) -> Fx {
        let n = self.len as usize;
        let mut best = (u64::MAX, lo.max(self.cum[0]));
        for k in 0..n - 1 {
            let (a0, a1) = (self.cum[k], self.cum[k + 1]);
            let len = a1 - a0;
            // Past either end the track runs straight on.
            let t_lo = if k == 0 {
                lo - a0
            } else {
                (lo - a0).max(Fx::ZERO)
            };
            let t_hi = if k + 2 == n {
                hi - a0
            } else {
                (hi - a0).min(len)
            };
            if t_lo > t_hi {
                continue;
            }
            let d = self.track[k + 1] - self.track[k];
            let dir = FxVec2::new(d.x / len, d.y / len);
            let t = (p - self.track[k]).dot(dir).clamp(t_lo, t_hi);
            let off = (p - (self.track[k] + dir * t)).length_sq_raw();
            if off < best.0 {
                best = (off, a0 + t);
            }
        }
        best.1
    }

    /// Face the way the track comes in to its end (over its last few
    /// tiles), or from as far as it has been searched toward the goal.
    fn set_frame(&mut self) {
        let end = self.track[self.len as usize - 1];
        let d = if self.partial {
            self.centre - end
        } else {
            end - self.at((self.total() - LOOK).max(self.cum[0])).0
        };
        let run = d.normalized();
        self.ahead = if run == FxVec2::ZERO {
            self.at(self.total()).1
        } else {
            run
        };
    }

    /// Forget the track behind `arc`, keeping a point before it.
    fn drop_behind(&mut self, arc: Fx) {
        let n = self.len as usize;
        let mut k = 0;
        while k + 2 < n && self.cum[k + 1] < arc {
            k += 1;
        }
        if k > 0 {
            self.track.copy_within(k..n, 0);
            self.cum.copy_within(k..n, 0);
            self.len -= k as u8;
        }
    }

    /// End the track at `arc` along it (between its ends).
    fn cut(&mut self, arc: Fx) {
        let n = self.len as usize;
        let mut k = 0;
        while k + 2 < n && self.cum[k + 1] <= arc {
            k += 1;
        }
        let p = self.at(arc).0;
        self.track[k + 1] = p;
        self.cum[k + 1] = arc;
        self.len = k as u8 + 2;
    }
}

/// The brigades on the march, and each regiment's place in its own.
pub(crate) struct Brigades {
    /// [`PER_PARTY`] for each party, in turn.
    pool: Vec<Brigade>,
    /// How many of them are in use.
    live: u16,
    /// Per regiment slot. The brigade it marches in: its place in `pool`
    /// plus one, or 0 for none. The rest means nothing with none.
    of: Vec<u8>,
    generation: Vec<u16>,
    mode: Vec<Mode>,
    /// Looks in a row it has seen nothing of the track.
    lost: Vec<u8>,
    /// Ticks in a row behind its place and not stepping.
    stall: Vec<u8>,
    /// Where its order sent it and how to face there.
    end: Vec<FxVec2>,
    facing: Vec<Option<Angle>>,
    /// The pace its order gave it.
    ordered: Vec<Fx>,
    /// The target and pace the brigade last gave it: with any other, it
    /// has had other orders.
    set: Vec<FxVec2>,
    paced: Vec<Fx>,
    /// How far along the track it is.
    arc: Vec<Fx>,
    /// Its place: how far behind the anchor and how far to the right of
    /// the track, now and where the last look at the ground put it.
    station: Vec<Fx>,
    side: Vec<Fx>,
    want_station: Vec<Fx>,
    want_side: Vec<Fx>,
    /// Room to work in, kept between ticks; not part of the state.
    list: Vec<u32>,
    room: Vec<[Fx; 2]>,
    leg: Vec<FxVec2>,
}

/// A member being placed in [`Brigades::pack`].
#[derive(Clone, Copy)]
struct Placing {
    slot: u32,
    /// Where it stands in the body as it arrives: right of the middle and
    /// behind it.
    lat: Fx,
    back: Fx,
    half_width: Fx,
    half_depth: Fx,
    /// Its turn when its row files: those furthest along the track go
    /// first (in steps of a few tiles), and of those level, the nearest
    /// the track.
    turn: (i32, i32),
    side: Fx,
    station: Fx,
}

impl Brigades {
    pub(crate) fn new(regiments: usize) -> Brigades {
        Brigades {
            pool: vec![Brigade::FREE; MAX_PARTIES * PER_PARTY],
            live: 0,
            of: vec![0; regiments],
            generation: vec![0; regiments],
            mode: vec![Mode::Plain; regiments],
            lost: vec![0; regiments],
            stall: vec![0; regiments],
            end: vec![FxVec2::ZERO; regiments],
            facing: vec![None; regiments],
            ordered: vec![Fx::ZERO; regiments],
            set: vec![FxVec2::ZERO; regiments],
            paced: vec![Fx::ZERO; regiments],
            arc: vec![Fx::ZERO; regiments],
            station: vec![Fx::ZERO; regiments],
            side: vec![Fx::ZERO; regiments],
            want_station: vec![Fx::ZERO; regiments],
            want_side: vec![Fx::ZERO; regiments],
            list: Vec::with_capacity(MAX_MEMBERS),
            room: vec![[Fx::ZERO; 2]; PROFILE],
            leg: vec![FxVec2::ZERO; nav::MAX_WAYPOINTS],
        }
    }

    /// The number of the brigade regiment slot `i` marches in, and where
    /// it is going with it (not the point ahead it walks at now).
    pub(crate) fn of(&self, i: usize) -> Option<(u16, FxVec2)> {
        let slot = *self.of.get(i)?;
        (slot != 0).then(|| (self.pool[usize::from(slot) - 1].number, self.end[i]))
    }

    /// How regiment slot `i` stands in its brigade, if it marches in one.
    pub(crate) fn view(&self, regs: &Regiments, i: usize) -> Option<BrigadeView> {
        let (number, goal) = self.of(i)?;
        let b = &self.pool[usize::from(self.of[i]) - 1];
        let marching = b.phase == Phase::Marching && self.mode[i] != Mode::Plain;
        Some(BrigadeView {
            number,
            goal,
            place: marching.then(|| b.lane(b.arc - self.station[i], self.side[i])),
            in_step: regs.step_with[i] != 0,
            behind: b.arc - self.station[i] - self.arc[i],
            steering: self.mode[i] as u8,
        })
    }

    /// Whether regiment slot `i` already marches in its owner's brigade
    /// `number` to `target` (or near it) to face `facing` there: the same
    /// order sent again, which changes nothing.
    pub(crate) fn marching_to(
        &self,
        i: usize,
        number: u16,
        target: FxVec2,
        facing: Option<Angle>,
    ) -> bool {
        self.of(i).is_some_and(|(n, end)| {
            let d = end - target;
            n == number
                && self.facing[i] == facing
                && d.x.abs() < SAME_PLACE
                && d.y.abs() < SAME_PLACE
        })
    }

    /// Regiment slot `i` has just been sent on a move with brigade
    /// `number` (`hunt`: an attack-move): it joins its owner's brigade of
    /// that number, or starts it. With none free, or the brigade full, it
    /// marches alone.
    pub(crate) fn join(&mut self, regs: &mut Regiments, i: usize, number: u16, hunt: bool) {
        let owner = usize::from(regs.owner[i]);
        let Some(end) = regs.target[i] else {
            return;
        };
        if owner >= MAX_PARTIES || number == 0 || i >= self.of.len() {
            return;
        }
        let own = owner * PER_PARTY..(owner + 1) * PER_PARTY;
        let Some(slot) = (own.clone().find(|&s| self.pool[s].number == number))
            .or_else(|| own.clone().find(|&s| self.pool[s].number == 0))
        else {
            self.leave(regs, i);
            return;
        };
        let tag = slot as u8 + 1;
        if self.of[i] != tag {
            let members = self.of.iter().filter(|&&t| t == tag).count();
            self.leave(regs, i);
            if members >= MAX_MEMBERS {
                return;
            }
        }
        let b = &mut self.pool[slot];
        if b.number == 0 {
            *b = Brigade {
                number,
                ..Brigade::FREE
            };
            self.live += 1;
        }
        b.hunt = hunt;
        b.changed = true;
        self.of[i] = tag;
        self.generation[i] = regs.generation[i];
        self.mode[i] = Mode::Plain;
        self.lost[i] = 0;
        self.stall[i] = 0;
        self.end[i] = end;
        self.facing[i] = regs.target_facing[i];
        self.ordered[i] = regs.pace[i];
        self.set[i] = end;
        self.paced[i] = regs.pace[i];
    }

    /// Regiment slot `i` is of no brigade from now, keeping whatever
    /// orders it has.
    fn leave(&mut self, regs: &mut Regiments, i: usize) {
        self.of[i] = 0;
        regs.step_with[i] = 0;
    }

    /// Regiment slot `i` leaves its brigade and marches on alone to where
    /// its order sent it.
    fn release(&mut self, regs: &mut Regiments, i: usize) {
        if self.mode[i] != Mode::Plain {
            regs.target[i] = Some(self.end[i]);
            regs.target_facing[i] = self.facing[i];
            regs.set_seeks_place(i, true);
            regs.pace[i] = self.ordered[i];
        }
        self.leave(regs, i);
    }

    /// The brigade in `slot` is over: its members (in `self.list`) march
    /// on alone.
    fn disband(&mut self, regs: &mut Regiments, slot: usize) {
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            self.release(regs, i);
        }
        self.pool[slot].number = 0;
        self.live -= 1;
    }

    /// Gather the members of the brigade in `slot` into `self.list`.
    fn muster(&mut self, slot: usize) {
        let tag = slot as u8 + 1;
        self.list.clear();
        for (i, &t) in self.of.iter().enumerate() {
            if t == tag && self.list.len() < MAX_MEMBERS {
                self.list.push(i as u32);
            }
        }
    }

    /// Steer every brigade for this tick: before the routes are planned
    /// and the regiments step.
    pub(crate) fn step(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &mut Nav,
        terrain: &Terrain,
        tick: u32,
    ) {
        if self.live == 0 {
            return;
        }
        // Who has had other orders, or is no more.
        for i in 0..self.of.len() {
            if self.of[i] == 0 {
                continue;
            }
            if !regs.alive[i] || regs.generation[i] != self.generation[i] {
                self.leave(regs, i);
                continue;
            }
            // On a route of its own to the track, it may get to the end
            // of it: it goes on from there.
            let d = regs.position[i] - self.set[i];
            if self.mode[i] == Mode::Astray
                && regs.target[i].is_none()
                && d.x.abs() < Fx::ONE
                && d.y.abs() < Fx::ONE
            {
                self.mode[i] = Mode::Short;
                self.lost[i] = 0;
                regs.target[i] = Some(self.set[i]);
            }
            if regs.target[i] != Some(self.set[i]) || regs.on_wall[i] {
                // The pace was the brigade's unless whoever gave the new
                // order set one.
                if regs.pace[i] == self.paced[i] {
                    regs.pace[i] = self.ordered[i];
                }
                self.leave(regs, i);
            } else if regs.run_left[i] > 0 {
                // Told to run, it runs on ahead.
                self.release(regs, i);
            }
        }
        // At most one search a tick, among all.
        let mut searched = false;
        for slot in 0..self.pool.len() {
            if self.pool[slot].number != 0 {
                self.march(slot, regs, units, nav, terrain, tick, &mut searched);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn march(
        &mut self,
        slot: usize,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &mut Nav,
        terrain: &Terrain,
        tick: u32,
        searched: &mut bool,
    ) {
        let owner = (slot / PER_PARTY) as PlayerId;
        self.muster(slot);
        if self.list.len() < 2 {
            self.disband(regs, slot);
            return;
        }
        if self.pool[slot].changed {
            let (mut x, mut y, mut pace) = (0i64, 0i64, Fx::MAX);
            for &i in &self.list {
                let i = i as usize;
                x += i64::from(self.end[i].x.raw());
                y += i64::from(self.end[i].y.raw());
                pace = pace.min(self.ordered[i]);
            }
            let n = self.list.len() as i64;
            let centre = FxVec2::new(Fx::from_raw((x / n) as i32), Fx::from_raw((y / n) as i32));
            let b = &mut self.pool[slot];
            b.changed = false;
            b.centre = centre;
            b.pace = pace;
            let moved = centre - b.goal;
            if b.len < 2 || moved.x.abs() >= SAME_PLACE || moved.y.abs() >= SAME_PLACE {
                b.len = 0;
                b.phase = Phase::Gathering;
            } else {
                b.set_frame();
            }
        }
        if self.pool[slot].phase == Phase::Gathering {
            if *searched {
                return;
            }
            *searched = true;
            if !self.gather(slot, owner, regs, units, nav) {
                self.disband(regs, slot);
                return;
            }
        }
        // The track: on from its end as the anchor nears it, and again
        // from the anchor when the ground ahead has changed.
        let b = &self.pool[slot];
        let near_end = b.partial && b.arc + EXTEND > b.total();
        let watch = (tick + slot as u32).is_multiple_of(WATCH_EVERY)
            && b.arc > b.cum[0]
            && b.arc < b.total()
            && !self.open_ahead(slot, owner, nav);
        if (near_end || watch) && !*searched {
            *searched = true;
            if watch {
                let arc = self.pool[slot].arc;
                self.pool[slot].cut(arc);
            }
            if !self.extend(slot, owner, regs, units, nav) {
                self.disband(regs, slot);
                return;
            }
        }
        // Members taken up since the last look.
        let mut fresh = false;
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            if self.mode[i] == Mode::Plain {
                let b = &self.pool[slot];
                self.arc[i] = b.project(regs.position[i], b.arc - LEFT_BEHIND, b.arc + JOIN_REACH);
                fresh = true;
            }
        }
        if fresh || (tick + slot as u32).is_multiple_of(REPACK) {
            self.pack(slot, owner, regs, units, nav, terrain);
        }
        self.measure(slot, regs);
        // An attack-move halts as a body while one of its own fights
        // near by.
        if self.pool[slot].hunt && (tick + slot as u32).is_multiple_of(SIGHT) {
            let b = &self.pool[slot];
            let (at, number) = (b.at(b.arc).0, b.number);
            self.pool[slot].halted = (0..regs.alive.len()).any(|j| {
                let plan = regs.plan(j);
                let d = regs.position[j] - at;
                regs.alive[j]
                    && regs.owner[j] == owner
                    && plan.diverted
                    && matches!(plan.hunt, Some(Step::Move { brigade, .. }) if brigade == number)
                    && d.x.abs() < HUNT_NEAR
                    && d.y.abs() < HUNT_NEAR
            });
        }
        let b = &mut self.pool[slot];
        b.arc += b.speed;
        // Arrived: each walks the last of the way to its own place.
        if !b.partial && b.arc + LEAD >= b.total() {
            self.disband(regs, slot);
            return;
        }
        self.steer(slot, owner, regs, nav, tick);
    }

    /// Find the brigade in `slot` its track, from the member nearest the
    /// middle of them all, and tell off its places. Those too far from
    /// the rest march alone. False when it is not to march as a brigade:
    /// there is no way, the way is short or too few are left.
    fn gather(
        &mut self,
        slot: usize,
        owner: PlayerId,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &mut Nav,
    ) -> bool {
        let (mut x, mut y) = (0i64, 0i64);
        for &i in &self.list {
            x += i64::from(regs.position[i as usize].x.raw());
            y += i64::from(regs.position[i as usize].y.raw());
        }
        let n = self.list.len() as i64;
        let mid = FxVec2::new(Fx::from_raw((x / n) as i32), Fx::from_raw((y / n) as i32));
        let far = |p: FxVec2| {
            let d = p - mid;
            d.x.abs() > JOIN_REACH || d.y.abs() > JOIN_REACH
        };
        let mut kept = 0;
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            if far(regs.position[i]) {
                self.release(regs, i);
            } else {
                self.list[kept] = i as u32;
                kept += 1;
            }
        }
        self.list.truncate(kept);
        if kept < 2 {
            return false;
        }
        let (mut lead, mut least, mut berth) = (0, u64::MAX, nav::CLEARANCE);
        for &i in &self.list {
            let i = i as usize;
            let d = (regs.position[i] - mid).length_sq_raw();
            if d < least {
                (lead, least) = (i, d);
            }
            berth = berth.max(regs.berth(units, i));
        }
        let b = &mut self.pool[slot];
        b.len = 0;
        b.partial = false;
        b.goal = b.centre;
        b.push(regs.position[lead]);
        if !self.extend(slot, owner, regs, units, nav) {
            return false;
        }
        let b = &mut self.pool[slot];
        if b.len < 2 || (!b.partial && b.total() < SHORT) {
            return false;
        }
        b.phase = Phase::Marching;
        b.speed = Fx::ZERO;
        b.halted = false;
        // The anchor starts where nobody is ahead of its place: the body
        // forms as it moves off.
        b.arc = b.cum[0];
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            let b = &self.pool[slot];
            self.arc[i] = b.project(regs.position[i], b.cum[0] - LEFT_BEHIND, b.cum[0] + LEFT_BEHIND);
        }
        let _ = berth;
        true
    }

    /// Search the track of the brigade in `slot` on from its end toward
    /// its goal. False when there is no way on.
    fn extend(
        &mut self,
        slot: usize,
        owner: PlayerId,
        regs: &Regiments,
        units: &[UnitStats],
        nav: &mut Nav,
    ) -> bool {
        // A wide berth, so the way keeps to ground with room for a row
        // where there is any.
        let mut berth = nav::CLEARANCE;
        for &i in &self.list {
            berth = berth.max(regs.berth(units, i as usize));
        }
        let berth = berth.saturating_mul(2).min(nav::MAX_BERTH);
        let b = &mut self.pool[slot];
        b.drop_behind(b.tail.min(b.arc) - REACH);
        let from = b.track[b.len as usize - 1];
        let found = if nav.has_map() {
            nav.track(owner, from, b.centre, berth, &mut self.leg)
        } else {
            self.leg[0] = b.centre;
            Some((1, false))
        };
        let Some((n, partial)) = found else {
            return false;
        };
        b.partial = partial;
        for k in 0..n {
            if !b.push(self.leg[k]) {
                // No room for the rest: searched for again further on.
                b.partial = true;
                break;
            }
        }
        if b.len < 2 {
            return false;
        }
        b.goal = b.centre;
        b.set_frame();
        true
    }

    /// Whether the track of the brigade in `slot` is still open ground
    /// for [`WATCH`] ahead of the anchor.
    fn open_ahead(&self, slot: usize, owner: PlayerId, nav: &Nav) -> bool {
        let b = &self.pool[slot];
        let n = b.len as usize;
        let mut from = b.at(b.arc).0;
        for k in 1..n {
            if b.cum[k] <= b.arc {
                continue;
            }
            let last = b.cum[k] >= b.arc + WATCH;
            let to = if last {
                b.at(b.arc + WATCH).0
            } else {
                b.track[k]
            };
            if !nav.clear_line(owner, from, to) {
                return false;
            }
            if last {
                break;
            }
            from = to;
        }
        true
    }

    /// Tell off the places of the brigade in `slot` for the ground it is
    /// on and coming to.
    ///
    /// Each member has a place in the body as it will stand on arriving:
    /// so far right of its middle and so far behind, seen the way the
    /// track comes in. Members about level are a row. A row marches
    /// abreast, as it will stand, where the ground either side of the
    /// track is wide enough for it from [`LOOK`] ahead of it back to its
    /// rear (shifted across to fit, if it must be); where it is not, as
    /// many of the row as fit side by side go first and the rest fall in
    /// behind them, as many abreast again. Nobody is put closer behind a
    /// member in its way than their blocks and [`GROUP_GAP`] allow, so
    /// rows that file through a narrows string out behind each other and
    /// close up again beyond it.
    ///
    /// Looking [`LOOK`] ahead, a row has filed before it reaches a
    /// narrows, and since it widens only once the ground has been wide
    /// that long, ground that pinches and opens does not make it
    /// flicker.
    fn pack(
        &mut self,
        slot: usize,
        owner: PlayerId,
        regs: &Regiments,
        units: &[UnitStats],
        nav: &Nav,
        terrain: &Terrain,
    ) {
        let b = self.pool[slot];
        let n = self.list.len();
        let right = FxVec2::new(-b.ahead.y, b.ahead.x);
        let mut m = [Placing {
            slot: 0,
            lat: Fx::ZERO,
            back: Fx::ZERO,
            half_width: Fx::ZERO,
            half_depth: Fx::ZERO,
            turn: (0, 0),
            side: Fx::ZERO,
            station: Fx::ZERO,
        }; MAX_MEMBERS];
        let (mut front, mut reach) = (Fx::MAX, REACH);
        for (k, &i) in self.list.iter().enumerate() {
            let i = i as usize;
            let rel = self.end[i] - b.centre;
            let now = Block::of(regs, units, i);
            let full = Block::drawn_up(regs, units, i, regs.position[i]);
            let lat = rel.dot(right);
            m[k] = Placing {
                slot: i as u32,
                lat,
                turn: (-(self.arc[i].raw() / TURN.raw()), lat.raw().abs()),
                back: -rel.dot(b.ahead),
                half_width: full.half_width,
                half_depth: now.half_depth.max(full.half_depth),
                side: Fx::ZERO,
                station: Fx::ZERO,
            };
            front = front.min(m[k].back);
            reach = reach.max(m[k].lat.abs() + m[k].half_width + EDGE + Fx::ONE);
        }
        let reach = reach.min(MAX_REACH);
        let m = &mut m[..n];
        // From the front back, and each row from the left.
        m.sort_unstable_by_key(|p| (p.back.raw(), p.lat.raw(), p.slot));

        // The ground's width along the track, from ahead of the front.
        let top = b.arc - front + LOOK;
        for k in 0..PROFILE {
            let (p, dir) = b.at_smooth(top - SAMPLE * k as i32);
            self.room[k] = nav.room_sides(owner, p, Angle::of_vec(dir), reach, |q| terrain.wet(q));
        }
        // The least room right and left of the track between two points
        // along it.
        let room = &self.room;
        let width = |lo: Fx, hi: Fx| {
            let k = |a: Fx| ((top - a).raw() / SAMPLE.raw()).clamp(0, PROFILE as i32 - 1) as usize;
            let (first, last) = (k(hi), (k(lo) + 1).min(PROFILE - 1));
            let mut least = [reach; 2];
            for r in &room[first..=last] {
                least = [least[0].min(r[0]), least[1].min(r[1])];
            }
            least
        };

        // The room right and left of the track for a row `station` behind
        // the anchor, `deep` from front to rear: an edge within reach
        // leaves a margin.
        let room_at = |station: Fx, deep: Fx| {
            let place = b.arc - station;
            let [r, l] = width(place - deep * 2 - Fx::ONE, place + LOOK);
            let margin = |w: Fx| if w < reach { w - EDGE } else { w };
            (margin(r), margin(l))
        };
        // How far behind the anchor the front of `m[first..last]` must be
        // to be clear of those placed before them in their way (`placed`
        // of them), at least `row_back`.
        let clear_of = |m: &[Placing], placed: usize, first: usize, last: usize, row_back: Fx| {
            let mut clear = row_back;
            for p in &m[first..last] {
                for q in &m[..placed] {
                    let beside = p.half_width + q.half_width + GROUP_GAP - QUARTER / 2;
                    if (p.side - q.side).abs() < beside {
                        let behind = q.station + q.half_depth + p.half_depth + GROUP_GAP;
                        clear = clear.max(behind - (p.back - row_back));
                    }
                }
            }
            clear
        };

        let mut row = 0;
        while row < n {
            let mut end = row + 1;
            while end < n && m[end].back - m[row].back <= ROW {
                end += 1;
            }
            let row_back = m[row].back;
            let deep = (m[row..end].iter())
                .map(|p| p.half_depth + (p.back - row_back))
                .max()
                .unwrap_or(Fx::ZERO);
            // Abreast, as they will stand, moved across to fit.
            m[row..end].sort_unstable_by_key(|p| (p.lat.raw(), p.slot));
            let (lo, hi) = (
                m[row].lat - m[row].half_width,
                m[end - 1].lat + m[end - 1].half_width,
            );
            let mut station = row_back;
            let mut abreast = false;
            // Twice: where it is put may not be where it was first
            // measured for.
            for _ in 0..2 {
                let (r, l) = room_at(station, deep);
                abreast = hi - lo <= r + l;
                if !abreast {
                    break;
                }
                let shift = (-l - lo).max(Fx::ZERO) + (r - hi).min(Fx::ZERO);
                for p in &mut m[row..end] {
                    p.side = p.lat + shift;
                }
                let clear = clear_of(m, row, row, end, row_back);
                if clear == station {
                    break;
                }
                station = clear;
            }
            if abreast {
                for p in &mut m[row..end] {
                    p.station = station + (p.back - row_back);
                }
                row = end;
                continue;
            }
            // Filing: those furthest along first, as many as fit side by
            // side, and the rest fold in behind them.
            let mut first = row;
            while first < end {
                let mut station = row_back;
                let mut last = first + 1;
                for _ in 0..2 {
                    m[first..end].sort_unstable_by_key(|p| (p.turn, p.slot));
                    let (r, l) = room_at(station, deep);
                    let wide = r + l;
                    let mut across = m[first].half_width * 2;
                    last = first + 1;
                    while last < end {
                        let more = m[last].half_width * 2 + GROUP_GAP;
                        if across + more > wide {
                            break;
                        }
                        across += more;
                        last += 1;
                    }
                    // Closed up about the track, or in the middle of a
                    // gap too narrow for one.
                    m[first..last].sort_unstable_by_key(|p| (p.lat.raw(), p.slot));
                    let mut x = if across > wide {
                        (r - l) / 2 - across / 2
                    } else {
                        (-across / 2).clamp(-l, r - across)
                    };
                    for p in &mut m[first..last] {
                        p.side = x + p.half_width;
                        x += p.half_width * 2 + GROUP_GAP;
                    }
                    let clear = clear_of(m, first, first, last, row_back);
                    if clear == station {
                        break;
                    }
                    station = clear;
                }
                for p in &mut m[first..last] {
                    p.station = station + (p.back - row_back);
                }
                first = last;
            }
            row = end;
        }
        for p in m.iter() {
            let i = p.slot as usize;
            self.want_station[i] = p.station;
            self.want_side[i] = p.side;
            if self.mode[i] == Mode::Plain {
                self.station[i] = p.station;
                self.side[i] = p.side;
            }
        }
        // A brigade setting off: the anchor starts where nobody is ahead
        // of its place, so the body forms as it moves off.
        if b.speed == Fx::ZERO && b.arc == b.cum[0] && !b.halted {
            let mut arc = Fx::MIN;
            for p in m.iter() {
                let i = p.slot as usize;
                arc = arc.max(self.arc[i] + self.station[i]);
            }
            self.pool[slot].arc = arc;
        }
    }

    /// Where each member of the brigade in `slot` is along the track and
    /// against its place, and so how fast the anchor goes this tick.
    fn measure(&mut self, slot: usize, regs: &mut Regiments) {
        let b = self.pool[slot];
        let tag = slot as u8 + 1;
        let (mut lag, mut tail) = (Fx::ZERO, b.arc);
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            let here = regs.position[i];
            if self.mode[i] != Mode::Astray {
                self.arc[i] = b.project(here, self.arc[i] - SAME_PLACE, self.arc[i] + LEAD);
            }
            // Its place moves gently to where the ground last put it.
            let to = self.want_station[i];
            self.station[i] = if to < self.station[i] {
                (self.station[i] - SLEW_UP).max(to)
            } else {
                (self.station[i] + SLEW_BACK).min(to)
            };
            let to = self.want_side[i];
            let across = (to - self.side[i]).clamp(-SLEW_SIDE, SLEW_SIDE);
            self.side[i] += across;
            let behind = b.arc - self.station[i] - self.arc[i];
            self.stall[i] = if behind > Fx::ONE && !regs.stepped(i) {
                self.stall[i].saturating_add(1)
            } else {
                0
            };
            let (at, dir) = b.at(self.arc[i]);
            let across = (here - at).dot(FxVec2::new(-dir.y, dir.x)) - self.side[i];
            let in_step = self.mode[i] == Mode::Lane
                && behind.abs() < IN_STEP_ALONG
                && across.abs() < IN_STEP_ACROSS;
            regs.step_with[i] = if in_step { tag } else { 0 };
            if self.kept_up(regs, i, behind) {
                lag = lag.max(behind);
                tail = tail.min(self.arc[i]);
            }
        }
        let slow = (Fx::ONE - (lag - LAG_FREE) / LAG_SPAN).clamp(CRAWL, Fx::ONE);
        let b = &mut self.pool[slot];
        b.tail = tail;
        b.speed = if b.halted { Fx::ZERO } else { b.pace * slow };
    }

    /// Whether the anchor waits for member `i`, `behind` its place: not
    /// for one fighting, trapped, left far behind or making no headway.
    fn kept_up(&self, regs: &Regiments, i: usize, behind: Fx) -> bool {
        regs.engaged[i].is_none()
            && regs.trapped_left[i] == 0
            && behind < LEFT_BEHIND
            && self.stall[i] < STALLED
            && self.mode[i] != Mode::Astray
    }

    /// Give each member of the brigade in `slot` the point it walks at
    /// this tick and its pace.
    fn steer(&mut self, slot: usize, owner: PlayerId, regs: &mut Regiments, nav: &mut Nav, tick: u32) {
        let b = self.pool[slot];
        for k in 0..self.list.len() {
            let i = self.list[k] as usize;
            let here = regs.position[i];
            let place = b.arc - self.station[i];
            let near = self.arc[i] + MIN_LEAD;
            let full = (place + LEAD).max(near);
            let was = self.mode[i];
            if was == Mode::Plain {
                // Taken up: the brigade steers it from now.
                regs.set_seeks_place(i, false);
                regs.target_facing[i] = None;
            }
            // Ahead of its place (the rows are filing), it keeps to where
            // it is across the track and waits, and folds in to its lane
            // as its place comes level: not all into the lane at once,
            // on top of each other.
            let (at, dir) = b.at(self.arc[i]);
            let across = (here - at).dot(FxVec2::new(-dir.y, dir.x));
            let ahead = (self.arc[i] - place).clamp(Fx::ZERO, FUNNEL);
            let side = self.side[i] + (across - self.side[i]) * (ahead / FUNNEL);
            let lane = b.lane(full, side);
            if was == Mode::Plain || (tick + i as u32).is_multiple_of(SIGHT) {
                let sees = |p: FxVec2| nav.clear_line(owner, here, p);
                let found = if sees(lane) {
                    Some(Mode::Lane)
                } else if sees(b.at(full).0) {
                    Some(Mode::Centre)
                } else if sees(b.at(near).0) {
                    Some(Mode::Short)
                } else {
                    None
                };
                match found {
                    Some(mode) => {
                        if was == Mode::Astray {
                            // Back by the track, wherever along it.
                            self.arc[i] = b.project(here, b.tail - LEFT_BEHIND, b.arc + JOIN_REACH);
                        }
                        self.mode[i] = mode;
                        self.lost[i] = 0;
                    }
                    None if was == Mode::Astray => {}
                    None => {
                        self.lost[i] = self.lost[i].saturating_add(1);
                        if self.lost[i] >= LOST || was == Mode::Plain {
                            // By a route of its own to where the body is
                            // (looked for afresh: its way so far led
                            // somewhere else).
                            self.mode[i] = Mode::Astray;
                            self.set[i] = b.at(place.clamp(b.cum[0], b.total())).0;
                            nav.lost(i);
                        } else if was == Mode::Lane {
                            self.mode[i] = Mode::Centre;
                        }
                    }
                }
            }
            let target = match self.mode[i] {
                Mode::Lane | Mode::Plain => lane,
                Mode::Centre => b.at(full).0,
                Mode::Short => b.at(near).0,
                Mode::Astray => self.set[i],
            };
            let behind = place - self.arc[i];
            let pace = if self.kept_up(regs, i, behind) {
                (b.speed + GAIN * behind).clamp(Fx::ZERO, regs.speed[i])
            } else {
                regs.speed[i]
            };
            regs.target[i] = Some(target);
            regs.pace[i] = pace;
            self.set[i] = target;
            self.paced[i] = pace;
        }
    }

    /// Bring in the brigades of `from` (a world on the same map) with the
    /// regiments `copied` says were copied here, slot for slot.
    pub(crate) fn copy_from(&mut self, from: &Brigades, copied: impl Fn(usize) -> bool) {
        self.pool.clone_from(&from.pool);
        self.live = from.live;
        for i in 0..self.of.len().min(from.of.len()) {
            if !copied(i) {
                continue;
            }
            self.of[i] = from.of[i];
            self.generation[i] = from.generation[i];
            self.mode[i] = from.mode[i];
            self.lost[i] = from.lost[i];
            self.stall[i] = from.stall[i];
            self.end[i] = from.end[i];
            self.facing[i] = from.facing[i];
            self.ordered[i] = from.ordered[i];
            self.set[i] = from.set[i];
            self.paced[i] = from.paced[i];
            self.arc[i] = from.arc[i];
            self.station[i] = from.station[i];
            self.side[i] = from.side[i];
            self.want_station[i] = from.want_station[i];
            self.want_side[i] = from.want_side[i];
        }
    }

    /// Nothing while no brigade is on the march, so a match without any
    /// hashes as it did before there were brigades.
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        if self.live == 0 {
            return;
        }
        let vec = |h: &mut StateHasher, v: FxVec2| {
            h.i32(v.x.raw());
            h.i32(v.y.raw());
        };
        for b in &self.pool {
            h.u16(b.number);
            if b.number == 0 {
                continue;
            }
            h.u8(u8::from(b.hunt) | u8::from(b.changed) << 1 | u8::from(b.halted) << 2);
            h.u8(b.phase as u8);
            vec(h, b.goal);
            vec(h, b.centre);
            vec(h, b.ahead);
            for v in [b.pace, b.arc, b.speed, b.tail] {
                h.i32(v.raw());
            }
            h.u8(b.len);
            h.u8(u8::from(b.partial));
            for k in 0..b.len as usize {
                vec(h, b.track[k]);
                h.i32(b.cum[k].raw());
            }
        }
        for i in 0..self.of.len() {
            h.u8(self.of[i]);
            if self.of[i] == 0 {
                continue;
            }
            h.u16(self.generation[i]);
            h.u8(self.mode[i] as u8);
            h.u8(self.lost[i]);
            h.u8(self.stall[i]);
            vec(h, self.end[i]);
            h.u32(self.facing[i].map_or(u32::MAX, |a| u32::from(a.0)));
            vec(h, self.set[i]);
            for v in [
                self.ordered[i],
                self.paced[i],
                self.arc[i],
                self.station[i],
                self.side[i],
                self.want_station[i],
                self.want_side[i],
            ] {
                h.i32(v.raw());
            }
        }
    }
}
