//! The timber bridge kit (docs/29, "Dynamic bridges"): how a drawn bridge
//! is built, tile by tile, from the kit's separate pieces (planked deck,
//! stringers, rails and posts on its outer edges, lantern posts, log-crib
//! abutments on the banks and piers in the water) rather than one model per
//! tile. The pieces are chosen from the tile, so the same bridge looks the
//! same on every screen and every frame, and from the water under it: a
//! pier reaches the river bed, and its look follows the bridge's level
//! (plain timber, braced, then stone with cutwaters).
//!
//! The pieces keep their authored positions (assets-src/buildings/bridges.ron):
//! 1 tile = 1 unit, `z = 0` on the plane the game draws (the water over a
//! river), deck tiles 1 x 1 running along the model's +z, rails and posts
//! on its +x side, piers 1 tile wide with a cutwater pointing +x.

use gc_assets::model::Models;
use gc_sim::structures::StructureView;
use glam::Vec2;

/// The kit's models, found by name. `None` from [`BridgeKit::new`] when
/// any piece is missing, and the bridges are drawn as boxes.
#[derive(Clone, Copy, Debug)]
pub struct BridgeKit {
    decks: [usize; 3],
    broken: usize,
    burnt: usize,
    stringer: usize,
    post: usize,
    rail: usize,
    rope: usize,
    lantern: usize,
    abutment: usize,
    /// By level (1 to 3), then short, mid and tall.
    piers: [[usize; 3]; 3],
    /// Short, mid and tall, for level 3.
    cutwaters: [usize; 3],
}

/// One kit piece of a bridge tile: its model, where it stands, the way its
/// +z faces, and how far it is lifted above the drawn ground there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub model: usize,
    pub at: Vec2,
    pub facing: Vec2,
    pub lift: f32,
}

/// What the generator needs to know of one bridge tile.
pub struct Tile<'a> {
    pub s: &'a StructureView,
    /// The way the bridge runs over this tile.
    pub dir: Vec2,
    /// Its level: 1 plain, 2 braced, 3 reinforced (stone piers).
    pub level: u8,
    /// Whether another bridge tile is at a tile.
    pub joined: &'a dyn Fn((i32, i32)) -> bool,
    /// How high the deck's underside is over this tile (see
    /// [`crate::bridge_deck`]): level from bank to bank, not the ground
    /// under the tile.
    pub floor: f32,
    /// The height the game draws at a point (the water over a river).
    pub drawn: &'a dyn Fn(Vec2) -> f32,
    /// The ground (the river bed under water) at a point.
    pub bed: &'a dyn Fn(Vec2) -> f32,
}

/// The bottoms of the short, mid and tall piers below the drawn plane.
/// Water deeper than the tallest takes the tallest, which then stops short
/// of the bed (rare: bridges are drawn over rivers, not lakes).
const PIER_REACH: [f32; 3] = [0.6, 1.1, 1.7];

/// Water at least this deep (in tiles) under a tile's middle takes a pier;
/// shallower is bank, which takes an abutment.
const WET: f32 = 0.08;

/// Every how many tiles along the bridge a row of piers stands.
const PIER_EVERY: i32 = 2;

/// Every how many tiles along the bridge a lantern post takes a post's
/// place, from level 2.
const LANTERN_EVERY: i32 = 4;

/// Below this share of its hit points a deck tile shows broken planks, and
/// below the second, burnt ones.
const BROKEN_BELOW: u32 = 50;
const BURNT_BELOW: u32 = 20;

impl BridgeKit {
    pub fn new(models: &Models<'_>) -> Option<BridgeKit> {
        let find = |name: &str| models.by_name(&format!("Bridge {name}"));
        let heights = ["Short", "Mid", "Tall"];
        let pier = |level: usize| -> Option<[usize; 3]> {
            Some([
                find(&format!("Pier L{level} {}", heights[0]))?,
                find(&format!("Pier L{level} {}", heights[1]))?,
                find(&format!("Pier L{level} {}", heights[2]))?,
            ])
        };
        Some(BridgeKit {
            decks: [find("Deck A")?, find("Deck B")?, find("Deck C")?],
            broken: find("Deck Broken")?,
            burnt: find("Deck Burnt")?,
            stringer: find("Stringer")?,
            post: find("Rail Post")?,
            rail: find("Rail")?,
            rope: find("Rail Rope")?,
            lantern: find("Lantern Post")?,
            abutment: find("Abutment")?,
            piers: [pier(1)?, pier(2)?, pier(3)?],
            cutwaters: [
                find("Cutwater Short")?,
                find("Cutwater Mid")?,
                find("Cutwater Tall")?,
            ],
        })
    }

