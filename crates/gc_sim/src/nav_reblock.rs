//! Ground that changes mid-match: buildings put up and pulled down (see
//! [`crate::structures`]). A change blocks or frees a rectangle of cells;
//! the walkable grid is brought up to date around it instead of being
//! built again, so a building going up costs a few milliseconds, not the
//! whole map's worth of searches:
//!
//! - **Clearance** is measured again only within [`CLEAR_CAP`] cells of the
//!   change: past that, clearances are capped and can't have moved.
//! - **Areas** are numbered again across the map, as a wall can cut one
//!   area in two anywhere along it: one pass joining each open cell to its
//!   neighbours behind and above, and one numbering what it joined, about
//!   ten times faster than [`Grid::label_areas`]'s flood fill (on a
//!   768-tile map that took most of the 200 ms a building cost).
//! - **Portals** are placed again everywhere (cheap), but linked again only
//!   in the sectors where a cell may have opened or shut (within
//!   [`CLEARANCE`] of the change), or whose portals moved because a
//!   neighbour's did: every other sector's links are copied across.
//!
//! The result is exactly what building the grid from scratch would give
//! (a test checks), so a match's grid never depends on the order its
//! buildings went up in. This runs inside a tick but only when the ground
//! changes, and it may allocate (the portal graph can grow).

use alloc::vec;
use alloc::vec::Vec;

use super::{BLOCKED, CLEAR_CAP, CLEARANCE, Grid, SECTOR, Search};

impl Grid {
    /// Bring the grid up to date after the cells `x0..x1` × `y0..y1` (cell
    /// coordinates, end exclusive) changed cost, searching in `search`'s
    /// room and leaving it room for searches over the new graph.
    pub(super) fn changed(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, search: &mut Search) {
        let reach = i32::from(CLEAR_CAP) + 1;
        let (wx0, wy0) = ((x0 - reach).max(0), (y0 - reach).max(0));
        let (wx1, wy1) = ((x1 + reach).min(self.side), (y1 + reach).min(self.side));
        // Cells that may open or shut: links, portals and areas read only
        // whether a cell is open (clearance of at least CLEARANCE) and its
        // cost, so a cell further than that from the change can't have
        // moved them, whatever its clearance did.
        let margin = i32::from(CLEARANCE);
        let (dx0, dy0) = ((x0 - margin).max(0), (y0 - margin).max(0));
        let (dx1, dy1) = ((x1 + margin).min(self.side), (y1 + margin).min(self.side));
        let near = [dx0, dy0, dx1, dy1];
        let was_open: Vec<bool> = self.cells_in(near).map(|c| self.open_cell(c)).collect();
        self.measure_clearance_in(wx0, wy0, wx1, wy1);
        if !self.renumber_near(near, &was_open) {
            self.number_areas();
        }
        // Sectors a cell that may have opened or shut lies in.
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        for sy in dy0 / SECTOR..=(dy1 - 1) / SECTOR {
            for sx in dx0 / SECTOR..=(dx1 - 1) / SECTOR {
                dirty[(sy * self.sectors + sx) as usize] = true;
            }
        }
        let old = OldGraph::take(self);
        let partner = self.place_portals();
        // A sector whose portals moved (a neighbour's edge changed) is
        // linked again too.
        for (s, d) in dirty.iter_mut().enumerate() {
            if !*d && !old.cells_of(s).eq(self.cells_of(s)) {
                *d = true;
            }
        }
        self.relink(&partner, &old, &dirty, search);
        search.fit(self);
    }

