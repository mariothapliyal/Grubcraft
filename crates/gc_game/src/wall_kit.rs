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
    /// Half a run, to fill out a diagonal tile, if the kit has one.
    half: Option<usize>,
    /// Below a fifth of its hit points, if the kit has one (charred logs).
    burnt: Option<usize>,
    /// A buttress on the back, every few tiles, if the kit has one.
    brace: Option<usize>,
    /// Laid with the runs on a raised wall: a stone footing under them,
    /// iron bands on their front and a fighting step behind them.
    footing: Option<usize>,
    bands: Option<usize>,
    walkway: Option<usize>,
}

/// The top of the walkway's deck, and how far behind the wall's centre
/// line its middle is, in tiles (assets-src/buildings/wall-kits.ron: deck
/// top 0.82, x -0.66 .. -0.14).
pub const WALKWAY_DECK: f32 = 0.82;
pub const WALKWAY_BACK: f32 = 0.4;

/// How much higher each overlapping copy of a piece laid evenly over a
/// diagonal tile sits than the last, so their tops never fight.
const OVERLAP_LIFT: f32 = 0.01;

/// What the generator needs to know of one wall tile.
pub struct Tile<'a> {
    pub s: &'a StructureView,
    /// The middle of the tile's piece of the line, the way it runs and how
    /// long it is (a tile, or more on a diagonal).
    pub centre: Vec2,
    pub dir: Vec2,
    pub len: f32,
    /// The way out of the fort (the side its front faces), if known: away
    /// from its owner's nearest hall. Without it a line faces one fixed way.
    pub out: Option<Vec2>,
    /// Whether a tile of the same side's walls or gates stands at a tile.
    pub walled: &'a dyn Fn((i32, i32)) -> bool,
}

/// How much of a run's length a piece may come short by before it takes
/// another run (as `buildings::SPOT_SLACK`).
const SLACK: f32 = 0.15;

/// How far a tile's piece of the line reaches past each joint, to overlap
/// the next (`buildings::LINE_JOINT`).
const JOINT: f32 = 0.05;

/// How much a tile's runs may come short by before a half run fills it out.
const HALF_FROM: f32 = 0.25;

/// A wall that carries on past a joint has a tile on that side: ahead, to
/// the side at a corner, but not behind (in a drawn line the tile before is
/// always well behind, so this never mistakes it for the next).
const AHEAD: f32 = -0.2;

