//! Ground that changes mid-match: buildings put up and pulled down, and
//! gates opening and shutting (see [`crate::structures`]). A change blocks
//! or frees a rectangle of cells;
//! the walkable grid is brought up to date around it instead of being
//! built again, so a building going up costs a few milliseconds, not the
//! whole map's worth of searches:
//!
//! - **Clearance** is measured again only within [`CLEAR_CAP`] cells of the
//!   change: past that, clearances are capped and can't have moved.
//! - **Areas** are numbered again only in a box a sector wider than the
//!   change, when that box can tell (a new piece of ground, or an area cut
//!   in two where at most one side runs on out of the box). Otherwise
//!   (two areas joined, or a wall cut an area right across, as a gate in
//!   a town wall does each time it swings) floods race out of the box,
//!   one cell each a turn, until all but one have run out of ground (see
//!   [`Grid::renumber_far`]): that costs about the second biggest area's
//!   cells, the town's rather than the map's. Only ground that both
//!   opened and shut in one change, or numbers past 16 bits, number the
//!   whole map again, in two passes (see [`Grid::number_areas`]).
//! - **Portals** are placed again only on sector edges within
//!   [`CLEAR_CAP`] of the change (a portal sits where clearance is
//!   greatest, so no other edge's can move), and linked again only in the
//!   sectors where a cell may have opened or shut (within [`CLEARANCE`] of
//!   the change), or whose portals moved because a neighbour's did: every
//!   other edge's portals and sector's links are copied across. A sector
//!   linked on one side's grid in a change is not searched again on
//!   another's whose sector came out the same (see `Search::linked`).
//! - **Each side's grid** (see [`Nav`]) shares every run of cells it holds
//!   alike with the shared grid (see [`super::cells`]): after a change,
//!   runs that came out the same are shared again, and every grid gives
//!   out new area numbers from the same one so that they can.
//!
//! The result has the same areas, portals and links as building the grid
//! from scratch (tests check), though an area's number may differ, as
//! areas are only ever compared. So a match's paths never depend on the
//! order its buildings went up in. This runs inside a tick but only when
//! the ground changes, and it may allocate (the portal graph can grow).

use alloc::vec;
use alloc::vec::Vec;
use core::ops::Range;

use alloc::sync::Arc;
use gc_fixed::{Fx, FxVec2};

use super::cells::Cells;
use super::{
    BLOCKED, CELLS_PER_TILE, CLEAR_CAP, CLEARANCE, DECK, FORD, GRAPH_BERTH, Grid, Nav, OPEN, Route,
    SECTOR, Search,
};
use crate::PlayerId;
use crate::regiments::Regiments;

/// Buildings put up or pulled down together whose footprints fit in a box
/// this many cells across are brought up to date as one change: a whole
/// base razed at once costs about one building's work, not one per
/// building (most of a change's cost is the same whatever its size).
const GROUP: i32 = 4 * SECTOR;

/// Whether a gate lets its owner's side through, and everyone else (see
/// [`Nav::set_door`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Door {
    pub(crate) own: bool,
    pub(crate) others: bool,
}

/// Each cell's area, as [`Grid::area`] holds it: in 16 bits while every
/// number fits, as on any real map (the 384-tile grand battle's has a few
/// hundred areas), and in 32 on ground cut into more pieces than that. The
/// grid's cells are the biggest part of the simulation's memory, and these
/// were half of them. Numbers are only ever compared, so which way they
/// are held never changes a route. They stay narrow because numbering
/// near a change refuses to give out a number past 16 bits (the whole map
/// is numbered again from 1 instead), and a full numbering narrows them
/// back when it can.
#[derive(Clone)]
pub(super) enum Areas {
    Narrow(Cells<u16>),
    Wide(Cells<u32>),
}

impl Areas {
    /// `cells` cells, none in an area yet.
    pub(super) fn new(cells: usize) -> Areas {
        Areas::Narrow(Cells::new(cells, 0))
    }

    #[inline]
    pub(super) fn get(&self, c: usize) -> u32 {
        match self {
            Areas::Narrow(a) => u32::from(a[c]),
            Areas::Wide(a) => a[c],
        }
    }

    /// Put cell `c` in area `n`, widening the cells if `n` doesn't fit.
    #[inline]
    pub(super) fn set(&mut self, c: usize, n: u32) {
        match self {
            Areas::Narrow(a) => match u16::try_from(n) {
                Ok(n) => a.set(c, n),
                Err(_) => self.widen_to_set(c, n),
            },
            Areas::Wide(a) => a.set(c, n),
        }
    }

    /// Hold the cells in 32 bits and put cell `c` in area `n`. This
    /// allocates, but no caller gives out such a number: numbering near a
    /// change checks [`Areas::holds`] first, and [`Grid::number_areas`]
    /// picks the width itself.
    #[cold]
    #[inline(never)]
    fn widen_to_set(&mut self, c: usize, n: u32) {
        if let Areas::Narrow(a) = self {
            let mut wide: Vec<u32> = a.iter().map(|&x| u32::from(x)).collect();
            wide[c] = n;
            *self = Areas::Wide(Cells::from_slice(&wide));
        }
    }

    /// Whether numbers up to `n` fit without widening.
    pub(super) fn holds(&self, n: u32) -> bool {
        matches!(self, Areas::Wide(_)) || n <= u32::from(u16::MAX)
    }

    /// Share with `other` the runs of cells that came out alike (see
    /// [`Cells::share_from`]).
    pub(super) fn share_from(&mut self, other: &Areas) {
        match (self, other) {
            (Areas::Narrow(a), Areas::Narrow(b)) => a.share_from(b),
            (Areas::Wide(a), Areas::Wide(b)) => a.share_from(b),
            (Areas::Narrow(a), _) => a.settle(),
            (Areas::Wide(a), _) => a.settle(),
        }
    }

    /// Whether [`Areas::share_from`] would share a run it doesn't yet.
    pub(super) fn shares_more_with(&self, other: &Areas) -> bool {
        match (self, other) {
            (Areas::Narrow(a), Areas::Narrow(b)) => a.shares_more_with(b),
            (Areas::Wide(a), Areas::Wide(b)) => a.shares_more_with(b),
            _ => false,
        }
    }

    pub(super) fn settle(&mut self) {
        match self {
            Areas::Narrow(a) => a.settle(),
            Areas::Wide(a) => a.settle(),
        }
    }

    pub(super) fn written(&self) -> bool {
        match self {
            Areas::Narrow(a) => a.written(),
            Areas::Wide(a) => a.written(),
        }
    }

    /// Every cell's area, in order.
    #[cfg(test)]
    pub(super) fn all(&self) -> Vec<u32> {
        match self {
            Areas::Narrow(a) => a.iter().map(|&x| u32::from(x)).collect(),
            Areas::Wide(a) => a.iter().copied().collect(),
        }
    }
}

/// [`Grid::number_areas`]'s two passes over cells `area`, `side` across,
/// whose clearances are `clear`: the number of areas, or `None` if the
/// first pass's labels outgrew `T` (the cells are then left half done).
fn number_cells<T>(clear: &Cells<u8>, area: &mut [T], side: usize) -> Option<u32>
where
    T: Copy + Into<u32> + TryFrom<u32>,
{
    let zero = T::try_from(0).ok()?;
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
            if clear[i] < CLEARANCE {
                area[i] = zero;
                continue;
            }
            let behind = if x > 0 { area[i - 1].into() } else { 0 };
            let above = if y > 0 { area[i - side].into() } else { 0 };
            let label = match (behind, above) {
                (0, 0) => {
                    parent.push(parent.len() as u32);
                    parent.len() as u32 - 1
                }
                (l, 0) | (0, l) => l,
                (a, b) if a == b => a,
                (a, b) => {
                    // Labels the root, so the cells after it in the run
                    // meet the same label above them and skip this.
                    let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
                    parent[ra.max(rb) as usize] = ra.min(rb);
                    ra.min(rb)
                }
            };
            area[i] = T::try_from(label).ok()?;
        }
    }
    // Flatten (a parent is never above its label), then number roots in
    // the order the map first meets them (no number is above its label,
    // so each fits where the label did).
    for l in 1..parent.len() {
        parent[l] = parent[parent[l] as usize];
    }
    let mut number = vec![0u32; parent.len()];
    let mut next = 0;
    for a in area.iter_mut() {
        let l: u32 = (*a).into();
        if l == 0 {
            continue;
        }
        let r = parent[l as usize] as usize;
        if number[r] == 0 {
            next += 1;
            number[r] = next;
        }
        *a = T::try_from(number[r]).ok()?;
    }
    Some(next)
}

/// Numbers a grid keeps free above those it gives out from, when the
/// grids give them out from the same one (see [`Nav::on_every_grid`]).
const ROOM: u32 = 4096;

/// The set `k` is in, as its first member (halving the way up as it goes):
/// each member of `set` holds one nearer the set's first, itself for that.
fn root(set: &mut [usize], mut k: usize) -> usize {
    while set[k] != k {
        set[k] = set[set[k]];
        k = set[k];
    }
    k
}

/// A change to a rectangle of cells.
#[derive(Clone, Copy)]
enum Change {
    /// A building put up (`true`) or pulled down.
    Built(bool),
    /// A door opened (`true`) or shut, on a building already standing.
    Opened(bool),
    /// Ground shut by a flood (`true`), or open again.
    Flooded(bool),
    /// Deep water made wadeable by low water (`true`), or deep again.
    Exposed(bool),
    /// A bridge tile finished over it (`true`), or fallen.
    Bridged(bool),
}

