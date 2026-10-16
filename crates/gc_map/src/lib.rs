//! Grubcraft's maps: where a battlefield comes from, and the one set of
//! terrain rules that turns it into what the simulation plays on.
//!
//! A map is made one of two ways, and both give the same [`Map`]:
//!
//! - [`Map::generate`]: the seeded noise map every Field Battle has played
//!   on so far (rolling hide, the Titan's rib-ridges, four start plateaus).
//! - [`Map::from_def`]: a map description, the RON format of `data/maps`
//!   ([`MapDef`]: a base height, terrain and cover stamps, objects, with
//!   rotational symmetry added by the loader). [`shipped`] lists the maps
//!   built into the game; [`MapDef::load`] reads one from a file.
//!
//! From a [`Map`], everything else is derived with the same integer rules
//! whichever way it was made: [`Map::nav_costs`] (the half-tile
//! walkable-ground grid), [`Map::ground`] (tile heights, water and cover for
//! `gc_sim::MatchSetup`), [`Map::tile_ground`] (blocked, open, steep or ford
//! per tile, for the map checker and object placement), [`Map::starts`]
//! and [`map_hash`] (the fingerprint machines compare before a network
//! match). The float views for drawing and the computer players are in
//! [`view`].
//!
//! Map making uses no floats (outside [`view`]), so every machine builds
//! the same bytes from the same seed or file; `gc_sim`'s `no_floats` test
//! and the lint below hold it to that.
//!
//! See `docs/07-world-and-maps.md` for the format and `docs/10-architecture.md`
//! for where this crate sits.

// The lint also catches float maths through inferred types or literals,
// which the token check in `gc_sim/tests/no_floats.rs` can miss. Test
// modules and the drawing views may use floats.
#![cfg_attr(not(test), deny(clippy::float_arithmetic))]

pub mod def;
mod generate;
mod map;
mod stamp;
pub mod view;

pub use def::{Kind, MapDef, Pos, SizeClass, Symmetry};
pub use generate::START_PLATEAUS;
pub use map::{Ground, MAX_HEIGHT, Map, Placed, SAMPLES_PER_TILE, fnv1a, map_hash};

/// The maps built into the game, by file name without `.ron`, in the
/// order the lobby lists them.
pub const SHIPPED: &[(&str, &str)] = &[
    (
        "ribcage_pass",
        include_str!("../../../data/maps/ribcage_pass.ron"),
    ),
    ("four_teeth", include_str!("../../../data/maps/four_teeth.ron")),
];

/// A shipped map's description, by file name without `.ron`
/// (`"ribcage_pass"`).
pub fn shipped(name: &str) -> Option<MapDef> {
    let (_, text) = SHIPPED.iter().find(|(n, _)| *n == name)?;
    // The shipped maps parse: a test checks every one.
    MapDef::parse(text).ok()
}
