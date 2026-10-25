//! Fords that flood (docs/32-changing-land.md, "Fords that flood"): long
//! rain swells the rivers. The water climbs the banks, so troops wade
//! further out from the river, and once it is up the fords can't be
//! crossed: their cells are shut on the walkable grid, and anyone still
//! in one is swept about and struggles to the nearest bank. When the rain
//! stops the water goes down again and the fords open.
//!
//! With seasons (docs/38-living-waterways.md, stage 1) the rivers follow
//! the year too: the winter's snow piles up and melts in the spring,
//! swelling the rivers as long rain does, and a dry summer drops the water
//! below its line, so deep water at the edge of the channels can be waded:
//! the summer fords.

use alloc::sync::Arc;
use alloc::vec::Vec;

use gc_fixed::FxVec2;

use crate::hash::StateHasher;
use crate::morale;
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::sky::{Season, WET_FULL};
use crate::terrain::{HEIGHT_UNITS, Terrain};

/// The rivers rise while the ground is at least this wet: under three
/// minutes of heavy rain on dry ground.
pub const FLOOD_WET: u16 = WET_FULL / 2;
/// They go down again once it has dried below this: a minute and a half of
/// fair weather after the rain stops, more after a long downpour.
pub const EBB_WET: u16 = WET_FULL * 3 / 10;
/// How far the water rises in a flood, in height units: three tenths of a
/// tile, about as deep as a ford was.
pub const FLOOD_RISE: u16 = (HEIGHT_UNITS * 3 / 10) as u16;
/// Height units the water rises a second while it floods (the full rise
/// in about 25 seconds) and falls a second as it goes down (about 75).
pub const RISE_PER_SECOND: u16 = 3;
pub const EBB_PER_SECOND: u16 = 1;
/// Hit points a regiment caught in a flooded ford loses a second.
pub const SWEPT_HP: u32 = 12;
/// Morale points it loses a second.
pub const SWEPT_MORALE: i16 = 4;
/// How far a dry summer drops the water below its line, in height units:
/// a fifth of a tile.
pub const LOW_DROP: u16 = (HEIGHT_UNITS / 5) as u16;
/// How deep below the line a regiment can wade, in height units, as the
/// map's walking grid has it (0.35 of a tile).
pub const WADE_DEPTH: u16 = (HEIGHT_UNITS * 35 / 100) as u16;
/// Height units the water falls a second in a dry summer (the full drop
/// in under a minute), and comes back once the summer ends.
pub const LOW_PER_SECOND: u16 = 1;
/// Snow piled up a second of the heaviest snowfall (at strength 255); the
/// pack melts at [`MELT_PER_SECOND`], so a second of heavy snow is about a
/// second and a half of melt.
const SNOW_PER_SECOND: u32 = 255;
/// Snow melting a second in the spring; what is left at the summer is
/// gone.
pub const MELT_PER_SECOND: u32 = 170;
/// Less snow than a minute of melt melts without swelling the rivers.
pub const MELT_MIN: u32 = 60 * MELT_PER_SECOND;

/// The rivers' fords, and whether a flood has shut them. How far the
/// water has risen is the terrain's ([`Terrain::rise`]).
#[derive(Clone, Debug)]
pub struct Flood {
    /// The tiles under the water line that a regiment can walk on (some
    /// cell of theirs isn't blocked ground), as `(y, x)`, sorted: the
    /// fords. Empty on a map with none.
    fords: Arc<[(i32, i32)]>,
    /// The fords as runs along each row (`[x, y, w, 1]`), to shut or open
    /// on the walkable grid.
    runs: Arc<[[i32; 4]]>,
    /// Whether the fords are shut.
    closed: bool,
    /// The tiles of deep water a dry summer makes wadeable, as `(y, x)`,
    /// sorted: the summer fords. Empty on a map with none.
    shallows: Arc<[(i32, i32)]>,
    /// The summer fords as runs along each row.
    shallow_runs: Arc<[[i32; 4]]>,
    /// Whether the summer fords are open.
    exposed: bool,
    /// The winter's snow, still to melt.
    snowpack: u32,
}

