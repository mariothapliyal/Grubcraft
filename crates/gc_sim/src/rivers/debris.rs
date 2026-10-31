//! Debris on the rivers (docs/39-water-engine.md, section 3; stage 3a):
//! a small, fixed pool of logs riding the current.
//!
//! **Where it comes from.** A Major flood takes a log off a wooded bank
//! of a stretch every [`MAJOR_EVERY_SECONDS`], a catastrophic one every
//! [`CATASTROPHIC_EVERY_SECONDS`], but no more than [`NATURAL_MOST`] of
//! these float at once on the whole map. A wood burning out on a bank
//! topples a log in, one time in [`TOPPLE_IN`].
//!
//! **How it moves.** A floating piece goes from tile to tile down the
//! river, the way the water runs, at the river's pace (slow half a tile a
//! second, steady one, fast two), up to twice that in a flood. It leaves
//! the map where the river does, or where it runs into still water.
//!
//! **Where it stops.** In low water (the river under its line) a piece
//! grounds wherever it is, and on a ford that is not fast it grounds
//! unless the water stands [`FLOAT_RISE`] over the line. It floats off
//! again when the water comes up: so logs pile up at the fords in quiet
//! weather and come down together in the next flood. Grounded pieces of
//! [`SNAG_MASS`] or more on one ford are a **snag**: the ford is slow to
//! wade, as deep water is, until the water lifts them off.
//!
//! **What it hits.** A floating piece passing through a regiment wading
//! (a ford, a summer ford or flooded floodplain) hurts it by the piece's
//! mass and pace, throws it into disorder and shakes it, once per piece.
//! Troops on a bridge or a wall are above it; troops being swept away
//! already are not hit again. The computer players keep off a ford with
//! a piece coming down on it within [`WARN_TILES`].
//!
//! The pool has [`POOL`] places, [`KEPT_FOR_WRECKS`] of them kept for
//! the wrecks of broken bridges (stage 4). When the rest are full, the
//! oldest grounded piece rots away first, then the oldest floating one.

use alloc::vec::Vec;

pub mod piers;

use gc_fixed::{Fx, FxVec2};

use super::{FLOOD_RISE, NONE, Rivers, Stretch, step_of, water_rise};
use crate::hash::StateHasher;
use crate::regiments::{RegimentHandle, Regiments};
use crate::terrain::{Cover, Flow, Growth, Terrain, index_in};
use crate::{TICKS_PER_SECOND, UnitStats, morale};

/// Places in the pool.
pub const POOL: usize = 48;
/// Places kept for wrecks and rubble.
pub const KEPT_FOR_WRECKS: usize = 8;
/// Most pieces the floods and fires put in that float at once.
pub const NATURAL_MOST: usize = 8;
/// How often a Major flood takes a log off a stretch's bank, and a
/// catastrophic one.
pub const MAJOR_EVERY_SECONDS: u32 = 30;
pub const CATASTROPHIC_EVERY_SECONDS: u32 = 10;
/// One wood burning out on a bank in this many topples a log in.
pub const TOPPLE_IN: u64 = 3;
/// How far over its line the water must stand for a piece on a ford to
/// float, height units.
pub const FLOAT_RISE: u16 = FLOOD_RISE / 2;
/// The grounded mass on one ford that makes it a snag.
pub const SNAG_MASS: u8 = 4;
/// How far down the river (tiles of reach) the computer players watch
/// for a piece coming down on a ford.
pub const WARN_TILES: u16 = 24;
/// Hit points a piece takes off a regiment for each mass and tile a
/// second of its pace (a log at a fast river's pace: 6).
pub const HIT_PER_MASS_PACE: u32 = 3;
/// The morale a regiment struck loses.
pub const HIT_MORALE: i16 = 3;
/// Regiments one piece remembers having struck.
const STRUCK_KEPT: usize = 6;
/// Places for natural pieces: all but those kept for wrecks.
const OPEN_PLACES: usize = POOL - KEPT_FOR_WRECKS;
/// Wooded bank places kept a stretch, one a quarter of its reach.
pub(super) const BANKS: usize = 4;
/// No tile.
const NO_TILE: u32 = u32::MAX;
/// The owner of a piece no player put in.
const NOBODY: u8 = u8::MAX;
/// Grub a raft costs (the timber; docs/39, "Dumping").
pub const RAFT_GRUB: u32 = 12;
/// Seconds a worker takes to tip a raft in, at Standard tempo.
pub const DUMP_SECONDS: u32 = 6;
/// Most pieces one player may have put in that are still on the water.
pub const SHARE: usize = 16;
/// No second.
const NEVER: u32 = u32::MAX;

