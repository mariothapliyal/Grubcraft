//! Bridges (docs/26-fortifications.md, "Bridges"): a faction's bridge is
//! drawn across a river like a wall, a tile at a time, by workers who work
//! out from the bank. A finished tile opens its ground on the walkable grid
//! to everyone, over deep water or a flood (`Nav::set_bridged_all`), and
//! nobody wades on it. It falls to blows like any building, and whoever
//! stands on a tile that falls drops into the river: hurt, in disorder,
//! and out on the nearest bank.

use alloc::vec::Vec;

use crate::conquest::Conquest;
use crate::conquest::overlaps;
use crate::drawn::{self, DrawnPath};
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{BuildingStats, LINE, Refusal, Structures};
use crate::terrain::Terrain;
use crate::walls::{Laying, Line, MAX_BRIDGE_LENGTH, WallTile};
use crate::{PlayerId, UnitStats};

/// Hit points each soldier on a bridge tile that falls loses: as for a
/// wall that falls under its men.
pub const DROP_HP: u32 = crate::manning::FALL_HP;

impl Structures {
    /// Whether building slot `i` is a bridge tile.
    pub(crate) fn is_bridge(&self, i: usize) -> bool {
        self.stats_of(i).is_bridge()
    }

    /// The level of bridge slot `i`'s piers: 1 plain, 2 braced, 3
    /// reinforced (stone-footed, with cutwaters), counted down its chain of
    /// upgrades to the plain bridge; 0 for anything that isn't a bridge.
    pub fn pier_level(&self, i: usize) -> u8 {
        if !self.is_bridge(i) {
            return 0;
        }
        let mut b = self.stats_of(i);
        let mut level = 1;
        // The data has no loops in its levels; three is the top anyway.
        while let (Some(from), true) = (b.upgrades_from, level < 3) {
            level += 1;
            b = &self.buildings()[usize::from(from)];
        }
        level
    }

    /// Whether a builder can reach unfinished bridge slot `j`: it was laid
    /// on dry ground (the banks at either end), or it is next to a
    /// finished tile of a bridge.
    pub(crate) fn reachable_deck(&self, j: usize) -> bool {
        if self.ashore[j] {
            return true;
        }
        let [x, y, _, _] = self.footprint(j);
        self.standing().any(|k| {
            let [kx, ky, _, _] = self.footprint(k);
            (kx - x).abs() + (ky - y).abs() == 1 && self.is_bridge(k) && self.finished(k)
        })
    }
}

/// Bring the ground up to date with the bridge tiles finished and fallen
/// since last time (`changes`, drained): the walkable grid and the water
/// on it, and those standing on a tile that fell drop into the river and
/// come out on the nearest bank.
pub(crate) fn settle_decks(
    changes: &mut Vec<([i32; 4], bool)>,
    nav: &mut Nav,
    terrain: &mut Terrain,
    regs: &mut Regiments,
    units: &[UnitStats],
) {
    if changes.is_empty() {
        return;
    }
    for on in [true, false] {
        let feet: Vec<[i32; 4]> = (changes.iter())
            .filter(|c| c.1 == on)
            .map(|c| c.0)
            .collect();
        if feet.is_empty() {
            continue;
        }
        if on {
            // Built over, a tile is a wall on the grid until it is
            // finished (see `Structures::found`).
            nav.set_built_all(&feet, false, regs);
        }
        nav.set_bridged_all(&feet, on, regs);
        for &[x, y, _, _] in &feet {
            terrain.set_deck(x, y, on);
        }
        if on {
            continue;
        }
        for i in 0..regs.alive.len() {
            if !regs.alive[i] || regs.on_wall[i] {
                continue;
            }
            let p = regs.position[i];
            let at = [p.x.floor_int(), p.y.floor_int(), 1, 1];
            if !feet.iter().any(|&f| overlaps(f, at)) || nav.walkable(regs.owner[i], p) {
                continue;
            }
            regs.wound_last(i, usize::from(regs.models[i]), DROP_HP);
            regs.disorder(i);
            wash_up(nav, regs, units, i, p);
        }
    }
    changes.clear();
}

/// Slot `i`, in the water at `p`, comes out on the nearest bank where its
/// block fits (or, failing that, any open ground near), and halts there.
/// Returns where it came out: `p` itself if there is nowhere.
pub(crate) fn wash_up(
    nav: &Nav,
    regs: &mut Regiments,
    units: &[UnitStats],
    i: usize,
    p: crate::FxVec2,
) -> crate::FxVec2 {
    let owner = regs.owner[i];
    let mut bank = nav.settle(owner, p, regs.berth(units, i));
    if !nav.walkable(owner, bank) {
        bank = nav.settle(owner, p, 0);
    }
    regs.position[i] = bank;
    regs.halt(i);
    bank
}