impl Nav {
    /// Put up (`on`) or pull down a building on each of `all`'s tiles
    /// (`[x, y, w, d]`, all on the map), on every side's grid, bringing
    /// each up to date once for each group of them close together. Routes
    /// that might cross the changed ground are searched again. No effect
    /// with no map.
    pub(crate) fn set_built_all(&mut self, all: &[[i32; 4]], on: bool, regs: &Regiments) {
        self.on_every_grid(|nav, v, from| nav.change(v, all, Change::Built(on), from, regs));
    }

    /// Shut (`on`) the ground on each of `all`'s tiles to everyone, as a
    /// flood shuts the fords (see [`crate::flood`]), or open it again, on
    /// every side's grid. A tile stays shut while anything stands on it.
    pub(crate) fn set_flooded_all(&mut self, all: &[[i32; 4]], on: bool, regs: &Regiments) {
        self.on_every_grid(|nav, v, from| nav.change(v, all, Change::Flooded(on), from, regs));
    }

    /// Make the deep water on each of `all`'s tiles wadeable (`on`), as low
    /// water does (see [`crate::flood`]), or deep again, on every side's
    /// grid. Cells the map gave as walkable are left as they are.
    pub(crate) fn set_exposed_all(&mut self, all: &[[i32; 4]], on: bool, regs: &Regiments) {
        self.on_every_grid(|nav, v, from| nav.change(v, all, Change::Exposed(on), from, regs));
    }

    /// Open (`on`) each of `all`'s tiles as a finished bridge's deck, over
    /// water or a flood, on every side's grid, or shut it again as the
    /// bridge falls (see [`crate::bridges`]).
    pub(crate) fn set_bridged_all(&mut self, all: &[[i32; 4]], on: bool, regs: &Regiments) {
        self.on_every_grid(|nav, v, from| nav.change(v, all, Change::Bridged(on), from, regs));
    }

    /// Freeze the fords (`on`), so routes cost them as open ground, or
    /// thaw them, on every side's grid (see
    /// [`crate::terrain::Terrain::frozen`]). Only costs change, so the
    /// portals stay put and only routes passing through a sector with a
    /// ford are found again.
    pub(crate) fn set_frozen(&mut self, on: bool, regs: &Regiments) {
        self.on_every_grid(|nav, v, _| {
            if nav.grids[v].frozen == on {
                return;
            }
            let work = nav.search.work;
            let changed = Arc::make_mut(&mut nav.grids[v]).reprice(on, &mut nav.search);
            nav.search.work = work;
            nav.reroute_near(v, &changed, 0, false, regs);
        });
    }

    /// The going on each of `tiles` (`(x, y, percent)`, those on the map)
    /// is now as given (see [`super::going_of`]): woods cleared or burnt
    /// out. Only costs change, so the portals stay put and only routes
    /// passing near the changed tiles are found again.
    pub(crate) fn set_going(&mut self, tiles: &[(i32, i32, u8)], regs: &Regiments) {
        self.on_every_grid(|nav, v, _| {
            let work = nav.search.work;
            let changed = Arc::make_mut(&mut nav.grids[v]).regoing(tiles, &mut nav.search);
            nav.search.work = work;
            nav.reroute_near(v, &changed, 0, false, regs);
        });
    }

    /// The gate standing on `tiles` (put up with [`Nav::set_built`], so a
    /// wall to everyone until now), owned by `owner`, goes from letting
    /// through as `was` says to as `now` does. From the first gate that
    /// lets its owner's side through but not everyone, that side walks a
    /// grid of its own (see [`Nav`]).
    pub(crate) fn set_door(
        &mut self,
        tiles: [i32; 4],
        owner: PlayerId,
        was: Door,
        now: Door,
        regs: &Regiments,
    ) {
        let Some(&side) = self.side.get(usize::from(owner)) else {
            return;
        };
        if self.grids.is_empty() || was == now {
            return;
        }
        let sharing =
            (0..self.view.len()).any(|p| self.view[p] == 0 && self.parties & !side & 1 << p != 0);
        if now.own != now.others && self.view[usize::from(owner)] == 0 && sharing {
            // A copy of the shared grid, where the owner's side's gates
            // stand as for everyone (this is its first that doesn't). The
            // last side left on the shared grid keeps it as its own.
            let own = Arc::new(Grid::clone(&self.grids[0]));
            self.grids.push(own);
            let v = (self.grids.len() - 1) as u8;
            for p in 0..self.view.len() {
                if side & 1 << p != 0 {
                    self.view[p] = v;
                }
            }
        }
        let own = usize::from(self.view[usize::from(owner)]);
        self.on_every_grid(|nav, v, from| {
            let pick = |d: Door| if v == own { d.own } else { d.others };
            if pick(was) != pick(now) {
                nav.change(v, &[tiles], Change::Opened(pick(now)), from, regs);
            }
        });
    }

    /// Make a change on every grid, `each` making it on grid `v` (and
    /// given the area number the grids give out new ones from). Every
    /// grid gives them out from the same one, so ground that changes alike
    /// on two grids is numbered alike; and the search keeps each sector it
    /// links meanwhile, so the grids that change alike link it once (see
    /// `Search::linked`). After, each side's grid shares again with the
    /// first every run of cells that came out alike on both (see
    /// [`super::cells`]), so a side's own grid keeps only the cells round
    /// its own gates.
    fn on_every_grid(&mut self, mut each: impl FnMut(&mut Nav, usize, u32)) {
        let from = self.grids.iter().map(|g| g.areas).max().unwrap_or(0);
        if self.grids.len() > 1 {
            self.search.linked = Some(Vec::new());
        }
        for v in 0..self.grids.len() {
            each(self, v, from);
        }
        self.search.linked = None;
        let Some((first, rest)) = self.grids.split_first_mut() else {
            return;
        };
        for g in rest {
            if g.shares_more_with(first) {
                Arc::make_mut(g).share_from(first);
            } else if g.written() {
                Arc::make_mut(g).settle();
            }
        }
        if first.written() {
            Arc::make_mut(first).settle();
        }
    }

    /// Make `change` on each of `all`'s tiles on grid `v`, rerouting the
    /// regiments walking it whose routes pass near. New areas are numbered
    /// on from `from`.
    fn change(&mut self, v: usize, all: &[[i32; 4]], change: Change, from: u32, regs: &Regiments) {
        // Linking the grid again isn't the routes' search work this tick.
        let work = self.search.work;
        let grid = Arc::make_mut(&mut self.grids[v]);
        // Not so near the most its cells hold that it would have to number
        // the whole map for want of new numbers (and so again each time).
        if grid.area.holds(from.saturating_add(ROOM)) {
            grid.areas = grid.areas.max(from);
        }
        let changed = grid.change_all(all, change, &mut self.search);
        self.search.work = work;
        let freed = match change {
            Change::Built(on) | Change::Flooded(on) => !on,
            Change::Bridged(on) | Change::Exposed(on) => on,
            Change::Opened(open) => open,
        };
        // Any route passing near a change is found again (walking the old
        // way meanwhile: if the change blocks it, the regiment stops at
        // the edge and looks again then), and, once ground is freed, any
        // that found no way may find one now. Clearance may have changed
        // this far round it.
        self.reroute_near(v, &changed, i32::from(CLEAR_CAP), freed, regs);
    }

    /// Find again the routes on grid `v` passing within `reach` cells of
    /// any of the boxes of cells `changed`, and, if ground was `freed`,
    /// look again for those that found no way.
    fn reroute_near(
        &mut self,
        v: usize,
        changed: &[[i32; 4]],
        reach: i32,
        freed: bool,
        regs: &Regiments,
    ) {
        let tile = |c: i32| Fx::from_ratio(c, CELLS_PER_TILE);
        let near: Vec<(FxVec2, FxVec2)> = changed
            .iter()
            .map(|&[x0, y0, x1, y1]| {
                (
                    FxVec2::new(tile(x0 - reach), tile(y0 - reach)),
                    FxVec2::new(tile(x1 + reach), tile(y1 + reach)),
                )
            })
            .collect();
        if near.is_empty() {
            return;
        }
        for i in 0..self.route.len() {
            if !regs.alive[i] || usize::from(self.view[usize::from(regs.owner[i])]) != v {
                continue;
            }
            let ahead = match self.route[i] {
                Route::Direct => core::slice::from_ref(&self.goal[i]),
                Route::Waypoints => self.ahead(i),
                Route::Failed if freed => {
                    self.lost(i);
                    continue;
                }
                Route::Failed | Route::Idle | Route::Waiting | Route::Rerouting => continue,
            };
            let (mut a, mut b) = (regs.position[i], regs.position[i]);
            for p in ahead {
                a = FxVec2::new(a.x.min(p.x), a.y.min(p.y));
                b = FxVec2::new(b.x.max(p.x), b.y.max(p.y));
            }
            if near
                .iter()
                .any(|(lo, hi)| a.x <= hi.x && a.y <= hi.y && b.x >= lo.x && b.y >= lo.y)
            {
                self.reroute(i);
            }
        }
    }
}

/// Calls macro `$m` with the names of every layer of a grid's cells, so
/// what is done to each (sharing, settling) can't leave one out.
macro_rules! layers {
    ($m:ident) => {
        $m!(cost ground going built opened flooded exposed bridged clear area)
    };
}