/// What a piece is (docs/39, "What a piece is").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A felled or burnt tree.
    Log,
    /// A load dumped on purpose (stage 3b).
    Raft,
    /// A fallen bridge tile (stage 4).
    Wreck,
    /// A broken dam or building (stage 6).
    Rubble,
}

impl Kind {
    /// How heavy it is, 1 to 8.
    pub const fn mass(self) -> u8 {
        match self {
            Kind::Log => 1,
            Kind::Raft => 3,
            Kind::Wreck => 2,
            Kind::Rubble => 4,
        }
    }

    const fn byte(self) -> u8 {
        self as u8
    }
}

/// A piece of debris on the water.
#[derive(Clone, Copy, Debug)]
struct Piece {
    kind: Kind,
    /// The tile it is on, and the tile it is going to ([`NO_TILE`]: off
    /// the river), as indices.
    tile: u32,
    next: u32,
    /// How far it is from `tile` to `next`, 0 to 1.
    along: Fx,
    /// Its place across the channel, in 256ths of a tile from the middle
    /// (for drawing and hitting).
    across: i8,
    /// How far it went the last tick (zero while grounded).
    pace: Fx,
    grounded: bool,
    /// Whether it is jammed against a bridge's pier on its tile.
    jammed: bool,
    /// Whether, jammed, the water has risen to a Minor flood or more at
    /// its pier since it jammed (it may wash free as the water falls).
    lifted: bool,
    /// Whether it came onto a new tile last tick (it grounds or not
    /// there at once).
    entered: bool,
    /// Whether a flood or a fire put it in.
    natural: bool,
    /// The player who dumped or felled it in ([`NOBODY`]: none).
    owner: u8,
    /// The tick it went in.
    born: u32,
    /// The regiments it has struck, the oldest pushed out.
    struck: [Option<RegimentHandle>; STRUCK_KEPT],
    struck_next: u8,
}

/// A piece as seen (see [`Debris::pieces`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seen {
    pub kind: Kind,
    /// Where it is, in tiles.
    pub at: FxVec2,
    /// The way it is going: towards the tile it goes to next.
    pub way: FxVec2,
    pub grounded: bool,
}

/// The pool of debris on a map's rivers.
#[derive(Clone, Debug, Default)]
pub struct Debris {
    /// [`POOL`] places, on maps with rivers (else none).
    pieces: Vec<Option<Piece>>,
    /// Per stretch, [`BANKS`] river tiles beside a wood (or [`NO_TILE`]),
    /// with the wooded tile.
    pub(super) banks: alloc::sync::Arc<[[(u32, u32); BANKS]]>,
    /// Per ford group: grounded mass on it, and whether a floating piece
    /// is coming down on it.
    snag: Vec<u8>,
    pub(super) fouled: Vec<bool>,
    /// Per stretch, the second a flood last tore a log off its banks
    /// ([`NEVER`] while the water is under a Major flood's).
    torn: Vec<u32>,
    /// Whether any piece has ever gone in (it is hashed from then on).
    ever: bool,
    /// Kept room for the pieces moving in a tick (not state).
    moving: Vec<(usize, FxVec2)>,
    /// The piers in the water, by tile (refreshed from the bridges each
    /// second).
    piers: Vec<piers::Pier>,
    /// Per stretch, how much of its outflow its jams hold back, percent.
    choke: Vec<u8>,
    /// What the debris did to bridges this tick, for the world to apply.
    blows: Vec<Blow>,
    /// The players (a bit each) whose bridges debris jammed on this tick,
    /// for the news (not hashed: told and forgotten in the tick).
    jammed_on: u8,
}

/// A blow debris or the water strikes a bridge tile: the tile, and the
/// ten-thousandths of its whole hit points it loses (all of them when
/// swept away).
pub(crate) type Blow = (crate::structures::StructureHandle, u32);

impl Debris {
    /// A pool for a map with `stretches` and `groups` of fords, with
    /// `banks` per stretch.
    pub(super) fn new(banks: Vec<[(u32, u32); BANKS]>, groups: usize) -> Debris {
        let stretches = banks.len();
        Debris {
            pieces: Vec::new(),
            banks: banks.into(),
            snag: alloc::vec![0; groups],
            fouled: alloc::vec![false; groups],
            torn: alloc::vec![NEVER; stretches],
            ever: false,
            moving: Vec::new(),
            piers: Vec::new(),
            choke: alloc::vec![0; stretches],

            blows: Vec::new(),
            jammed_on: 0,
        }
    }

