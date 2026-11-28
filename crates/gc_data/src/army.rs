//! A Field Battle army as the lobby keeps it (docs/06-game-modes.md, "Field
//! Battle"; docs/43-heroes.md, H3): one entry a regiment, which the lobby,
//! the network and the computer players' army builder all read alike.
//!
//! An entry is a unit type in its low [`UNIT_BITS`] bits and, for a hero,
//! the hero's level (1 to [`MAX_LEVEL`]) in the bits above; a regiment's
//! entry is its unit type alone (level 0). A hero's points grow with its
//! level ([`hero_points`]).

/// Bits of an entry that hold its unit type.
pub const UNIT_BITS: u16 = 12;
/// The highest unit type an entry holds.
pub const MAX_UNIT: u16 = (1 << UNIT_BITS) - 1;
/// A hero's highest level (as `gc_sim::heroes::MAX_LEVEL`).
pub const MAX_LEVEL: u8 = 10;
/// Most heroes one army takes (as `gc_sim::heroes::MAX_HEROES`).
pub const MAX_HEROES: usize = 3;
/// What each level past the first adds to a hero's points, in percent of
/// its first level's.
pub const LEVEL_PERCENT: u32 = 15;

/// The entry for a regiment of `unit` (`level` 0), or hero `unit` at
/// `level`. Levels past [`MAX_LEVEL`] are held at it.
pub const fn entry(unit: u16, level: u8) -> u16 {
    let level = if level > MAX_LEVEL { MAX_LEVEL } else { level };
    (unit & MAX_UNIT) | (level as u16) << UNIT_BITS
}

/// The unit type of entry `e`.
pub const fn unit(e: u16) -> u16 {
    e & MAX_UNIT
}

/// The level of entry `e`: a hero's, or 0 for a regiment.
pub const fn level(e: u16) -> u8 {
    (e >> UNIT_BITS) as u8
}

/// What a hero whose first level costs `first` points costs at `level`:
/// [`LEVEL_PERCENT`] of `first` more for each level past the first, to the
/// nearest 10.
pub const fn hero_points(first: u32, level: u8) -> u32 {
    let past = if level > 1 { level as u32 - 1 } else { 0 };
    let raw = first * (100 + LEVEL_PERCENT * past) / 100;
    (raw + 5) / 10 * 10
}

/// What entry `e` costs, its unit's points being `points` (a hero's at its
/// first level).
pub const fn points(e: u16, points: u32) -> u32 {
    match level(e) {
        0 => points,
        l => hero_points(points, l),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_keeps_its_unit_and_level() {
        assert_eq!((unit(entry(37, 0)), level(entry(37, 0))), (37, 0));
        assert_eq!(entry(37, 0), 37, "a regiment's entry is its unit");
        assert_eq!((unit(entry(61, 7)), level(entry(61, 7))), (61, 7));
        assert_eq!(level(entry(61, 14)), MAX_LEVEL);
        assert_eq!(unit(entry(MAX_UNIT, MAX_LEVEL)), MAX_UNIT);
    }

    #[test]
    fn a_heroes_points_grow_with_its_level() {
        assert_eq!(hero_points(600, 1), 600);
        assert_eq!(hero_points(600, 2), 690);
        assert_eq!(hero_points(600, 10), 1410);
        assert_eq!(points(entry(3, 0), 360), 360);
        assert_eq!(points(entry(3, 5), 600), 960);
    }
}
