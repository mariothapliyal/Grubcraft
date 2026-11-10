//! The Engineer: the works a Conquest mind puts up against its enemies
//! (docs/18-ai.md, "Engineer"; docs/26-fortifications.md, stage 8). It
//! is part of the Steward, spending the economy's attention, a spare
//! worker, and only the money left over after what the Steward is saving
//! for, every couple of seconds:
//!
//! - a **front wall** across the way the nearest enemy comes: a gate
//!   where its own way out runs, and the wall drawn a few tiles at a time
//!   out from the gate either way, the stretches by the gate raised a
//!   level once it is rich;
//! - then, at tier 2, the **ring** the front wall is the first part of:
//!   the shortest wall that, with the cliffs and deep water, closes round
//!   its buildings, its seeps and veins and room to grow (see `ringwork`),
//!   so it is a walled town (see `gc_sim::towns`); a breach in it is
//!   mended before any other work;
//! - a **barricade** in front of its rally point, mid-game;
//! - **stakes** planted by spearmen and archers waiting at the rally point
//!   once it has seen enemy riders or monsters;
//! - a few hidden **traps** on the approach once Grub piles up;
//! - a **bridge** over the narrowest water on the way to its enemies when
//!   water cuts it off from all of them;
//! - a **path cut through thicket** on the way to the nearest enemy where
//!   grubbing it out beats marching round (docs/41, step 6).
//!
//! Its level says which of these it does ([`Fortify`]); its personality's
//! caution how soon and how much. Manning the walls and assaulting the
//! enemy's are the Marshal's.

