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

pub mod battle;
pub mod conquest;
pub mod def;
mod generate;
pub mod grown;
mod map;
pub mod recipe;
mod stamp;
pub mod view;
pub mod wilds;

pub use def::{Kind, MapDef, Pos, SizeClass, Symmetry};
pub use generate::{START_PLATEAUS, plateaus};
pub use map::{
    CLIFF_RISE_PER_TILE, Ground, MAX_HEIGHT, Map, Placed, SAMPLES_PER_TILE, fnv1a, map_hash,
};

/// Which version of the map maker this build is. Bump it whenever the same
/// lobby map choice, size and seed make different land (grown land, the
/// Wilds, the recipes, the seeded map), so a replay recorded before can tell
/// its map would now be made differently and is drawn from the ground it
/// stored instead. A change that leaves every map as it was doesn't need it.
pub const MAP_MAKER: u16 = 1;

/// The maps a Field Battle can be played on, in the lobby's order: the
/// seeded map, a random map from each recipe, a random recipe, the Classic
/// Wilds as wild as the seed picks and at each [`wilds::Wildness`], then
/// grown land (Living Land, [`grown`]) the same way, mirrored, then the
/// same with measured fairness. Each group was added after the last, so
/// older replays keep their choices.
pub const FIELD_MAPS: [&str; 18] = [
    "Proving Ground",
    "Random: Pass",
    "Random: Ford",
    "Random: Hill",
    "Random: Open",
    "Random: Any",
    "Classic: Random",
    "Classic: Plains",
    "Classic: Mixed",
    "Classic: Wild",
    "Land: Random",
    "Land: Plains",
    "Land: Mixed",
    "Land: Wild",
    "Measured: Random",
    "Measured: Plains",
    "Measured: Mixed",
    "Measured: Wild",
];

/// The first Classic Wilds choice in [`FIELD_MAPS`], as wild as the seed
/// picks. Each [`wilds::Wildness`] follows it.
pub const WILDS: usize = 6;

/// The first grown land choice in [`FIELD_MAPS`] (mirrored), as wild as
/// the seed picks. Each
/// [`wilds::Wildness`] follows it.
pub const GROWN: usize = WILDS + 1 + wilds::Wildness::ALL.len();

/// The first grown land choice with measured fairness, as wild as the seed
/// picks: the one a new lobby starts on. Each [`wilds::Wildness`] follows
/// it.
pub const MEASURED: usize = GROWN + 1 + wilds::Wildness::ALL.len();

/// A Field Battle map: the lobby's choice (an index into [`FIELD_MAPS`]),
/// its size and its seed, and for Measured land made for three or five
/// players that count. The same make the same map on every machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldMap {
    pub choice: usize,
    pub tiles: u32,
    pub seed: u64,
    /// Three or five on a Measured choice made for that many players,
    /// whose starts stand round a ring ([`grown::Grown::ring`]); 0
    /// otherwise.
    pub ring: u8,
}

impl FieldMap {
    /// The map the lobby makes for `players`: its starts round a ring on a
    /// Measured choice with three or five ([`FieldMap::ring`]).
    pub fn new(choice: usize, tiles: u32, seed: u64, players: usize) -> FieldMap {
        let measured = (MEASURED..=MEASURED + wilds::Wildness::ALL.len()).contains(&choice);
        let fairness = if measured {
            grown::Fairness::Measured
        } else {
            grown::Fairness::Mirrored
        };
        FieldMap {
            choice,
            tiles,
            seed,
            ring: grown::ring_for(players, fairness),
        }
    }

