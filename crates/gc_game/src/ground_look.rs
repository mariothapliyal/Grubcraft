//! How each habitat's ground looks (docs/27-living-land.md, docs/36-habitat-ground.md):
//! a colour for the grass and a pattern over it, one entry a tile for the
//! renderer. Presentation only, and made once from the map when it is loaded.
//!
//! The alpha byte holds the pattern in its low four bits and, in its high
//! four, how much snow lies on the tile whatever the weather: the Highland's
//! high ground keeps snow on it (more as it climbs), so its peaks read as
//! mountains from far off. The shader adds the weather's own snow on top.

use gc_map::Map;
use gc_map::def::Land;
use gc_map::grown::Habitat;
use gc_render::{
    GROUND_BOG, GROUND_BONE, GROUND_CRUST, GROUND_DAMP, GROUND_FLOWERS, GROUND_GRAVEL,
    GROUND_HEATHER, GROUND_LITTER, GROUND_MOSS, GROUND_MUD, GROUND_RIPPLE, GROUND_STRAW,
};

/// What the grass colour is multiplied by (128 leaves it) and the pattern
/// (0 for none), for a habitat.
fn look_of(habitat: Habitat) -> [u8; 4] {
    use Habitat::*;
    match habitat {
        FlowerMeadow => [140, 134, 122, GROUND_FLOWERS],
        Grassland => [128, 128, 128, 0],
        ScrubEdge => [122, 118, 105, 0],
        WetMeadow => [116, 128, 122, GROUND_DAMP],
        OldGrowth => [100, 102, 90, GROUND_LITTER],
        BirchFringe => [138, 138, 122, 0],
        FernHollow => [102, 125, 102, GROUND_MOSS],
        Glade => [141, 143, 122, 0],
        SedgeTussock => [128, 125, 96, 0],
        AlderCarr => [102, 108, 90, GROUND_MUD],
        PeatBog => [90, 90, 83, GROUND_BOG],
        HeatherMoor => [122, 109, 109, GROUND_HEATHER],
        Scree => [128, 128, 128, GROUND_GRAVEL],
        TreeLine => [109, 109, 100, GROUND_LITTER],
        BareRock => [128, 128, 128, GROUND_GRAVEL],
        CrackedFlat => [128, 128, 128, GROUND_CRUST],
        BoneDrift => [128, 128, 128, GROUND_RIPPLE],
        RibField => [128, 128, 128, GROUND_BONE],
        DyingGrass => [147, 134, 102, GROUND_STRAW],
        GravelBar => [168, 162, 150, GROUND_GRAVEL],
        ReedBed => [118, 128, 92, GROUND_DAMP],
        Beach => [200, 180, 128, GROUND_RIPPLE],
        // No pattern: a pattern is drawn tile by tile, and a trail one tile
        // wide would show as blocks; the colour alone blends smoothly.
        Trail => [132, 100, 70, 0],
    }
}

/// The look of ground that is soft underfoot (`Map::footing`) but whose
/// habitat does not already show it: churned mud, so the soft ground reads
/// as a place a charge bogs down. `base` is the look it had.
fn soft_look(base: [u8; 4]) -> [u8; 4] {
    match base[3] {
        GROUND_BOG | GROUND_MUD => base,
        _ => {
            let darker = |c: u8| (u16::from(c) * 4 / 5) as u8;
            [
                darker(base[0]),
                darker(base[1]),
                darker(base[2]),
                GROUND_MUD,
            ]
        }
    }
}

/// The ground's look for every tile of `map`, row by row. A map with
/// habitats looks as they say, and its soft ground ([`Map::footing`]) is
/// always muddy; one without (painted ones) keeps the plain ground except
/// for its soft ground; one with neither is empty, which keeps the plain
/// ground everywhere.
pub fn ground_look(map: &Map) -> Vec<[u8; 4]> {
    let mut look = patterns(map);
    let caps = snow_caps(map);
    if caps.iter().any(|&c| c > 0) {
        if look.is_empty() {
            look = vec![PLAIN; caps.len()];
        }
        for (l, c) in look.iter_mut().zip(caps) {
            l[3] |= c << 4;
        }
    }
    look
}

/// Where the Highland's snow begins, as a share of the way up its tiles'
/// heights (and never below this share of the whole map's), and where it
/// lies in full.
const SNOW_LINE: f32 = 0.55;
const SNOW_LINE_MAP: f32 = 0.6;
const SNOW_FULL: f32 = 0.97;
/// The most snow a tile keeps, of 15 (the weather's blanket is 15): patches
/// on the peaks, never a sheet that hides the rock and heather.
const SNOW_MOST: f32 = 11.0;

/// A tile's look with nothing on it.
const PLAIN: [u8; 4] = [128, 128, 128, 0];

