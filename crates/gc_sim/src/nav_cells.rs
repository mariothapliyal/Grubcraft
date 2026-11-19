//! A grid layer held in runs of cells that grids can share (see
//! [`Cells`]).
//!
//! From the first gate, each side whose gates let it through walks a grid
//! of its own (see [`super::Nav`]): a copy of the shared grid that differs
//! from it only round that side's gates. Held as one long list, every copy
//! cost a whole map's worth of cells (on a Large map, about 13 MB a side).
//! Held in runs, a copy shares every run with the grid it was copied from
//! and copies a run only when it writes to it; after each change to the
//! ground, the runs that came out the same on two grids are shared again
//! (see [`Cells::share_from`]). So a side's grid costs about the cells
//! round its own gates.

use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use core::ops::{Index, IndexMut};

/// Cells to a run, as a power of two: 4,096 cells (two to five rows of
/// the maps played).
const SHIFT: usize = 12;
const RUN: usize = 1 << SHIFT;
const MASK: usize = RUN - 1;

/// A list of cells held in runs of [`RUN`], each shared until written:
/// reading one costs one more look-up than a plain list. The last run is
/// filled out to a whole one, so a cell's place in its run needs no check.
#[derive(Clone)]
pub(super) struct Cells<T> {
    runs: Vec<Arc<[T; RUN]>>,
    len: usize,
    /// The runs written since the cells were last shared (one bit a run),
    /// so sharing them again compares only those.
    written: Vec<u64>,
}

impl<T: Copy + PartialEq> Cells<T> {
    /// `len` cells, each `value`. Every full run starts as the same one.
    pub(super) fn new(len: usize, value: T) -> Cells<T> {
        let run = Arc::new([value; RUN]);
        Cells::of_runs(vec![run; len.div_ceil(RUN)], len)
    }

    /// The cells of `list`, in order (the last run filled out with copies
    /// of its last cell).
    pub(super) fn from_slice(list: &[T]) -> Cells<T> {
        let runs = (list.chunks(RUN))
            .map(|c| Arc::new(core::array::from_fn(|i| c[i.min(c.len() - 1)])))
            .collect();
        Cells::of_runs(runs, list.len())
    }

    fn of_runs(runs: Vec<Arc<[T; RUN]>>, len: usize) -> Cells<T> {
        let written = vec![0; len.div_ceil(RUN).div_ceil(64)];
        Cells { runs, len, written }
    }

    /// No cells: a layer not made yet.
    pub(super) fn none() -> Cells<T> {
        Cells::of_runs(Vec::new(), 0)
    }

    pub(super) fn len(&self) -> usize {
        self.len
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub(super) fn get(&self, i: usize) -> Option<&T> {
        (i < self.len).then(|| &self[i])
    }

    /// Set cell `i` to `value`, leaving its run alone (and shared, if it
    /// is) when it is already that.
    #[inline]
    pub(super) fn set(&mut self, i: usize, value: T) {
        if self[i] != value {
            self[i] = value;
        }
    }

    /// Every cell, in order.
    pub(super) fn iter(&self) -> impl Iterator<Item = &T> + '_ {
        self.runs.iter().flat_map(|r| r.iter()).take(self.len)
    }

    /// Set every cell to `value`.
    #[cfg(test)]
    pub(super) fn fill(&mut self, value: T) {
        *self = Cells::new(self.len, value);
    }

    /// Whether any run was written since the cells were last shared.
    pub(super) fn written(&self) -> bool {
        self.written.iter().any(|&w| w != 0)
    }

    /// Share with `other` (the same number of cells) every run written on
    /// either since they were last shared that came out the same on both,
    /// and start counting writes afresh.
    pub(super) fn share_from(&mut self, other: &Cells<T>) {
        if self.len == other.len {
            for (w, (&a, &b)) in self.written.iter().zip(&other.written).enumerate() {
                let mut bits = a | b;
                while bits != 0 {
                    let k = w * 64 + bits.trailing_zeros() as usize;
                    bits &= bits - 1;
                    let (mine, theirs) = (&self.runs[k], &other.runs[k]);
                    if !Arc::ptr_eq(mine, theirs) && **mine == **theirs {
                        self.runs[k] = theirs.clone();
                    }
                }
            }
        }
        self.settle();
    }

    /// Start counting writes afresh.
    pub(super) fn settle(&mut self) {
        self.written.fill(0);
    }

    /// How many runs these cells don't share with `other`.
    #[cfg(test)]
    pub(super) fn runs_apart_from(&self, other: &Cells<T>) -> usize {
        (self.runs.iter().zip(&other.runs))
            .filter(|(a, b)| !Arc::ptr_eq(a, b))
            .count()
    }
}

impl<T> Index<usize> for Cells<T> {
    type Output = T;

    #[inline]
    fn index(&self, i: usize) -> &T {
        debug_assert!(i < self.len);
        &self.runs[i >> SHIFT][i & MASK]
    }
}

impl<T: Clone> IndexMut<usize> for Cells<T> {
    /// The cell to write, its run copied first if shared.
    #[inline]
    fn index_mut(&mut self, i: usize) -> &mut T {
        debug_assert!(i < self.len);
        let k = i >> SHIFT;
        self.written[k / 64] |= 1 << (k % 64);
        &mut Arc::make_mut(&mut self.runs[k])[i & MASK]
    }
}

impl<T: Copy + PartialEq> PartialEq for Cells<T> {
    fn eq(&self, other: &Cells<T>) -> bool {
        self.len == other.len && self.iter().eq(other.iter())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_read_and_write_across_runs_and_share_runs_alike() {
        let n = 3 * RUN + 17;
        let mut a = Cells::new(n, 0u8);
        let list: Vec<u8> = (0..n).map(|i| (i % 251) as u8).collect();
        let b = Cells::from_slice(&list);
        assert!(b.iter().copied().eq(list.iter().copied()));
        assert_eq!(b[n - 1], list[n - 1]);
        assert_eq!(b.get(n), None);
        // A copy shares every run until written.
        a[RUN + 5] = 9;
        let mut c = a.clone();
        a.settle();
        c.settle();
        assert_eq!(c.runs_apart_from(&a), 0);
        c[RUN + 5] = 3;
        c[2 * RUN] = 4;
        assert_eq!(c.runs_apart_from(&a), 2);
        assert_eq!((a[RUN + 5], c[RUN + 5]), (9, 3));
        // The same write on the other makes one run alike again.
        a[2 * RUN] = 4;
        c.share_from(&a);
        assert_eq!(c.runs_apart_from(&a), 1);
        assert!(c != a);
        c[RUN + 5] = 9;
        c.share_from(&a);
        assert_eq!(c.runs_apart_from(&a), 0);
        assert!(c == a);
    }
}