    /// How many pieces there are.
    pub fn len(&self) -> usize {
        self.pieces.iter().flatten().count()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.pieces.iter().all(Option::is_none)
    }

    /// Whether any piece floats.
    pub fn floating(&self) -> bool {
        self.pieces
            .iter()
            .flatten()
            .any(|p| !p.grounded && !p.jammed)
    }

    /// How many pieces `player` dumped or felled in are on the water (not
    /// held in a jam: a jam is bounded by its pier, and stuck pieces don't
    /// use up a player's share).
    pub fn owned(&self, player: u8) -> usize {
        (self.pieces.iter().flatten())
            .filter(|p| p.owner == player && !p.jammed)
            .count()
    }

    /// Whether ford group `g` is a snag.
    pub(super) fn snagged(&self, g: usize) -> bool {
        self.snag.get(g).is_some_and(|&m| m >= SNAG_MASS)
    }

    /// The pieces, as seen, on a map `side` tiles across.
    pub fn pieces(&self, side: i32) -> impl Iterator<Item = Seen> + '_ {
        self.pieces.iter().flatten().map(move |p| Seen {
            kind: p.kind,
            at: p.point(side),
            way: p.way(side),
            grounded: p.grounded || p.jammed,
        })
    }

    /// Put `piece` in, if there is room: when the open places are full the
    /// oldest grounded piece rots away to make room, else the oldest
    /// floating one. Returns whether it went in.
    fn put(&mut self, piece: Piece) -> bool {
        // The pool is made only when the first piece goes in.
        if self.pieces.is_empty() {
            self.pieces.resize(POOL, None);
        }
        let open = |p: &Piece| matches!(p.kind, Kind::Log | Kind::Raft);
        let taken = self.pieces.iter().flatten().filter(|p| open(p)).count();
        if piece.natural {
            let natural = (self.pieces.iter().flatten())
                .filter(|p| p.natural && !p.grounded && !p.jammed)
                .count();
            if natural >= NATURAL_MOST {
                return false;
            }
        }
        let slot = if taken < OPEN_PLACES || !open(&piece) {
            self.pieces.iter().position(Option::is_none)
        } else {
            // The oldest grounded, else the oldest of all; a jam holds its
            // pieces until it is cleared or lets go.
            (self.pieces.iter().enumerate())
                .filter_map(|(i, p)| p.filter(|p| open(p) && !p.jammed).map(|p| (i, p)))
                .min_by_key(|(i, p)| (!p.grounded, p.born, *i))
                .map(|(i, _)| i)
        };
        let Some(slot) = slot else {
            return false;
        };
        self.pieces[slot] = Some(piece);
        self.ever = true;
        true
    }

    /// Hashed only once a piece has gone in.
    pub(super) fn hash_into(&self, h: &mut StateHasher) {
        if !self.ever {
            return;
        }
        for p in &self.pieces {
            let Some(p) = p else {
                h.u8(0xff);
                continue;
            };
            h.u8(p.kind.byte());
            h.u32(p.tile);
            h.u32(p.next);
            h.u32(p.along.raw() as u32);
            h.u8(p.across as u8);
            h.u8(u8::from(p.grounded)
                | u8::from(p.natural) << 1
                | u8::from(p.entered) << 2
                | u8::from(p.jammed) << 3
                | u8::from(p.lifted) << 4);
            h.u32(p.born);
            h.u8(p.owner);
            h.u8(p.struck_next);
            for r in p.struck {
                let (i, g) = r.map_or((u32::MAX, 0), |r| (r.index, r.generation));
                h.u32(i);
                h.u16(g);
            }
        }
        for &t in &self.torn {
            h.u32(t);
        }
        for &c in &self.choke {
            h.u8(c);
        }
    }
}

impl Piece {
    /// A piece of `kind` going in at river tile `tile` on tick `tick`,
    /// `across` the channel.
    fn new(kind: Kind, tile: u32, across: i8, natural: bool, tick: u32) -> Piece {
        Piece {
            kind,
            tile,
            next: tile,
            along: Fx::ZERO,
            across,
            pace: Fx::ZERO,
            grounded: false,
            jammed: false,
            lifted: false,
            entered: false,
            natural,
            owner: NOBODY,
            born: tick,
            struck: [None; STRUCK_KEPT],
            struck_next: 0,
        }
    }

