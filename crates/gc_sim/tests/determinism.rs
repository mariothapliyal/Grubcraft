//! Golden determinism test. CI runs this on macOS (arm64), Windows (x64) and
//! Linux (x64); the hashes must match everywhere, which is what lets Mac and
//! Windows players share a lockstep match.
//!
//! If a change intentionally alters simulation behaviour, update the golden
//! values in the same pull request and say why in its description.

use gc_sim::scenario::{DRILL_UNITS, Drill, ribs_map};
use gc_sim::{Capacities, Ground, MatchSetup, World};

const SEED: u64 = 0x6772_7562; // "grub"
const TICKS: u32 = 2_400; // two minutes of game time

/// Expected state hash every 600 ticks (30 s of game time).
const GOLDEN: [(u32, u64); 4] = [
    (600, 0x6a5f_0045_193b_8d69),
    (1200, 0x152d_dcd9_e187_a5f1),
    (1800, 0x5be0_9538_52bc_ab57),
    (2400, 0x03c0_d197_d136_7599),
];

/// The same drill on the ridged test map, where most orders need routes.
const GOLDEN_RIBS: [(u32, u64); 4] = [
    (600, 0x4dc5_a4ec_383c_ac40),
    (1200, 0x00f2_c465_655a_7256),
    (1800, 0xe52d_45d9_5622_3153),
    (2400, 0xf0fa_5e8c_8edf_7296),
];

/// The same drill on terraced, river-cut ground, where high ground and
/// fords change the fighting.
const GOLDEN_GROUND: [(u32, u64); 4] = [
    (600, 0x0a18_7102_ea3a_1038),
    (1200, 0x3d77_4ddb_c5d3_3de2),
    (1800, 0x3349_bb3a_82d7_7c89),
    (2400, 0x6d7e_a633_b4c8_f45c),
];

/// Terraces a little under a tile high, every 16 tiles, crossed by shallow
/// rivers four tiles wide; the water line is a tile up.
fn terraces() -> Vec<u16> {
    (0..256u32 * 256)
        .map(|i| {
            let (x, y) = (i % 256, i / 256);
            if (30..34).contains(&(y % 64)) {
                0
            } else {
                (512 + ((x / 16 + y / 16) % 3) * 200) as u16
            }
        })
        .collect()
}

fn run() -> Vec<(u32, u64)> {
    run_on(&[], Ground::FLAT)
}

fn run_on(nav: &[u8], ground: Ground) -> Vec<(u32, u64)> {
    let mut world = World::new(&MatchSetup {
        seed: SEED,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav,
        teams: &[],
        ground,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let mut drill = Drill::new(SEED);
    let mut commands = Vec::with_capacity(512);
    let mut checkpoints = Vec::new();
    for _ in 0..TICKS {
        drill.commands_for(&world, &mut commands);
        world.tick(&commands);
        if world.tick_count().is_multiple_of(600) {
            checkpoints.push((world.tick_count(), world.state_hash()));
        }
    }
    checkpoints
}

#[test]
fn same_inputs_same_hashes() {
    assert_eq!(run(), run());
}

#[test]
fn matches_golden_hashes() {
    let got = run();
    for (g, (tick, hash)) in GOLDEN.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn matches_golden_hashes_with_pathfinding() {
    let nav = ribs_map(256);
    let got = run_on(&nav, Ground::FLAT);
    assert_eq!(got, run_on(&nav, Ground::FLAT));
    for (g, (tick, hash)) in GOLDEN_RIBS.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn matches_golden_hashes_on_terrain() {
    let heights = terraces();
    let ground = Ground {
        heights: &heights,
        water_level: 256,
        cover: &[],
        growth: &[],
        footing: &[],
        flow: &[],
    };
    let got = run_on(&[], ground);
    assert_eq!(got, run_on(&[], ground));
    assert_ne!(got, run(), "the ground changed nothing");
    for (g, (tick, hash)) in GOLDEN_GROUND.iter().zip(&got) {
        assert_eq!(g.0, *tick);
        assert_eq!(
            g.1, *hash,
            "state hash at tick {tick} is {hash:#018x}; full run: {got:#x?}"
        );
    }
}

#[test]
fn different_seed_different_hash() {
    let mut a = World::new(&MatchSetup {
        seed: 1,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    let b = World::new(&MatchSetup {
        seed: 2,
        capacities: Capacities::default(),
        units: &DRILL_UNITS,
        map_tiles: 256,
        nav: &[],
        teams: &[],
        ground: gc_sim::Ground::FLAT,
        champions: false,
        sky: gc_sim::Sky::STILL,
        generals: true,
    });
    assert_ne!(a.state_hash(), b.state_hash());
    a.tick(&[]);
    assert_ne!(a.state_hash(), b.state_hash());
}