    /// Chessboard distance from each cell in the window `x0..x1` × `y0..y1`
    /// to the nearest blocked cell (the ground beyond the map edge counts
    /// as blocked), up to [`CLEAR_CAP`], in two sweeps, reading the cells
    /// just outside the window as they stand. Over the whole map this is
    /// the grid's clearance; over a window it is exact as long as every
    /// cell outside it is more than [`CLEAR_CAP`] from the cells that
    /// changed.
    pub(super) fn measure_clearance_in(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        for y in y0..y1 {
            for x in x0..x1 {
                let i = self.idx(x, y) as usize;
                self.clear[i] = if self.cost[i] == BLOCKED {
                    0
                } else {
                    CLEAR_CAP
                };
            }
        }
        let side = self.side;
        let whole = x0 == 0 && y0 == 0 && x1 == side && y1 == side;
        let at = |clear: &[u8], x: i32, y: i32| -> u8 {
            if x < 0 || y < 0 || x >= side || y >= side {
                0
            } else {
                clear[(y * side + x) as usize]
            }
        };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y * side + x) as usize;
                if self.clear[i] == 0 {
                    continue;
                }
                let c = &self.clear;
                let mut d = at(c, x - 1, y)
                    .min(at(c, x - 1, y - 1))
                    .min(at(c, x, y - 1))
                    .min(at(c, x + 1, y - 1));
                // In a window, every neighbour, so the cells just outside it
                // on the far sides are read too (over the whole map there
                // are none, and the cells ahead are still to be measured).
                if !whole {
                    d = d
                        .min(at(c, x + 1, y))
                        .min(at(c, x + 1, y + 1))
                        .min(at(c, x, y + 1))
                        .min(at(c, x - 1, y + 1));
                }
                self.clear[i] = self.clear[i].min(d.saturating_add(1).min(CLEAR_CAP));
            }
        }
        for y in (y0..y1).rev() {
            for x in (x0..x1).rev() {
                let i = (y * side + x) as usize;
                if self.clear[i] == 0 {
                    continue;
                }
                let c = &self.clear;
                let d = at(c, x + 1, y)
                    .min(at(c, x + 1, y + 1))
                    .min(at(c, x, y + 1))
                    .min(at(c, x - 1, y + 1));
                self.clear[i] = self.clear[i].min(d.saturating_add(1).min(CLEAR_CAP));
            }
        }
    }

    /// The cells of the box `[x0, y0, x1, y1]` (end exclusive), row by row.
    fn cells_in(&self, [x0, y0, x1, y1]: [i32; 4]) -> impl Iterator<Item = u32> + '_ {
        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| self.idx(x, y)))
    }

    /// Bring the areas up to date after only cells in `near` (a box) may
    /// have opened or shut, `was_open` saying which were open before, by
    /// looking only in a box [`SECTOR`] cells wider: the open cells there
    /// are joined into pieces, and each piece takes the area its cells
    /// had. A piece of new ground takes a new number; an area cut in two
    /// inside the box gives the piece cut off a new number. Returns false,
    /// changing nothing, when that can't be told from the box alone (two
    /// areas joined, or an area cut where both sides reach out of the box);
    /// then the whole map must be numbered again. Areas are only ever
    /// compared, so their numbers may differ from a fresh numbering's.
    pub(super) fn renumber_near(&mut self, near: [i32; 4], was_open: &[bool]) -> bool {
        let r = SECTOR;
        let [nx0, ny0, nx1, ny1] = near;
        let (bx0, by0) = ((nx0 - r).max(0), (ny0 - r).max(0));
        let (bx1, by1) = ((nx1 + r).min(self.side), (ny1 + r).min(self.side));
        let (w, h) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
        let in_box = |x: i32, y: i32| x >= bx0 && y >= by0 && x < bx1 && y < by1;
        // Cells that shut have no area; cells that opened have none yet.
        let side = self.side;
        let near_cells = (ny0..ny1).flat_map(|y| (nx0..nx1).map(move |x| (y * side + x) as usize));
        for (c, &was) in near_cells.zip(was_open) {
            if was != (self.clear[c] >= CLEARANCE) {
                self.area[c] = 0;
            }
        }
        // Join the box's open cells into pieces (along rows and columns, as
        // areas join). For each piece: its cells, the areas they had, and
        // whether it reaches the edge of the box (and so may go on beyond).
        let mut piece = vec![u32::MAX; w * h];
        let mut cells: Vec<u32> = Vec::new();
        // Per piece: the range of `cells` it holds, and whether it reaches
        // out of the box. Then (area, piece) for each piece's old area.
        let mut pieces: Vec<(usize, usize, bool)> = Vec::new();
        let mut had: Vec<(u32, usize)> = Vec::new();
        for y in by0..by1 {
            for x in bx0..bx1 {
                let local = (y - by0) as usize * w + (x - bx0) as usize;
                if piece[local] != u32::MAX || !self.open(x, y) {
                    continue;
                }
                let p = pieces.len();
                let start = cells.len();
                piece[local] = p as u32;
                cells.push(self.idx(x, y));
                let mut head = start;
                // Whether it reaches a side of the box that isn't the map's
                // edge, and so may go on beyond.
                let mut edge = false;
                while head < cells.len() {
                    let (cx, cy) = self.xy(cells[head]);
                    head += 1;
                    edge |= (cx == bx0 && bx0 > 0)
                        || (cy == by0 && by0 > 0)
                        || (cx == bx1 - 1 && bx1 < self.side)
                        || (cy == by1 - 1 && by1 < self.side);
                    for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (mx, my) = (cx + dx, cy + dy);
                        if !in_box(mx, my) || !self.open(mx, my) {
                            continue;
                        }
                        let m = (my - by0) as usize * w + (mx - bx0) as usize;
                        if piece[m] == u32::MAX {
                            piece[m] = p as u32;
                            cells.push(self.idx(mx, my));
                        }
                    }
                }
                let mut areas: Vec<u32> = cells[start..]
                    .iter()
                    .map(|&c| self.area[c as usize])
                    .filter(|&a| a != 0)
                    .collect();
                areas.sort_unstable();
                areas.dedup();
                if areas.len() > 1 {
                    return false; // two areas joined
                }
                had.extend(areas.first().map(|&a| (a, p)));
                pieces.push((start, cells.len(), edge));
            }
        }
        // Each piece's area: the one its cells had, unless that area is
        // split between pieces, when all but one take new numbers (the one
        // reaching out of the box keeps it; two reaching out can't be told
        // apart here).
        let mut number: Vec<u32> = vec![0; pieces.len()];
        let mut fresh: Vec<usize> = Vec::new();
        had.sort_unstable();
        for run in had.chunk_by(|a, b| a.0 == b.0) {
            let reaching = run.iter().filter(|&&(_, p)| pieces[p].2).count();
            if reaching > 1 {
                return false;
            }
            let keeper = run.iter().find(|&&(_, p)| pieces[p].2).unwrap_or(&run[0]).1;
            for &(a, p) in run {
                if p == keeper {
                    number[p] = a;
                } else {
                    fresh.push(p);
                }
            }
        }
        // Pieces of new ground, with no area yet, take new numbers too.
        let new_ground: Vec<usize> = (0..pieces.len())
            .filter(|&p| number[p] == 0 && !fresh.contains(&p))
            .collect();
        fresh.extend(new_ground);
        if !fresh.is_empty() {
            let mut next = self.area.iter().copied().max().unwrap_or(0);
            fresh.sort_unstable();
            for p in fresh {
                next += 1;
                number[p] = next;
            }
        }
        for (p, &(start, end, _)) in pieces.iter().enumerate() {
            for &c in &cells[start..end] {
                self.area[c as usize] = number[p];
            }
        }
        true
    }

    /// What [`Grid::label_areas`] gives, numbered the same way, but in two
    /// passes over the map instead of a flood fill. A step on the diagonal
    /// needs both squares beside it open, so areas join along rows and
    /// columns only: the first pass gives each open cell the label of the
    /// cell behind or above it (a new one if both are shut), noting labels
    /// that meet as one; the second numbers each area by the first cell
    /// met in it, as the flood fill does.
    pub(super) fn number_areas(&mut self) {
        let side = self.side as usize;
        // Each label's parent, never above the label: a root is its own.
        let mut parent: Vec<u32> = vec![0];
        let root = |parent: &mut [u32], mut l: u32| {
            while parent[l as usize] != l {
                let up = parent[parent[l as usize] as usize];
                parent[l as usize] = up;
                l = up;
            }
            l
        };
        for y in 0..side {
            let row = y * side;
            for x in 0..side {
                let i = row + x;
                if self.clear[i] < CLEARANCE {
                    self.area[i] = 0;
                    continue;
                }
                let behind = if x > 0 { self.area[i - 1] } else { 0 };
                let above = if y > 0 { self.area[i - side] } else { 0 };
                self.area[i] = match (behind, above) {
                    (0, 0) => {
                        parent.push(parent.len() as u32);
                        parent.len() as u32 - 1
                    }
                    (l, 0) | (0, l) => l,
                    (a, b) => {
                        if a != b {
                            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                            parent[ra.max(rb) as usize] = ra.min(rb);
                        }
                        a
                    }
                };
            }
        }
        // Flatten (a parent is never above its label), then number roots
        // in the order the map first meets them.
        for l in 1..parent.len() {
            parent[l] = parent[parent[l] as usize];
        }
        let mut number = vec![0u32; parent.len()];
        let mut next = 0;
        for a in &mut self.area {
            if *a == 0 {
                continue;
            }
            let r = parent[*a as usize] as usize;
            if number[r] == 0 {
                next += 1;
                number[r] = next;
            }
            *a = number[r];
        }
    }

    /// The node cells of sector `s`, in order.
    fn cells_of(&self, s: usize) -> impl Iterator<Item = u32> + '_ {
        self.nodes_in(s).iter().map(|&n| self.node_cell[n as usize])
    }

    /// [`Grid::link_portals`], searching only in `dirty` sectors and copying
    /// the links of every other sector from `old`.
    fn relink(&mut self, partner: &[u32], old: &OldGraph, dirty: &[bool], search: &mut Search) {
        let nodes = self.node_cell.len();
        let place = places(&self.sector_first, &self.sector_nodes, nodes);
        let (mut first, mut to, mut cost) = (
            Vec::with_capacity(nodes + 1),
            Vec::with_capacity(old.edge_to.len()),
            Vec::with_capacity(old.edge_to.len()),
        );
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            let cell = self.node_cell[u];
            to.push(p);
            cost.push(self.step_cost(cell, self.node_cell[p as usize], false));
            let s = self.sector_of(cell) as usize;
            if dirty[s] {
                self.link_in_sector(search, u, &mut to, &mut cost);
            } else {
                // The same node, by its place in the sector, in the old
                // graph; its first edge was to its partner.
                let was = old.sector_nodes[old.sector_first[s] as usize + place[u] as usize];
                let nodes_here = self.nodes_in(s);
                for e in old.edge_first[was as usize] + 1..old.edge_first[was as usize + 1] {
                    let v = old.edge_to[e as usize];
                    to.push(nodes_here[old.place[v as usize] as usize]);
                    cost.push(old.edge_cost[e as usize]);
                }
            }
            first.push(to.len() as u32);
        }
        self.edge_first = first;
        self.edge_to = to;
        self.edge_cost = cost;
    }
}