/// How much snow lies on each tile whatever the weather, 0 to 15, row by
/// row: on the Highland's higher ground only, rising with the height (empty
/// for a map with no Highland).
fn snow_caps(map: &Map) -> Vec<u8> {
    let tiles = (map.tiles * map.tiles) as usize;
    if map.heights.len() != tiles {
        return Vec::new();
    }
    let highland: Vec<bool> = (0..tiles)
        .map(|i| {
            let land = map
                .land_at(i)
                .or_else(|| map.habitat.get(i).map(|&b| Habitat::from_u8(b).land()));
            land == Some(Land::Highland) && map.heights[i] > map.level_of(i)
        })
        .collect();
    let mut high: Vec<u16> = (0..tiles)
        .filter(|&i| highland[i])
        .map(|i| map.heights[i])
        .collect();
    // A scrap of Highland is no mountain range.
    if high.len() < 64 {
        return Vec::new();
    }
    let line =
        quantile(&mut high, SNOW_LINE).max(quantile(&mut map.heights.clone(), SNOW_LINE_MAP));
    let full = quantile(&mut high, SNOW_FULL);
    if full <= line {
        return Vec::new();
    }
    (0..tiles)
        .map(|i| {
            if !highland[i] {
                return 0;
            }
            let share = ((f32::from(map.heights[i]) - line) / (full - line)).clamp(0.0, 1.0);
            (share * SNOW_MOST).round() as u8
        })
        .collect()
}

/// The height `share` of the way up `heights` (which it reorders).
fn quantile(heights: &mut [u16], share: f32) -> f32 {
    let at = ((heights.len() - 1) as f32 * share) as usize;
    f32::from(*heights.select_nth_unstable(at).1)
}

/// The grass colour and pattern of every tile (see [`ground_look`]).
fn patterns(map: &Map) -> Vec<[u8; 4]> {
    let tiles = (map.tiles * map.tiles) as usize;
    let soft = |i: usize| {
        (map.footing.get(i)).is_some_and(|&f| gc_sim::Footing::from_u8(f) == gc_sim::Footing::Soft)
    };
    if map.habitat.len() == tiles {
        (map.habitat.iter().enumerate())
            .map(|(i, &b)| {
                let look = look_of(Habitat::from_u8(b));
                if soft(i) { soft_look(look) } else { look }
            })
            .collect()
    } else if map.footing.len() == tiles {
        (0..tiles)
            .map(|i| if soft(i) { soft_look(PLAIN) } else { PLAIN })
            .collect()
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_map_without_habitats_has_no_look() {
        assert!(ground_look(&Map::generate(1, 64)).is_empty());
    }

    #[test]
    fn the_bog_and_carr_read_wet_and_sedge_stays_plain() {
        assert_eq!(look_of(Habitat::PeatBog)[3], GROUND_BOG);
        assert_eq!(look_of(Habitat::AlderCarr)[3], GROUND_MUD);
        // Sedge stays firm and plain: it must not read as wet as the soft ground.
        assert_eq!(look_of(Habitat::SedgeTussock)[3], 0);
    }

    #[test]
    fn soft_ground_is_always_muddy_and_firm_ground_is_left_alone() {
        // A painted map has no habitats: only its soft ground has a look.
        let mut map = Map::generate(1, 64);
        assert!(ground_look(&map).is_empty());
        map.footing = vec![0; 64 * 64];
        map.footing[5] = 1;
        let look = ground_look(&map);
        assert_eq!(look.len(), 64 * 64);
        assert_eq!(look[5][3], GROUND_MUD);
        assert_eq!(look[4], [128, 128, 128, 0]);
        // On a grown map a soft tile of a dry habitat gets mud in its own colour;
        // bog and carr keep theirs.
        map.habitat = vec![Habitat::Grassland as u8; 64 * 64];
        map.habitat[6] = Habitat::PeatBog as u8;
        map.footing[6] = 1;
        let look = ground_look(&map);
        assert_eq!(look[5], [102, 102, 102, GROUND_MUD]);
        assert_eq!(look[6], look_of(Habitat::PeatBog));
        map.habitat[7] = Habitat::AlderCarr as u8;
        map.footing[7] = 1;
        assert_eq!(ground_look(&map)[7], look_of(Habitat::AlderCarr));
        assert_eq!(look[4], look_of(Habitat::Grassland));
    }

    #[test]
    fn snow_lies_on_the_highlands_high_ground_only() {
        let mut map = Map::generate(1, 64);
        let n = 64 * 64;
        // A ramp: the west half meadow, the east half Highland, rising east.
        map.heights = (0..n)
            .map(|i| map.water + 10 + (i % 64) as u16 * 100)
            .collect();
        map.land = (0..n)
            .map(|i| {
                if i % 64 >= 32 {
                    Land::Highland as u8
                } else {
                    Land::Meadow as u8
                }
            })
            .collect();
        let look = ground_look(&map);
        assert_eq!(look.len(), n);
        let snow = |x: usize| look[10 * 64 + x][3] >> 4;
        // Meadow keeps none however high; the Highland's foot none, its top most.
        assert_eq!(snow(31), 0);
        assert_eq!(snow(33), 0);
        assert!(snow(63) >= 10, "{}", snow(63));
        assert!(snow(50) > 0 && snow(50) < snow(63));
        // The pattern bits are left alone.
        assert!(look.iter().all(|l| l[3] & 15 == 0));
        // Without Highland there is no snow, and no look.
        map.land = vec![Land::Meadow as u8; n];
        assert!(ground_look(&map).is_empty());
    }

    #[test]
    fn a_grown_map_gets_a_look_a_tile() {
        let mut map = Map::generate(1, 64);
        map.habitat = (0..64 * 64).map(|i| (i % 19) as u8).collect();
        let look = ground_look(&map);
        assert_eq!(look.len(), 64 * 64);
        assert_eq!(look[1], look_of(Habitat::from_u8(1)));
    }
}