    /// Make the map. A random map whose starts aren't all joined by
    /// walkable ground is made again from the next seed (none of the seeds
    /// the tests sweep need it), so nobody is shown a broken one.
    pub fn make(self) -> Map {
        let FieldMap {
            choice,
            tiles,
            seed,
            ring,
        } = self;
        enum Maker {
            Recipe(recipe::Recipe),
            Wilds(wilds::Wildness),
            Grown(wilds::Wildness, grown::Fairness),
        }
        let (recipes, lands) = (recipe::Recipe::ALL, wilds::Wildness::ALL);
        let pick = |count: usize| (seed % count as u64) as usize;
        let maker = match choice {
            0 => return Map::generate(seed, tiles),
            c if c <= recipes.len() => Maker::Recipe(recipes[c - 1]),
            c if c == recipes.len() + 1 => Maker::Recipe(recipes[pick(recipes.len())]),
            WILDS => Maker::Wilds(lands[pick(lands.len())]),
            c if c <= WILDS + lands.len() => Maker::Wilds(lands[c - WILDS - 1]),
            GROWN => Maker::Grown(lands[pick(lands.len())], grown::Fairness::Mirrored),
            c if c <= GROWN + lands.len() => {
                Maker::Grown(lands[c - GROWN - 1], grown::Fairness::Mirrored)
            }
            MEASURED => Maker::Grown(lands[pick(lands.len())], grown::Fairness::Measured),
            c if c <= MEASURED + lands.len() => {
                Maker::Grown(lands[c - MEASURED - 1], grown::Fairness::Measured)
            }
            _ => return Map::generate(seed, tiles),
        };
        let make = |k| {
            let seed = seed.wrapping_add(k);
            Map::from_def(&match maker {
                Maker::Recipe(recipe) => recipe::describe(recipe, tiles as i32, seed),
                Maker::Wilds(wild) => wilds::describe(tiles as i32, seed, wild),
                Maker::Grown(wild, fair) => {
                    grown::describe(tiles as i32, seed, wild, fair, usize::from(ring))
                }
            })
        };
        let first = make(0);
        if first.starts_joined() {
            return first;
        }
        (1..8).map(make).find(Map::starts_joined).unwrap_or(first)
    }
}

/// The maps built into the game, by file name without `.ron`, in the
/// order the lobby lists them.
pub const SHIPPED: &[(&str, &str)] = &[
    (
        "ribcage_pass",
        include_str!("../../../data/maps/ribcage_pass.ron"),
    ),
    (
        "four_teeth",
        include_str!("../../../data/maps/four_teeth.ron"),
    ),
];