use gc_sim::structures::{GATE, GROWN, Refusal, Role, StructureHandle};
use gc_sim::walls::WallTile;
use gc_sim::{BuildingId, Command, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::ringwork::{self, Door, Ground, Piece, Site};
use super::{Look, Sense, Steward};
use crate::conquest::placing;
use crate::conquest::strategy::Fortify;
use crate::orders::Hands as Clicks;

/// Ticks between its looks.
const THINK_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// When it starts on the front wall without tier 2, ticks: 10 minutes for
/// the boldest, 6 for the most cautious.
const WALL_AFTER_BOLD: u32 = 10 * 60 * TICKS_PER_SECOND;
const WALL_AFTER_CAUTIOUS: u32 = 6 * 60 * TICKS_PER_SECOND;
/// How far round the ring from its gate the front wall goes either way,
/// tiles: what it draws before tier 2.
const FRONT: u32 = 9;
/// Room kept inside the ring: this far round home whatever stands there,
/// this far round each of its buildings, and round each seep and vein it
/// works within [`NODES_IN`] tiles of home (with the lane to it).
const HOME_ROOM: i32 = 8;
const ROOM: i32 = 3;
const NODE_ROOM: i32 = 2;
const NODES_IN: f32 = 16.0;
/// Its buildings this far from home, tiles, are inside the ring; any
/// further out is left outside.
const TOWN: i32 = 20;
/// Grub it keeps in hand before it spends on works, and the most of what
/// the Steward is saving for that it also keeps: works are cheap, and a
/// mind always saving for its next hall would never put any up.
const RESERVE: u32 = 120;
const SAVING_KEPT: u32 = 150;
/// Grub in hand beyond its saving from which it raises the wall.
const RICH: u32 = 500;
/// How long to wait before laying the ring out again: where none could be
/// laid (a Choir's power fields grow), or what was laid is all drawn and
/// the town still isn't walled (its base has grown, a piece couldn't go
/// up); and for a breach.
const LINE_RETRY: u32 = 60 * TICKS_PER_SECOND;
const BREACH_RETRY: u32 = 10 * TICKS_PER_SECOND;
/// How many places it remembers having given a gate up at.
const NO_GATES: usize = 8;
/// How long it waits for troops to clear the gate's ground before it
/// walls the middle instead.
const GATE_WAIT: u32 = 30 * TICKS_PER_SECOND;
/// The barricade: tiles across, and how far in front of the rally point.
const BARRICADE_HALF: i32 = 2;
const BARRICADE_OUT: f32 = 4.0;
/// Looks it tries for a barricade at a rally point before giving it up
/// (troops may stand where it would go).
const BARRICADE_TRIES: u8 = 5;
/// Stakes: planted by regiments this near the rally point, at most this
/// many lines.
const STAKES_NEAR: f32 = 8.0;
const STAKE_LINES: usize = 2;
/// Traps: laid from this many tiles out, once it has this much Grub to
/// spare; two, or three for a cautious mind.
const TRAPS_OUT: f32 = 22.0;
const TRAP_GRUB: u32 = 300;
const TRAPS: u8 = 2;
/// When it puts its towers up though no front wall line has fitted, ticks
/// (15 minutes).
const TOWERS_ANYWAY: u32 = 15 * 60 * TICKS_PER_SECOND;
/// Longest bridge it draws, tiles.
const BRIDGE_LONGEST: i32 = 16;

/// A tile of thicket costs this many tiles of open ground to cross, when
/// it weighs cutting a path against marching round: about what one worker
/// takes to grub it out, in a regiment's march.
const PER_THICKET: u32 = 12;
/// Ticks between its looks for thicket in the way (the search is the
/// whole map's).
const CLEAR_EVERY: u32 = 90 * TICKS_PER_SECOND;
/// The most thicket tiles along the way it orders cleared at once; the
/// path is cut three tiles wide round them.
const CLEAR_RUN: usize = 6;

/// A straight line of tiles, from one tile to another.
type Line = ((i32, i32), (i32, i32));

/// The walls it has laid out round home (see [`ringwork`]).
#[derive(Clone, Debug)]
struct Works {
    /// The lines still to draw, nearest the gate first.
    pieces: Vec<Piece>,
    /// The gate still to lay: its tile and turn.
    gate: Option<((i32, i32), u8)>,
    /// When troops first stood on the gate's ground, while they do.
    crowded: Option<u32>,
    /// Its gate, laid or to lay; none for a faction that builds none.
    door: Option<Door>,
    /// A wall tile of each stretch it raises: either side of the gate, or
    /// the front of a ring with no gate.
    raise: Vec<(i32, i32)>,
    /// Whether the ring closes once every line is drawn (and its ground
    /// to clear is cleared and walled).
    closes: bool,
    /// The tiles of the ring to clear before wall can go up on them, each
    /// with how far round the ring it is.
    clear: Vec<((i32, i32), u32)>,
    /// When it was laid out, and the tier it had then.
    planned: u32,
    tier: u8,
}

impl Works {
    /// Whether line `p` is drawn at `tier`: the front wall before tier 2
    /// (and of a ring that can't close yet), all of it from then.
    fn due(&self, p: &Piece, tier: u8) -> bool {
        self.due_at(p.round, tier)
    }

    /// As [`Works::due`], for the tile `round` tiles round the ring.
    fn due_at(&self, round: u32, tier: u8) -> bool {
        round <= FRONT || (tier >= 2 && self.closes)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct Engineer {
    next_think: u32,
    works: Option<Works>,
    /// Whether its ring has ever closed (so a gap in it now is a breach).
    closed: bool,
    /// No ring could be laid out: look again from then.
    next_line: u32,
    /// Where it gave a gate up (troops never left its ground, or it was
    /// built on): the next is put somewhere else.
    no_gate: Vec<(i32, i32)>,
    raised: usize,
    /// The rally point it has barricaded, or tried to.
    barricaded: Option<Vec2>,
    /// Looks spent trying for a barricade where none fitted.
    barricade_tries: u8,
    /// The first tile of the barricade it last ordered, and how many it has
    /// ordered again because none went up there (the bank ran short before
    /// the order came through, say).
    barricade_at: Option<(i32, i32)>,
    barricade_redone: u8,
    /// Regiments sent to plant stakes.
    staked: Vec<RegimentHandle>,
    traps: u8,
    /// The bridge it means to draw: `None` until it has looked, then the
    /// crossing and its tiles, or none where it needs or can draw none (or
    /// has drawn it).
    bridge: Option<Option<(Line, u32)>>,
    /// When it next looks for thicket in the way, and the worker it last
    /// sent to clear some (no other is sent while that one is at it).
    next_clear: u32,
    clearer: Option<RegimentHandle>,
}

impl Engineer {
    /// Whether its front wall is all drawn (its gate laid and every line
    /// drawn or given up), or it has given up looking for a line: towers
    /// come after the wall, so they don't take its Grub or its ground.
    pub(super) fn walled(&self, now: u32) -> bool {
        match &self.works {
            Some(w) => w.gate.is_none() && !w.pieces.iter().any(|p| p.round <= FRONT),
            None => self.next_line > now && now >= TOWERS_ANYWAY,
        }
    }

    /// The ground its buildings keep off: the lines of wall still to
    /// draw, and the way through its gate.
    pub(super) fn keep_clear(&self) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
        let mid = |t: (i32, i32)| Vec2::new(t.0 as f32 + 0.5, t.1 as f32 + 0.5);
        let works = self.works.iter();
        works.flat_map(move |w| {
            let way = w.door.map(|d| (mid(d.step(-4)), mid(d.step(5))));
            (w.pieces.iter().map(move |p| (mid(p.from), mid(p.to)))).chain(way)
        })
    }
}

impl Steward {
    /// The Engineer's look: put up whatever works are due, with what
    /// Grub is spare, in front of `rally`. One order a look, given while
    /// the economy has attention for it.
    pub(super) fn fortify(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        rally: Option<Vec2>,
    ) {
        if s.plan.fortifies == Fortify::None || s.now < self.engineer.next_think {
            return;
        }
        self.engineer.next_think = s.now + THINK_TICKS;
        let (Some(home), Some(bank)) = (self.home, s.world.bank(s.party)) else {
            return;
        };
        let Some(toward) =
            super::super::nearest_other_start(s, home).and_then(|e| (e - home).try_normalize())
        else {
            return;
        };
        if !clicks.free() {
            return;
        }
        // A barricade ordered but never begun is tried again, a few times.
        if let Some(at) = self.engineer.barricade_at.take() {
            let begun = look
                .mine
                .iter()
                .any(|m| m.view.tile == at && m.stats.role == Role::Barricade);
            if !begun && self.engineer.barricade_redone < BARRICADE_TRIES {
                self.engineer.barricade_redone += 1;
                self.engineer.barricaded = None;
            }
        }
        let kept = RESERVE + self.ledger.saving.0.min(SAVING_KEPT);
        let spare = bank.grub.saturating_sub(kept);
        let full = s.plan.fortifies == Fortify::Full;
        let late = s.now >= wall_after(s.caution);
        // Most pressing first: a breach in its ring, a way to the enemy,
        // then field works, then the wall and its ring, then traps.
        let walled = s.world.walled_town(s.party);
        self.engineer.closed |= walled;
        let breached = self.engineer.closed && !walled;
        let order = (full && breached)
            .then(|| self.wall(s, look, home, spare))
            .flatten()
            .or_else(|| full.then(|| self.bridge(s, look, home, spare)).flatten())
            .or_else(|| self.clear_way(s, look, home))
            .or_else(|| {
                let rally = rally?;
                ((look.tier >= 2 || late)
                    && self
                        .engineer
                        .barricaded
                        .is_none_or(|b| b.distance(rally) >= 4.0))
                .then(|| self.barricade(s, look, rally, toward, spare))
                .flatten()
                .or_else(|| self.stakes(s, look, rally))
            })
            // Short of an army, soldiers come before walls and traps.
            .or_else(|| {
                if self.arming || !full {
                    return None;
                }
                (look.tier >= 2 || late)
                    .then(|| self.wall(s, look, home, spare))
                    .flatten()
                    .or_else(|| self.traps(s, look, home, toward, spare))
                    .or_else(|| self.raise_bridge(s, look, spare))
            });
        if let Some((order, worker)) = order {
            // There is attention for it (checked above), so it is given.
            clicks.give(s.now, order);
            if let Some(w) = worker {
                self.sent.push((w, s.now));
            }
        }
    }

    /// Who builds a work at `at`: nobody for one that grows on its own,
    /// else a spare worker; `None` when it needs one and none is spare.
    fn hands(
        &self,
        s: &Sense,
        look: &Look,
        grown: bool,
        at: (i32, i32),
    ) -> Option<Option<RegimentHandle>> {
        if grown {
            return Some(None);
        }
        let at = Vec2::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5);
        self.builder(s, look, at).map(Some)
    }

    /// The walls' next order: laid out round home when first due (and
    /// again when what was laid out is drawn and the town isn't walled, at
    /// tier 2, or after a breach), then its gate first, then the next line
    /// that fits, nearest the gate first (the front wall only before tier
    /// 2; a line only part of which fits is drawn tile by tile), then
    /// raising the stretches by the gate once rich. The order and the
    /// worker it sends, if any.
    fn wall(&mut self, s: &Sense, look: &Look, home: Vec2, spare: u32) -> Option<Order> {
        let wall = s.kit.wall()?;
        let walled = s.world.walled_town(s.party);
        let again = match &self.engineer.works {
            None => s.now >= self.engineer.next_line,
            Some(w) => {
                let idle = w.gate.is_none() && !w.pieces.iter().any(|p| w.due(p, look.tier));
                let wait = if self.engineer.closed {
                    BREACH_RETRY
                } else {
                    LINE_RETRY
                };
                // Its base has grown since the front wall was laid out.
                let grown = w.gate.is_none() && w.tier < 2 && look.tier >= 2;
                (grown || idle && (look.tier >= 2 || w.pieces.is_empty()))
                    && !walled
                    && s.now >= w.planned + wait
            }
        };
        if again {
            let t0 = std::time::Instant::now();
            let works = lay_works(s, look, home, &self.engineer.no_gate);
            if std::env::var("AI_RING_DEBUG").is_ok() {
                eprintln!(
                    "RING p{} t{}s tier{} took {:?}: {}",
                    s.party,
                    s.now / TICKS_PER_SECOND,
                    look.tier,
                    t0.elapsed(),
                    match &works {
                        Some(w) => format!(
                            "{} pieces ({} front), closes {}, door {:?}, new gate {:?}",
                            w.pieces.len(),
                            w.pieces.iter().filter(|p| p.round <= FRONT).count(),
                            w.closes,
                            w.door,
                            w.gate
                        ),
                        None => "none".into(),
                    }
                );
            }
            if works.is_none() && self.engineer.works.is_none() {
                self.engineer.next_line = s.now + LINE_RETRY;
            }
            // Raised stretches stay raised: only a new gate's are looked
            // at again.
            if works.as_ref().map(|w| &w.raise) != self.engineer.works.as_ref().map(|w| &w.raise) {
                self.engineer.raised = 0;
            }
            match works {
                Some(w) => self.engineer.works = Some(w),
                // Nothing can be laid out now: what stands is kept, and
                // looked at again later.
                None => {
                    if let Some(w) = self.engineer.works.as_mut() {
                        w.planned = s.now;
                    }
                }
            }
        }
        let gate_at = self.engineer.works.as_ref()?.gate;
        if let Some((tile, turn)) = gate_at {
            let gate = s.kit.gate()?;
            if spare < u32::from(gate.stats.cost_grub) {
                return None;
            }
            let laid = s.world.can_lay(s.party, gate.id, tile, turn);
            // Troops standing there move on: it waits a while for them.
            if laid == Err(Refusal::Crowded) {
                let works = self.engineer.works.as_mut()?;
                let since = *works.crowded.get_or_insert(s.now);
                if s.now < since + GATE_WAIT {
                    return None;
                }
            }
            // Its ground taken meanwhile, or troops that never leave it:
            // the ring is laid out again, with its gate somewhere else.
            if laid.is_err() {
                if self.engineer.no_gate.len() >= NO_GATES {
                    self.engineer.no_gate.clear();
                }
                let mid = (tile.0 + i32::from(turn == 1), tile.1 + i32::from(turn == 0));
                self.engineer.no_gate.push(mid);
                self.engineer.works = None;
                self.engineer.next_line = s.now + BREACH_RETRY;
                return None;
            }
            let w = self.builder_at(s, look, tile)?;
            self.engineer.works.as_mut()?.gate = None;
            let order = Command::Build {
                regiment: w,
                building: gate.id,
                tile: (tile.0 as i16, tile.1 as i16),
                turn,
            };
            return Some((order, Some(w)));
        }
        let cost = u32::from(wall.stats.cost_grub);
        if spare < cost {
            return None;
        }
        let grown = wall.stats.flags & GROWN != 0;
        let mut k = 0;
        while let Some(&p) = self.engineer.works.as_ref()?.pieces.get(k) {
            if !self.engineer.works.as_ref()?.due(&p, look.tier) {
                k += 1;
                continue;
            }
            let tiles = s
                .world
                .wall_fits(s.party, wall.id, p.from, p.to)
                .unwrap_or_default();
            let crowded = |t: &WallTile| t.fits == Err(Refusal::Crowded);
            if std::env::var("AI_RING_DEBUG").is_ok() {
                eprintln!(
                    "RING p{} t{}s piece {:?} spare {spare}: {:?}",
                    s.party,
                    s.now / TICKS_PER_SECOND,
                    p,
                    tiles.iter().map(|t| t.fits).collect::<Vec<_>>()
                );
            }
            // Walled already: on to the next.
            if tiles.is_empty() {
                self.engineer.works.as_mut()?.pieces.remove(k);
                continue;
            }
            if tiles.iter().all(|t| t.fits.is_ok()) {
                // The whole line is paid for as it is drawn.
                if spare < cost * tiles.len() as u32 {
                    return None;
                }
                let worker = self.hands(s, look, grown, p.from)?;
                self.engineer.works.as_mut()?.pieces.remove(k);
                let order = Command::BuildWall {
                    regiment: worker,
                    building: wall.id,
                    from: (p.from.0 as i16, p.from.1 as i16),
                    to: (p.to.0 as i16, p.to.1 as i16),
                };
                return Some((order, worker));
            }
            // Someone standing there: tried again at the next look, and
            // the lines after it meanwhile.
            if tiles.iter().all(|t| t.fits.is_ok() || crowded(t)) {
                k += 1;
                continue;
            }
            // Not all of it can go up: tile by tile, what can.
            let pieces = &mut self.engineer.works.as_mut()?.pieces;
            pieces.remove(k);
            if p.from != p.to {
                for t in tiles.iter().rev().filter(|t| t.fits.is_ok() || crowded(t)) {
                    let one = Piece {
                        from: t.tile,
                        to: t.tile,
                        round: p.round,
                    };
                    pieces.insert(k, one);
                }
            }
        }
        if let Some(order) = self.clear_ring(s, look) {
            return Some(order);
        }
        // All drawn: raise the stretches by the gate once rich, each once.
        let raise = &self.engineer.works.as_ref()?.raise;
        if self.engineer.raised >= raise.len() || spare < RICH {
            return None;
        }
        let tile = raise[self.engineer.raised];
        let h = own_wall_at(look, tile);
        let raise = h.and_then(|h| s.world.wall_level(s.party, h).ok());
        let Some((h, raise)) = h.zip(raise).filter(|(_, r)| r.afford && spare >= r.grub) else {
            self.engineer.raised += 1;
            return None;
        };
        let grown = s
            .kit
            .building(raise.to)
            .is_some_and(|b| b.stats.flags & GROWN != 0);
        let worker = self.hands(s, look, grown, tile)?;
        self.engineer.raised += 1;
        let order = Command::RaiseWall {
            structure: h,
            regiment: worker,
        };
        Some((order, worker))
    }

    /// Where the ring runs over scrub, woods or thicket (shorter than
    /// going round it), send a spare worker to clear the next few tiles of
    /// its line, three wide; the ring is laid out again over the cleared
    /// ground later. One worker at a time.
    fn clear_ring(&mut self, s: &Sense, look: &Look) -> Option<Order> {
        if (self.engineer.clearer).is_some_and(|w| s.world.clearing_of(w).is_some()) {
            return None;
        }
        let works = self.engineer.works.as_ref()?;
        let &(first, _) = (works.clear.iter()).find(|c| works.due_at(c.1, look.tier))?;
        let far = |t: (i32, i32)| (t.0 - first.0).abs().max((t.1 - first.1).abs());
        let mut way: Vec<(i32, i32)> = (works.clear.iter())
            .filter(|c| works.due_at(c.1, look.tier) && far(c.0) < CLEAR_RUN as i32)
            .map(|c| c.0)
            .collect();
        way.sort_by_key(|&t| far(t));
        let ((x0, y0), (x1, y1)) = clear_run(&way, s.terrain.map_tiles() as i32)?;
        let w = self.builder_at(s, look, first)?;
        let works = self.engineer.works.as_mut()?;
        (works.clear).retain(|&((x, y), _)| !((x0..=x1).contains(&x) && (y0..=y1).contains(&y)));
        self.engineer.clearer = Some(w);
        let order = Command::Clear {
            regiment: w,
            tile: (x0 as i16, y0 as i16),
            size: ((x1 - x0 + 1) as u8, (y1 - y0 + 1) as u8),
        };
        Some((order, Some(w)))
    }

    /// Once rich, raise its own finished bridge a level (braced, then
    /// reinforced piers), so the crossing its armies need stands up to
    /// blows: the whole joined bridge, paid now.
    fn raise_bridge(&self, s: &Sense, look: &Look, spare: u32) -> Option<Order> {
        if spare < RICH {
            return None;
        }
        // One try a drawn bridge: its tiles are raised together.
        let mut tried = Vec::new();
        let (tile, h, raise) = look
            .mine
            .iter()
            .filter(|m| m.finished() && m.stats.role == Role::Bridge)
            .filter(|m| {
                m.view.drawing.is_none_or(|d| {
                    let new = !tried.contains(&d);
                    tried.push(d);
                    new
                })
            })
            .find_map(|m| {
                let raise = s.world.wall_level(s.party, m.view.handle).ok()?;
                (raise.afford && spare >= raise.grub + RICH / 2).then_some((
                    m.view.tile,
                    m.view.handle,
                    raise,
                ))
            })?;
        let grown = s
            .kit
            .building(raise.to)
            .is_some_and(|b| b.stats.flags & GROWN != 0);
        let worker = self.hands(s, look, grown, tile)?;
        let order = Command::RaiseWall {
            structure: h,
            regiment: worker,
        };
        Some((order, worker))
    }

    /// The nearest spare worker to tile `at`.
    pub(super) fn builder_at(
        &self,
        s: &Sense,
        look: &Look,
        at: (i32, i32),
    ) -> Option<RegimentHandle> {
        self.hands(s, look, false, at).flatten()
    }

    /// A barricade across the front of the rally point, once for each
    /// rally point: out in front, a little further, or nearer, whichever
    /// fits first.
    fn barricade(
        &mut self,
        s: &Sense,
        look: &Look,
        rally: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> Option<Order> {
        let b = s.kit.works(Role::Barricade)?;
        let n = 2 * BARRICADE_HALF + 1;
        if spare < u32::from(b.stats.cost_grub) * n as u32 {
            return None;
        }
        let w = self.builder(s, look, rally)?;
        let line = [BARRICADE_OUT, BARRICADE_OUT + 2.0, BARRICADE_OUT - 2.0]
            .into_iter()
            .map(|out| across(rally + toward * out, upright(toward), BARRICADE_HALF))
            .find(|&(from, to)| {
                s.world
                    .wall_fits(s.party, b.id, from, to)
                    .is_ok_and(|t| t.len() as i32 == n && t.iter().all(|t| t.fits.is_ok()))
            });
        // Troops in the way: tried again a few looks later, then given up.
        self.engineer.barricade_tries += 1;
        if line.is_some() || self.engineer.barricade_tries >= BARRICADE_TRIES {
            self.engineer.barricaded = Some(rally);
            self.engineer.barricade_tries = 0;
        }
        let (from, to) = line?;
        self.engineer.barricade_at = Some(from);
        let order = Command::BuildWall {
            regiment: Some(w),
            building: b.id,
            from: (from.0 as i16, from.1 as i16),
            to: (to.0 as i16, to.1 as i16),
        };
        Some((order, Some(w)))
    }

    /// Against enemy riders or monsters it has seen, a spear or archer
    /// regiment waiting at the rally point plants a line of stakes, up to
    /// [`STAKE_LINES`].
    fn stakes(&mut self, s: &Sense, look: &Look, rally: Vec2) -> Option<Order> {
        if self.engineer.staked.len() >= STAKE_LINES || !crate::conquest::spymaster::heavy_foes(s) {
            return None;
        }
        let b = s.kit.works(Role::Stakes)?;
        let r = look.army.iter().find(|r| {
            !r.moving
                && !r.engaged
                && r.pos.distance(rally) <= STAKES_NEAR
                && !self.engineer.staked.contains(&r.handle)
                && s.world.plant_check(r.handle, b.id).is_ok()
        })?;
        self.engineer.staked.push(r.handle);
        let order = Command::PlantStakes {
            regiment: r.handle,
            building: b.id,
        };
        Some((order, None))
    }

    /// A hidden trap on the approach, up to [`TRAPS`] (one more for a
    /// cautious mind), once Grub piles up; none where laying one would use
    /// up the worker (the Brood's Gnawers).
    fn traps(
        &mut self,
        s: &Sense,
        look: &Look,
        home: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> Option<Order> {
        let most = TRAPS + u8::from(s.caution > 0.5);
        if self.engineer.traps >= most || spare < TRAP_GRUB {
            return None;
        }
        let b = s.kit.works(Role::Trap)?;
        if s.kit.uses_up_builder(b) {
            return None;
        }
        let k = f32::from(self.engineer.traps);
        let side = Vec2::new(-toward.y, toward.x);
        // Spread across the way in and a little further out each time.
        let at = home + toward * (TRAPS_OUT + 3.0 * k) + side * (k - 1.0) * 3.0;
        let tile = placing::spot_for(s.world, s.party, b.id, &b.stats, at, &self.lanes(s, look));
        let Some(tile) = tile else {
            // Nowhere near there: that one is given up.
            self.engineer.traps += 1;
            return None;
        };
        let w = self.builder_at(s, look, tile)?;
        self.engineer.traps += 1;
        let order = Command::Build {
            regiment: w,
            building: b.id,
            tile: (tile.0 as i16, tile.1 as i16),
            turn: 0,
        };
        Some((order, Some(w)))
    }

    /// When water cuts it off from every enemy start, bridge the narrowest
    /// crossing near the straight way to the nearest. It looks once.
    fn bridge(&mut self, s: &Sense, look: &Look, home: Vec2, spare: u32) -> Option<Order> {
        let b = s.kit.works(Role::Bridge)?;
        let line = (*self.engineer.bridge.get_or_insert_with(|| {
            let target = super::super::nearest_other_start(s, home)?;
            if !s.terrain.cut_off(home, super::super::other_starts(s, home)) {
                return None;
            }
            narrowest_crossing(s, b.id, home, target)
        }))?;
        let ((from, to), tiles) = line;
        if spare < u32::from(b.stats.cost_grub) * tiles {
            return None;
        }
        // Standard width, so its army crosses a few files abreast.
        let deck = bridge_line(from, to);
        let w = self.builder_at(s, look, from)?;
        self.engineer.bridge = Some(None);
        let order = Command::DrawLine {
            regiment: Some(w),
            building: b.id,
            path: deck.path,
            width: deck.width,
        };
        Some((order, Some(w)))
    }
}

impl Steward {
    /// Where thicket stands on the cheapest way to the nearest enemy start
    /// (crossing it costed at [`PER_THICKET`]), send a spare worker to cut
    /// a path three tiles wide through the first few tiles of it. Every
    /// [`CLEAR_EVERY`]; the next look finds what is left.
    fn clear_way(&mut self, s: &Sense, look: &Look, home: Vec2) -> Option<Order> {
        if s.now < self.engineer.next_clear {
            return None;
        }
        self.engineer.next_clear = s.now + CLEAR_EVERY;
        if (self.engineer.clearer).is_some_and(|w| s.world.clearing_of(w).is_some()) {
            return None;
        }
        let target = super::super::nearest_other_start(s, home)?;
        let way = s.terrain.thicket_on_way(home, target, PER_THICKET);
        let run = clear_run(&way, s.terrain.map_tiles() as i32)?;
        let ((x0, y0), (x1, y1)) = run;
        let at = Vec2::new((x0 + x1) as f32 * 0.5 + 0.5, (y0 + y1) as f32 * 0.5 + 0.5);
        let w = self.builder(s, look, at)?;
        let order = Command::Clear {
            regiment: w,
            tile: (x0 as i16, y0 as i16),
            size: ((x1 - x0 + 1) as u8, (y1 - y0 + 1) as u8),
        };
        self.engineer.clearer = Some(w);
        Some((order, Some(w)))
    }
}

/// The area to clear for the thicket tiles `way` (in order along the
/// way), on a map `n` tiles a side: round the first [`CLEAR_RUN`] of them,
/// a tile wider each side, while it stays three tiles across one way (a
/// path, not a square: a diagonal way gets its first step or two) and at
/// most [`gc_sim::clearing::MAX_CLEAR_SIDE`] the other; as its corner
/// tiles.
fn clear_run(way: &[(i32, i32)], n: i32) -> Option<Line> {
    let first = *way.first()?;
    let side = gc_sim::clearing::MAX_CLEAR_SIDE;
    let (mut lo, mut hi) = ((first.0 - 1, first.1 - 1), (first.0 + 1, first.1 + 1));
    for &(x, y) in way.iter().take(CLEAR_RUN).skip(1) {
        let (nlo, nhi) = (
            (lo.0.min(x - 1), lo.1.min(y - 1)),
            (hi.0.max(x + 1), hi.1.max(y + 1)),
        );
        let (w, h) = (nhi.0 - nlo.0 + 1, nhi.1 - nlo.1 + 1);
        if w.max(h) > side || w.min(h) > 3 {
            break;
        }
        (lo, hi) = (nlo, nhi);
    }
    Some((
        (lo.0.max(0), lo.1.max(0)),
        (hi.0.min(n - 1), hi.1.min(n - 1)),
    ))
}

/// An order for the economy's hands, and the worker it sends (to leave
/// alone a while), if any.
type Order = (Command, Option<RegimentHandle>);

/// When a mind with `caution` starts on its wall without tier 2.
fn wall_after(caution: f32) -> u32 {
    let c = caution.clamp(0.0, 1.0);
    let span = (WALL_AFTER_BOLD - WALL_AFTER_CAUTIOUS) as f32;
    WALL_AFTER_BOLD - (span * c) as u32
}

/// Whether a line across `toward` runs upright (north to south): when
/// the way is more east-west than north-south.
fn upright(toward: Vec2) -> bool {
    toward.x.abs() >= toward.y.abs()
}

/// The tiles from and to of a straight line `half` tiles each side of
/// `mid`, upright or level.
fn across(mid: Vec2, upright: bool, half: i32) -> Line {
    let (x, y) = (mid.x.floor() as i32, mid.y.floor() as i32);
    if upright {
        ((x, y - half), (x, y + half))
    } else {
        ((x - half, y), (x + half, y))
    }
}

/// The tile `p` is on.
fn tile_of(p: Vec2) -> (i32, i32) {
    (p.x.floor() as i32, p.y.floor() as i32)
}

/// The ground round `home` as a ring would be laid on it (see
/// [`ringwork`]): what closes a town of itself, its own walls, what it
/// keeps inside (room round home, its buildings near home with room round
/// them, the seeps and veins it works near home and the lanes to them),
/// and where a tile of wall could go up. `None` when it can't lay walls at
/// all now.
fn ring_site(s: &Sense, look: &Look, home: Vec2) -> Option<Site> {
    let wall = s.kit.wall()?;
    let at = tile_of(home);
    let t0 = std::time::Instant::now();
    let mut site = Site::round(at);
    let n = site.ground.len();
    for i in 0..n {
        let (x, y) = site.tile(i);
        if s.world.town_ground_shut(x, y) {
            site.ground[i] = Ground::Shut;
        }
    }
    let foot = |m: &super::Mine| {
        let ((x0, y0), w, d) = (
            m.view.tile,
            i32::from(m.view.width),
            i32::from(m.view.depth),
        );
        (x0..x0 + w).flat_map(move |x| (y0..y0 + d).map(move |y| (x, y)))
    };
    let set = |site: &mut Site, t: (i32, i32), g: Ground| {
        if let Some(i) = site.index(t) {
            site.ground[i] = g;
        }
    };
    // Its walls and gates, standing or going up; its buildings far from
    // home close a ring as walls do, and are left outside it.
    let mut inside = Vec::new();
    for m in &look.mine {
        let near = (m.at() - home).abs().max_element() <= TOWN as f32;
        match m.stats.role {
            Role::Wall => foot(m).for_each(|t| set(&mut site, t, Ground::Walled)),
            Role::Bridge | Role::Stakes | Role::Barricade | Role::Trap => {}
            _ if near => inside.push(m),
            _ => foot(m).for_each(|t| set(&mut site, t, Ground::Walled)),
        }
    }
    for p in s.world.plans().iter().filter(|p| p.owner == s.party) {
        set(&mut site, p.tile, Ground::Walled);
    }
    // What it keeps inside.
    let keep = |site: &mut Site, t: (i32, i32), room: i32| {
        for dy in -room..=room {
            for dx in -room..=room {
                site.keep_tile((t.0 + dx, t.1 + dy));
            }
        }
    };
    for dy in -HOME_ROOM..=HOME_ROOM {
        for dx in -HOME_ROOM..=HOME_ROOM {
            if dx * dx + dy * dy <= HOME_ROOM * HOME_ROOM {
                site.keep_tile((at.0 + dx, at.1 + dy));
            }
        }
    }
    // A lane from home to `to`, so the ring goes round both as one town.
    let lane = |site: &mut Site, to: Vec2| {
        let steps = (to.distance(home) * 2.0).max(1.0);
        for k in 0..=steps as i32 {
            keep(site, tile_of(home + (to - home) * (k as f32 / steps)), 1);
        }
    };
    for m in inside {
        for t in foot(m) {
            keep(&mut site, t, ROOM);
        }
        lane(&mut site, m.at());
    }
    for node in look.served.iter().map(|n| n.2) {
        if node.distance(home) <= NODES_IN {
            keep(&mut site, tile_of(node), NODE_ROOM);
            lane(&mut site, node);
        }
    }
    site.keep_joined(at);
    let t1 = t0.elapsed();
    // Where wall could go up, on the ground that matters: what joins the
    // kept ground.
    let joined = site.joined();
    let t2 = t0.elapsed();
    if std::env::var("AI_RING_DEBUG").is_ok() {
        eprintln!("RING site: shut+keep {t1:?}, joined {:?} ({} tiles)", t2 - t1, joined.len());
    }
    for i in joined.into_iter().filter(|&i| !site.keep[i]) {
        site.ground[i] = match s.world.wall_ground(s.party, wall.id, site.tile(i)) {
            Ok(()) => Ground::Open,
            Err(Refusal::Overgrown) => Ground::Grown,
            Err(_) => Ground::Hard,
        };
    }
    if std::env::var("AI_RING_DEBUG").is_ok() {
        eprintln!("RING site: classify {:?}", t0.elapsed() - t2);
    }
    Some(site)
}

/// Lay out the walls round `home`: the ring (see [`ringwork`]) through
/// its gate, a gate of its own already standing on the ring or a new one
/// where its way out toward the nearest enemy crosses it. `None` when no
/// ring can be laid out now, or its faction builds gates and none fits.
fn lay_works(s: &Sense, look: &Look, home: Vec2, no_gate: &[(i32, i32)]) -> Option<Works> {
    let site = ring_site(s, look, home);
    if site.is_none() && std::env::var("AI_RING_DEBUG").is_ok() {
        eprintln!("RING p{} no site", s.party);
    }
    let mut site = site?;
    let toward = super::super::nearest_other_start(s, home).map_or(tile_of(home), tile_of);
    let way = ringwork::way_out(&site, toward);
    let gate = s.kit.gate();
    // A gate of its own the ring runs through already.
    let first = ringwork::lay(&site, way.map(|d| d.mid));
    if first.is_none() && std::env::var("AI_RING_DEBUG").is_ok() {
        eprintln!("RING p{} no first layout, way {:?}", s.party, way);
    }
    let first = first?;
    let standing = look
        .mine
        .iter()
        .filter(|m| m.stats.flags & GATE != 0)
        .find_map(|m| {
            let v = &m.view;
            let (mid, along) = if v.depth > v.width {
                ((v.tile.0, v.tile.1 + 1), (1, 0))
            } else {
                ((v.tile.0 + 1, v.tile.1), (0, 1))
            };
            let side = |k: i32| first.town(&site, (mid.0 + along.0 * k, mid.1 + along.1 * k));
            let out = match (side(-1), side(1)) {
                (true, false) => along,
                (false, true) => (-along.0, -along.1),
                _ => return None,
            };
            Some(Door { mid, out })
        });
    // Where a gate can be laid: not where one was given up, and where
    // nobody stands if there is such a place.
    let lays = |d: Door, crowded: bool| {
        let (tile, turn) = gate_at(d);
        let near = |t: &(i32, i32)| (t.0 - d.mid.0).abs().max((t.1 - d.mid.1).abs()) <= 2;
        let fits = !no_gate.iter().any(near)
            && ringwork::door_fits(&site, d)
            && match s.world.can_lay(s.party, gate?.id, tile, turn) {
                Ok(()) => true,
                Err(Refusal::Crowded) => crowded,
                Err(_) => false,
            };
        fits.then_some(d)
    };
    if std::env::var("AI_RING_DEBUG").is_ok() {
        eprintln!(
            "RING p{} first: {} pieces closes {} way {:?} standing {:?} kept {} hard {}",
            s.party,
            first.pieces.len(),
            first.closes,
            way,
            standing,
            site.keep.iter().filter(|&&k| k).count(),
            site.ground.iter().filter(|&&g| g == Ground::Hard).count(),
        );
    }
    let new = match (standing, gate, way) {
        (None, Some(_), Some(way)) => Some(
            (ringwork::door_places(way).find_map(|d| lays(d, false)))
                .or_else(|| ringwork::door_places(way).find_map(|d| lays(d, true)))?,
        ),
        // A faction with gates rings its home only through a gate: with
        // no way out to put one on, the ground closes the town already.
        _ => None,
    };
    let door = standing.or(new);
    let layout = match door {
        Some(d) => {
            site.set_door(d);
            ringwork::lay(&site, Some(d.mid))?
        }
        None => first,
    };
    let front = door.map(|d| d.mid).or(way.map(|d| d.mid));
    Some(Works {
        gate: new.map(gate_at),
        crowded: None,
        door,
        raise: match door {
            Some(d) => d.beside().to_vec(),
            None => layout
                .pieces
                .first()
                .map(|p| p.from)
                .or(front)
                .into_iter()
                .collect(),
        },
        closes: layout.closes,
        clear: layout.clear,
        pieces: layout.pieces,
        planned: s.now,
        tier: look.tier,
    })
}

/// Where a gate (1 by 3) is laid for `door`, and its turn: as it stands
/// across a way east or west, turned a quarter across one north or south.
fn gate_at(door: Door) -> ((i32, i32), u8) {
    if door.out.0 != 0 {
        ((door.mid.0, door.mid.1 - 1), 0)
    } else {
        ((door.mid.0 - 1, door.mid.1), 1)
    }
}

/// Its own finished wall tile (not a gate) at `tile`, if one stands
/// there.
fn own_wall_at(look: &Look, tile: (i32, i32)) -> Option<StructureHandle> {
    look.mine
        .iter()
        .find(|m| {
            m.view.tile == tile
                && m.finished()
                && m.stats.role == Role::Wall
                && m.stats.flags & GATE == 0
        })
        .map(|m| m.view.handle)
}

/// The shortest bridge (from bank to bank) that fits over the water on
/// the way from `home` to `target`: crossings straight along that way and
/// along each axis, from points either side of it; with its tiles.
fn narrowest_crossing(
    s: &Sense,
    bridge: BuildingId,
    home: Vec2,
    target: Vec2,
) -> Option<(Line, u32)> {
    let way = (target - home).try_normalize()?;
    let side = Vec2::new(-way.y, way.x);
    let len = home.distance(target);
    let mut best: Option<(i32, Line, u32)> = None;
    for k in -6..=6 {
        let start = home + side * (k as f32 * 3.0);
        // Where this walk first meets water it can't wade.
        let Some(wet) = (0..len as i32)
            .map(|d| start + way * d as f32)
            .find(|&p| s.terrain.water(p) && s.terrain.blocked(p))
        else {
            continue;
        };
        let axes = [
            way,
            Vec2::new(way.x.signum(), 0.0),
            Vec2::new(0.0, way.y.signum()),
        ];
        for dir in axes.into_iter().filter(|d| d.length() > 0.5) {
            let dir = dir.normalize();
            let Some((from, to)) = banks(s, wet, dir) else {
                continue;
            };
            let n = (to.0 - from.0).abs().max((to.1 - from.1).abs());
            if n > BRIDGE_LONGEST || best.is_some_and(|(b, ..)| b <= n) {
                continue;
            }
            let fits = s.world.line_fits(s.party, bridge, &bridge_line(from, to));
            if let Ok(t) = fits
                && !t.is_empty()
                && t.iter().all(|t| t.fits.is_ok())
            {
                best = Some((n, (from, to), t.len() as u32));
            }
        }
    }
    best.map(|(_, line, tiles)| (line, tiles))
}

/// A standard-width bridge drawn straight from tile `from` to tile `to`
/// (at a slant, if that is the shortest way over).
fn bridge_line(from: (i32, i32), to: (i32, i32)) -> gc_sim::walls::Line {
    gc_sim::walls::Line::drawn(
        gc_sim::drawn::DrawnPath::straight(from, to),
        gc_sim::drawn::STANDARD,
    )
}

/// The last land tile before the water at `wet` going back along `dir`,
/// and the first land tile after it going on: a bridge's two ends.
fn banks(s: &Sense, wet: Vec2, dir: Vec2) -> Option<Line> {
    let tile = |p: Vec2| (p.x.floor() as i32, p.y.floor() as i32);
    let land = |p: Vec2| !s.terrain.water(p) && !s.terrain.blocked(p);
    let back = (1..=BRIDGE_LONGEST)
        .map(|d| wet - dir * d as f32)
        .find(|&p| land(p))?;
    let on = (1..=BRIDGE_LONGEST)
        .map(|d| wet + dir * d as f32)
        .find(|&p| land(p))?;
    Some((tile(back), tile(on)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_across_the_way_runs_on_the_axis_more_across_it() {
        assert!(upright(Vec2::new(1.0, 0.2)));
        assert!(!upright(Vec2::new(0.3, -1.0)));
        assert_eq!(across(Vec2::new(10.5, 20.5), true, 2), ((10, 18), (10, 22)));
        assert_eq!(across(Vec2::new(10.5, 20.5), false, 2), ((8, 20), (12, 20)));
    }

    #[test]
    fn a_path_is_cut_round_the_first_tiles_of_thicket_three_wide() {
        assert_eq!(clear_run(&[], 64), None);
        assert_eq!(clear_run(&[(5, 5)], 64), Some(((4, 4), (6, 6))));
        // A run going east: the first six, a tile round them.
        let way: Vec<_> = (10..20).map(|x| (x, 7)).collect();
        assert_eq!(clear_run(&way, 64), Some(((9, 6), (16, 8))));
        // A diagonal run: no square cut, just its first tile.
        let way: Vec<_> = (10..20).map(|x| (x, x)).collect();
        assert_eq!(clear_run(&way, 64), Some(((9, 9), (11, 11))));
        // Never off the map, at either corner.
        assert_eq!(clear_run(&[(0, 0)], 64), Some(((0, 0), (1, 1))));
        assert_eq!(clear_run(&[(63, 63)], 64), Some(((62, 62), (63, 63))));
    }

    #[test]
    fn the_cautious_wall_sooner() {
        assert_eq!(wall_after(0.0), WALL_AFTER_BOLD);
        assert_eq!(wall_after(1.0), WALL_AFTER_CAUTIOUS);
        assert!(wall_after(0.5) < WALL_AFTER_BOLD && wall_after(0.5) > WALL_AFTER_CAUTIOUS);
    }
}
