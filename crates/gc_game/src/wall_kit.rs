//! Wall kits (docs/29, "Curved walls"): how a drawn Ember Palisade or
//! Carrion Concord bone wall is built, tile by tile, from separate pieces
//! (runs of logs or bone, posts at the joints, an end post where the wall
//! stops) laid along its piece of the curve, rather than one model per
//! tile. The pieces are chosen from the tile, so a wall looks the same on
//! every screen and every frame, and from its hit points (broken runs).
//!
//! The pieces keep their authored positions (assets-src/buildings/wall-kits.ron):
//! 1 tile = 1 unit, origin on the ground at the wall's centre line, a run
//! exactly a tile long along the model's +z, its front (outside) facing +x.

use gc_assets::model::Models;
use gc_sim::structures::StructureView;
use glam::Vec2;

use crate::bridge_kit::Piece;

/// One look of a wall kit: the models of one wall type.
#[derive(Clone, Debug)]
pub struct WallKit {
    runs: Vec<usize>,
    post: usize,
    end: usize,
    broken: usize,
    /// Below a fifth of its hit points, if the kit has one (charred logs).
    burnt: Option<usize>,
    /// A buttress on the back, every few tiles, if the kit has one.
    brace: Option<usize>,
}

/// What the generator needs to know of one wall tile.
pub struct Tile<'a> {
    pub s: &'a StructureView,
    /// The middle of the tile's piece of the line, the way it runs and how
    /// long it is (a tile, or more on a diagonal).
    pub centre: Vec2,
    pub dir: Vec2,
    pub len: f32,
    /// Whether another tile of a drawn wall stands at a tile.
    pub walled: &'a dyn Fn((i32, i32)) -> bool,
}

/// How much of a run's length a piece may come short by before it takes
/// another run (as `buildings::SPOT_SLACK`).
const SLACK: f32 = 0.15;

/// Every how many tiles a palisade has a brace behind it.
const BRACE_EVERY: usize = 3;

/// Below this share of its hit points a run shows broken, and below the
/// second burnt where the kit has it.
const BROKEN_BELOW: u32 = 50;
const BURNT_BELOW: u32 = 20;

impl WallKit {
    /// The kit for wall type `name`, if it has one: the Palisade, and the
    /// Bone Fence with its raised Ossuary and Mausoleum Walls.
    pub fn for_wall(models: &Models<'_>, name: &str) -> Option<WallKit> {
        let find = |n: &str| models.by_name(n);
        match name {
            "Palisade" => Some(WallKit {
                runs: ["A", "B", "C", "D"]
                    .iter()
                    .map(|v| find(&format!("Palisade Kit Run {v}")))
                    .collect::<Option<_>>()?,
                post: find("Palisade Kit Post")?,
                end: find("Palisade Kit End Post")?,
                broken: find("Palisade Kit Run Broken")?,
                burnt: find("Palisade Kit Run Burnt"),
                brace: find("Palisade Kit Brace"),
            }),
            "Bone Fence" | "Ossuary Wall" | "Mausoleum Wall" => {
                let (level, variants): (&str, &[&str]) = match name {
                    "Bone Fence" => ("", &["A", "B", "C"]),
                    "Ossuary Wall" => ("L2 ", &["A", "B"]),
                    _ => ("L3 ", &["A", "B"]),
                };
                Some(WallKit {
                    runs: (variants.iter())
                        .map(|v| find(&format!("Bone Fence Kit {level}Run {v}")))
                        .collect::<Option<_>>()?,
                    post: find(&format!("Bone Fence Kit {level}Post"))?,
                    end: find(&format!("Bone Fence Kit {level}End"))?,
                    broken: find(&format!("Bone Fence Kit {level}Broken"))?,
                    burnt: None,
                    brace: None,
                })
            }
            _ => None,
        }
    }

    /// The run model a finished wall tile shows (for its height).
    pub fn run(&self) -> usize {
        self.runs[0]
    }