impl Conquest {
    /// The new tiles of `player`'s bridge `b` drawn along `line` (see
    /// [`crate::drawn::DrawnPath::band`]), each with whether it can go up
    /// there. The centre line bends gently or not at all (no corners) and
    /// is at most [`MAX_BRIDGE_LENGTH`] long; the tiles at both ends are
    /// dry land a regiment can walk on or a bridge of `player`'s, so a gap
    /// can be drawn again from deck to deck. Tiles where `player` already
    /// has a bridge are left out, as walls leave out their own tiles, and
    /// every other tile must be water or open ground with nothing built or
    /// bridged on it, and something new must be over water.
    ///
    /// A bridge drawn the old way (`Command::BuildWall`) runs straight
    /// along a row or column, two tiles wide: the tiles drawn and those
    /// beside them to the east or south, or to the west or north when only
    /// that side will do.
    pub(crate) fn bridge_line(
        &self,
        player: PlayerId,
        b: BuildingStats,
        line: &Line,
        nav: &Nav,
        terrain: &Terrain,
    ) -> Result<Laying, Refusal> {
        if !line.old {
            if !drawn::BRIDGE_WIDTHS.contains(&line.width) {
                return Err(Refusal::Unknown);
            }
            return self.bridge_band(player, b, line.path, line.width, nav, terrain);
        }
        let (from, to) = line.path.ends();
        if from.0 != to.0 && from.1 != to.1 {
            return Err(Refusal::Crooked);
        }
        // The centre line runs along the edge between the tiles drawn and
        // the next row (or column) to one side.
        let side = |up: bool| {
            let k = if up { 2 } else { 0 };
            let at = |(x, y): (i32, i32)| {
                if from.1 == to.1 {
                    ((2 * x + 1) as i16, (2 * y + k) as i16)
                } else {
                    ((2 * x + k) as i16, (2 * y + 1) as i16)
                }
            };
            let path = DrawnPath::new(&[at(from), at(to)], |_| true).ok_or(Refusal::Unknown)?;
            self.bridge_band(player, b, path, line.width, nav, terrain)
        };
        let clear = |l: &Result<Laying, Refusal>| {
            l.as_ref()
                .is_ok_and(|l| l.tiles.iter().all(|t| t.fits.is_ok()))
        };
        // Beside a lane of this bridge already standing first (a gap in one
        // lane is mended on its own), else the side it will go up on.
        let s = &self.structures;
        let beside = |dy: i32| {
            let (ax, ay, bx, by) = if from.1 == to.1 {
                (from.0, from.1 + dy, to.0, to.1 + dy)
            } else {
                (from.0 + dy, from.1, to.0 + dy, to.1)
            };
            let area = crate::walls::tiles_area(&[(ax, ay), (bx, by)]);
            s.own_in(player, LINE, area).any(|i| s.is_bridge(i))
        };
        let below = beside(1);
        if below || beside(-1) {
            return side(below);
        }
        let up = side(true);
        if clear(&up) {
            return up;
        }
        let down = side(false);
        if clear(&down) || up.is_err() && down.is_ok() {
            return down;
        }
        up
    }

    /// [`Conquest::bridge_line`] for a bridge `width` tiles across along
    /// `path`.
    fn bridge_band(
        &self,
        player: PlayerId,
        b: BuildingStats,
        path: DrawnPath,
        width: u8,
        nav: &Nav,
        terrain: &Terrain,
    ) -> Result<Laying, Refusal> {
        if path.has_corners() || !path.bends_gently() {
            return Err(Refusal::Crooked);
        }
        if path.length() > MAX_BRIDGE_LENGTH * i64::from(drawn::SUB) {
            return Err(Refusal::Unknown);
        }
        let tiles = path.band(width);
        let area = crate::walls::tiles_area(&tiles);
        let area = [area[0] - 1, area[1] - 1, area[2] + 2, area[3] + 2];
        let s = &self.structures;
        let own: Vec<[i32; 4]> = (s.own_in(player, LINE, area))
            .filter(|&i| s.is_bridge(i))
            .map(|i| s.footprint(i))
            .collect();
        let owned = |(x, y): (i32, i32)| own.iter().any(|&f| overlaps(f, [x, y, 1, 1]));
        // The map's own water, whatever the weather or bridges on it.
        let water = |(x, y): (i32, i32)| terrain.tile_under_water(x, y);
        let ground = |(x, y): (i32, i32)| nav.walkable_ground(x, y);
        let bank = |t: (i32, i32)| owned(t) || (ground(t) && !water(t));
        for end in path.band_ends(width) {
            if end.is_empty() || !end.iter().all(|&t| bank(t)) {
                return Err(Refusal::Rough);
            }
        }
        let plan: Vec<WallTile> = tiles
            .into_iter()
            .filter(|&t| !owned(t))
            .map(|tile| WallTile {
                tile,
                // Unfinished bridges count as built (see `settle_decks`).
                fits: nav.deck_footing(tile.0, tile.1).and_then(|()| {
                    if water(tile) || ground(tile) {
                        Ok(())
                    } else {
                        Err(Refusal::Rough)
                    }
                }),
            })
            .collect();
        // Something new over water.
        if !plan.iter().any(|t| water(t.tile)) {
            return Err(Refusal::Rough);
        }
        Ok(Laying {
            stats: b,
            tiles: plan,
            path,
            width,
        })
    }
}