/// Each node's place in its sector's list, from a graph's sector lists.
fn places(sector_first: &[u32], sector_nodes: &[u32], nodes: usize) -> Vec<u32> {
    let mut place = vec![0; nodes];
    for w in sector_first.windows(2) {
        for (k, &n) in sector_nodes[w[0] as usize..w[1] as usize]
            .iter()
            .enumerate()
        {
            place[n as usize] = k as u32;
        }
    }
    place
}

/// The portal graph as it stood before a change.
struct OldGraph {
    node_cell: Vec<u32>,
    sector_first: Vec<u32>,
    sector_nodes: Vec<u32>,
    edge_first: Vec<u32>,
    edge_to: Vec<u32>,
    edge_cost: Vec<u32>,
    /// Each node's place in its sector's list.
    place: Vec<u32>,
}

impl OldGraph {
    /// The grid's portal graph, leaving it empty for placing again.
    fn take(grid: &mut Grid) -> OldGraph {
        let sector_first = core::mem::take(&mut grid.sector_first);
        let sector_nodes = core::mem::take(&mut grid.sector_nodes);
        let node_cell = core::mem::take(&mut grid.node_cell);
        OldGraph {
            place: places(&sector_first, &sector_nodes, node_cell.len()),
            node_cell,
            sector_first,
            sector_nodes,
            edge_first: core::mem::take(&mut grid.edge_first),
            edge_to: core::mem::take(&mut grid.edge_to),
            edge_cost: core::mem::take(&mut grid.edge_cost),
        }
    }

