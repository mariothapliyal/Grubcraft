//! What the player has selected: nothing, some regiments, or one building.

use std::ops::Deref;

use gc_sim::{RegimentHandle, StructureHandle};

/// The selection. Regiments and a building are never picked together: a
/// building is selected only while no regiment is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Selection {
    #[default]
    Nothing,
    /// Never empty: the last regiment going leaves `Nothing`.
    Regiments(Vec<RegimentHandle>),
    Structure(StructureHandle),
}

/// A selection reads as its regiments, none for a building or nothing.
impl Deref for Selection {
    type Target = [RegimentHandle];

    fn deref(&self) -> &[RegimentHandle] {
        self.regiments()
    }
}

impl From<Vec<RegimentHandle>> for Selection {
    fn from(regiments: Vec<RegimentHandle>) -> Selection {
        if regiments.is_empty() {
            Selection::Nothing
        } else {
            Selection::Regiments(regiments)
        }
    }
}

impl Selection {
    /// The regiments selected, if any are.
    pub fn regiments(&self) -> &[RegimentHandle] {
        match self {
            Selection::Regiments(regiments) => regiments,
            Selection::Nothing | Selection::Structure(_) => &[],
        }
    }

    /// The building selected, if one is.
    pub fn structure(&self) -> Option<StructureHandle> {
        match self {
            Selection::Structure(h) => Some(*h),
            Selection::Nothing | Selection::Regiments(_) => None,
        }
    }

    /// Select nothing.
    pub fn clear(&mut self) {
        *self = Selection::Nothing;
    }

    /// Let go of the regiments, keeping a building if that is what is selected.
    pub fn clear_regiments(&mut self) {
        if matches!(self, Selection::Regiments(_)) {
            self.clear();
        }
    }

    /// Let go of the building, keeping regiments if those are what is selected.
    pub fn drop_structure(&mut self) {
        if matches!(self, Selection::Structure(_)) {
            self.clear();
        }
    }

    /// Select this building alone.
    pub fn select_structure(&mut self, h: StructureHandle) {
        *self = Selection::Structure(h);
    }

    /// Select this regiment alone.
    pub fn select_only(&mut self, h: RegimentHandle) {
        *self = Selection::Regiments(vec![h]);
    }

    /// Add `h` to the regiments selected, if it is not among them. A building
    /// selected is let go.
    pub fn add(&mut self, h: RegimentHandle) {
        match self {
            Selection::Regiments(regiments) => {
                if !regiments.contains(&h) {
                    regiments.push(h);
                }
            }
            Selection::Nothing | Selection::Structure(_) => self.select_only(h),
        }
    }

    /// Add `h` to the regiments selected, or take it out if it is already in.
    pub fn toggle(&mut self, h: RegimentHandle) {
        match self {
            Selection::Regiments(regiments) => match regiments.iter().position(|&s| s == h) {
                Some(i) => {
                    regiments.swap_remove(i);
                    if regiments.is_empty() {
                        self.clear();
                    }
                }
                None => regiments.push(h),
            },
            Selection::Nothing | Selection::Structure(_) => self.select_only(h),
        }
    }

    /// Keep only the regiments `keep` says yes to. Losing the last one leaves
    /// nothing selected, not the building that was selected before them.
    pub fn retain(&mut self, keep: impl FnMut(&RegimentHandle) -> bool) {
        if let Selection::Regiments(regiments) = self {
            regiments.retain(keep);
            if regiments.is_empty() {
                self.clear();
            }
        }
    }

    /// Take the regiments out of the selection, leaving none (a building
    /// selected stays).
    pub fn take_regiments(&mut self) -> Vec<RegimentHandle> {
        match std::mem::take(self) {
            Selection::Regiments(regiments) => regiments,
            kept => {
                *self = kept;
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn regiment(index: u32) -> RegimentHandle {
        RegimentHandle {
            index,
            generation: 0,
        }
    }

    fn structure(index: u32) -> StructureHandle {
        StructureHandle {
            index,
            generation: 0,
        }
    }

    #[test]
    fn regiments_and_a_building_never_mix() {
        let mut s = Selection::default();
        s.select_structure(structure(1));
        assert_eq!(s.structure(), Some(structure(1)));
        assert!(s.is_empty());
        s.add(regiment(1));
        assert_eq!(s.regiments(), [regiment(1)]);
        assert_eq!(s.structure(), None);
        s.select_structure(structure(2));
        s.select_only(regiment(2));
        assert_eq!(s.structure(), None);
        s.select_structure(structure(3));
        s.toggle(regiment(3));
        assert_eq!(s.regiments(), [regiment(3)]);
        assert_eq!(s.structure(), None);
    }

    #[test]
    fn losing_the_regiments_does_not_bring_a_building_back() {
        let mut s = Selection::default();
        s.select_structure(structure(1));
        s.select_only(regiment(1));
        s.retain(|_| false);
        assert_eq!(s, Selection::Nothing);
        s.select_only(regiment(2));
        s.toggle(regiment(2));
        assert_eq!(s, Selection::Nothing);
    }

    #[test]
    fn clearing_keeps_what_it_was_not_asked_to_drop() {
        let mut s = Selection::from(vec![regiment(1), regiment(2)]);
        s.drop_structure();
        assert_eq!(s.len(), 2);
        s.clear_regiments();
        assert_eq!(s, Selection::Nothing);
        s.select_structure(structure(1));
        s.clear_regiments();
        assert_eq!(s.structure(), Some(structure(1)));
        assert!(s.take_regiments().is_empty());
        s.drop_structure();
        assert_eq!(s, Selection::Nothing);
        assert_eq!(Selection::from(Vec::new()), Selection::Nothing);
    }

    #[test]
    fn adding_skips_one_already_there_and_toggling_removes_it() {
        let mut s = Selection::default();
        s.add(regiment(1));
        s.add(regiment(1));
        s.add(regiment(2));
        assert_eq!(s.len(), 2);
        s.toggle(regiment(1));
        assert_eq!(s.regiments(), [regiment(2)]);
        assert_eq!(s.take_regiments(), [regiment(2)]);
        assert_eq!(s, Selection::Nothing);
    }
}
