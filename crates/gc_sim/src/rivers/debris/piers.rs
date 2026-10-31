//! Bridges in the water (docs/39-water-engine.md, section 4; stage 4): a
//! pier stands under every finished bridge tile off the banks. A piece of
//! debris that reaches one jams against it or strikes it and goes on. A
//! jam chokes its stretch's flow and pushes on its tile while the water is
//! up; small jams wash free when it falls. High water strains piers that
//! are already hurt, and catastrophic water sweeps weak spans away. A tile
//! that falls lets its jam go and floats on down as a wreck.

use super::{Blow, Kind, Piece, roll};
use crate::rivers::{Band, FLOOD_RISE, Rivers};
use crate::structures::StructureHandle;
use crate::terrain::index_in;

/// Chance in a hundred a piece jams on a pier, by level (plain, braced,
/// reinforced with cutwaters).
pub const JAM_PERCENT: [u64; 3] = [40, 30, 10];
/// What a piece that strikes a pier takes off its tile, in ten-thousandths
/// of the tile's hit points per mass point at a steady river's pace, by
/// level. Four times that in a full flood (the square of the pace).
pub const STRIKE_PER_TEN_THOUSAND: [u32; 3] = [600, 400, 150];
/// The most mass a jam holds; more passes over or round it.
pub const JAM_MOST: u8 = 12;
/// A stretch's outflow each mass point of jam on it holds back, percent.
pub const CHOKE_PER_MASS: u8 = 5;
/// The most a stretch's outflow is held back, percent.
pub const CHOKE_MOST: u8 = 60;
/// What a jam's push takes off a plain tile a second while the water is up
/// a Minor flood, in ten-thousandths of its hit points per mass point
/// (braced two thirds, reinforced a third).
pub const PUSH_PER_TEN_THOUSAND: u32 = 10;
/// How much harder a jam pushes, by the band of the water: Minor, Flood,
/// Major, Catastrophic.
pub const PUSH_STAGE: [u32; 4] = [1, 2, 4, 4];
/// From Major water up, a pier under this much of its tile's hit points
/// (thousandths) is strained.
pub const STRAIN_BELOW: u16 = 750;
/// What straining takes off a tile a second, in ten-thousandths of its
/// hit points, by level.
pub const STRAIN_PER_TEN_THOUSAND: [u32; 3] = [50, 12, 0];
/// Catastrophic water sweeps away a span tile under this much of its hit
/// points (thousandths), by level: reinforced spans stand.
pub const SWEEP_BELOW: [u16; 3] = [500, 250, 0];
/// A blow that sweeps a tile away: all its hit points.
const SWEPT: u32 = 10_000;
/// A jam this heavy or lighter washes free once the water falls back.
pub const WASH_FREE_MASS: u8 = 3;
/// Where a jammed piece lies: this far from the middle of the tile above
/// the pier towards it, against the pier's upstream face.
const JAMMED_ALONG: gc_fixed::Fx = gc_fixed::Fx::from_raw(26_214);

/// A pier in the water: the river tile under a finished bridge tile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Pier {
    tile: u32,
    slot: StructureHandle,
    /// 1 plain, 2 braced, 3 reinforced.
    level: u8,
    /// Its tile's hit points left, thousandths.
    left: u16,
}

impl Pier {
    /// Its level as an index into the tables above.
    fn at(self) -> usize {
        usize::from(self.level.clamp(1, 3) - 1)
    }
}

impl Rivers {
    /// The piers standing now: each finished bridge tile off the banks
    /// (tile, handle, level, hit points left in thousandths) that stands
    /// over running river. Refreshed each second.
    pub(crate) fn set_piers(
        &mut self,
        decks: impl Iterator<Item = (i32, i32, StructureHandle, u8, u16)>,
        terrain: &crate::terrain::Terrain,
    ) {
        let mut piers = core::mem::take(&mut self.debris.piers);
        piers.clear();
        if self.any() {
            let n = self.side;
            piers.extend(decks.filter_map(|(x, y, slot, level, left)| {
                let i = index_in(n, x, y).filter(|&i| self.runs(terrain, i))?;
                Some(Pier {
                    tile: i as u32,
                    slot,
                    level,
                    left,
                })
            }));
            piers.sort_unstable_by_key(|p| p.tile);
        }
        self.debris.piers = piers;
    }

    /// The pier on river tile `i`, if one stands there.
    fn pier_on(&self, i: u32) -> Option<Pier> {
        let piers = &self.debris.piers;
        (piers.binary_search_by_key(&i, |p| p.tile).ok()).map(|k| piers[k])
    }

    /// The mass jammed against the pier on tile `i` (lying on the tile
    /// above it).
    pub(super) fn jam_on(&self, i: u32) -> u8 {
        (self.debris.pieces.iter().flatten())
            .filter(|p| p.jammed && p.next == i)
            .map(|p| p.kind.mass())
            .sum()
    }