    /// The middle of tile `i` on a map `side` across.
    fn middle(i: u32, side: i32) -> FxVec2 {
        let i = i as i32;
        FxVec2::from_ints(i % side, i / side) + FxVec2::new(Fx::HALF, Fx::HALF)
    }

    /// The way from its tile to the next (zero when they are one).
    fn way(&self, side: i32) -> FxVec2 {
        if self.next == NO_TILE || self.next == self.tile {
            return FxVec2::ZERO;
        }
        (Piece::middle(self.next, side) - Piece::middle(self.tile, side)).normalized()
    }

    /// Where it is, in tiles.
    fn point(&self, side: i32) -> FxVec2 {
        let from = Piece::middle(self.tile, side);
        let to = if self.next == NO_TILE {
            from
        } else {
            Piece::middle(self.next, side)
        };
        let way = self.way(side);
        let across = FxVec2::new(-way.y, way.x) * Fx::from_ratio(i32::from(self.across), 256);
        from + (to - from) * self.along + across
    }

    /// Whether it has struck the regiment `h`; if not, it remembers it has.
    fn strikes(&mut self, h: RegimentHandle) -> bool {
        if self.struck.contains(&Some(h)) {
            return false;
        }
        self.struck[usize::from(self.struck_next)] = Some(h);
        self.struck_next = (self.struck_next + 1) % STRUCK_KEPT as u8;
        true
    }
}

/// Whether tile `(x, y)` of `terrain` is wooded.
fn wooded(terrain: &Terrain, x: i32, y: i32) -> bool {
    matches!(terrain.growth(x, y), Growth::Woods | Growth::Thicket)
        || terrain.cover(FxVec2::from_ints(x, y) + FxVec2::new(Fx::HALF, Fx::HALF)) == Cover::Wood
}

/// The river tile next down the river's reach from tile `i` on a map
/// `n` across (the way the floods run): where the water's `flow` points,
/// if that is nearer the river's end, else the neighbour nearest it;
/// [`NO_TILE`] at the river's end (off the map, or into still water).
fn downstream(flow: &[u8], reach: &[u16], n: i32, i: u32) -> u32 {
    let here = reach[i as usize];
    if here == 0 || here == NONE {
        return NO_TILE;
    }
    let (x, y) = (i as i32 % n, i as i32 / n);
    let down = |j: usize| flow[j] != 0 && reach[j] < here;
    if let Some(j) = (flow.get(i as usize).and_then(|&f| step_of(f)))
        .and_then(|(dx, dy)| index_in(n, x + dx, y + dy))
        .filter(|&j| down(j))
    {
        return j as u32;
    }
    (crate::nav::DIRS.iter())
        .filter_map(|&(dx, dy)| index_in(n, x + dx, y + dy))
        .filter(|&j| down(j))
        .min_by_key(|&j| (reach[j], j))
        .map_or(NO_TILE, |j| j as u32)
}

/// A roll from the match seed, a tick and a number.
fn roll(seed: u64, tick: u32, what: u64) -> u64 {
    crate::fire::splitmix(
        seed ^ u64::from(tick).rotate_left(23) ^ what.rotate_left(41) ^ 0xd0b1_15d0_0000_0000,
    )
}

/// The wooded banks of each stretch: for each quarter of its reach, the
/// first river tile (by index) beside a wooded tile of land, and that
/// tile. For [`Debris::new`].
pub(super) fn banks_of(
    terrain: &Terrain,
    n: i32,
    reach: &[u16],
    stretch_of: &[u16],
    stretches: usize,
) -> Vec<[(u32, u32); BANKS]> {
    let flow = terrain.flow_layer();
    let mut banks = alloc::vec![[(NO_TILE, NO_TILE); BANKS]; stretches];
    if !terrain.has_cover() {
        return banks;
    }
    for i in 0..(n * n) as usize {
        let s = stretch_of[i];
        if s == NONE || flow[i] == 0 {
            continue;
        }
        let slot = usize::from(reach[i] % super::STRETCH_TILES) * BANKS
            / usize::from(super::STRETCH_TILES);
        let place = &mut banks[usize::from(s)][slot];
        if place.0 != NO_TILE {
            continue;
        }
        let (x, y) = (i as i32 % n, i as i32 / n);
        let wood = (crate::nav::DIRS.iter())
            .map(|&(dx, dy)| (x + dx, y + dy))
            .find(|&(wx, wy)| {
                index_in(n, wx, wy).is_some_and(|j| flow[j] == 0)
                    && !terrain.tile_under_water(wx, wy)
                    && wooded(terrain, wx, wy)
            });
        if let Some((wx, wy)) = wood {
            *place = (i as u32, (wy * n + wx) as u32);
        }
    }
    banks
}