/// `boxes` (cells, `[x0, y0, x1, y1]`, end exclusive) with every two that
/// overlap or touch joined into the box round both, until none do: the
/// grown boxes round a patch of tiles come out as one, so no cell in it
/// is visited twice.
fn merged(mut boxes: Vec<[i32; 4]>) -> Vec<[i32; 4]> {
    let mut joined = true;
    while joined {
        joined = false;
        let mut k = 0;
        while k < boxes.len() {
            let a = boxes[k];
            let touching = (k + 1..boxes.len()).find(|&j| {
                let b = boxes[j];
                a[0] <= b[2] && b[0] <= a[2] && a[1] <= b[3] && b[1] <= a[3]
            });
            if let Some(j) = touching {
                let b = boxes.swap_remove(j);
                boxes[k] = [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[2].max(b[2]),
                    a[3].max(b[3]),
                ];
                joined = true;
            } else {
                k += 1;
            }
        }
    }
    boxes
}

impl Grid {
    /// Put up or pull down a building on the tiles `x..x + w` × `y..y + d`
    /// and bring the grid up to date, searching in `search`'s room. Returns
    /// the cells changed; `None` (and no change) unless the tiles are on
    /// the map.
    #[cfg(test)]
    pub(super) fn set_built(
        &mut self,
        tiles: [i32; 4],
        on: bool,
        search: &mut Search,
    ) -> Option<[i32; 4]> {
        self.set_built_all(&[tiles], on, search).pop()
    }

    /// [`Grid::set_built`] for each of `all`'s tiles (those on the map),
    /// bringing the grid up to date once for each group of them that fits
    /// in a [`GROUP`]-wide box. Returns each group's box of changed cells.
    #[cfg(test)]
    pub(super) fn set_built_all(
        &mut self,
        all: &[[i32; 4]],
        on: bool,
        search: &mut Search,
    ) -> Vec<[i32; 4]> {
        self.change_all(all, Change::Built(on), search)
    }

    /// Make `change` on each of `all`'s tiles (those on the map), as
    /// [`Grid::set_built_all`] does.
    fn change_all(
        &mut self,
        all: &[[i32; 4]],
        change: Change,
        search: &mut Search,
    ) -> Vec<[i32; 4]> {
        // Each footprint's cells join the first group they fit in: the
        // group's box of cells, and its footprints' cells.
        let mut groups: Vec<([i32; 4], Vec<[i32; 4]>)> = Vec::new();
        for c in all.iter().filter_map(|&tiles| self.cells(tiles)) {
            let joined = |g: [i32; 4]| {
                [
                    g[0].min(c[0]),
                    g[1].min(c[1]),
                    g[2].max(c[2]),
                    g[3].max(c[3]),
                ]
            };
            let fits = |u: [i32; 4]| u[2] - u[0] <= GROUP && u[3] - u[1] <= GROUP;
            match groups.iter_mut().find(|(g, _)| fits(joined(*g))) {
                Some((g, feet)) => {
                    *g = joined(*g);
                    feet.push(c);
                }
                None => groups.push((c, vec![c])),
            }
        }
        if matches!(change, Change::Opened(_)) && self.opened.is_empty() {
            // Made on the first door, so maps without gates don't carry it.
            self.opened = Cells::new(self.built.len(), 0);
        }
        if matches!(change, Change::Flooded(_)) && self.flooded.is_empty() {
            self.flooded = Cells::new(self.built.len(), false);
        }
        if matches!(change, Change::Exposed(_)) && self.exposed.is_empty() {
            self.exposed = Cells::new(self.built.len(), false);
        }
        if matches!(change, Change::Bridged(_)) && self.bridged.is_empty() {
            self.bridged = Cells::new(self.built.len(), 0);
        }
        for (g, feet) in &groups {
            for &[cx0, cy0, cx1, cy1] in feet {
                for cy in cy0..cy1 {
                    for cx in cx0..cx1 {
                        let i = self.idx(cx, cy) as usize;
                        let bump = |count: &mut Cells<u8>, on: bool| {
                            let n = count[i];
                            count.set(
                                i,
                                if on {
                                    n.saturating_add(1)
                                } else {
                                    n.saturating_sub(1)
                                },
                            );
                        };
                        match change {
                            Change::Built(on) => bump(&mut self.built, on),
                            Change::Opened(open) => bump(&mut self.opened, open),
                            Change::Flooded(on) => self.flooded.set(i, on),
                            Change::Exposed(on) => self.exposed.set(i, on),
                            Change::Bridged(on) => bump(&mut self.bridged, on),
                        }
                        self.cost.set(i, self.cell_cost(i));
                    }
                }
            }
            let [x0, y0, x1, y1] = *g;
            self.changed(x0, y0, x1, y1, search);
        }
        groups.into_iter().map(|(g, _)| g).collect()
    }

    /// Share with `other` every run of cells written on either since they
    /// were last shared that came out alike (see [`Cells::share_from`]).
    fn share_from(&mut self, other: &Grid) {
        macro_rules! each { ($($l:ident)*) => { $(self.$l.share_from(&other.$l);)* } }
        layers!(each);
    }

    /// Whether [`Grid::share_from`] would share a run it doesn't yet.
    fn shares_more_with(&self, other: &Grid) -> bool {
        macro_rules! any { ($($l:ident)*) => { $(self.$l.shares_more_with(&other.$l))||* } }
        layers!(any)
    }

    /// Whether any run of cells was written since they were last shared.
    fn written(&self) -> bool {
        macro_rules! any { ($($l:ident)*) => { $(self.$l.written())||* } }
        layers!(any)
    }

    /// Start counting writes afresh.
    fn settle(&mut self) {
        macro_rules! each { ($($l:ident)*) => { $(self.$l.settle();)* } }
        layers!(each);
    }

    /// What cell `i` costs to walk as things stand: blocked under a
    /// building (unless a door lets this grid's side through) or a flood,
    /// open on a bridge's deck or a frozen ford, a ford on deep water low
    /// water has bared (summer fords, which stay deep under the ice),
    /// else the map's ground.
    fn cell_cost(&self, i: usize) -> u8 {
        let opened = self.opened.get(i).copied().unwrap_or(0);
        if self.built[i] > opened {
            BLOCKED
        } else if self.is_bridged(i) {
            super::price(DECK, 100)
        } else if self.is_flooded(i) {
            BLOCKED
        } else if self.ground[i] == BLOCKED && self.is_exposed(i) {
            self.priced(i, super::SHALLOWS)
        } else if self.frozen && self.ground[i] == FORD {
            self.priced(i, OPEN)
        } else {
            self.priced(i, self.ground[i])
        }
    }

    /// Set the going on each of `tiles` (`(x, y, percent)`) and bring the
    /// costs and links up to date, searching in `search`'s room. Returns
    /// the boxes of cells whose cost may have changed: each changed tile's
    /// grown by [`super::BODY_REACH`]. No change while the grid has no
    /// going and every tile keeps its whole pace.
    fn regoing(&mut self, tiles: &[(i32, i32, u8)], search: &mut Search) -> Vec<[i32; 4]> {
        let side = self.side / CELLS_PER_TILE;
        let on_map = |&&(x, y, _): &&(i32, i32, u8)| self.cells([x, y, 1, 1]).is_some();
        if self.going.is_empty() {
            if tiles.iter().filter(on_map).all(|&(_, _, g)| g == 100) {
                return Vec::new();
            }
            self.going = Cells::new((side * side) as usize, 100);
        }
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        let mut boxes = Vec::new();
        for &(x, y, g) in tiles {
            let Some(cells) = self.cells([x, y, 1, 1]) else {
                continue;
            };
            let t = (y * side + x) as usize;
            if self.going[t] == g {
                continue;
            }
            self.going.set(t, g);
            boxes.push(self.grow(cells, super::BODY_REACH));
        }
        // A cell's cost reads the going within reach of it (see
        // `Grid::block_going_at`), so every cell that near a changed tile
        // is priced again, once: a burnt-out patch is one box.
        let boxes = merged(boxes);
        for &cells in &boxes {
            for c in self.cells_in(cells).collect::<Vec<_>>() {
                let cost = self.cell_cost(c as usize);
                if cost != self.cost[c as usize] {
                    self.cost.set(c as usize, cost);
                    dirty[self.sector_of(c) as usize] = true;
                }
            }
        }
        if !dirty.contains(&true) {
            return Vec::new();
        }
        self.relink_costs(&dirty, search);
        boxes
    }

    /// Link the sectors marked `dirty` again after only cell costs changed
    /// (portals stay put), searching in `search`'s room.
    fn relink_costs(&mut self, dirty: &[bool], search: &mut Search) {
        let old = OldGraph::take(self);
        let partner = self.place_portals(Some((&old, [0, 0, 0, 0])));
        self.relink(&partner, &old, dirty, search);
        self.node_room = self.node_room();
        search.fit(self);
    }

    /// Freeze the fords (`on`) or thaw them and bring the links up to
    /// date, searching in `search`'s room. Only costs change (never
    /// whether a cell is blocked), so clearance, areas and portals stay as
    /// they are and only the sectors holding a ford are linked again.
    /// Returns each of those sectors' box of cells.
    fn reprice(&mut self, on: bool, search: &mut Search) -> Vec<[i32; 4]> {
        self.frozen = on;
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        for i in 0..self.cost.len() {
            if self.ground[i] != FORD {
                continue;
            }
            let cost = self.cell_cost(i);
            if cost != self.cost[i] {
                self.cost.set(i, cost);
                dirty[self.sector_of(i as u32) as usize] = true;
            }
        }
        let boxes: Vec<[i32; 4]> = dirty
            .iter()
            .enumerate()
            .filter(|&(_, &d)| d)
            .map(|(k, _)| {
                let (sx, sy) = (
                    k as i32 % self.sectors * SECTOR,
                    k as i32 / self.sectors * SECTOR,
                );
                [
                    sx,
                    sy,
                    (sx + SECTOR).min(self.side),
                    (sy + SECTOR).min(self.side),
                ]
            })
            .collect();
        if boxes.is_empty() {
            return boxes;
        }
        self.relink_costs(&dirty, search);
        boxes
    }