    /// Floating piece `p` (in place `k`) has just come onto tile `p.tile`
    /// from tile `from`: if a pier stands there, it jams on it (true),
    /// lying back against its upstream face, or strikes it and goes on,
    /// with the map's water `lift`.
    pub(super) fn meet_pier(
        &mut self,
        p: &mut Piece,
        from: u32,
        k: usize,
        lift: i32,
        tick: u32,
    ) -> bool {
        let Some(pier) = self.pier_on(p.tile) else {
            return false;
        };
        let mass = p.kind.mass();
        let r = roll(self.seed, tick, (k as u64) << 32 | u64::from(p.tile));
        if r % 100 < JAM_PERCENT[pier.at()] {
            if self.jam_on(p.tile) + mass > JAM_MOST {
                // The jam is full: it passes over or round it.
                return false;
            }
            p.jammed = true;
            p.next = p.tile;
            p.tile = from;
            p.along = JAMMED_ALONG;
            p.pace = gc_fixed::Fx::ZERO;
            return true;
        }
        // The square of its pace against the river's steady pace there.
        let up = self
            .water_on(lift, p.tile as usize)
            .clamp(0, i32::from(FLOOD_RISE)) as u32;
        let pace = 100 + 100 * up / u32::from(FLOOD_RISE);
        let blow = STRIKE_PER_TEN_THOUSAND[pier.at()] * u32::from(mass) * pace * pace / 10_000;
        self.debris.blows.push((pier.slot, blow));
        false
    }

    /// One second of the piers, with the map's water `lift`: jams push on
    /// their tiles while the water is up and the small ones wash free when
    /// it falls; each stretch is choked by the jams on it; high water
    /// strains hurt piers and catastrophic water sweeps weak spans away.
    pub(super) fn weigh_piers(&mut self, lift: i32) {
        self.debris.choke.fill(0);
        for k in 0..self.debris.piers.len() {
            let pier = self.debris.piers[k];
            let i = pier.tile;
            let rise = self.water_on(lift, i as usize);
            let band = (rise >= i32::from(Band::Minor.crest()))
                .then(|| Band::of(u16::try_from(rise).unwrap_or(u16::MAX)));
            let jam = self.jam_on(i);
            if jam > 0 {
                match band {
                    None if jam <= WASH_FREE_MASS => self.wash_free(i),
                    None => {}
                    Some(band) => {
                        let push = PUSH_PER_TEN_THOUSAND
                            * u32::from(jam)
                            * PUSH_STAGE[band as usize]
                            * (3 - pier.at() as u32)
                            / 3;
                        self.debris.blows.push((pier.slot, push.max(1)));
                    }
                }
                let s = self.stretch_of[i as usize];
                if let Some(c) = self.debris.choke.get_mut(usize::from(s)) {
                    *c = c
                        .saturating_add(jam.saturating_mul(CHOKE_PER_MASS))
                        .min(CHOKE_MOST);
                }
            }
            let Some(band) = band else {
                continue;
            };
            if band == Band::Catastrophic && pier.left < SWEEP_BELOW[pier.at()] {
                self.debris.blows.push((pier.slot, SWEPT));
            } else {
                let strain = STRAIN_PER_TEN_THOUSAND[pier.at()];
                if strain > 0 && band >= Band::Major && pier.left < STRAIN_BELOW {
                    self.debris.blows.push((pier.slot, strain));
                }
            }
        }
    }

    /// The jam on the pier on tile `i` breaks up and floats on, past it.
    fn wash_free(&mut self, i: u32) {
        for p in self.debris.pieces.iter_mut().flatten() {
            if p.jammed && p.next == i {
                p.jammed = false;
                p.tile = i;
                p.along = gc_fixed::Fx::ZERO;
                p.entered = false;
            }
        }
    }

    /// A bridge tile at `(x, y)` fell on tick `tick`: its jam lets go, and
    /// if it stood over running river its wreck floats on down.
    pub(crate) fn bridge_fell(
        &mut self,
        terrain: &crate::terrain::Terrain,
        x: i32,
        y: i32,
        tick: u32,
    ) {
        let Some(i) = index_in(self.side, x, y).filter(|&i| self.runs(terrain, i)) else {
            return;
        };
        let i = i as u32;
        self.wash_free(i);
        // Nothing on it any more, until the piers are next weighed.
        self.debris.piers.retain(|p| p.tile != i);
        let r = roll(self.seed, tick, u64::from(i) << 2 | 2);
        let wreck = Piece::new(Kind::Wreck, i, super::across(r), false, tick);
        // Some places are kept for wrecks; with all of them taken it sinks.
        self.debris.put(wreck);
    }

    /// What the debris and the water did to bridges this tick, for the
    /// world to apply (and clear).
    pub(crate) fn blows(&mut self) -> &mut alloc::vec::Vec<Blow> {
        &mut self.debris.blows
    }

    /// How much of stretch `s`'s outflow its jams hold back, percent.
    pub(crate) fn choke(&self, s: usize) -> u8 {
        self.debris.choke.get(s).copied().unwrap_or(0)
    }
}