impl Rivers {
    /// The debris on the rivers.
    pub fn debris(&self) -> &Debris {
        &self.debris
    }

    /// The pieces of debris, as seen.
    pub fn debris_seen(&self) -> impl Iterator<Item = Seen> + '_ {
        self.debris.pieces(self.side)
    }

    /// How far the water stands over the line on tile `i`, with the map's
    /// water `lift`.
    fn water_on(&self, lift: i32, i: usize) -> i32 {
        let s = self.stretch_of[i];
        if s == NONE {
            return lift;
        }
        lift + i32::from(water_rise(
            &self.stretches,
            &self.swell,
            self.fronts(),
            s,
            self.reach[i],
        ))
    }

    /// A wood at `(x, y)` burnt out on tick `tick`: if it stood on a
    /// river's bank, one time in [`TOPPLE_IN`] a log topples in.
    pub(crate) fn topple(&mut self, terrain: &Terrain, x: i32, y: i32, tick: u32) {
        if !self.any() || !wooded(terrain, x, y) {
            return;
        }
        let Some(river) = self.river_beside(terrain, x, y) else {
            return;
        };
        let r = roll(self.seed, tick, (y * self.side + x) as u64);
        if !r.is_multiple_of(TOPPLE_IN) {
            return;
        }
        let piece = Piece::new(Kind::Log, river as u32, across(r), true, tick);
        self.debris.put(piece);
    }

    /// A running river tile beside land tile `(x, y)`, if there is one.
    fn river_beside(&self, terrain: &Terrain, x: i32, y: i32) -> Option<usize> {
        let n = self.side;
        (crate::nav::DIRS.iter())
            .filter_map(|&(dx, dy)| index_in(n, x + dx, y + dy))
            .find(|&j| self.runs(terrain, j))
    }

    /// Whether river tile `i` runs (has a current and a stretch).
    fn runs(&self, terrain: &Terrain, i: usize) -> bool {
        terrain.flow_layer().get(i).is_some_and(|&f| f != 0)
            && self.stretch_of.get(i).is_some_and(|&s| s != NONE)
    }

    /// Whether a worker may fell land tile `(x, y)` into a river beside it.
    pub fn fells_into(&self, terrain: &Terrain, x: i32, y: i32) -> bool {
        self.any() && self.river_beside(terrain, x, y).is_some()
    }

    /// Whether `player` may tip a raft into river tile `(x, y)`: it runs,
    /// lies along a bank (where a worker on the land can reach it), and the
    /// player has fewer than [`SHARE`] pieces on the water.
    pub fn takes_raft(&self, terrain: &Terrain, x: i32, y: i32, player: u8) -> bool {
        self.bank_river(terrain, x, y) && !self.share_used(player)
    }

    /// Whether `(x, y)` is running river along a bank: some tile beside it
    /// is not running river.
    pub fn bank_river(&self, terrain: &Terrain, x: i32, y: i32) -> bool {
        let n = self.side;
        self.any()
            && index_in(n, x, y).is_some_and(|i| self.runs(terrain, i))
            && (crate::nav::DIRS.iter())
                .filter_map(|&(dx, dy)| index_in(n, x + dx, y + dy))
                .any(|j| !self.runs(terrain, j))
    }

    /// Whether `player` already has [`SHARE`] pieces on the water.
    pub fn share_used(&self, player: u8) -> bool {
        self.debris.owned(player) >= SHARE
    }

    /// `logs` logs felled off land tile `(x, y)` by `player` go into the
    /// river beside it, as far as the pool and the player's share allow.
    pub(crate) fn fell_in(
        &mut self,
        terrain: &Terrain,
        x: i32,
        y: i32,
        logs: u8,
        player: u8,
        tick: u32,
    ) {
        let Some(river) = self.river_beside(terrain, x, y) else {
            return;
        };
        for k in 0..logs {
            if self.share_used(player) {
                return;
            }
            let r = roll(
                self.seed,
                tick,
                ((y * self.side + x) as u64) << 2 | u64::from(k),
            );
            let mut piece = Piece::new(Kind::Log, river as u32, across(r), false, tick);
            piece.owner = player;
            self.debris.put(piece);
        }
    }

    /// `player` tips a raft into river tile `(x, y)`. Returns whether it
    /// went in.
    pub(crate) fn dump(
        &mut self,
        terrain: &Terrain,
        x: i32,
        y: i32,
        player: u8,
        tick: u32,
    ) -> bool {
        if !self.takes_raft(terrain, x, y, player) {
            return false;
        }
        let i = (y * self.side + x) as u32;
        let r = roll(self.seed, tick, u64::from(i) << 2 | 3);
        let mut piece = Piece::new(Kind::Raft, i, across(r), false, tick);
        piece.owner = player;
        self.debris.put(piece)
    }

    /// One tick of the debris, with the map's water `lift`: floating
    /// pieces ride the current on; and once a `second`, the floods take
    /// logs off the banks, pieces ground and float off, and the snags and
    /// the fords pieces are coming down on are weighed again.
    /// Returns whether the fords that are snags changed.
    pub(crate) fn drift(&mut self, terrain: &Terrain, lift: i32, tick: u32, second: bool) -> bool {
        if !self.any() {
            return false;
        }
        if second {
            self.flood_banks(terrain, lift, tick);
        }
        if self.debris.is_empty() {
            if second {
                self.weigh_piers(lift);
            }
            // The last piece may have just gone: weigh the fords clear.
            return second && self.debris.ever && self.weigh_debris();
        }
        let (flow, n) = (terrain.flow_layer(), self.side);
        for k in 0..self.debris.pieces.len() {
            let Some(mut p) = self.debris.pieces[k] else {
                continue;
            };
            if p.jammed {
                continue;
            }
            if second || p.entered {
                p.grounded = self.grounds(terrain, lift, p.tile);
            }
            p.entered = false;
            p.pace = Fx::ZERO;
            if !p.grounded {
                if p.next == p.tile {
                    p.next = downstream(flow, &self.reach, n, p.tile);
                }
                p.pace = self.pace(flow, lift, p.tile);
                p.along += p.pace;
                while p.along >= Fx::ONE && p.next != NO_TILE {
                    p.along -= Fx::ONE;
                    let from = p.tile;
                    p.tile = p.next;
                    p.next = downstream(flow, &self.reach, n, p.tile);
                    p.entered = true;
                    if self.meet_pier(&mut p, from, k, lift, tick) {
                        break;
                    }
                }
                if p.next == NO_TILE && p.along >= Fx::HALF {
                    // Off the map, or into still water.
                    self.debris.pieces[k] = None;
                    continue;
                }
            }
            self.debris.pieces[k] = Some(p);
        }
        if second {
            self.weigh_piers(lift);
        }
        second && self.weigh_debris()
    }

    /// How far a floating piece on tile `i` goes in a tick: the river's
    /// pace there, up to twice it as the water rises to a full flood.
    fn pace(&self, flow: &[u8], lift: i32, i: u32) -> Fx {
        let speed = speed(flow, i).unwrap_or(1);
        // Half a tile, one, or two a second.
        let tiles = Fx::from_ratio(1 << speed.saturating_sub(1), 2);
        let up = self
            .water_on(lift, i as usize)
            .clamp(0, i32::from(FLOOD_RISE));
        let flood = Fx::ONE + Fx::from_ratio(up, i32::from(FLOOD_RISE));
        tiles * flood / TICKS_PER_SECOND as i32
    }

    /// Whether a piece on tile `i` grounds, with the map's water `lift`:
    /// anywhere on frozen water or under the line, and on a ford that is
    /// not fast under [`FLOAT_RISE`] of water.
    fn grounds(&self, terrain: &Terrain, lift: i32, i: u32) -> bool {
        if terrain.frozen() {
            return true;
        }
        let water = self.water_on(lift, i as usize);
        if water < 0 {
            return true;
        }
        let n = self.side;
        let (x, y) = (i as i32 % n, i as i32 / n);
        let fast = speed(terrain.flow_layer(), i).is_some_and(|s| s >= 3);
        self.group_at(x, y).is_some() && !fast && water < i32::from(FLOAT_RISE)
    }

    /// One second of the floods tearing logs off the banks: each stretch
    /// the water stands Major or more over (at its middle) takes one off
    /// one of its wooded banks every [`MAJOR_EVERY_SECONDS`] (catastrophic:
    /// [`CATASTROPHIC_EVERY_SECONDS`]).
    fn flood_banks(&mut self, terrain: &Terrain, lift: i32, tick: u32) {
        if self.fronts().all(|f| f.band < super::Band::Major) {
            self.debris.torn.fill(NEVER);
            return;
        }
        let second = tick / TICKS_PER_SECOND;
        let (n, stretches) = (self.side, self.stretches.len());
        // Starting at a stretch rolled each second, so none is always
        // first to the floods' share.
        let start = (roll(self.seed, tick, u64::MAX) % stretches.max(1) as u64) as usize;
        for j in 0..stretches {
            let s = (start + j) % stretches;
            let Stretch { at: (x, y), .. } = self.stretches[s];
            let Some(i) = index_in(n, x, y) else {
                continue;
            };
            let water = self.water_on(lift, i).clamp(0, i32::from(u16::MAX)) as u16;
            let every = match super::Band::of(water) {
                super::Band::Catastrophic => CATASTROPHIC_EVERY_SECONDS,
                super::Band::Major => MAJOR_EVERY_SECONDS,
                _ => {
                    self.debris.torn[s] = NEVER;
                    continue;
                }
            };
            // The first log as the water comes up, then one each `every`.
            let last = self.debris.torn[s];
            if last != NEVER && second < last + every {
                continue;
            }
            let r = roll(self.seed, tick, s as u64);
            let banks = self.debris.banks[s];
            let k = (r % BANKS as u64) as usize;
            let Some(&(river, _)) =
                (0..BANKS)
                    .map(|j| &banks[(k + j) % BANKS])
                    .find(|&&(river, wood)| {
                        river != NO_TILE && wooded(terrain, wood as i32 % n, wood as i32 / n)
                    })
            else {
                continue;
            };
            if !self
                .debris
                .put(Piece::new(Kind::Log, river, across(r), true, tick))
            {
                // The floods have put in all they may.
                break;
            }
            self.debris.torn[s] = second;
        }
    }

    /// Weigh again which fords are snags and which a floating piece is
    /// coming down on; the fords the minds keep off follow.
    /// Returns whether the fords that are snags changed.
    fn weigh_debris(&mut self) -> bool {
        let n = self.side;
        // Each piece's ford group if grounded on one, else where it floats.
        let (mut grounded, mut on_ford) = ([(0usize, 0u8); POOL], 0);
        let (mut floating, mut afloat) = ([(0u16, 0u16); POOL], 0);
        for p in self.debris.pieces.iter().flatten() {
            let t = p.tile as usize;
            if p.grounded {
                let (x, y) = (p.tile as i32 % n, p.tile as i32 / n);
                if let Some(g) = self.group_at(x, y) {
                    grounded[on_ford] = (g, p.kind.mass());
                    on_ford += 1;
                }
            } else if !p.jammed && self.stretch_of[t] != NONE {
                floating[afloat] = (self.stretch_of[t], self.reach[t]);
                afloat += 1;
            }
        }
        let (mut snags, mut fouled_changed) = (false, false);
        for g in 0..self.groups.len() {
            let group = &self.groups[g];
            let here = &self.stretches[usize::from(group.stretch)];
            let mass = (grounded[..on_ford].iter())
                .filter(|&&(at, _)| at == g)
                .fold(0u8, |m, &(_, kg)| m.saturating_add(kg));
            let fouled = floating[..afloat].iter().any(|&(s, reach)| {
                here.holds(&self.stretches[usize::from(s)])
                    && reach >= group.last
                    && reach <= group.first.saturating_add(WARN_TILES)
            });
            let was = self.debris.snagged(g);
            self.debris.snag[g] = mass;
            snags |= was != self.debris.snagged(g);
            if fouled != self.debris.fouled[g] {
                self.debris.fouled[g] = fouled;
                fouled_changed = true;
            }
        }
        if fouled_changed {
            self.weigh_threats();
        }
        snags
    }

    /// Bring the terrain's copy of the water up to date with the ford
    /// tiles that are snags.
    pub(crate) fn write_snags(&self, water: &mut super::RiverWater) {
        let out = &mut water.snags;
        out.clear();
        for (g, group) in self.groups.iter().enumerate() {
            if self.debris.snagged(g) {
                out.extend_from_slice(&group.fords);
            }
        }
        out.sort_unstable();
    }

    /// One tick of floating pieces striking the regiments wading where
    /// they pass (see the module's docs). Returns the players whose
    /// regiments were struck, a bit each.
    pub(crate) fn strike(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        terrain: &Terrain,
    ) -> u8 {
        if !self.debris.floating() {
            return 0;
        }
        let n = self.side;
        // Where each piece moving this tick is.
        let mut moving = core::mem::take(&mut self.debris.moving);
        moving.clear();
        moving.extend(
            (self.debris.pieces.iter().enumerate())
                .filter_map(|(k, p)| {
                    p.filter(|p| !p.grounded && p.pace != Fx::ZERO)
                        .map(|p| (k, p))
                })
                .map(|(k, p)| (k, p.point(n))),
        );
        let mut struck = 0u8;
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.models[i] == 0 || regs.on_wall[i] {
                continue;
            }
            // Wading (not on a bridge), and not being swept away.
            let pos = regs.position[i];
            if !terrain.wet(pos) || self.carries(pos.x.floor_int(), pos.y.floor_int()) {
                continue;
            }
            // Half its block's side, and half a tile.
            let reach = Fx::from_ratio(i32::from(regs.berth(units, i)), 2) + Fx::HALF;
            for &(k, at) in &moving {
                let off = pos - at;
                if off.x.abs() > reach
                    || off.y.abs() > reach
                    || off.x * off.x + off.y * off.y > reach * reach
                {
                    continue;
                }
                let Some(p) = self.debris.pieces[k].as_mut() else {
                    continue;
                };
                if !p.strikes(regs.handle(i)) {
                    continue;
                }
                let per_second = p.pace * TICKS_PER_SECOND as i32;
                let hp = (Fx::from_int((HIT_PER_MASS_PACE * u32::from(p.kind.mass())) as i32)
                    * per_second)
                    .ceil_int()
                    .max(1);
                regs.take_damage(i, hp as u32);
                struck |= crate::events::owner_bit(regs.owner[i]);
                if regs.models[i] == 0 {
                    // Gone: no later piece strikes it.
                    break;
                }
                regs.disorder(i);
                morale::shake(regs, i, HIT_MORALE, morale::WHY_WATER);
            }
        }
        self.debris.moving = moving;
        struck
    }
}