/// The eight tiles around a tile.
const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

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
                half: find("Palisade Kit Half Run"),
                burnt: find("Palisade Kit Run Burnt"),
                brace: find("Palisade Kit Brace"),
                footing: None,
                bands: None,
                walkway: None,
            }),
            // The Ember's raised wall: the palisade on a stone footing,
            // bound in iron, with a fighting step behind. It no longer burns
            // like logs, and its walkway takes the braces' place.
            "Stone Wall" => Some(WallKit {
                runs: ["A", "B", "C", "D"]
                    .iter()
                    .map(|v| find(&format!("Palisade Kit Run {v}")))
                    .collect::<Option<_>>()?,
                post: find("Palisade Kit Post")?,
                end: find("Palisade Kit End Post")?,
                broken: find("Palisade Kit Run Broken")?,
                half: find("Palisade Kit Half Run"),
                burnt: None,
                brace: None,
                footing: Some(find("Palisade Kit Footing")?),
                bands: Some(find("Palisade Kit Bands")?),
                walkway: Some(find("Palisade Kit Walkway")?),
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
                    half: None,
                    burnt: None,
                    brace: None,
                    footing: None,
                    bands: None,
                    walkway: None,
                })
            }
            _ => None,
        }
    }

    /// The run model a finished wall tile shows (for its height).
    pub fn run(&self) -> usize {
        self.runs[0]
    }

    /// Whether men manning it stand on a walkway behind it (at
    /// [`WALKWAY_DECK`], [`WALKWAY_BACK`] inside), not on its top.
    pub fn has_walkway(&self) -> bool {
        self.walkway.is_some()
    }

    /// The pieces that build wall tile `t`, added to `out`: runs end to
    /// end along its piece of the line, a post at a joint where the wall
    /// carries on and an end post where it stops. A tile going up shows its
    /// posts, and its runs from half built.
    pub fn pieces(&self, t: &Tile<'_>, out: &mut Vec<Piece>) {
        let s = t.s;
        // The front faces out of the fort; without a fort, every piece of a
        // line faces the same way whichever way it was drawn.
        let facing = facing(t.dir, t.out);
        let mut lay = |model: usize, at: Vec2, lift: f32| {
            out.push(Piece {
                model,
                at,
                facing,
                lift,
            });
        };
        let mut put = |model: usize, at: Vec2| lay(model, at, 0.0);
        let half = t.dir * (t.len / 2.0);
        // Does the wall carry on past a joint: is any of the tile's eight
        // neighbours on that side one of its side's walls or gates?
        let goes_on = |way: Vec2| {
            NEIGHBOURS.iter().any(|&(dx, dy)| {
                Vec2::new(dx as f32, dy as f32).normalize().dot(way) > AHEAD
                    && (t.walled)((s.tile.0 + dx, s.tile.1 + dy))
            })
        };
        for (joint, way) in [(t.centre - half, -t.dir), (t.centre + half, t.dir)] {
            if !goes_on(way) {
                put(self.end, joint);
            } else if forward(way) {
                // Of the two tiles meeting at a joint, the one it lies
                // ahead of puts the post, however each was drawn.
                put(self.post, joint);
            }
        }
        if !s.finished() && s.built_percent < 50 {
            return;
        }
        let hp = s.hp * 100 / s.max_hp.max(1);
        let run = |k: usize| match self.burnt {
            Some(burnt) if hp < BURNT_BELOW => burnt,
            _ if hp < BROKEN_BELOW => self.broken,
            _ => self.runs[pick(s.tile, k, self.runs.len())],
        };
        // Whole runs to the tile's length less its overlaps, and a half run
        // where they would come well short (a diagonal); without a half
        // run, one more whole run, the runs closing up to fit.
        let core = t.len - 2.0 * JOINT;
        let mut count = (core + SLACK).floor().max(1.0) as usize;
        let short = core - count as f32;
        let half_run = self
            .half
            .filter(|_| short > HALF_FROM && hp >= BROKEN_BELOW);
        if short > HALF_FROM && half_run.is_none() {
            count += 1;
        }
        let laid = if half_run.is_some() {
            count as f32 + 0.5
        } else {
            count as f32
        };
        // Laid end to end from one end, or evenly over the core when the
        // runs are more than it.
        let step = if laid > core && half_run.is_none() && count > 1 {
            (core - 1.0) / (count as f32 - 1.0)
        } else {
            1.0
        };
        let from =
            -(step * (count as f32 - 1.0) + 1.0 + if half_run.is_some() { 0.5 } else { 0.0 }) / 2.0;
        for k in 0..count {
            put(run(k), t.centre + t.dir * (from + 0.5 + step * k as f32));
        }
        if let Some(half_run) = half_run {
            let at = from + step * (count as f32 - 1.0) + 1.25;
            put(half_run, t.centre + t.dir * at);
        }
        if let Some(brace) = self.brace
            && s.finished()
            && pick(s.tile, 7, BRACE_EVERY) == 0
        {
            put(brace, t.centre);
        }
        // Footing, bands and walkway are a tile long each, laid evenly over
        // the tile's core: one on a straight tile, two overlapping on a
        // diagonal, the second a hair higher.
        let works = (core - SLACK).ceil().max(1.0) as usize;
        let spread = if works > 1 {
            (core - 1.0).max(0.0) / (works as f32 - 1.0)
        } else {
            0.0
        };
        let first = -spread * (works as f32 - 1.0) / 2.0;
        for k in 0..works {
            let at = t.centre + t.dir * (first + spread * k as f32);
            let lift = OVERLAP_LIFT * k as f32;
            for piece in [self.footing, self.bands, self.walkway]
                .into_iter()
                .flatten()
            {
                lay(piece, at, lift);
            }
        }
    }
}

/// The way a kit piece along `dir` faces: its front out of the fort
/// (`out`), or without one the same fixed way for a whole line, whichever
/// way it was drawn. The front, a piece's +x, is `(f.y, -f.x)`.
pub fn facing(dir: Vec2, out: Option<Vec2>) -> Vec2 {
    match out {
        Some(way) => facing_out(dir, way),
        None if forward(dir) => dir,
        None => -dir,
    }
}