/// A shipped map's description, by file name without `.ron`
/// (`"ribcage_pass"`).
pub fn shipped(name: &str) -> Option<MapDef> {
    let (_, text) = SHIPPED.iter().find(|(n, _)| *n == name)?;
    // The shipped maps parse: a test checks every one.
    MapDef::parse(text).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_map_in_data_maps_is_shipped_and_parses() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/maps");
        let mut files: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(|e| {
                let path = e.unwrap().path();
                (path.extension()? == "ron")
                    .then(|| path.file_stem()?.to_str().map(String::from))?
            })
            .collect();
        files.sort();
        let mut shipped: Vec<String> = SHIPPED.iter().map(|(n, _)| (*n).to_string()).collect();
        shipped.sort();
        assert_eq!(files, shipped, "add new maps to SHIPPED");
        for (name, text) in SHIPPED {
            let def = MapDef::parse(text).unwrap_or_else(|e| panic!("{name}: {e}"));
            let map = Map::from_def(&def);
            assert_eq!(map.starts().len(), usize::from(def.players), "{name}");
        }
    }

    /// Like the seeded maps' fingerprints: a described map builds the same
    /// bytes on every machine. If a map file or the stamping changes on
    /// purpose, update the numbers.
    #[test]
    fn shipped_maps_are_the_same_on_every_machine() {
        let hash = |name| Map::from_def(&shipped(name).unwrap()).hash();
        assert_eq!(hash("ribcage_pass"), 0x3dd0_e864_31a5_88c4);
        assert_eq!(hash("four_teeth"), 0x44a9_affb_53d9_a5a6);
    }

    /// Choice 0 is the seeded map, as it has always been.
    #[test]
    fn choice_0_is_the_seeded_map() {
        for seed in [1, 7] {
            assert_eq!(
                FieldMap {
                    choice: 0,
                    tiles: 256,
                    seed,
                    ring: 0,
                }
                .make()
                .hash(),
                Map::generate(seed, 256).hash(),
                "seed {seed}"
            );
        }
    }

    /// After the recipes (see `recipe_names_are_the_lobby_names`), the
    /// lobby names the Classic Wilds and grown land by how wild.
    #[test]
    fn the_wilds_choices_are_named_for_how_wild() {
        for (first, kind) in [(WILDS, "Classic"), (GROWN, "Land"), (MEASURED, "Measured")] {
            assert_eq!(FIELD_MAPS[first], format!("{kind}: Random"));
            for (k, wild) in wilds::Wildness::ALL.iter().enumerate() {
                let name = format!("{kind}: {}", wild.name());
                assert_eq!(FIELD_MAPS[first + 1 + k], name);
            }
        }
        assert_eq!(FIELD_MAPS.len(), MEASURED + 1 + wilds::Wildness::ALL.len());
    }

    /// Every lobby choice, at both Small sizes, makes a map whose starts
    /// can all walk to each other and that is the size asked for (the big
    /// sizes are in `gc_mapcheck`'s tests, which run them side by side).
    #[test]
    fn every_field_map_joins_its_starts() {
        for choice in 0..FIELD_MAPS.len() {
            for tiles in [256, 320] {
                for seed in 1..=4 {
                    let map = FieldMap {
                        choice,
                        tiles,
                        seed,
                        ring: 0,
                    }
                    .make();
                    let what = format!("choice {choice}, {tiles} tiles, seed {seed}");
                    assert!(map.starts_joined(), "{what}");
                    assert_eq!(map.tiles, tiles, "{what}");
                }
            }
        }
    }

    /// Highland water (docs/27 stage 5) is the same water to the routes and
    /// to the simulation: a tile whose middle the simulation has under its
    /// own level is wading or deep water on the route grid, and one well
    /// above it is not.
    #[test]
    fn routes_and_the_simulation_agree_on_highland_water() {
        let mut highland = 0;
        for choice in [GROWN + 1, GROWN + 2, GROWN + 3] {
            let map = FieldMap {
                choice,
                tiles: 256,
                seed: 7,
                ring: 0,
            }
            .make();
            if map.levels.is_empty() {
                continue;
            }
            let nav = map.nav_costs();
            let world = gc_sim::World::new(&gc_sim::MatchSetup {
                seed: 1,
                capacities: gc_sim::Capacities::default(),
                units: &gc_sim::scenario::DRILL_UNITS,
                map_tiles: map.tiles,
                nav: &nav,
                teams: &[],
                ground: map.ground(),
                champions: false,
                sky: gc_sim::Sky::STILL,
                generals: false,
            });
            let terrain = world.terrain();
            let per = gc_sim::nav::CELLS_PER_TILE as usize;
            let side = map.tiles as usize * per;
            let (mut agree, mut asked) = (0, 0);
            for (i, &level) in map.levels.iter().enumerate() {
                let height = map.heights[i];
                if level == 0 || height.abs_diff(level) < 24 {
                    continue;
                }
                let (x, y) = (i % map.tiles as usize, i / map.tiles as usize);
                let centre = gc_fixed::FxVec2::new(
                    gc_fixed::Fx::from_int(x as i32) + gc_fixed::Fx::HALF,
                    gc_fixed::Fx::from_int(y as i32) + gc_fixed::Fx::HALF,
                );
                let wet = terrain.wet(centre);
                assert_eq!(wet, height < level, "tile {x},{y}");
                // The tile's middle is the corner its four cells share.
                let cells =
                    (0..per * per).map(|k| nav[(y * per + k / per) * side + x * per + k % per]);
                let watery = cells
                    .map(|c| c == gc_sim::nav::BLOCKED || c == gc_sim::nav::FORD)
                    .collect::<Vec<_>>();
                asked += 1;
                // Wet: some cell is water. Dry: not all are (a cliff
                // blocks too).
                agree += usize::from(if wet {
                    watery.iter().any(|&w| w)
                } else {
                    !watery.iter().all(|&w| w)
                });
                highland += usize::from(wet);
                // A dry shore tile with dry land all round has no wading
                // cell: the shore band stops short of falling land.
                let n = map.tiles as usize;
                let dry_round = (y > 0 && y + 1 < n && x > 0 && x + 1 < n)
                    && (0..9).all(|k| {
                        let j = (y + k / 3 - 1) * n + x + k % 3 - 1;
                        map.heights[j] >= map.level_of(j)
                    });
                if !wet && dry_round {
                    let fords = (0..per * per)
                        .filter(|k| {
                            nav[(y * per + k / per) * side + x * per + k % per] == gc_sim::nav::FORD
                        })
                        .count();
                    assert_eq!(fords, 0, "{choice}: wading on dry shore tile {x},{y}");
                }
            }
            assert!(
                agree * 100 >= asked * 97,
                "{choice}: {agree} of {asked} agree"
            );
        }
        assert!(highland > 0, "no highland water on the grown maps");
    }

    /// Like the shipped maps' fingerprints: the random Field Battle maps
    /// build the same bytes on every machine. The recipes and the stamping
    /// set these numbers, so a change to either changes every random map.
    /// Update the numbers only when that is meant.
    #[test]
    fn random_field_maps_are_the_same_on_every_machine() {
        let hash = |choice| {
            FieldMap {
                choice,
                tiles: 256,
                seed: 7,
                ring: 0,
            }
            .make()
            .hash()
        };
        assert_eq!(hash(1), 0xa2f8_4316_7491_1a36);
        assert_eq!(hash(2), 0x5359_e1ae_9a42_f5a4);
        assert_eq!(hash(3), 0x659c_f795_05bd_b656);
        assert_eq!(hash(4), 0xc44a_2da5_5aeb_eb5a);
        // The Small size for five or six players too.
        let at_320 = |choice| {
            FieldMap {
                choice,
                tiles: 320,
                seed: 1,
                ring: 0,
            }
            .make()
            .hash()
        };
        assert_eq!(at_320(1), 0x1175_1bfe_378b_818e);
        assert_eq!(at_320(4), 0x4810_c8ad_25ff_bc7c);
        // Grown land: erosion and the river lines (and their current) are
        // the parts most likely to drift, and each of the dials.
        assert_eq!(hash(GROWN + 1), 0x4703_8b16_3011_30c2);
        assert_eq!(hash(GROWN + 2), 0x1ff1_ebb1_47ce_def2);
        assert_eq!(hash(GROWN + 3), 0xf96d_3d17_4c9b_bd9a);
        // Measured fairness: the start search, its settling on the true
        // walks and the paired sites.
        assert_eq!(hash(MEASURED + 2), 0x5ec5_f069_fb84_881e);
        // Three and five players round a ring.
        let ring = |tiles, seed, ring| {
            FieldMap {
                choice: MEASURED + 2,
                tiles,
                seed,
                ring,
            }
            .make()
            .hash()
        };
        assert_eq!(ring(256, 7, 3), 0xb239_9986_fa36_6314);
        assert_eq!(ring(320, 1, 5), 0xeabe_c64d_5701_940d);
    }

    /// Only a Measured choice for three or five players puts its starts
    /// round a ring.
    #[test]
    fn rings_are_for_three_or_five_on_measured_land() {
        for players in 0..=6 {
            for choice in 0..FIELD_MAPS.len() {
                let ring = FieldMap::new(choice, 256, 1, players).ring;
                let measured = (MEASURED..FIELD_MAPS.len()).contains(&choice);
                let want = if measured && matches!(players, 3 | 5) {
                    players as u8
                } else {
                    0
                };
                assert_eq!(ring, want, "{} with {players}", FIELD_MAPS[choice]);
            }
        }
        let map = FieldMap::new(MEASURED + 1, 256, 2, 3).make();
        assert_eq!(map.starts().len(), 3);
    }
}
