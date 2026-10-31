//! Walled towns (docs/26 stage 8): when its own walls, gates and cliffs
//! close all the way round a player's main building, its base is a walled
//! town. Buildings inside take a quarter less from ordinary regiments'
//! blows, and its own regiments inside stand firmer and their wounded
//! mend. A breach (a wall piece down, a gate set Open) ends it until the
//! ring is closed again.
//!
//! Every [`SURVEY_TICKS`] each player's ring is found again by a flood
//! fill out from the middle of its main building across the tiles within
//! [`TOWN_REACH`] of it: the fill stops at its own finished walls and
//! gates (a gate set Open is a gap; a Choir curtain without power is
//! none) and at ground nobody walks on (cliffs, deep water without a
//! deck). If it reaches the edge of that square, the town is open.
//! Everything is reserved when the Conquest starts, so a survey never
//! allocates.

use crate::gates::GateMode;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{GATE, PASS, Role, Structures};
use crate::terrain::Terrain;
use crate::{MAX_PARTIES, PlayerId, TICKS_PER_SECOND, Vec};
use alloc::vec;
use gc_fixed::{Fx, FxVec2};

/// How far from the middle of its main building, in tiles, a town's ring
/// may stand (as the larger of across and along).
pub const TOWN_REACH: i32 = 40;
/// How often the rings are found again: every two seconds.
pub const SURVEY_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// Buildings inside a walled town take this many percent of what an
/// ordinary regiment's blows would do (a quarter less).
pub const SHELTERED_PERCENT: u64 = 75;
/// Morale drain a regiment inside its own walled town is spared each
/// second, in points (as a general near it, see `generals::LEAD`).
pub const TOWN_STEADY: i16 = 1;
/// Hit points a second each wounded soldier of a regiment resting in its
/// own walled town gets back (not while it fights).
pub const TOWN_MEND_HP: u16 = 1;

const SIDE: usize = (2 * TOWN_REACH + 1) as usize;
const CELLS: usize = SIDE * SIDE;
const WORDS: usize = CELLS.div_ceil(64);

/// The walled towns, by player, and the room to find them in.
#[derive(Clone)]
pub struct Towns {
    /// By player: the tile at the corner of its town's square, if walled.
    walled: [Option<(i32, i32)>; MAX_PARTIES],
    /// By player, [`WORDS`] each: the tiles of its square inside the ring.
    inside: Vec<u64>,
    /// The tiles of the square the fill can't cross, for the survey.
    wall: Vec<u64>,
    /// Tiles the fill has yet to spread from.
    queue: Vec<u16>,
    /// Players whose town was walled or breached at the last survey:
    /// `(player, walled)`, for the screen.
    pub(crate) changed: Vec<(PlayerId, bool)>,
}

impl Default for Towns {
    fn default() -> Towns {
        Towns {
            walled: [None; MAX_PARTIES],
            inside: vec![0; WORDS * MAX_PARTIES],
            wall: vec![0; WORDS],
            queue: Vec::with_capacity(CELLS),
            changed: Vec::with_capacity(MAX_PARTIES),
        }
    }
}

fn bit(words: &[u64], i: usize) -> bool {
    words[i / 64] & (1 << (i % 64)) != 0
}

fn set(words: &mut [u64], i: usize) {
    words[i / 64] |= 1 << (i % 64);
}

impl Towns {
    /// Whether player `p` has a walled town now.
    pub fn walled(&self, p: PlayerId) -> bool {
        self.walled.get(usize::from(p)).is_some_and(Option::is_some)
    }

    /// Whether tile `(x, y)` is inside player `p`'s walled town.
    pub fn inside(&self, p: PlayerId, x: i32, y: i32) -> bool {
        let Some(&Some((x0, y0))) = self.walled.get(usize::from(p)) else {
            return false;
        };
        let (dx, dy) = (x - x0, y - y0);
        let side = SIDE as i32;
        (0..side).contains(&dx)
            && (0..side).contains(&dy)
            && bit(
                &self.inside[usize::from(p) * WORDS..][..WORDS],
                (dy * side + dx) as usize,
            )
    }

    /// Whether `at` is inside player `p`'s walled town.
    pub fn inside_at(&self, p: PlayerId, at: FxVec2) -> bool {
        self.inside(p, at.x.floor_int(), at.y.floor_int())
    }

    pub(crate) fn hash_into(&self, h: &mut crate::hash::StateHasher) {
        for w in &self.walled {
            match *w {
                Some((x, y)) => {
                    h.u8(1);
                    h.i32(x);
                    h.i32(y);
                }
                None => h.u8(0),
            }
        }
    }
}