    fn cells_of(&self, s: usize) -> impl Iterator<Item = u32> + '_ {
        self.sector_nodes[self.sector_first[s] as usize..self.sector_first[s + 1] as usize]
            .iter()
            .map(|&n| self.node_cell[n as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nav::CELLS_PER_TILE;
    use crate::scenario::ribs_map;
    use gc_fixed::Pcg32;

    /// The same cells in the same areas, whatever the areas' numbers.
    fn same_areas(a: &Grid, b: &Grid) {
        let mut to_b = alloc::collections::BTreeMap::new();
        let mut to_a = alloc::collections::BTreeMap::new();
        for (i, (&x, &y)) in a.area.iter().zip(&b.area).enumerate() {
            assert_eq!(x == 0, y == 0, "open differs at cell {i}");
            if x != 0 {
                assert_eq!(*to_b.entry(x).or_insert(y), y, "areas differ at cell {i}");
                assert_eq!(*to_a.entry(y).or_insert(x), x, "areas differ at cell {i}");
            }
        }
    }

    /// Every part of the grid that routes read.
    fn same(a: &Grid, b: &Grid) {
        assert!(a.cost == b.cost, "costs differ");
        for (i, (x, y)) in a.clear.iter().zip(&b.clear).enumerate() {
            assert_eq!(x, y, "clearance differs at cell {i}");
        }
        same_areas(a, b);
        assert_eq!(a.node_cell, b.node_cell, "portals differ");
        assert_eq!(a.sector_first, b.sector_first);
        assert_eq!(a.sector_nodes, b.sector_nodes);
        assert_eq!(a.edge_first, b.edge_first, "links differ");
        assert_eq!(a.edge_to, b.edge_to, "links differ");
        assert_eq!(a.edge_cost, b.edge_cost, "link costs differ");
    }

    #[test]
    fn a_changed_grid_is_the_grid_built_afresh() {
        let tiles: i32 = 96;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        let mut build_on = |g: &mut Grid, b, on| {
            assert!(g.set_built(b, on, &mut search).is_some());
        };
        let mut rng = Pcg32::new(11, 1);
        let mut standing: Vec<[i32; 4]> = Vec::new();
        for step in 0..40 {
            // Mostly put up, sometimes pull one down; some by the map edge,
            // some across sector edges, some walls a tile thick.
            if standing.len() > 3 && rng.below(3) == 0 {
                let k = rng.below(standing.len() as u32) as usize;
                let b = standing.swap_remove(k);
                build_on(&mut g, b, false);
            } else {
                let (w, d) = if step % 5 == 0 {
                    (1, 2 + rng.below(6) as i32)
                } else {
                    (2 + rng.below(4) as i32, 2 + rng.below(4) as i32)
                };
                let x = rng.below((tiles - w) as u32) as i32;
                let y = rng.below((tiles - d) as u32) as i32;
                standing.push([x, y, w, d]);
                build_on(&mut g, [x, y, w, d], true);
            }
            let mut fresh = Grid::build(&g.cost, side);
            fresh.ground.clone_from(&g.ground);
            fresh.built.clone_from(&g.built);
            same(&g, &fresh);
        }
        // Everything pulled down: the map as it began.
        for b in standing.drain(..) {
            build_on(&mut g, b, false);
        }
        same(&g, &Grid::build(&ground, side));
    }

    #[test]
    fn areas_are_numbered_as_the_flood_fill_numbers_them() {
        // Speckled ground: many small areas, and labels that meet late
        // (a U whose arms join at the bottom).
        let side = 96;
        let mut rng = Pcg32::new(5, 2);
        for blocked in [20, 40, 55] {
            let cost: Vec<u8> = (0..side * side)
                .map(|_| if rng.below(100) < blocked { BLOCKED } else { 1 })
                .collect();
            let mut g = Grid::build(&cost, side);
            let flooded = g.area.clone();
            g.area.fill(7);
            g.number_areas();
            assert!(g.area == flooded, "{blocked}% blocked");
        }
    }

    /// Put up (or pull down) a building as [`Grid::set_built`] does, but
    /// bring only clearance and areas up to date, and say whether the areas
    /// could be told from near the building alone.
    fn renumbered_near(g: &mut Grid, tiles: [i32; 4], on: bool) -> bool {
        let [x0, y0, x1, y1] = g.cells(tiles).unwrap();
        let margin = i32::from(CLEARANCE);
        let near = [
            (x0 - margin).max(0),
            (y0 - margin).max(0),
            (x1 + margin).min(g.side),
            (y1 + margin).min(g.side),
        ];
        let was_open: Vec<bool> = g.cells_in(near).map(|c| g.open_cell(c)).collect();
        for c in g.cells_in([x0, y0, x1, y1]).collect::<Vec<_>>() {
            let c = c as usize;
            g.built[c] = if on { g.built[c] + 1 } else { g.built[c] - 1 };
            g.cost[c] = if g.built[c] > 0 { BLOCKED } else { g.ground[c] };
        }
        g.measure_clearance_in(0, 0, g.side, g.side);
        let near_only = g.renumber_near(near, &was_open);
        if !near_only {
            g.number_areas();
        }
        let fresh = Grid::build(&g.cost, g.side);
        same_areas(g, &fresh);
        near_only
    }

    #[test]
    fn areas_are_renumbered_near_a_building_when_they_can_be() {
        let tiles = 64;
        let open = vec![1u8; (tiles * CELLS_PER_TILE * tiles * CELLS_PER_TILE) as usize];
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        // A building in the open: nothing cut off.
        assert!(renumbered_near(&mut g, [30, 30, 2, 2], true));
        // A ring of walls round a pocket in the middle of the map: the last
        // wall cuts the pocket off, inside the box looked in.
        let ring = [[20, 20, 8, 1], [20, 27, 8, 1], [20, 21, 1, 6]];
        for wall in ring {
            assert!(renumbered_near(&mut g, wall, true));
        }
        assert!(
            renumbered_near(&mut g, [27, 21, 1, 6], true),
            "pocket cut off"
        );
        // Pulling a wall down joins two areas: the whole map is numbered.
        assert!(!renumbered_near(&mut g, [27, 21, 1, 6], false));
        // A wall right across the map: the last piece cuts it in two, and
        // both sides reach out of the box.
        for x in (0..tiles - 8).step_by(8) {
            assert!(renumbered_near(&mut g, [x, 50, 8, 1], true));
        }
        assert!(
            !renumbered_near(&mut g, [tiles - 8, 50, 8, 1], true),
            "map cut in two"
        );
        assert!(
            !renumbered_near(&mut g, [0, 50, 8, 1], false),
            "halves joined"
        );
    }
}
