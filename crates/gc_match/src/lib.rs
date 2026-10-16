//! A match without a window (review fix 13): what the game, the golden
//! replays, the computer players' ladders and the benchmarks all need to
//! play a battle the same way, with no window, GPU or sound. The map is made
//! here (`terrain_gen`, without floats, so every machine makes the same one)
//! with its float views for the computer players (`terrain_view`), and
//! [`map_hash`] fingerprints it so players can check they all made the same.

pub mod terrain_gen;
pub mod terrain_view;

/// A fingerprint of a map's walkable ground, hills, water and cover (FNV-1a), so a joiner whose
/// map came out differently finds out before the battle starts.
pub fn map_hash(nav: &[u8], ground: &gc_sim::Ground) -> u64 {
    let heights = ground.heights.iter().chain([&ground.water_level]);
    let bytes = (nav.iter().copied())
        .chain(heights.flat_map(|v| v.to_le_bytes()))
        .chain(ground.cover.iter().copied());
    fnv1a(bytes) ^ nav.len() as u64
}

/// FNV-1a over a stream of bytes.
pub fn fnv1a(bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes.into_iter().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_hash_sees_every_cell() {
        let flat = gc_sim::Ground::FLAT;
        let a = vec![1u8; 1000];
        let mut b = a.clone();
        b[999] = 2;
        assert_ne!(map_hash(&a, &flat), map_hash(&b, &flat));
        assert_ne!(map_hash(&a, &flat), map_hash(&a[..999], &flat));
        assert_eq!(map_hash(&a, &flat), map_hash(&a.clone(), &flat));
        // Heights and water count too.
        let heights = vec![7u16; 1000];
        let mut raised = heights.clone();
        raised[3] = 8;
        let ground = |heights, water_level| gc_sim::Ground {
            heights,
            water_level,
            cover: &[],
        };
        let base = map_hash(&a, &ground(&heights, 2));
        assert_ne!(base, map_hash(&a, &ground(&raised, 2)));
        assert_ne!(base, map_hash(&a, &ground(&heights, 3)));
        assert_ne!(base, map_hash(&a, &flat));
        // And so do the woods and tall grass.
        let wood = vec![2u8; 1000];
        let mut grass = wood.clone();
        grass[500] = 1;
        let covered = |cover| gc_sim::Ground {
            cover,
            ..ground(&heights, 2)
        };
        assert_ne!(base, map_hash(&a, &covered(&wood)));
        assert_ne!(
            map_hash(&a, &covered(&wood)),
            map_hash(&a, &covered(&grass))
        );
    }
}