    /// Bring the grid up to date after the cells `x0..x1` × `y0..y1` (cell
    /// coordinates, end exclusive) changed cost, searching in `search`'s
    /// room and leaving it room for searches over the new graph.
    pub(super) fn changed(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, search: &mut Search) {
        let change = [x0, y0, x1, y1];
        let [wx0, wy0, wx1, wy1] = self.grow(change, i32::from(CLEAR_CAP) + 1);
        // Cells that may open or shut: areas read only whether a cell is
        // open (clearance of at least CLEARANCE), so a cell further than
        // that from the change can't have moved them, whatever its
        // clearance did.
        let near = self.grow(change, i32::from(CLEARANCE));
        let was_open: Vec<bool> = self.cells_in(near).map(|c| self.open_cell(c)).collect();
        let wide = [wx0, wy0, wx1, wy1];
        let room_was: Vec<u8> = self.cells_in(wide).map(|c| self.link_room(c)).collect();
        self.measure_clearance_in(wx0, wy0, wx1, wy1);
        if !self.renumber_near(near, &was_open) && !self.renumber_far(near, &was_open) {
            self.number_areas();
        }
        // Sectors a cell that may have opened or shut lies in, and those
        // where a cell's room as links read it changed (links keep
        // GRAPH_BERTH from blocked ground where they can, so their costs
        // read clearance up to it, further out than cells open or shut).
        let [dx0, dy0, dx1, dy1] = near;
        let mut dirty = vec![false; (self.sectors * self.sectors) as usize];
        for sy in dy0 / SECTOR..=(dy1 - 1) / SECTOR {
            for sx in dx0 / SECTOR..=(dx1 - 1) / SECTOR {
                dirty[(sy * self.sectors + sx) as usize] = true;
            }
        }
        for (c, &was) in self.cells_in(wide).zip(&room_was) {
            if self.link_room(c) != was {
                dirty[self.sector_of(c) as usize] = true;
            }
        }
        let old = OldGraph::take(self);
        // Portals sit where clearance is greatest, so only edges near
        // cells whose clearance may have changed can move.
        let partner = self.place_portals(Some((&old, [wx0, wy0, wx1, wy1])));
        // A sector whose portals moved (a neighbour's edge changed) is
        // linked again too.
        for (s, d) in dirty.iter_mut().enumerate() {
            if !*d && !old.cells_of(s).eq(self.cells_of(s)) {
                *d = true;
            }
        }
        self.relink(&partner, &old, &dirty, search);
        self.node_room = self.node_room();
        search.fit(self);
    }

    /// The box `[x0, y0, x1, y1]` (cells, end exclusive) grown by `by` on
    /// every side, kept within the map.
    fn grow(&self, [x0, y0, x1, y1]: [i32; 4], by: i32) -> [i32; 4] {
        [
            (x0 - by).max(0),
            (y0 - by).max(0),
            (x1 + by).min(self.side),
            (y1 + by).min(self.side),
        ]
    }

    /// Chessboard distance from each cell in the window `x0..x1` × `y0..y1`
    /// to the nearest blocked cell (the ground beyond the map edge counts
    /// as blocked), up to [`CLEAR_CAP`], in two sweeps, reading the cells
    /// just outside the window as they stand. Over the whole map this is
    /// the grid's clearance; over a window it is exact as long as every
    /// cell outside it is more than [`CLEAR_CAP`] from the cells that
    /// changed.
    pub(super) fn measure_clearance_in(&mut self, x0: i32, y0: i32, x1: i32, y1: i32) {
        // Measured in a list of the window's cells and a ring of cells
        // round it (as they stand; 0 beyond the map's edge), then written
        // back where they changed, so cells shared with other grids that
        // come out the same stay shared.
        let side = self.side;
        let (w, h) = ((x1 - x0 + 2) as usize, (y1 - y0 + 2) as usize);
        let mut local = vec![0u8; w * h];
        for (ly, y) in (y0 - 1..=y1).enumerate() {
            for (lx, x) in (x0 - 1..=x1).enumerate() {
                if x < 0 || y < 0 || x >= side || y >= side {
                    continue;
                }
                let i = (y * side + x) as usize;
                let inside = (x0..x1).contains(&x) && (y0..y1).contains(&y);
                local[ly * w + lx] = if !inside {
                    self.clear[i]
                } else if self.cost[i] == BLOCKED {
                    0
                } else {
                    CLEAR_CAP
                };
            }
        }
        let whole = x0 == 0 && y0 == 0 && x1 == side && y1 == side;
        // The window's cells, as places in the list.
        let cells = (1..h - 1).flat_map(|ly| (1..w - 1).map(move |lx| ly * w + lx));
        let step = |d: u8, here: u8| here.min(d.saturating_add(1).min(CLEAR_CAP));
        for l in cells.clone() {
            if local[l] == 0 {
                continue;
            }
            let mut d = local[l - 1]
                .min(local[l - 1 - w])
                .min(local[l - w])
                .min(local[l + 1 - w]);
            // In a window, every neighbour, so the cells just outside it
            // on the far sides are read too (over the whole map there
            // are none, and the cells ahead are still to be measured).
            if !whole {
                d = d
                    .min(local[l + 1])
                    .min(local[l + 1 + w])
                    .min(local[l + w])
                    .min(local[l - 1 + w]);
            }
            local[l] = step(d, local[l]);
        }
        for l in cells.clone().rev() {
            if local[l] == 0 {
                continue;
            }
            let d = local[l + 1]
                .min(local[l + 1 + w])
                .min(local[l + w])
                .min(local[l - 1 + w]);
            local[l] = step(d, local[l]);
        }
        for (ly, y) in (y0..y1).enumerate() {
            for (lx, x) in (x0..x1).enumerate() {
                self.clear
                    .set((y * side + x) as usize, local[(ly + 1) * w + lx + 1]);
            }
        }
    }

    /// Cell `c`'s clearance as links' costs read it (see [`Grid::weight`]),
    /// and whether it is open.
    fn link_room(&self, c: u32) -> u8 {
        self.clear[c as usize].min(GRAPH_BERTH)
    }