/// The way a kit piece along `dir` fronts: `out`, or the fixed way a
/// line with no fort faces (see [`facing`]).
pub fn front(dir: Vec2, out: Option<Vec2>) -> Vec2 {
    out.unwrap_or_else(|| {
        let f = facing(dir, None);
        Vec2::new(f.y, -f.x)
    })
}

/// Of `dir` and its reverse, the one that puts a piece's +x (its front,
/// `(f.y, -f.x)`) on the `out` side.
fn facing_out(dir: Vec2, out: Vec2) -> Vec2 {
    if Vec2::new(dir.y, -dir.x).dot(out) >= 0.0 {
        dir
    } else {
        -dir
    }
}

/// Whether `way` points toward higher x (or straight toward higher y):
/// one of a line's two ways, the same whichever way it was drawn.
fn forward(way: Vec2) -> bool {
    way.x > 1e-3 || (way.x.abs() <= 1e-3 && way.y > 0.0)
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
        for name in [
            "Palisade",
            "Stone Wall",
            "Bone Fence",
            "Ossuary Wall",
            "Mausoleum Wall",
        ] {
            let kit = WallKit::for_wall(&models, name);
            assert!(kit.is_some(), "{name} has no kit");
        }
        assert!(WallKit::for_wall(&models, "Castle Wall").is_none());
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
            out: None,
            walled: &walled,
        };
        let mut out = Vec::new();
        kit.pieces(&tile, &mut out);
        // An end post at the west end (nothing beyond), a post at the east
        // joint (the joint lies east of this tile, so it is this tile's),
        // and one run.
        let ends: Vec<_> = out.iter().filter(|p| p.model == kit.end).collect();
        assert_eq!(ends.len(), 1);
        assert_eq!(ends[0].at, Vec2::new(5.0, 5.5));
        assert_eq!(
            out.iter().filter(|p| kit.runs.contains(&p.model)).count(),
            1
        );
        assert_eq!(out.iter().filter(|p| p.model == kit.post).count(), 1);
        // A diagonal piece takes a run and a half, not two runs one over
        // the other; a piece a little longer than a tile, just the one.
        for (dir, len, runs, halves) in [
            (Vec2::new(1.0, 1.0), std::f32::consts::SQRT_2, 1, 1),
            (Vec2::new(10.0, 4.0), 1.077, 1, 0),
        ] {
            out.clear();
            let piece = Tile {
                dir: dir.normalize(),
                len: len + 2.0 * JOINT,
                ..tile
            };
            kit.pieces(&piece, &mut out);
            let count = |m: &dyn Fn(usize) -> bool| out.iter().filter(|p| m(p.model)).count();
            assert_eq!(count(&|m| kit.runs.contains(&m)), runs, "{dir}");
            assert_eq!(count(&|m| Some(m) == kit.half), halves, "{dir}");
        }
    }

    #[test]
    fn a_corner_and_lines_drawn_toward_each_other_join_without_end_posts() {
        let models = crate::models::load().unwrap();
        let kit = WallKit::for_wall(&models, "Palisade").unwrap();
        // Drawn east to (5, 0), then north from there; and a second line
        // drawn west from (9, 0) back to (6, 0).
        let walled = |t: (i32, i32)| matches!(t, (0..=9, 0)) || t == (5, -1) || t == (5, -2);
        let pieces = |tile: (i32, i32), dir: Vec2| {
            let mut s = crate::buildings::footprint_view(tile, 1, 1, 0);
            s.built_percent = 100;
            let t = Tile {
                s: &s,
                centre: Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
                dir,
                len: 1.0,
                out: None,
                walled: &walled,
            };
            let mut out = Vec::new();
            kit.pieces(&t, &mut out);
            out
        };
        let ends = |out: &[Piece]| out.iter().filter(|p| p.model == kit.end).count();
        let posts = |out: &[Piece]| out.iter().filter(|p| p.model == kit.post).count();
        // The corner tile carries on to the side: no end post.
        assert_eq!(ends(&pieces((5, 0), Vec2::X)), 0);
        // At the joint where the two lines meet, one post between them.
        let west = pieces((5, 0), Vec2::X);
        let east = pieces((6, 0), Vec2::NEG_X);
        let at = |out: &[Piece]| {
            (out.iter())
                .filter(|p| p.model == kit.post && (p.at.x - 6.0).abs() < 0.1)
                .count()
        };
        assert_eq!(at(&west) + at(&east), 1);
        assert!(posts(&east) <= 1);
        // Both lines face the same way.
        assert_eq!(west[0].facing, east[0].facing);
    }

    #[test]
    fn a_stepped_curve_has_end_posts_only_at_its_two_ends() {
        let models = crate::models::load().unwrap();
        let kit = WallKit::for_wall(&models, "Palisade").unwrap();
        // A wall stepping east, then north-east, then north: every middle
        // tile has a neighbour each way, though not straight on.
        let line = [(0, 0), (1, 0), (2, 1), (3, 2), (3, 3), (4, 4), (4, 5)];
        let walled = |t: (i32, i32)| line.contains(&t);
        let mut ends = 0;
        for (i, &tile) in line.iter().enumerate() {
            let prev = line[i.saturating_sub(1)];
            let next = line[(i + 1).min(line.len() - 1)];
            let way = Vec2::new((next.0 - prev.0) as f32, (next.1 - prev.1) as f32);
            let mut s = crate::buildings::footprint_view(tile, 1, 1, 0);
            s.built_percent = 100;
            let t = Tile {
                s: &s,
                centre: Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
                dir: way.normalize(),
                len: 1.0,
                out: None,
                walled: &walled,
            };
            let mut out = Vec::new();
            kit.pieces(&t, &mut out);
            let here = out.iter().filter(|p| p.model == kit.end).count();
            let first_or_last = i == 0 || i == line.len() - 1;
            assert_eq!(here, usize::from(first_or_last), "tile {tile:?}");
            ends += here;
        }
        assert_eq!(ends, 2);
    }

    #[test]
    fn the_front_faces_out_of_the_fort_whichever_way_the_wall_was_drawn() {
        for dir in [Vec2::X, Vec2::NEG_X] {
            // The fort lies to the north (-y): out is south.
            let f = facing(dir, Some(Vec2::Y));
            assert_eq!(Vec2::new(f.y, -f.x), Vec2::Y, "{dir}");
        }
        // Without a fort, both ways of drawing face the same way.
        assert_eq!(facing(Vec2::X, None), facing(Vec2::NEG_X, None));
    }

    #[test]
    fn a_stone_wall_is_the_palisade_on_a_footing_with_a_walkway_and_never_burns() {
        let models = crate::models::load().unwrap();
        let kit = WallKit::for_wall(&models, "Stone Wall").unwrap();
        assert!(kit.has_walkway());
        assert!(
            !WallKit::for_wall(&models, "Palisade")
                .unwrap()
                .has_walkway()
        );
        let mut s = crate::buildings::footprint_view((5, 5), 1, 1, 0);
        s.built_percent = 100;
        let mut hurt = s;
        hurt.hp = s.max_hp / 10;
        let walled = |_: (i32, i32)| true;
        let tile = Tile {
            s: &s,
            centre: Vec2::new(5.5, 5.5),
            dir: Vec2::X,
            len: 1.0 + 2.0 * JOINT,
            out: Some(Vec2::Y),
            walled: &walled,
        };
        let mut out = Vec::new();
        kit.pieces(&tile, &mut out);
        let has = |m: Option<usize>| out.iter().filter(|p| Some(p.model) == m).count();
        assert_eq!(has(kit.walkway), 1);
        assert_eq!(has(kit.footing), 1);
        assert_eq!(has(kit.bands), 1);
        assert_eq!(has(kit.brace), 0);
        // Every piece's front faces out.
        assert!(
            out.iter()
                .all(|p| Vec2::new(p.facing.y, -p.facing.x) == Vec2::Y)
        );
        // Badly hurt it shows broken runs, not burnt ones.
        out.clear();
        kit.pieces(&Tile { s: &hurt, ..tile }, &mut out);
        assert!(out.iter().any(|p| p.model == kit.broken));
        // A diagonal tile takes two walkways, the second a hair higher.
        out.clear();
        let diagonal = Tile {
            dir: Vec2::new(1.0, 1.0).normalize(),
            len: std::f32::consts::SQRT_2 + 2.0 * JOINT,
            ..tile
        };
        kit.pieces(&diagonal, &mut out);
        let walks: Vec<_> = out
            .iter()
            .filter(|p| Some(p.model) == kit.walkway)
            .collect();
        assert_eq!(walks.len(), 2);
        assert!(walks[1].lift > walks[0].lift);
    }
}