impl Structures {
    /// The walled towns.
    pub fn towns(&self) -> &Towns {
        &self.towns
    }

    /// Find every player's ring again (see the module docs), and mark the
    /// regiments standing inside their own walled town.
    pub(crate) fn survey_towns(&mut self, nav: &Nav, terrain: &Terrain, regs: &mut Regiments) {
        let mut towns = core::mem::take(&mut self.towns);
        towns.changed.clear();
        for p in 0..MAX_PARTIES {
            let was = towns.walled[p].is_some();
            towns.walled[p] = self.ring_of(p as PlayerId, nav, terrain, &mut towns);
            let now = towns.walled[p].is_some();
            if was != now {
                towns.changed.push((p as PlayerId, now));
            }
        }
        for i in 0..regs.alive.len() {
            regs.in_town[i] = regs.alive[i]
                && !regs.on_wall[i]
                && towns.inside_at(regs.owner[i], regs.position[i]);
        }
        self.towns = towns;
    }

    /// Player `p`'s town square's corner if its ring closes round its main
    /// building, filling in its `inside` tiles.
    fn ring_of(
        &self,
        p: PlayerId,
        nav: &Nav,
        terrain: &Terrain,
        towns: &mut Towns,
    ) -> Option<(i32, i32)> {
        let main = self.standing().find(|&i| {
            self.owner_of(i) == p && self.finished(i) && self.stats_of(i).role == Role::Main
        })?;
        let mid = self.centre(main);
        let (cx, cy) = (mid.x.floor_int(), mid.y.floor_int());
        let (x0, y0) = (cx - TOWN_REACH, cy - TOWN_REACH);
        let side = SIDE as i32;
        let wall = &mut towns.wall;
        wall.fill(0);
        // Its own walls and gates.
        let mut walls = 0;
        for i in self.standing() {
            if self.owner_of(i) != p || !self.finished(i) || self.stats_of(i).role != Role::Wall {
                continue;
            }
            let flags = self.flags_of(i);
            let gap = flags & GATE != 0 && self.gate[i] == GateMode::Open
                || flags & PASS != 0 && flags & GATE == 0 && !self.powered(i);
            if gap {
                continue;
            }
            let [x, y, w, d] = self.footprint(i);
            for ty in y.max(y0)..(y + d).min(y0 + side) {
                for tx in x.max(x0)..(x + w).min(x0 + side) {
                    set(wall, ((ty - y0) * side + (tx - x0)) as usize);
                    walls += 1;
                }
            }
        }
        if walls == 0 {
            return None;
        }
        let inside = &mut towns.inside[usize::from(p) * WORDS..][..WORDS];
        inside.fill(0);
        let queue = &mut towns.queue;
        queue.clear();
        let start = (TOWN_REACH * side + TOWN_REACH) as usize;
        set(inside, start);
        queue.push(start as u16);
        while let Some(c) = queue.pop() {
            let c = i32::from(c);
            let (dx, dy) = (c % side, c / side);
            for (nx, ny) in [(dx - 1, dy), (dx + 1, dy), (dx, dy - 1), (dx, dy + 1)] {
                // Out past the edge of the square: the ring is open.
                if !(0..side).contains(&nx) || !(0..side).contains(&ny) {
                    return None;
                }
                let n = (ny * side + nx) as usize;
                if bit(inside, n) || bit(wall, n) {
                    continue;
                }
                let (tx, ty) = (x0 + nx, y0 + ny);
                let at = FxVec2::new(Fx::from_int(tx), Fx::from_int(ty));
                if !nav.walkable_ground(tx, ty) && !terrain.on_deck(at) {
                    continue;
                }
                set(inside, n);
                queue.push(n as u16);
            }
        }
        Some((x0, y0))
    }
}

/// Once a second: each wounded soldier of a regiment resting in its own
/// walled town (not fighting) gets [`TOWN_MEND_HP`] hit points back, up to
/// his unit's full.
pub(crate) fn mend_in_towns(regs: &mut Regiments, units: &[crate::UnitStats]) {
    for i in 0..regs.alive.len() {
        if !regs.alive[i] || !regs.in_town[i] || regs.engaged[i].is_some() {
            continue;
        }
        let full = units[usize::from(regs.unit[i])].hp_per_model;
        regs.mend(i, TOWN_MEND_HP, full);
    }
}