    /// The pieces that build bridge tile `t`, added to `out`. A tile going
    /// up shows its piers or abutment and stringers, then the rest once
    /// finished. Every piece stands as high as the tile's deck, so the
    /// tile's pieces meet even on a sloping bank, and a pier reaches from
    /// the deck down to the river bed.
    pub fn pieces(&self, t: &Tile<'_>, out: &mut Vec<Piece>) {
        let s = t.s;
        let centre = Vec2::new(s.tile.0 as f32 + 0.5, s.tile.1 as f32 + 0.5);
        let floor = t.floor;
        let mut put = |model: usize, at: Vec2, facing: Vec2| {
            out.push(Piece {
                model,
                at,
                facing,
                lift: floor - (t.drawn)(at),
            });
        };
        // The deck's planks lie across it, square to the tile, so the tiles
        // of a curving deck meet edge to edge.
        let along = snap(t.dir);
        let across = Vec2::new(-along.y, along.x);
        // The map axis the bridge runs along, the same way for every lane
        // whichever way it was drawn, and the tile's place along it, so the
        // pier rows and lantern posts of side-by-side lanes line up.
        let step = |v: Vec2| (v.x as i32, v.y as i32);
        let axis = along.abs();
        let (ax, ay) = step(axis);
        let row = s.tile.0 * ax + s.tile.1 * ay;
        let level = usize::from(t.level.clamp(1, 3));
        // A pier stands where the tile is over water, reaching from the
        // deck to the bed; a crib where it is bank (even under a deck
        // lifted level with a higher bank across the river).
        let wet = (t.drawn)(centre) - (t.bed)(centre) >= WET;
        let depth = floor - (t.bed)(centre);
        let neighbour = |v: Vec2| {
            let (dx, dy) = step(v);
            (t.joined)((s.tile.0 + dx, s.tile.1 + dy))
        };

        // Under the deck: a pier row in the water, every other tile along,
        // and the log crib on the banks.
        if wet {
            if row.rem_euclid(PIER_EVERY) == 0 {
                let tall = PIER_REACH
                    .iter()
                    .position(|&reach| depth <= reach)
                    .unwrap_or(PIER_REACH.len() - 1);
                put(self.piers[level - 1][tall], centre, along);
                if level == 3 {
                    // A stone prow at each end of the row, pointing out.
                    for n in [across, -across] {
                        if !neighbour(n) {
                            put(self.cutwaters[tall], centre + n * 0.5, side_facing(n));
                        }
                    }
                }
            }
        } else {
            put(self.abutment, centre, along);
        }
        for x in [-0.3, 0.3] {
            put(self.stringer, centre + across * x, along);
        }
        if !s.finished() {
            return;
        }

        let hp = s.hp * 100 / s.max_hp.max(1);
        let deck = if hp < BURNT_BELOW {
            self.burnt
        } else if hp < BROKEN_BELOW {
            self.broken
        } else {
            self.decks[pick(s.tile, self.decks.len())]
        };
        // Turned end for end at random, so the same plank pattern does not
        // repeat down the deck.
        let flip = if pick((s.tile.1, s.tile.0), 2) == 0 {
            1.0
        } else {
            -1.0
        };
        put(deck, centre, along * flip);

        // Rails and posts on the outer edges that run along the bridge: not
        // across its ends, where it meets the bank.
        let rail = if level == 1 { self.rope } else { self.rail };
        for n in [across, -across] {
            if neighbour(n) {
                continue;
            }
            let facing = side_facing(n);
            put(rail, centre, facing);
            let lantern = level >= 2 && row.rem_euclid(LANTERN_EVERY) == 0;
            let post = if lantern { self.lantern } else { self.post };
            put(post, centre - axis * 0.5, facing);
            // A closing post where the deck ends, unless it carries on
            // straight or steps aside on a curve.
            if ![axis, axis + across, axis - across]
                .into_iter()
                .any(neighbour)
            {
                put(self.post, centre + axis * 0.5, facing);
            }
        }
    }
}

/// `dir` snapped to the nearest of the four map axes.
pub fn snap(dir: Vec2) -> Vec2 {
    if dir.x.abs() >= dir.y.abs() {
        Vec2::new(dir.x.signum(), 0.0)
    } else {
        Vec2::new(0.0, if dir.y < 0.0 { -1.0 } else { 1.0 })
    }
}

/// The way a piece authored on its +x side faces so that side is `out`
/// (a model's +x is its facing turned a quarter clockwise seen from above:
/// `(facing.y, -facing.x)`, as a gate's leaves take it).
pub fn side_facing(out: Vec2) -> Vec2 {
    Vec2::new(-out.y, out.x)
}

/// A number below `n` that tile `tile` always gets.
fn pick(tile: (i32, i32), n: usize) -> usize {
    let mut h =
        (tile.0 as u32).wrapping_mul(0x9E37_79B1) ^ (tile.1 as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 13;
    h as usize % n.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_piece_faces_so_its_plus_x_points_out() {
        for out in [Vec2::X, Vec2::NEG_X, Vec2::Y, Vec2::NEG_Y] {
            let f = side_facing(out);
            assert_eq!(Vec2::new(f.y, -f.x), out);
        }
    }

    #[test]
    fn snapping_keeps_the_nearest_axis() {
        assert_eq!(snap(Vec2::new(0.9, 0.3)), Vec2::X);
        assert_eq!(snap(Vec2::new(-0.2, -0.7)), Vec2::NEG_Y);
        assert_eq!(snap(Vec2::new(0.0, 0.0)), Vec2::X);
    }

    #[test]
    fn every_tile_picks_within_range_and_varies() {
        let picks: Vec<usize> = (0..40).map(|k| pick((k, 7), 3)).collect();
        assert!(picks.iter().all(|&p| p < 3));
        assert!((0..3).all(|v| picks.contains(&v)));
    }
}