/// What one second of the rivers changed (see [`Flood::second`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Turn {
    /// The fords just shut (`Some(true)`) or opened again.
    pub(crate) fords: Option<bool>,
    /// The summer fords just opened (`Some(true)`) or went deep again.
    pub(crate) shallows: Option<bool>,
}

/// Runs along each row of the sorted `(y, x)` tiles.
fn runs_of(tiles: &[(i32, i32)]) -> Vec<[i32; 4]> {
    let mut runs: Vec<[i32; 4]> = Vec::new();
    for &(y, x) in tiles {
        match runs.last_mut() {
            Some(run) if run[1] == y && run[0] + run[2] == x => run[2] += 1,
            _ => runs.push([x, y, 1, 1]),
        }
    }
    runs
}

impl Flood {
    /// The fords of a map `map_tiles` a side with `terrain`, walked on
    /// the grid `nav`.
    pub(crate) fn new(terrain: &Terrain, nav: &Nav, map_tiles: u32) -> Flood {
        let tiles = map_tiles as i32;
        let (mut fords, mut shallows) = (Vec::new(), Vec::new());
        for (y, x) in (0..tiles).flat_map(|y| (0..tiles).map(move |x| (y, x))) {
            if !terrain.tile_under_water(x, y) {
                continue;
            }
            if nav.walkable_ground(x, y) {
                fords.push((y, x));
            }
            // Deep water no deeper than wading depth below the low line.
            if terrain.tile_within(x, y, LOW_DROP + WADE_DEPTH) && nav.deep_ground(x, y) {
                shallows.push((y, x));
            }
        }
        Flood {
            runs: runs_of(&fords).into(),
            fords: fords.into(),
            closed: false,
            shallow_runs: runs_of(&shallows).into(),
            shallows: shallows.into(),
            exposed: false,
            snowpack: 0,
        }
    }

    /// Whether the fords are shut by the flood.
    pub fn closed(&self) -> bool {
        self.closed
    }

    /// The fords, as (column, row): shut while [`Flood::closed`].
    pub fn fords(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.fords.iter().map(|&(y, x)| (x, y))
    }

    /// The fords as runs along each row (`[x, y, w, 1]`).
    pub(crate) fn runs(&self) -> &[[i32; 4]] {
        &self.runs
    }

    /// Whether the summer fords are open.
    pub fn exposed(&self) -> bool {
        self.exposed
    }

