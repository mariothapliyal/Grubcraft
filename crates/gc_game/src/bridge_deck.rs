//! How high a bridge's deck is drawn over each of its tiles. A deck runs
//! level from bank to bank, or slopes evenly between banks of different
//! heights, rather than each tile sitting on the ground (or water) under
//! it: that left the tiles on the banks standing a step above the ones
//! over the river. The drawing alone: soldiers walk the same tiles.

use glam::Vec2;

use crate::buildings::centre_of;
use crate::wall_kit::NEIGHBOURS;

/// A bridge tile and the map axis its bridge runs along there (one step,
/// e.g. `(1, 0)`).
pub type Lane = ((i32, i32), (i32, i32));

/// The deck height over every bridge tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Decks {
    /// Every bridge tile with the map axis its bridge runs along there
    /// (one step, e.g. `(1, 0)`), sorted for a binary search.
    tiles: Vec<Lane>,
    /// The height of the deck's underside at each tile's middle, in tiles
    /// (where the drawn ground would be).
    floors: Vec<f32>,
}

/// At most this many times the deck is lifted over ground standing above
/// it and laid again.
const LIFTS: usize = 8;

/// Ground more than this above the deck lifts it (in tiles).
const RISE: f32 = 1e-3;

impl Decks {
    /// The decks over `tiles` (each with the axis its bridge runs along
    /// there). A deck runs straight from the tile where it meets the bank
    /// at one end to where it meets it at the other, each at the height
    /// drawn there (`drawn`, at a tile's middle), so it is level between
    /// banks of one height and slopes evenly between banks of two; where
    /// the ground under it rises above that line, it rests on the ground
    /// there and runs straight on either side. A tile meets the bank where
    /// the next tile along its axis is dry land (`dry`, at a tile's middle)
    /// with no bridge on it. A deck never sinks below what is drawn under
    /// it, and a stretch meeting no bank stays on the water.
    pub fn new(
        mut tiles: Vec<Lane>,
        drawn: impl Fn(Vec2) -> f32,
        dry: impl Fn(Vec2) -> bool,
    ) -> Decks {
        tiles.sort_unstable();
        let mid = |t: (i32, i32)| centre_of(t, 1, 1);
        let find = |t: (i32, i32)| find(&tiles, t);
        let ground: Vec<f32> = tiles.iter().map(|&(t, _)| drawn(mid(t))).collect();
        let mut fixed: Vec<bool> = (tiles.iter())
            .map(|&((x, y), (ax, ay))| {
                [(x + ax, y + ay), (x - ax, y - ay)]
                    .into_iter()
                    .any(|t| find(t).is_none() && dry(mid(t)))
            })
            .collect();
        // Each tile's neighbours on the bridge, diagonals too, so a curving
        // deck stays joined.
        let near: Vec<Vec<usize>> = (tiles.iter())
            .map(|&((x, y), _)| {
                (NEIGHBOURS.iter())
                    .filter_map(|&(dx, dy)| find((x + dx, y + dy)))
                    .collect()
            })
            .collect();
        let mut floors = ground.clone();
        for _ in 0..LIFTS {
            floors = straight(&near, &fixed, &ground);
            // Ground above the deck holds it up there.
            let mut lifted = false;
            for i in 0..tiles.len() {
                if !fixed[i] && ground[i] > floors[i] + RISE {
                    fixed[i] = true;
                    lifted = true;
                }
            }
            if !lifted {
                break;
            }
        }
        for (floor, g) in floors.iter_mut().zip(&ground) {
            *floor = floor.max(*g);
        }
        Decks { tiles, floors }
    }

    /// Whether a bridge tile is at `tile`.
    pub fn has(&self, tile: (i32, i32)) -> bool {
        self.find(tile).is_some()
    }

    /// The height of the deck's underside over `tile`, if a bridge is there.
    pub fn floor(&self, tile: (i32, i32)) -> Option<f32> {
        self.find(tile).map(|i| self.floors[i])
    }

