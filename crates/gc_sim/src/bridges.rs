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
use crate::nav::Nav;
use crate::regiments::Regiments;
use crate::structures::{BuildingId, LINE, Refusal, StructureHandle, Structures};
use crate::terrain::Terrain;
use crate::walls::{WallTile, line_area, wall_tiles};
use crate::{PlayerId, UnitStats};

/// Hit points each soldier on a bridge tile that falls loses: as for a
/// wall that falls under its men.
pub const DROP_HP: u32 = crate::manning::FALL_HP;

impl Structures {
    /// Whether building slot `i` is a bridge tile.
    pub(crate) fn is_bridge(&self, i: usize) -> bool {
        self.stats_of(i).is_bridge()
    }

    /// Whether a builder can reach unfinished bridge slot `j`: it is an
    /// end of the line it was drawn in (on land), or next to a finished
    /// tile of a bridge.
    pub(crate) fn reachable_deck(&self, j: usize) -> bool {
        let [x, y, _, _] = self.footprint(j);
        let [x0, y0, x1, y1] = self.line[j].map(i32::from);
        if (x, y) == (x0, y0) || (x, y) == (x1, y1) {
            return true;
        }
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

/// One of a bridge's two lanes: the line it was laid along (stored on each
/// of its tiles) and the tiles still to put up.
pub(crate) struct Lane {
    pub line: [i16; 4],
    pub tiles: Vec<WallTile>,
}

impl Conquest {
    /// The new tiles of `player`'s bridge drawn along `tiles` (in the box
    /// `area`), each with whether it can go up there. A bridge is two
    /// lanes wide, the line drawn and one beside it (the next tile east
    /// of a line running north to south, south of one running west to
    /// east, or the other side when that one won't do); see
    /// [`Conquest::bridge_lanes`] for what each lane needs.
    pub(crate) fn bridge_line(
        &self,
        player: PlayerId,
        tiles: &[(i32, i32)],
        area: [i32; 4],
        nav: &Nav,
        terrain: &Terrain,
    ) -> Result<Vec<WallTile>, Refusal> {
        let lanes = self.bridge_lanes(player, tiles, area, nav, terrain)?;
        Ok(lanes.into_iter().flat_map(|l| l.tiles).collect())
    }

    /// The two lanes of `player`'s bridge drawn along `tiles`, lower
    /// coordinates first. Each lane is a straight line (a corner step is
    /// too narrow to cross) over some water, each end on dry land a
    /// regiment can walk on or on a bridge of `player`'s, so a gap in one
    /// can be drawn again from deck to deck. Tiles where `player` already
    /// has a bridge are left out, as walls leave out their own tiles, and
    /// every other tile must be water or open ground with nothing built or
    /// bridged on it. The lower lane is stored running towards higher
    /// coordinates and the upper one back, so a tile knows its side.
    pub(crate) fn bridge_lanes(
        &self,
        player: PlayerId,
        tiles: &[(i32, i32)],
        area: [i32; 4],
        nav: &Nav,
        terrain: &Terrain,
    ) -> Result<[Lane; 2], Refusal> {
        let (Some(&first), Some(&last)) = (tiles.first(), tiles.last()) else {
            return Err(Refusal::Unknown);
        };
        if first.0 != last.0 && first.1 != last.1 {
            return Err(Refusal::Crooked);
        }
        // Along x, the lanes lie a row apart; along y, a column apart.
        let across = if first.1 == last.1 { (0, 1) } else { (1, 0) };
        let shift = |(x, y): (i32, i32), k: i32| (x + across.0 * k, y + across.1 * k);
        let area = [
            area[0] - across.0,
            area[1] - across.1,
            area[2] + 2 * across.0,
            area[3] + 2 * across.1,
        ];
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
        let lane = |step: i32| -> Result<Vec<WallTile>, Refusal> {
            if !bank(shift(first, step)) || !bank(shift(last, step)) {
                return Err(Refusal::Rough);
            }
            Ok(tiles
                .iter()
                .map(|&t| shift(t, step))
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
                .collect())
        };
        let drawn = lane(0)?;
        // The other lane goes beside a lane of this bridge already
        // standing (a gap in one lane is mended on its own), else to the
        // higher coordinates if it will go up there, else to the lower.
        let beside = |step: i32| tiles.iter().any(|&t| owned(shift(t, step)));
        let clear = |l: &Result<Vec<WallTile>, Refusal>| {
            l.as_ref().is_ok_and(|l| l.iter().all(|t| t.fits.is_ok()))
        };
        let up = lane(1);
        let step = if beside(1) || !beside(-1) && (clear(&up) || up.is_ok() && !clear(&lane(-1))) {
            1
        } else {
            -1
        };
        let other = if step == 1 { up } else { lane(-1) }?;
        // Something new over water, in one lane or the other.
        if !drawn.iter().chain(&other).any(|t| water(t.tile)) {
            return Err(Refusal::Rough);
        }
        // The lower lane is the one with the lower row or column; the
        // line is stored running towards higher coordinates there and
        // back on the upper lane.
        let (lo, hi, lo_row) = if step == 1 {
            (drawn, other, 0)
        } else {
            (other, drawn, -1)
        };
        let (a, b) = (first.min(last), first.max(last));
        let line = |from: (i32, i32), to: (i32, i32), row: i32| {
            let (f, t) = (shift(from, row), shift(to, row));
            [f.0, f.1, t.0, t.1].map(|v| v as i16)
        };
        Ok([
            Lane {
                line: line(a, b, lo_row),
                tiles: lo,
            },
            Lane {
                line: line(b, a, lo_row + 1),
                tiles: hi,
            },
        ])
    }

    /// Lay both lanes of `player`'s bridge drawn from `from` to `to` (see
    /// [`Conquest::bridge_lanes`]; the caller has checked every tile), and
    /// the handles of the first and last tile laid.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn found_bridge(
        &mut self,
        player: PlayerId,
        building: BuildingId,
        from: (i32, i32),
        to: (i32, i32),
        nav: &mut Nav,
        regs: &Regiments,
        terrain: &Terrain,
    ) -> Option<(StructureHandle, StructureHandle)> {
        let tiles = wall_tiles(from, to)?;
        let lanes = self.bridge_lanes(player, &tiles, line_area(from, to), nav, terrain);
        let mut ends = None;
        for lane in lanes.ok()?.iter().filter(|l| !l.tiles.is_empty()) {
            let (a, b) =
                self.structures
                    .found_wall(player, building, &lane.tiles, lane.line, nav, regs)?;
            ends = Some(ends.map_or((a, b), |(first, _)| (first, b)));
        }
        ends
    }
}