    /// The cells of the box `[x0, y0, x1, y1]` (end exclusive), row by row.
    fn cells_in(&self, [x0, y0, x1, y1]: [i32; 4]) -> impl Iterator<Item = u32> + '_ {
        (y0..y1).flat_map(move |y| (x0..x1).map(move |x| self.idx(x, y)))
    }

    /// Bring the areas up to date after only cells in `near` (a box) may
    /// have opened or shut, `was_open` saying which were open before, by
    /// looking only in a box [`SECTOR`] cells wider. The box's open cells
    /// are joined into pieces, as they stand now and as they stood before,
    /// and each piece now takes the area its cells had. Pieces that reach
    /// out of the box keep their area; so does the first of an area's
    /// pieces when none of them do. The rest (pockets cut off inside the
    /// box, and new ground) take new numbers. Returns false when that
    /// can't be told from the box alone: two areas joined, or a piece from
    /// before cut into two that both reach out (they may or may not meet
    /// again beyond), or new numbers would no longer fit 16 bits (see
    /// [`Areas`]). Then the whole map must be numbered again: cells in
    /// `near` that opened or shut have already lost their areas. Areas are only ever compared, so their numbers may
    /// differ from a fresh numbering's.
    pub(super) fn renumber_near(&mut self, near: [i32; 4], was_open: &[bool]) -> bool {
        let b @ [bx0, by0, bx1, by1] = self.grow(near, SECTOR);
        let (w, h) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
        let side = self.side;
        // The map's index of the box's `l`th cell.
        let cell = |l: u32| {
            let l = l as usize;
            ((by0 + (l / w) as i32) * side + bx0 + (l % w) as i32) as usize
        };
        // Which of the box's cells are open now, and were before (only
        // cells in `near` can differ).
        let now: Vec<bool> = self.cells_in(b).map(|c| self.open_cell(c)).collect();
        let mut before = now.clone();
        let [nx0, ny0, nx1, ny1] = near;
        let near_local = (ny0..ny1).flat_map(|y| (nx0..nx1).map(move |x| (x, y)));
        for ((x, y), &was) in near_local.zip(was_open) {
            let l = (y - by0) as usize * w + (x - bx0) as usize;
            before[l] = was;
            // Cells that shut have no area; cells that opened have none yet.
            if was != now[l] {
                self.area.set((y * side + x) as usize, 0);
            }
        }
        let (_, pieces, cells) = pieces_of(&now, w, h);
        let (piece_before, pieces_before, _) = pieces_of(&before, w, h);
        // Whether a cell is on a side of the box that isn't the map's edge,
        // and so may lead on beyond. Such cells never change.
        let edge = |l: u32| {
            let (x, y) = (bx0 + (l as usize % w) as i32, by0 + (l as usize / w) as i32);
            (x == bx0 && bx0 > 0)
                || (y == by0 && by0 > 0)
                || (x == bx1 - 1 && bx1 < side)
                || (y == by1 - 1 && by1 < side)
        };
        // Per piece now: whether it reaches out of the box; (area, piece)
        // for each piece's one old area; and which piece now holds each
        // piece from before's way out of the box.
        let mut reaches = vec![false; pieces.len()];
        let mut had: Vec<(u32, usize)> = Vec::new();
        let mut holder = vec![u32::MAX; pieces_before.len()];
        for (p, range) in pieces.iter().enumerate() {
            let mut area = 0;
            for &l in &cells[range.clone()] {
                match (area, self.area.get(cell(l))) {
                    (_, 0) => {}
                    (0, a) => area = a,
                    (a, b) if a != b => return false, // two areas joined
                    _ => {}
                }
                if edge(l) {
                    reaches[p] = true;
                    let o = &mut holder[piece_before[l as usize] as usize];
                    if *o == u32::MAX {
                        *o = p as u32;
                    } else if *o != p as u32 {
                        return false; // cut in two, both reaching out
                    }
                }
            }
            if area != 0 {
                had.push((area, p));
            }
        }
        let mut number: Vec<u32> = vec![0; pieces.len()];
        had.sort_unstable();
        for run in had.chunk_by(|a, b| a.0 == b.0) {
            let mut kept = false;
            for &(a, p) in run.iter().filter(|&&(_, p)| reaches[p]) {
                number[p] = a;
                kept = true;
            }
            if !kept {
                number[run[0].1] = run[0].0;
            }
        }
        // Numbers that would no longer fit the areas' narrow cells are
        // given out afresh instead, by numbering the whole map again.
        let new = number.iter().filter(|&&n| n == 0).count() as u32;
        if !self.area.holds(self.areas.saturating_add(new)) {
            return false;
        }
        for n in number.iter_mut().filter(|n| **n == 0) {
            self.areas += 1;
            *n = self.areas;
        }
        for (range, &n) in pieces.iter().zip(&number) {
            for &l in &cells[range.clone()] {
                self.area.set(cell(l), n);
            }
        }
        true
    }

    /// Bring the areas up to date where [`Grid::renumber_near`] couldn't
    /// tell from its box alone (it has already cleared the areas of the
    /// cells in `near` that opened or shut), by racing floods out of the
    /// box instead of numbering the whole map again. Every flood takes one
    /// cell a turn, and the race ends when all but one have run out of
    /// ground, so it costs about the second biggest area's cells rather
    /// than the map's: a gate opening or shutting in a town wall walks the
    /// town, not the map.
    ///
    /// - Ground that opened can only join areas: each set of areas joined
    ///   floods within its own cells, and those that run out take the
    ///   number of the one still going.
    /// - Ground that shut can only cut them: the pieces of each piece from
    ///   before that was cut in two or more flood over their area's cells,
    ///   pieces whose floods meet are one, and each that runs out on its
    ///   own takes a new number (and so does any other piece of the area
    ///   its flood reached). An area none of whose pieces was cut in the
    ///   box keeps its number unraced.
    ///
    /// Returns false, for the whole map to be numbered again, when ground
    /// both opened and shut, the numbers wouldn't fit 16 bits, or the
    /// floods would cover more cells than the map holds (numbering the
    /// map costs about that).
    pub(super) fn renumber_far(&mut self, near: [i32; 4], was_open: &[bool]) -> bool {
        let (mut opened, mut shut) = (false, false);
        for (c, &was) in self.cells_in(near).zip(was_open) {
            let now = self.open_cell(c);
            opened |= now && !was;
            shut |= was && !now;
        }
        if opened && shut {
            return false;
        }
        let b @ [bx0, by0, bx1, _] = self.grow(near, SECTOR);
        let w = (bx1 - bx0) as usize;
        let side = self.side;
        let cell = |l: u32| {
            let l = l as usize;
            ((by0 + (l / w) as i32) * side + bx0 + (l % w) as i32) as u32
        };
        let now: Vec<bool> = self.cells_in(b).map(|c| self.open_cell(c)).collect();
        let h = now.len() / w.max(1);
        let (_, pieces, local) = pieces_of(&now, w, h);
        let cells: Vec<u32> = local.iter().map(|&l| cell(l)).collect();
        // Cells the floods may still cover, all races together.
        let mut budget = (side * side) as usize;
        // Each piece's areas from before, in order, once each.
        let held: Vec<Vec<u32>> = pieces
            .iter()
            .map(|r| {
                let mut a: Vec<u32> = (cells[r.clone()].iter())
                    .map(|&c| self.area.get(c as usize))
                    .filter(|&a| a != 0)
                    .collect();
                a.sort_unstable();
                a.dedup();
                a
            })
            .collect();
        // Each piece's number once the race is run; 0 for a new one.
        let mut number: Vec<u32> = vec![0; pieces.len()];
        if opened {
            // Areas joined in a piece, as sets: each area's set by the
            // smallest area in it.
            let mut all: Vec<u32> = held.iter().flatten().copied().collect();
            all.sort_unstable();
            all.dedup();
            let at = |a: u32| all.binary_search(&a).expect("a held area");
            let mut set: Vec<usize> = (0..all.len()).collect();
            for areas in &held {
                for pair in areas.windows(2) {
                    let (ra, rb) = (root(&mut set, at(pair[0])), root(&mut set, at(pair[1])));
                    set[ra.max(rb)] = ra.min(rb);
                }
            }
            let mut joined: Vec<Vec<u32>> = vec![Vec::new(); all.len()];
            for (k, &a) in all.iter().enumerate() {
                joined[root(&mut set, k)].push(a);
            }
            let mut to = all.clone();
            for areas in joined.iter().filter(|j| j.len() > 1) {
                // A cell of each area to flood from: the first the box has.
                let seeds: Vec<Vec<u32>> = areas
                    .iter()
                    .map(|&a| {
                        let c = cells.iter().find(|&&c| self.area.get(c as usize) == a);
                        vec![*c.expect("a cell of each held area")]
                    })
                    .collect();
                let Some(won) = self.race(areas, seeds, false, &mut budget) else {
                    return false;
                };
                for &a in areas {
                    to[at(a)] = won;
                }
            }
            for (n, areas) in number.iter_mut().zip(&held) {
                if let Some(&a) = areas.first() {
                    *n = to[at(a)];
                }
            }
        } else {
            // Each piece now lies in one piece from before (cells only
            // shut): the pieces from before, as the box stood.
            let mut before = now.clone();
            let [nx0, ny0, nx1, ny1] = near;
            let near_local = (ny0..ny1).flat_map(|y| (nx0..nx1).map(move |x| (x, y)));
            for ((x, y), &was) in near_local.zip(was_open) {
                before[(y - by0) as usize * w + (x - bx0) as usize] = was;
            }
            let (piece_before, _, _) = pieces_of(&before, w, h);
            // The pieces of each area, by the piece from before each was
            // part of.
            let mut by_area: Vec<(u32, u32, usize)> = (held.iter().enumerate())
                .filter_map(|(p, a)| {
                    let was = piece_before[local[pieces[p].start] as usize];
                    a.first().map(|&a| (a, was, p))
                })
                .collect();
            by_area.sort_unstable();
            for run in by_area.chunk_by(|x, y| x.0 == y.0) {
                let area = run[0].0;
                // Pieces of a piece from before that was cut.
                let cut: Vec<usize> = (run.chunk_by(|x, y| x.1 == y.1))
                    .filter(|was| was.len() > 1)
                    .flat_map(|was| was.iter().map(|&(_, _, p)| p))
                    .collect();
                if cut.is_empty() {
                    for &(_, _, p) in run {
                        number[p] = area;
                    }
                    continue;
                }
                let seeds: Vec<Vec<u32>> = (cut.iter())
                    .map(|&p| cells[pieces[p].clone()].to_vec())
                    .collect();
                let areas = vec![area; cut.len()];
                let Some(_) = self.race(&areas, seeds, true, &mut budget) else {
                    return false;
                };
                // The race left each piece's ground numbered (the pieces
                // it didn't start from too): read it back.
                for &(_, _, p) in run {
                    number[p] = self.area.get(cells[pieces[p].start] as usize);
                }
            }
        }
        let new = number.iter().filter(|&&n| n == 0).count() as u32;
        if !self.area.holds(self.areas.saturating_add(new)) {
            return false;
        }
        for (range, n) in pieces.iter().zip(number.iter_mut()) {
            if *n == 0 {
                self.areas += 1;
                *n = self.areas;
            }
            for &c in &cells[range.clone()] {
                self.area.set(c as usize, *n);
            }
        }
        true
    }

    /// Race floods from each of `seeds` (cells of area `areas[k]` for
    /// flood `k`) over the cells of its area, one cell each a turn, until
    /// at most one is still going (see [`Grid::renumber_far`]), and number
    /// the ground they covered. If `meet`, the areas are all one and
    /// floods that run into each other go on as one; a flood out of
    /// ground takes a new number, and the one still going (or, if none is,
    /// the first) keeps the area's. Otherwise each flood stays in its own
    /// area, and every area takes the number of the one still going (or,
    /// if none is, the biggest): that number is returned. `None` if the
    /// numbers wouldn't fit 16 bits, or the floods would cover more than
    /// `budget` cells (which counts down by those they cover); the areas
    /// must then be numbered afresh.
    fn race(
        &mut self,
        areas: &[u32],
        seeds: Vec<Vec<u32>>,
        meet: bool,
        budget: &mut usize,
    ) -> Option<u32> {
        let n = seeds.len() as u32;
        // Each flood marks the cells it covers with a number of its own,
        // above every number given out, and its cells double as its queue.
        let mark = self.areas + 1;
        if !self.area.holds(self.areas.saturating_add(2 * n)) {
            return None;
        }
        let mut floods: Vec<(Vec<u32>, usize)> = seeds.into_iter().map(|s| (s, 0)).collect();
        for (k, (cells, _)) in floods.iter().enumerate() {
            for &c in cells {
                self.area.set(c as usize, mark + k as u32);
            }
        }
        // Floods that met, as sets (each flood's set by its first flood).
        let mut set: Vec<usize> = (0..floods.len()).collect();
        let side = self.side;
        // Floods still going in each set (by its first flood), and how many
        // sets have any.
        let mut left: Vec<usize> = vec![1; floods.len()];
        for (k, (cells, _)) in floods.iter().enumerate() {
            if cells.is_empty() {
                left[k] = 0;
            }
        }
        let mut alive = left.iter().filter(|&&l| l > 0).count();
        'race: while alive > 1 {
            // Each flood still going takes one cell.
            for k in 0..floods.len() {
                if floods[k].1 == floods[k].0.len() {
                    continue;
                }
                let c = floods[k].0[floods[k].1];
                floods[k].1 += 1;
                *budget = budget.checked_sub(1)?;
                let (x, y) = (c as i32 % side, c as i32 / side);
                let next = [
                    (x + 1 < side).then(|| c + 1),
                    (x > 0).then(|| c - 1),
                    (y + 1 < side).then(|| c + side as u32),
                    (y > 0).then(|| c - side as u32),
                ];
                for m in next.into_iter().flatten() {
                    let a = self.area.get(m as usize);
                    if a == areas[k] {
                        self.area.set(m as usize, mark + k as u32);
                        floods[k].0.push(m);
                    } else if meet && (mark..mark + n).contains(&a) {
                        let (ra, rb) = (root(&mut set, k), root(&mut set, (a - mark) as usize));
                        if ra != rb {
                            let (lo, hi) = (ra.min(rb), ra.max(rb));
                            if left[lo] > 0 && left[hi] > 0 {
                                alive -= 1;
                            }
                            set[hi] = lo;
                            left[lo] += left[hi];
                        }
                    }
                }
                if floods[k].1 == floods[k].0.len() {
                    let r = root(&mut set, k);
                    left[r] -= 1;
                    if left[r] == 0 {
                        alive -= 1;
                    }
                }
                if alive <= 1 {
                    break 'race;
                }
            }
        }
        let going = (0..floods.len())
            .find(|&k| floods[k].1 < floods[k].0.len())
            .map(|k| root(&mut set, k));
        if meet {
            // Sets out of ground take new numbers; the one going keeps it.
            let keep = going.unwrap_or(0);
            let mut numbers: Vec<u32> = vec![0; floods.len()];
            for k in 0..floods.len() {
                let r = root(&mut set, k);
                if numbers[r] == 0 {
                    numbers[r] = if r == keep {
                        areas[k]
                    } else {
                        self.areas += 1;
                        self.areas
                    };
                }
                numbers[k] = numbers[r];
            }
            for (k, (cells, _)) in floods.iter().enumerate() {
                for &c in cells {
                    self.area.set(c as usize, numbers[k]);
                }
            }
            Some(areas[keep])
        } else {
            let won = going.unwrap_or_else(|| {
                (0..floods.len())
                    .max_by_key(|&k| (floods[k].0.len(), core::cmp::Reverse(k)))
                    .expect("a flood")
            });
            for (cells, _) in &floods {
                for &c in cells {
                    self.area.set(c as usize, areas[won]);
                }
            }
            Some(areas[won])
        }
    }

    /// Number the connected areas of open ground, so a search never starts
    /// for a goal that can't be reached: each area by the first cell met in
    /// it, row by row, as a flood fill from each new cell would, but in two
    /// passes over the map. A step on the diagonal
    /// needs both squares beside it open, so areas join along rows and
    /// columns only: the first pass gives each open cell the label of the
    /// cell behind or above it (a new one if both are shut), noting labels
    /// that meet as one; the second numbers each area by the first cell
    /// met in it.
    pub(super) fn number_areas(&mut self) {
        let side = self.side as usize;
        let n = side * side;
        let mut narrow = vec![0u16; n];
        self.area = match number_cells(&self.clear, &mut narrow, side) {
            Some(next) => {
                self.areas = next;
                Areas::Narrow(Cells::from_slice(&narrow))
            }
            // More labels than 16 bits hold (only on ground cut into tens
            // of thousands of pieces): label it again in 32, and hold the
            // numbers in 16 if they fit.
            None => {
                drop(narrow);
                let mut wide = vec![0u32; n];
                let next = number_cells(&self.clear, &mut wide, side);
                self.areas = next.expect("a label for every cell");
                match u16::try_from(self.areas) {
                    Ok(_) => Areas::Narrow(Cells::from_slice(
                        &wide.iter().map(|&a| a as u16).collect::<Vec<_>>(),
                    )),
                    Err(_) => Areas::Wide(Cells::from_slice(&wide)),
                }
            }
        };
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
        let costs: Vec<Vec<u32>> = dirty
            .iter()
            .enumerate()
            .map(|(s, &d)| {
                if d {
                    self.sector_costs(search, s)
                } else {
                    Vec::new()
                }
            })
            .collect();
        let (mut first, mut to, mut cost) = (
            Vec::with_capacity(nodes + 1),
            Vec::with_capacity(old.edge_to.len()),
            Vec::with_capacity(old.edge_to.len()),
        );
        first.push(0);
        for (u, &p) in partner.iter().enumerate() {
            let cell = self.node_cell[u];
            self.link_across(u, p, &mut to, &mut cost);
            let s = self.sector_of(cell) as usize;
            if dirty[s] {
                self.link_within(u, place[u], &costs[s], &mut to, &mut cost);
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

/// The pieces the open cells of a `w` × `h` box (`open`, row by row) form,
/// joined along rows and columns as areas are: each cell's piece
/// (`u32::MAX` if shut), each piece's range of the list of cells, and that
/// list, piece by piece, each piece from the first cell met in it.
fn pieces_of(open: &[bool], w: usize, h: usize) -> (Vec<u32>, Vec<Range<usize>>, Vec<u32>) {
    let mut piece = vec![u32::MAX; w * h];
    let mut ranges = Vec::new();
    let mut cells: Vec<u32> = Vec::with_capacity(w * h);
    for first in 0..w * h {
        if piece[first] != u32::MAX || !open[first] {
            continue;
        }
        let p = ranges.len() as u32;
        let start = cells.len();
        piece[first] = p;
        cells.push(first as u32);
        let mut head = start;
        while head < cells.len() {
            let l = cells[head] as usize;
            head += 1;
            let (x, y) = (l % w, l / w);
            let next = [
                (x + 1 < w).then(|| l + 1),
                (x > 0).then(|| l - 1),
                (y + 1 < h).then(|| l + w),
                (y > 0).then(|| l - w),
            ];
            for m in next.into_iter().flatten() {
                if open[m] && piece[m] == u32::MAX {
                    piece[m] = p;
                    cells.push(m as u32);
                }
            }
        }
        ranges.push(start..cells.len());
    }
    (piece, ranges, cells)
}

/// Each node's place in its sector's list, from a graph's sector lists.
pub(super) fn places(sector_first: &[u32], sector_nodes: &[u32], nodes: usize) -> Vec<u32> {
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
pub(super) struct OldGraph {
    pub(super) node_cell: Vec<u32>,
    pub(super) pair_first: Vec<u32>,
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
            pair_first: core::mem::take(&mut grid.pair_first),
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
    use crate::nav::{CELLS_PER_TILE, DIRS};
    use crate::scenario::ribs_map;
    use gc_fixed::Pcg32;

    /// The same cells in the same areas, whatever the areas' numbers.
    fn same_areas(a: &Grid, b: &Grid) {
        let mut to_b = alloc::collections::BTreeMap::new();
        let mut to_a = alloc::collections::BTreeMap::new();
        for (i, (x, y)) in a.area.all().into_iter().zip(b.area.all()).enumerate() {
            assert_eq!(x == 0, y == 0, "open differs at cell {i}");
            if x != 0 {
                assert_eq!(*to_b.entry(x).or_insert(y), y, "areas differ at cell {i}");
                assert_eq!(*to_a.entry(y).or_insert(x), x, "areas differ at cell {i}");
            }
        }
    }

    /// The costs of a grid with no slow going, as the map would give
    /// them (in steps of open ground), to build it afresh from.
    fn unpriced(g: &Grid) -> Vec<u8> {
        assert!(g.going.is_empty());
        let back = |c: u8| {
            (0..=u8::MAX)
                .find(|&m| super::super::price(m, 100) == c)
                .expect("a map cost")
        };
        g.cost.iter().map(|&c| back(c)).collect()
    }

    /// Every part of the grid that routes read.
    fn same(a: &Grid, b: &Grid) {
        assert!(a.cost == b.cost, "costs differ");
        for (i, (x, y)) in a.clear.iter().zip(b.clear.iter()).enumerate() {
            assert_eq!(x, y, "clearance differs at cell {i}");
        }
        same_areas(a, b);
        assert_eq!(a.node_cell, b.node_cell, "portals differ");
        assert_eq!(a.pair_first, b.pair_first, "portals differ");
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
            let mut fresh = Grid::build(&unpriced(&g), side);
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
    fn an_open_door_is_ground_as_if_nothing_stood_there() {
        let tiles: i32 = 96;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        // A wall right across, a gate in it, and a house the gate's door
        // knows nothing of.
        let (wall, gate, house) = ([0, 40, 50, 1], [50, 40, 3, 1], [70, 60, 3, 3]);
        for b in [wall, gate, house] {
            assert!(g.set_built(b, true, &mut search).is_some());
        }
        let mut walled = Grid::build(&unpriced(&g), side);
        walled.ground.clone_from(&g.ground);
        walled.built.clone_from(&g.built);
        same(&g, &walled);
        g.change_all(&[gate], Change::Opened(true), &mut search);
        let mut free = Grid::build(&ground, side);
        let mut s2 = Search::for_grid(&free);
        for b in [wall, house] {
            assert!(free.set_built(b, true, &mut s2).is_some());
        }
        same(&g, &free);
        // Shut again, the wall it was; the gate still stands either way.
        g.change_all(&[gate], Change::Opened(false), &mut search);
        same(&g, &walled);
        assert!(g.built[g.idx(101, 81) as usize] > 0);
    }

    #[test]
    fn frozen_fords_cost_as_open_ground_till_the_thaw() {
        // A ford across the map (some cells on its tiles left deep): frozen,
        // the grid is the one built with the ford as open ground; thawed,
        // the map as it was.
        let tiles: i32 = 64;
        let side = tiles * CELLS_PER_TILE;
        let mut ground = vec![OPEN; (side * side) as usize];
        for cy in 60..66 {
            for cx in 0..side {
                ground[(cy * side + cx) as usize] = if cx % 9 == 0 { BLOCKED } else { FORD };
            }
        }
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        assert!(!g.reprice(true, &mut search).is_empty());
        let iced: Vec<u8> = ground
            .iter()
            .map(|&c| if c == FORD { OPEN } else { c })
            .collect();
        let mut fresh = Grid::build(&iced, side);
        fresh.ground.clone_from(&g.ground);
        same(&g, &fresh);
        g.reprice(false, &mut search);
        same(&g, &Grid::build(&ground, side));
    }

    #[test]
    fn woods_burnt_out_leave_the_grid_built_afresh() {
        // A wood across sector edges, then a patch of it burnt out: every
        // link a block's cost reads it from comes out as on a grid built
        // with the new going (see `Grid::block_going_at`).
        let tiles: i32 = 96;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let going_with = |burnt: &dyn Fn(i32, i32) -> bool| -> Vec<u8> {
            (0..tiles * tiles)
                .map(|t| {
                    let (x, y) = (t % tiles, t / tiles);
                    let wood = (10..70).contains(&x) && (20..60).contains(&y);
                    if wood && !burnt(x, y) { 85 } else { 100 }
                })
                .collect()
        };
        let mut g = Grid::build_standing(
            &ground,
            side,
            core::iter::empty(),
            going_with(&|_, _| false),
        );
        let mut search = Search::for_grid(&g);
        // Burnt just south of a sector edge (cell row 64), so cells of the
        // sectors north of it read the change.
        let fire = |x: i32, y: i32| (20..60).contains(&x) && (32..35).contains(&y);
        let tiles_burnt: Vec<(i32, i32, u8)> = (0..tiles * tiles)
            .map(|t| (t % tiles, t / tiles))
            .filter(|&(x, y)| fire(x, y))
            .map(|(x, y)| (x, y, 100))
            .collect();
        assert!(!g.regoing(&tiles_burnt, &mut search).is_empty());
        let fresh = Grid::build_standing(&ground, side, core::iter::empty(), going_with(&fire));
        same(&g, &fresh);
    }

    #[test]
    fn a_straight_walk_is_checked_through_every_cell_it_crosses() {
        // Random blocked cells and random walks, many starting or ending on
        // cell edges: a walk passed as clear never touches a blocked cell
        // anywhere along it.
        let side = 24;
        let mut rng = Pcg32::new(5, 3);
        let mut costs = vec![1u8; (side * side) as usize];
        for c in &mut costs {
            if rng.below(6) == 0 {
                *c = BLOCKED;
            }
        }
        let g = Grid::build(&costs, side);
        // A position in cells, in halves or in finer steps, as tiles.
        let at = |rng: &mut Pcg32| {
            let raw = if rng.below(2) == 0 {
                rng.below(2 * side as u32) as i32 * (1 << 15)
            } else {
                rng.below((side as u32) << 16) as i32
            };
            Fx::from_raw(raw / CELLS_PER_TILE)
        };
        let mut passed = 0;
        for _ in 0..4000 {
            let (p, q) = (
                FxVec2::new(at(&mut rng), at(&mut rng)),
                FxVec2::new(at(&mut rng), at(&mut rng)),
            );
            if !g.segment_within(p, q, CLEARANCE, u8::MAX, &mut 0) {
                continue;
            }
            passed += 1;
            for k in 0..=512i64 {
                let along = |a: Fx, b: Fx| {
                    Fx::from_raw(
                        (i64::from(a.raw()) + (i64::from(b.raw()) - i64::from(a.raw())) * k / 512)
                            as i32,
                    )
                };
                let m = FxVec2::new(along(p.x, q.x), along(p.y, q.y));
                assert!(
                    g.cost[g.cell_at(m) as usize] != BLOCKED,
                    "{p:?} to {q:?} crosses blocked ground at {m:?}"
                );
            }
        }
        assert!(passed > 200, "{passed}");
        // The case that slipped through: a start on a cell's west edge,
        // heading west.
        let mut costs = vec![1u8; (side * side) as usize];
        costs[(side + 2) as usize] = BLOCKED; // cell (2, 1)
        let g = Grid::build(&costs, side);
        let tiles = |x: i32, y: i32| FxVec2::new(Fx::from_raw(x << 14), Fx::from_raw(y << 14));
        // Cells (4.0, 0.5) to (1.5, 3.5), in quarter tiles.
        assert!(!g.segment_within(tiles(8, 1), tiles(3, 7), CLEARANCE, u8::MAX, &mut 0));
    }

    #[test]
    fn a_building_near_a_sector_edge_reprices_the_sector_beyond() {
        // A building a few cells short of a sector edge narrows the room
        // in the next sector too, which its links' costs read, even where
        // that sector's portals stay put.
        let tiles: i32 = 64;
        let side = tiles * CELLS_PER_TILE;
        let ground = vec![1; (side * side) as usize];
        // Sector edges every 16 tiles: from right by x = 16 to 5 tiles short.
        for y in (0..28).step_by(3) {
            for x in 9..14 {
                let mut g = Grid::build(&ground, side);
                let mut search = Search::for_grid(&g);
                assert!(g.set_built([x, y, 2, 3], true, &mut search).is_some());
                let mut fresh = Grid::build(&unpriced(&g), side);
                fresh.ground.clone_from(&g.ground);
                fresh.built.clone_from(&g.built);
                same(&g, &fresh);
            }
        }
    }

    #[test]
    fn buildings_put_up_and_pulled_down_together_leave_the_grid_built_afresh() {
        // Two bases' worth of buildings far apart, and a few strays: all
        // put up in one go, half pulled down in another, then the rest.
        let tiles: i32 = 192;
        let side = tiles * CELLS_PER_TILE;
        let ground = ribs_map(tiles as u32);
        let mut g = Grid::build(&ground, side);
        let mut search = Search::for_grid(&g);
        let mut rng = Pcg32::new(4, 7);
        let mut all: Vec<[i32; 4]> = Vec::new();
        for (bx, by) in [(20, 20), (150, 140), (90, 30)] {
            for _ in 0..12 {
                let (w, d) = (2 + rng.below(4) as i32, 2 + rng.below(4) as i32);
                all.push([bx + rng.below(30) as i32, by + rng.below(30) as i32, w, d]);
            }
        }
        let check = |g: &Grid| {
            let mut fresh = Grid::build(&unpriced(g), side);
            fresh.ground.clone_from(&g.ground);
            fresh.built.clone_from(&g.built);
            same(g, &fresh);
        };
        let groups = g.set_built_all(&all, true, &mut search);
        assert!(groups.len() >= 3 && groups.len() < all.len(), "{groups:?}");
        check(&g);
        let (half, rest) = all.split_at(all.len() / 2);
        g.set_built_all(half, false, &mut search);
        check(&g);
        g.set_built_all(rest, false, &mut search);
        same(&g, &Grid::build(&ground, side));
    }

    /// The areas numbered by a flood fill from each new open cell met,
    /// row by row: what [`Grid::number_areas`] must give.
    fn flood_areas(g: &Grid) -> Vec<u32> {
        let mut area = vec![0; g.cost.len()];
        let mut queue: Vec<u32> = Vec::with_capacity(g.cost.len());
        let mut next = 0;
        for c in 0..g.cost.len() as u32 {
            if !g.open_cell(c) || area[c as usize] != 0 {
                continue;
            }
            next += 1;
            area[c as usize] = next;
            queue.clear();
            queue.push(c);
            let mut head = 0;
            while head < queue.len() {
                let (x, y) = g.xy(queue[head]);
                head += 1;
                for (k, &(dx, dy)) in DIRS.iter().enumerate() {
                    if !g.open(x + dx, y + dy)
                        || (k >= 4 && !(g.open(x + dx, y) && g.open(x, y + dy)))
                    {
                        continue;
                    }
                    let m = g.idx(x + dx, y + dy);
                    if area[m as usize] == 0 {
                        area[m as usize] = next;
                        queue.push(m);
                    }
                }
            }
        }
        area
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
            let g = Grid::build(&cost, side);
            assert!(g.area.all() == flood_areas(&g), "{blocked}% blocked");
        }
    }

    /// How [`Grid::changed`] brought the areas up to date.
    #[derive(Debug, PartialEq, Eq)]
    enum Numbered {
        Near,
        Far,
        WholeMap,
    }

    /// Put up (or pull down) a building as [`Grid::set_built`] does, but
    /// bring only clearance and areas up to date, as [`Grid::changed`]
    /// does (near it, by the race out of the box, or over the whole map),
    /// and say which it took.
    fn renumbered(g: &mut Grid, tiles: [i32; 4], on: bool) -> Numbered {
        let [x0, y0, x1, y1] = g.cells(tiles).unwrap();
        let near = g.grow([x0, y0, x1, y1], i32::from(CLEARANCE));
        let was_open: Vec<bool> = g.cells_in(near).map(|c| g.open_cell(c)).collect();
        for c in g.cells_in([x0, y0, x1, y1]).collect::<Vec<_>>() {
            let c = c as usize;
            g.built[c] = if on { g.built[c] + 1 } else { g.built[c] - 1 };
            g.cost[c] = if g.built[c] > 0 {
                BLOCKED
            } else {
                g.priced(c, g.ground[c])
            };
        }
        g.measure_clearance_in(0, 0, g.side, g.side);
        let how = if g.renumber_near(near, &was_open) {
            Numbered::Near
        } else if g.renumber_far(near, &was_open) {
            Numbered::Far
        } else {
            g.number_areas();
            Numbered::WholeMap
        };
        let fresh = Grid::build(&unpriced(g), g.side);
        same_areas(g, &fresh);
        how
    }

    #[test]
    fn areas_joined_or_cut_beyond_the_box_are_settled_by_racing_floods() {
        let tiles = 64;
        let open = vec![1u8; (tiles * CELLS_PER_TILE * tiles * CELLS_PER_TILE) as usize];
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        // A walled town much wider than the box looked in near a change,
        // with a gate in its south wall that opens and shuts: each swing
        // joins the town to the land round it or cuts it off again.
        let ring = [
            [10, 10, 30, 1],
            [10, 40, 30, 1],
            [10, 11, 1, 29],
            [39, 11, 1, 29],
        ];
        for wall in ring {
            renumbered(&mut g, wall, true);
        }
        // A gap in the south wall joins the town to the land round it, and
        // the gate put up in it cuts the town off again (both sides reach
        // out of the box, so only the race tells).
        assert_eq!(renumbered(&mut g, [24, 40, 2, 1], false), Numbered::Far);
        assert_eq!(renumbered(&mut g, [24, 40, 2, 1], true), Numbered::Far);
        for _ in 0..3 {
            assert_eq!(
                renumbered(&mut g, [24, 40, 2, 1], false),
                Numbered::Far,
                "opened"
            );
            assert_eq!(
                renumbered(&mut g, [24, 40, 2, 1], true),
                Numbered::Far,
                "shut"
            );
        }
        // A building beside the shut gate, inside the town: one piece of
        // ground on each side of the wall, nothing joined or cut.
        assert_eq!(renumbered(&mut g, [24, 37, 2, 2], true), Numbered::Near);
        // A wall right across the map below the town cuts it in two, and
        // pulling a piece down joins the halves again.
        for x in (0..tiles - 8).step_by(8) {
            renumbered(&mut g, [x, 50, 8, 1], true);
        }
        assert_eq!(
            renumbered(&mut g, [tiles - 8, 50, 8, 1], true),
            Numbered::Far
        );
        assert_eq!(renumbered(&mut g, [0, 50, 8, 1], false), Numbered::Far);
        // Three areas joined at once: a cross of walls cut a corner into
        // pockets, and the building at their middle comes down.
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        for wall in [
            [0, 30, 30, 1],
            [32, 30, 32, 1],
            [30, 0, 1, 30],
            [30, 32, 1, 32],
        ] {
            renumbered(&mut g, wall, true);
        }
        renumbered(&mut g, [30, 30, 2, 2], true);
        assert_eq!(renumbered(&mut g, [30, 30, 2, 2], false), Numbered::Far);
    }

    #[test]
    fn areas_are_renumbered_near_a_building_when_they_can_be() {
        let tiles = 64;
        let open = vec![1u8; (tiles * CELLS_PER_TILE * tiles * CELLS_PER_TILE) as usize];
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        // A building in the open: nothing cut off.
        assert_eq!(renumbered(&mut g, [30, 30, 2, 2], true), Numbered::Near);
        // A ring of walls round a pocket in the middle of the map: the last
        // wall cuts the pocket off, inside the box looked in.
        let ring = [[20, 20, 8, 1], [20, 27, 8, 1], [20, 21, 1, 6]];
        for wall in ring {
            assert_eq!(renumbered(&mut g, wall, true), Numbered::Near);
        }
        assert_eq!(
            renumbered(&mut g, [27, 21, 1, 6], true),
            Numbered::Near,
            "pocket cut off"
        );
        // Pulling a wall down joins two areas: not told near it.
        assert_ne!(renumbered(&mut g, [27, 21, 1, 6], false), Numbered::Near);
        // A wall right across the map: the last piece cuts it in two, and
        // both sides reach out of the box.
        for x in (0..tiles - 8).step_by(8) {
            assert_eq!(renumbered(&mut g, [x, 50, 8, 1], true), Numbered::Near);
        }
        // Beside the wall, an area is two pieces in the box, joined round
        // the wall's end out of it; a building there cuts neither.
        assert_eq!(renumbered(&mut g, [8, 47, 2, 2], true), Numbered::Near);
        assert_eq!(renumbered(&mut g, [8, 47, 2, 2], false), Numbered::Near);
        assert_ne!(
            renumbered(&mut g, [tiles - 8, 50, 8, 1], true),
            Numbered::Near,
            "map cut in two"
        );
        assert_ne!(
            renumbered(&mut g, [0, 50, 8, 1], false),
            Numbered::Near,
            "halves joined"
        );
    }

    #[test]
    fn areas_numbers_too_high_to_fit_are_given_out_afresh() {
        let tiles = 64;
        let open = vec![1u8; (tiles * CELLS_PER_TILE * tiles * CELLS_PER_TILE) as usize];
        let mut g = Grid::build(&open, tiles * CELLS_PER_TILE);
        // As if a long match had given out every number 16 bits hold: a
        // pocket walled off needs a new one, so the map is numbered again,
        // from 1, and the cells stay narrow.
        g.areas = u32::from(u16::MAX);
        let ring = [[20, 20, 8, 1], [20, 27, 8, 1], [20, 21, 1, 6]];
        for wall in ring {
            assert_eq!(renumbered(&mut g, wall, true), Numbered::Near);
        }
        assert_ne!(renumbered(&mut g, [27, 21, 1, 6], true), Numbered::Near);
        assert_eq!(g.areas, 2);
        assert!(matches!(g.area, Areas::Narrow(_)));
    }

    #[test]
    fn ground_in_more_pieces_than_16_bits_count_is_numbered_in_32() {
        // Open cells a cell apart, each its own area: 260 x 260 of them.
        let side = 520;
        let cost: Vec<u8> = (0..side * side)
            .map(|i| {
                if (i % side) % 2 == 0 && (i / side) % 2 == 0 {
                    1
                } else {
                    BLOCKED
                }
            })
            .collect();
        let g = Grid::build(&cost, side);
        assert!(matches!(g.area, Areas::Wide(_)));
        assert_eq!(g.areas, 260 * 260);
        assert!(g.area.all() == flood_areas(&g), "areas differ");
        // Numbered again with few enough areas, the cells narrow back.
        let mut fewer = g.clone();
        fewer.clear.fill(0);
        fewer.clear[0] = CLEARANCE;
        fewer.number_areas();
        assert!(matches!(fewer.area, Areas::Narrow(_)));
        assert_eq!(fewer.area.get(0), 1);
    }

    #[test]
    fn areas_stay_right_as_walls_come_and_go() {
        // Walls a tile thick put up and pulled down at random in one corner
        // of the ridged map, cutting pockets off and joining them again,
        // the areas checked against a fresh build after each.
        let tiles = 64;
        let mut g = Grid::build(&ribs_map(tiles as u32), tiles * CELLS_PER_TILE);
        let mut rng = Pcg32::new(9, 1);
        let mut up: Vec<[i32; 4]> = Vec::new();
        let mut count = [0; 3];
        for _ in 0..400 {
            let how = if !up.is_empty() && rng.below(3) == 0 {
                let k = rng.below(up.len() as u32) as usize;
                renumbered(&mut g, up.swap_remove(k), false)
            } else {
                let long = 2 + rng.below(8) as i32;
                let [w, d] = if rng.below(2) == 0 {
                    [long, 1]
                } else {
                    [1, long]
                };
                let b = [rng.below(24) as i32, rng.below(24) as i32, w, d];
                up.push(b);
                renumbered(&mut g, b, true)
            };
            count[how as usize] += 1;
        }
        // Joins and cuts the box can't tell are raced, never numbered
        // across the map.
        assert!(count[0] > 0 && count[1] > 0 && count[2] == 0, "{count:?}");
    }
}