    fn find(&self, tile: (i32, i32)) -> Option<usize> {
        find(&self.tiles, tile)
    }

    /// The deck's floor over the tile at point `p`, if a bridge is there.
    pub fn floor_at(&self, p: Vec2) -> Option<f32> {
        self.floor((p.x.floor() as i32, p.y.floor() as i32))
    }
}

fn find(tiles: &[Lane], tile: (i32, i32)) -> Option<usize> {
    tiles.binary_search_by_key(&tile, |e| e.0).ok()
}

/// The deck over every tile, straight between the `fixed` ones (which
/// keep their `ground`). The fixed tiles touching one another are one
/// bank (or rock); a free tile takes a mix of the nearest two banks it is
/// joined to over the bridge, each weighted by how near the other is, so
/// it lies on the straight line between them (a lane of a straight bridge
/// exactly), or the one bank it is joined to, or else its own ground.
fn straight(near: &[Vec<usize>], fixed: &[bool], ground: &[f32]) -> Vec<f32> {
    const FAR: u32 = u32::MAX;
    let n = near.len();
    // The nearest two banks to each tile: (steps, height).
    let mut best = vec![[(FAR, 0.0f32); 2]; n];
    let mut bank = vec![false; n];
    let mut steps = vec![FAR; n];
    let mut height = vec![0.0f32; n];
    let mut reached = Vec::new();
    for start in 0..n {
        if !fixed[start] || bank[start] {
            continue;
        }
        // This bank's tiles, then how far each free tile is from it over
        // the bridge (not through another bank: the one behind that is
        // not the next along) and the height of the bank tile nearest it.
        // Only the tiles reached are touched, so each bank costs only its
        // own bridge.
        reached.clear();
        reached.push(start);
        bank[start] = true;
        let mut k = 0;
        while let Some(&i) = reached.get(k) {
            k += 1;
            steps[i] = 0;
            height[i] = ground[i];
            for &j in &near[i] {
                if fixed[j] && !bank[j] {
                    bank[j] = true;
                    reached.push(j);
                }
            }
        }
        let mut k = 0;
        while let Some(&i) = reached.get(k) {
            k += 1;
            for &j in &near[i] {
                if !fixed[j] && steps[j] == FAR {
                    steps[j] = steps[i] + 1;
                    height[j] = height[i];
                    reached.push(j);
                }
            }
        }
        for &i in &reached {
            let b = &mut best[i];
            let here = (steps[i], height[i]);
            if here.0 < b[0].0 {
                *b = [here, b[0]];
            } else if here.0 < b[1].0 {
                b[1] = here;
            }
            steps[i] = FAR;
        }
    }
    (0..n)
        .map(|i| match best[i] {
            _ if fixed[i] => ground[i],
            [(FAR, _), _] => ground[i],
            [(_, h), (FAR, _)] => h,
            [(d1, h1), (d2, h2)] => (h1 * d2 as f32 + h2 * d1 as f32) / (d1 + d2) as f32,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: f32 = 1.0;
    const EAST: (i32, i32) = (1, 0);

    /// A river over columns 3 to 8 (drawn at the water), banks at `west`
    /// and `east` on either side.
    fn river(west: f32, east: f32) -> (impl Fn(Vec2) -> f32, impl Fn(Vec2) -> bool) {
        let drawn = move |p: Vec2| {
            if p.x < 3.0 {
                west
            } else if p.x >= 9.0 {
                east
            } else {
                WATER
            }
        };
        let dry = |p: Vec2| !(3.0..9.0).contains(&p.x);
        (drawn, dry)
    }

    /// A bridge running east over columns 2 to 9, `rows` lanes wide.
    fn span(rows: std::ops::Range<i32>) -> Vec<Lane> {
        (2..=9)
            .flat_map(|x| rows.clone().map(move |y| ((x, y), EAST)))
            .collect()
    }

    fn row(decks: &Decks, y: i32) -> Vec<f32> {
        (2..=9).map(|x| decks.floor((x, y)).unwrap()).collect()
    }

    fn even(row: &[f32]) -> bool {
        let step = (row[row.len() - 1] - row[0]) / (row.len() - 1) as f32;
        row.windows(2).all(|w| (w[1] - w[0] - step).abs() < 2e-3)
    }

    #[test]
    fn a_deck_between_banks_of_one_height_is_level() {
        let (drawn, dry) = river(1.4, 1.4);
        let decks = Decks::new(span(0..2), drawn, dry);
        for y in 0..2 {
            assert!(row(&decks, y).iter().all(|f| (f - 1.4).abs() < 1e-3));
        }
    }

    #[test]
    fn a_deck_between_banks_of_two_heights_slopes_evenly() {
        let (drawn, dry) = river(1.2, 1.9);
        let decks = Decks::new(span(0..3), drawn, dry);
        let middle = row(&decks, 1);
        assert!((middle[0] - 1.2).abs() < 1e-6 && (middle[7] - 1.9).abs() < 1e-6);
        assert!(even(&middle), "{middle:?}");
        // The lanes side by side are at one height.
        for y in [0, 2] {
            let lane = row(&decks, y);
            assert!(
                lane.iter().zip(&middle).all(|(a, b)| (a - b).abs() < 2e-3),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn a_steep_bank_under_the_deck_does_not_step_it() {
        // Dry land under the east end: low by the water, a cliff up to the
        // far bank. The deck runs straight from end to end over it.
        let drawn = |p: Vec2| match p.x {
            x if x < 3.0 => 1.2,
            x if x < 6.0 => WATER,
            x if x < 9.0 => 1.1,
            _ => 1.8,
        };
        let dry = |p: Vec2| !(3.0..6.0).contains(&p.x);
        let decks = Decks::new(span(0..2), drawn, dry);
        let lane = row(&decks, 0);
        assert!(even(&lane), "{lane:?}");
        assert!((lane[7] - 1.8).abs() < 1e-6);
    }

    #[test]
    fn a_deck_rests_on_ground_rising_above_it() {
        // A rock in the river higher than both banks: the deck goes over it.
        let drawn = |p: Vec2| if (5.0..6.0).contains(&p.x) { 2.0 } else { 1.2 };
        let dry = |p: Vec2| !(3.0..9.0).contains(&p.x) || (5.0..6.0).contains(&p.x);
        let decks = Decks::new(span(0..1), drawn, dry);
        let lane = row(&decks, 0);
        assert!((lane[3] - 2.0).abs() < 1e-6, "{lane:?}");
        assert!(even(&lane[..4]) && even(&lane[3..]), "{lane:?}");
    }

    #[test]
    fn a_deck_runs_straight_from_a_rock_to_the_far_bank() {
        // A rock near the west bank: east of it the deck runs straight from
        // the rock to the east bank, not bent back toward the west bank.
        let drawn = |p: Vec2| if (3.0..4.0).contains(&p.x) { 2.0 } else { 1.2 };
        let dry = |p: Vec2| !(3.0..9.0).contains(&p.x) || (3.0..4.0).contains(&p.x);
        let decks = Decks::new(span(0..1), drawn, dry);
        let lane = row(&decks, 0);
        assert!((lane[1] - 2.0).abs() < 1e-6, "{lane:?}");
        assert!(even(&lane[1..]), "{lane:?}");
    }

    #[test]
    fn a_deck_with_no_bank_stays_on_the_water_and_never_sinks() {
        let decks = Decks::new(span(0..1), |_| WATER, |_| false);
        assert!(row(&decks, 0).iter().all(|&f| f == WATER));
        // Banks lower than the water's drawn plane: the deck stays on it.
        let (drawn, dry) = river(0.5, 0.5);
        let decks = Decks::new(span(0..1), drawn, dry);
        assert_eq!(decks.floor((5, 0)), Some(WATER));
        assert_eq!(decks.floor_at(Vec2::new(5.7, 0.2)), Some(WATER));
        assert_eq!(decks.floor((20, 0)), None);
        assert!(decks.has((9, 0)) && !decks.has((10, 0)));
    }
}