    /// The pieces that build wall tile `t`, added to `out`: runs end to
    /// end along its piece of the line, a post at the joint where it
    /// starts and an end post where the wall stops. A tile going up shows
    /// its posts, and its runs from half built.
    pub fn pieces(&self, t: &Tile<'_>, out: &mut Vec<Piece>) {
        let s = t.s;
        let mut put = |model: usize, at: Vec2, facing: Vec2| {
            out.push(Piece {
                model,
                at,
                facing,
                lift: 0.0,
            });
        };
        let half = t.dir * (t.len / 2.0);
        let (start, end) = (t.centre - half, t.centre + half);
        // Is there wall beyond a joint (the next tile along, straight or
        // stepped aside)?
        let goes_on = |joint: Vec2, out_way: Vec2| {
            let beyond = joint + out_way * 0.5;
            let tile = (beyond.x.floor() as i32, beyond.y.floor() as i32);
            tile != s.tile && (t.walled)(tile)
        };
        let ends = [(start, -t.dir), (end, t.dir)];
        for (k, &(joint, way)) in ends.iter().enumerate() {
            if !goes_on(joint, way) {
                put(self.end, joint, t.dir);
            } else if k == 0 {
                put(self.post, joint, t.dir);
            }
        }
        if !s.finished() && s.built_percent < 50 {
            return;
        }
        let hp = s.hp * 100 / s.max_hp.max(1);
        let count = (t.len - SLACK).ceil().max(1.0) as usize;
        let step = (t.len - 1.0).max(0.0) / (count as f32 - 1.0).max(1.0);
        let first = -step * (count as f32 - 1.0) / 2.0;
        for k in 0..count {
            let at = t.centre + t.dir * (first + step * k as f32);
            let run = match self.burnt {
                Some(burnt) if hp < BURNT_BELOW => burnt,
                _ if hp < BROKEN_BELOW => self.broken,
                _ => self.runs[pick(s.tile, k, self.runs.len())],
            };
            put(run, at, t.dir);
        }
        if let Some(brace) = self.brace
            && s.finished()
            && pick(s.tile, 7, BRACE_EVERY) == 0
        {
            put(brace, t.centre, t.dir);
        }
    }
}

/// A number below `n` that tile `tile` always gets for its `k`th piece.
fn pick(tile: (i32, i32), k: usize, n: usize) -> usize {
    let mut h = (tile.0 as u32).wrapping_mul(0x9E37_79B1)
        ^ (tile.1 as u32).wrapping_mul(0x85EB_CA77)
        ^ (k as u32).wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 13;
    h as usize % n.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_wall_kit_is_in_the_pack() {
        let models = crate::models::load().unwrap();
        for name in ["Palisade", "Bone Fence", "Ossuary Wall", "Mausoleum Wall"] {
            let kit = WallKit::for_wall(&models, name);
            assert!(kit.is_some(), "{name} has no kit");
        }
        assert!(WallKit::for_wall(&models, "Stone Wall").is_none());
    }

    #[test]
    fn a_wall_tile_has_runs_end_to_end_and_end_posts_where_it_stops() {
        let models = crate::models::load().unwrap();
        let kit = WallKit::for_wall(&models, "Palisade").unwrap();
        let mut s = crate::buildings::footprint_view((5, 5), 1, 1, 0);
        s.built_percent = 100;
        let walled = |t: (i32, i32)| t == (6, 5);
        let tile = Tile {
            s: &s,
            centre: Vec2::new(5.5, 5.5),
            dir: Vec2::X,
            len: 1.0,
            walled: &walled,
        };
        let mut out = Vec::new();
        kit.pieces(&tile, &mut out);
        // An end post at the west end (nothing beyond), no post at the east
        // joint (the next tile has its own), and one run.
        let ends: Vec<_> = out.iter().filter(|p| p.model == kit.end).collect();
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].at, Vec2::new(5.0, 5.5));
        assert_eq!(
            out.iter().filter(|p| kit.runs.contains(&p.model)).count(),
            1
        );
        // A diagonal piece takes two runs.
        out.clear();
        let diagonal = Tile {
            dir: Vec2::new(1.0, 1.0).normalize(),
            len: std::f32::consts::SQRT_2,
            ..tile
        };
        kit.pieces(&diagonal, &mut out);
        assert_eq!(
            out.iter().filter(|p| kit.runs.contains(&p.model)).count(),
            2
        );
    }
}