/// How fast the water runs on tile `i`, if it runs there.
fn speed(flow: &[u8], i: u32) -> Option<u8> {
    let (_, s) = Flow::of(*flow.get(i as usize)?)?;
    Some(s)
}

/// A piece's place across the channel from a roll: within a third of a
/// tile of the middle.
fn across(r: u64) -> i8 {
    (((r >> 24) % 171) as i16 - 85) as i8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masses_follow_the_design() {
        assert_eq!(Kind::Log.mass(), 1);
        assert_eq!(Kind::Raft.mass(), 3);
        assert_eq!(Kind::Wreck.mass(), 2);
        assert_eq!(Kind::Rubble.mass(), 4);
    }

    #[test]
    fn pieces_go_in_within_a_third_of_a_tile_of_the_middle() {
        for r in 0..2000u64 {
            let a = across(crate::fire::splitmix(r));
            assert!((-85..=85).contains(&a));
        }
    }

    #[test]
    fn a_full_pool_rots_its_oldest_grounded_piece_first() {
        let mut d = Debris::new(Vec::new(), 0);
        for t in 0..OPEN_PLACES as u32 {
            let mut p = Piece::new(Kind::Log, t, 0, false, t);
            p.grounded = t == 5 || t == 9;
            assert!(d.put(p));
        }
        assert!(d.put(Piece::new(Kind::Log, 99, 0, false, 100)));
        assert_eq!(d.len(), OPEN_PLACES);
        // The oldest grounded (born 5) went; the other stays.
        let born: Vec<u32> = d.pieces.iter().flatten().map(|p| p.born).collect();
        assert!(!born.contains(&5));
        assert!(born.contains(&9));
        // Wrecks have their own places.
        assert!(d.put(Piece::new(Kind::Wreck, 7, 0, false, 101)));
        assert_eq!(d.len(), OPEN_PLACES + 1);
    }

    #[test]
    fn only_so_many_natural_pieces_float() {
        let mut d = Debris::new(Vec::new(), 0);
        for t in 0..NATURAL_MOST as u32 {
            assert!(d.put(Piece::new(Kind::Log, t, 0, true, t)));
        }
        assert!(!d.put(Piece::new(Kind::Log, 50, 0, true, 50)));
        // A dumped one still goes in.
        assert!(d.put(Piece::new(Kind::Raft, 51, 0, false, 51)));
    }
}