    /// The summer fords, as (column, row): wadeable while
    /// [`Flood::exposed`].
    pub fn shallows(&self) -> impl Iterator<Item = (i32, i32)> + '_ {
        self.shallows.iter().map(|&(y, x)| (x, y))
    }

    /// The summer fords as runs along each row (`[x, y, w, 1]`).
    pub(crate) fn shallow_runs(&self) -> &[[i32; 4]] {
        &self.shallow_runs
    }

    /// Whether `p` is on a summer ford.
    pub fn on_shallows(&self, p: FxVec2) -> bool {
        (self.shallows)
            .binary_search(&(p.y.floor_int(), p.x.floor_int()))
            .is_ok()
    }

    /// The winter's snow still to melt (0 with none).
    pub fn snowpack(&self) -> u32 {
        self.snowpack
    }

    /// Whether `p` is in a ford the flood has shut.
    pub fn sweeps(&self, p: FxVec2, terrain: &Terrain) -> bool {
        self.closed
            && !terrain.on_deck(p)
            && (self.fords)
                .binary_search(&(p.y.floor_int(), p.x.floor_int()))
                .is_ok()
    }

    /// One second of the rivers, with the ground `wet`, in `season` (with
    /// seasons), with snow falling at `snow` (0 to 255): the water in
    /// `terrain` rises, holds or goes down, and with `shut` the fords shut
    /// or open with it, and the summer fords open or go deep (a rehearsal
    /// keeps them as it found them, so it never changes the walkable grid
    /// it shares). Returns what changed.
    pub(crate) fn second(
        &mut self,
        terrain: &mut Terrain,
        wet: u16,
        season: Option<Season>,
        snow: u8,
        shut: bool,
    ) -> Turn {
        if terrain.is_flat() || terrain.water_level() == 0 {
            return Turn::default();
        }
        // The winter's snow piles up, and melts in the spring: a little
        // melts away unseen, more swells the rivers as rain does.
        let melting = match season {
            Some(Season::Winter) => {
                self.snowpack += SNOW_PER_SECOND * u32::from(snow) / 255;
                false
            }
            Some(Season::Spring) if self.snowpack < MELT_MIN => {
                self.snowpack = 0;
                false
            }
            Some(Season::Spring) => {
                self.snowpack -= self.snowpack.min(MELT_PER_SECOND);
                true
            }
            // What the spring didn't melt is gone by the summer.
            _ => {
                self.snowpack = 0;
                false
            }
        };
        let (rise, low) = (terrain.rise(), terrain.low());
        if wet >= FLOOD_WET || melting {
            // Low water comes back first, then the flood.
            if low > 0 {
                terrain.set_low(low.saturating_sub(RISE_PER_SECOND));
            } else {
                terrain.set_rise((rise + RISE_PER_SECOND).min(FLOOD_RISE));
            }
        } else {
            if wet < EBB_WET {
                terrain.set_rise(rise.saturating_sub(EBB_PER_SECOND));
            }
            let dry_summer = season == Some(Season::Summer) && wet < EBB_WET;
            if dry_summer && terrain.rise() == 0 {
                terrain.set_low((low + LOW_PER_SECOND).min(LOW_DROP));
            } else if !dry_summer {
                terrain.set_low(low.saturating_sub(LOW_PER_SECOND));
            }
        }
        let mut turn = Turn::default();
        if !shut {
            return turn;
        }
        let need = if self.closed {
            FLOOD_RISE / 2
        } else {
            FLOOD_RISE
        };
        let now = !self.fords.is_empty() && terrain.rise() >= need;
        if now != self.closed {
            self.closed = now;
            turn.fords = Some(now);
        }
        let need = if self.exposed { LOW_DROP / 2 } else { LOW_DROP };
        let now = !self.shallows.is_empty() && terrain.low() >= need;
        if now != self.exposed {
            self.exposed = now;
            turn.shallows = Some(now);
        }
        turn
    }

    /// One second of the regiments caught in shut fords: they lose hit
    /// points and morale. Into `out` (cleared first), the slots of those
    /// left alive with nowhere to go but where they stand: no order, or
    /// a chase the shut ford cuts off, and not fighting. They struggle to
    /// the nearest bank (see `World::rivers`).
    pub(crate) fn sweep(&self, regs: &mut Regiments, terrain: &Terrain, out: &mut Vec<usize>) {
        out.clear();
        if !self.closed {
            return;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.models[i] == 0 || !self.sweeps(regs.position[i], terrain) {
                continue;
            }
            regs.take_damage(i, SWEPT_HP);
            if regs.models[i] == 0 {
                continue;
            }
            morale::shift(regs, i, -SWEPT_MORALE);
            let stuck = regs.target[i].is_none() || regs.attack_target[i].is_some();
            if stuck && regs.foe(i).is_none() {
                out.push(i);
            }
        }
    }

    /// Whether the year has left anything to hash: low water, or snow
    /// still to melt (see [`Flood::hash_into`]).
    pub(crate) fn seasonal(&self, low: u16) -> bool {
        low > 0 || self.snowpack > 0
    }

    /// Hashed with the ground while the water is up (`rise` height units
    /// above its line) or the year has left something (`low` water, or
    /// snow to melt), so a match it never changes hashes as before.
    pub(crate) fn hash_into(&self, rise: u16, low: u16, h: &mut StateHasher) {
        h.u16(rise);
        h.u8(u8::from(self.closed));
        if self.seasonal(low) {
            h.u16(low);
            h.u8(u8::from(self.exposed));
            h.u32(self.snowpack);
        }
    }
}
