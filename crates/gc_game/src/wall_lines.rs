//! Wall lines: where each tile of a drawn wall shows its piece, so the
//! pieces of a wall meet end to end however it was drawn (docs/26,
//! "How a wall's pieces are laid").
//!
//! The simulation keeps a wall as tiles, each with the line it was drawn
//! along. Drawn tile by tile from that alone, a curve came out as short
//! straight pieces that didn't meet, and two lines meeting at a corner, or
//! a single tile mended into a gap, as blocks with end posts between them.
//! Here every tile gets a point of its own on its line (where the curve
//! passes it), the tiles of a wall are linked into chains (along each
//! drawing, and from a free end to the wall or gate tile it touches), and
//! each tile shows the piece from the joint with the tile before to the
//! joint with the tile after, a joint lying half way between their points.
//! So a wall is one unbroken line through its joints: it bends at the
//! joints, where the posts stand, and stops only where no tile follows.
//!
//! All of it is worked out from the tiles shown, so a wall looks the same
//! every frame; a wall hidden in the fog takes no end post away.

use std::collections::{HashMap, HashSet};

use gc_sim::drawn::{Drawing, DrawnPath, SUB};
use gc_sim::structures::StructureView;
use gc_sim::{PlayerId, World};
use glam::Vec2;

use crate::buildings::{LINE_JOINT, WallPiece};

/// A free end joins a tile beside it that lies this far ahead of it at
/// least (as the cosine of the angle off straight on): ahead or to the
/// side, not behind.
const AHEAD: f32 = -0.2;
/// A lone tile's second link lies this far round from its first at least
/// (as the most the cosine between them may be).
const APART: f32 = 0.3;
/// Two joints closer than this round a tile's point (as a cosine) are a
/// hairpin: the tile shows a piece to each, not one between them.
const HAIRPIN: f32 = 0.5;
/// The most links a tile takes.
const LINKS: usize = 4;

/// What a tile laid along a line is, for joining: walls join walls and
/// gates, stakes stakes, barricades barricades.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Kind {
    Wall,
    /// A gate's tile: walls run up to it; it shows no piece of its own.
    Gate,
    Stakes,
    Barricade,
}

impl Kind {
    fn class(self) -> u8 {
        match self {
            Kind::Wall | Kind::Gate => 0,
            Kind::Stakes => 1,
            Kind::Barricade => 2,
        }
    }
}

/// What stands at an end of a tile's piece.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Joint {
    /// The wall stops here: an end post.
    Stop,
    /// A joint with the next tile, whose post this tile puts.
    Post,
    /// A joint with the next tile, which puts the post.
    Bare,
}

/// The piece of its wall one tile shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// From joint to joint (or to where the wall stops), the way its line
    /// was drawn where it has one.
    pub from: Vec2,
    pub to: Vec2,
    pub ends: [Joint; 2],
    /// Where three walls or more meet at the tile: a short piece from its
    /// own point to each further joint, and what stands there.
    pub arms: Vec<(Vec2, Vec2, Joint)>,
}

impl Shape {
    /// The piece as a slab or a row of kit pieces is laid: its middle,
    /// way and length, reaching a hair past each joint.
    pub fn piece(&self) -> WallPiece {
        piece_of(self.from, self.to, self.ends)
    }

    /// Each arm (see [`Shape::arms`]) from end to end, with what stands
    /// at its two ends: nothing at the tile's own point but for the first,
    /// which puts the post there.
    pub fn arm_spans(&self) -> impl Iterator<Item = (Vec2, Vec2, [Joint; 2])> + '_ {
        self.arms.iter().enumerate().map(|(k, &(a, b, joint))| {
            let ends = [if k == 0 { Joint::Post } else { Joint::Bare }, joint];
            (a, b, ends)
        })
    }

    /// The arms as slabs are laid (see [`Shape::piece`]).
    pub fn arm_pieces(&self) -> impl Iterator<Item = WallPiece> + '_ {
        self.arm_spans().map(|(a, b, ends)| piece_of(a, b, ends))
    }
}

fn piece_of(from: Vec2, to: Vec2, ends: [Joint; 2]) -> WallPiece {
    let dir = (to - from).normalize_or(Vec2::X);
    let over = ends.map(|e| if e == Joint::Stop { 0.0 } else { LINE_JOINT });
    WallPiece {
        centre: (from + to) / 2.0 + dir * (over[1] - over[0]) / 2.0,
        dir,
        len: from.distance(to) + over[0] + over[1],
    }
}

/// Where a drawing's curve passes a tile: the point, how far along the
/// curve it is, and the way the curve runs there.
#[derive(Clone, Copy, Debug)]
struct Anchor {
    at: Vec2,
    arc: f32,
    dir: Option<Vec2>,
}

/// A drawing's curve as points, for finding its tiles' anchors.
struct Curve {
    drawing: Drawing,
    pts: Vec<Vec2>,
    arc: Vec<f32>,
    tiles: HashMap<(i32, i32), Anchor>,
}

impl Curve {
    fn new(drawing: Drawing) -> Curve {
        Curve::along(drawing, &drawing.path)
    }

    fn along(drawing: Drawing, path: &DrawnPath) -> Curve {
        let pts: Vec<Vec2> = (path.samples().iter())
            .map(|p| Vec2::new(p.x as f32, p.y as f32) / SUB as f32)
            .collect();
        let mut arc = Vec::with_capacity(pts.len());
        let mut far = 0.0;
        for (k, p) in pts.iter().enumerate() {
            if k > 0 {
                far += p.distance(pts[k - 1]);
            }
            arc.push(far);
        }
        Curve {
            drawing,
            pts,
            arc,
            tiles: HashMap::new(),
        }
    }

    /// Where the curve passes `tile`: the point of it nearest the tile's
    /// middle.
    fn anchor(&mut self, tile: (i32, i32)) -> Anchor {
        if let Some(&a) = self.tiles.get(&tile) {
            return a;
        }
        let c = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
        let mut best = Anchor {
            at: self.pts.first().copied().unwrap_or(c),
            arc: 0.0,
            dir: None,
        };
        let mut near = f32::MAX;
        for (k, w) in self.pts.windows(2).enumerate() {
            let d = w[1] - w[0];
            let len = d.length();
            if len < 1e-4 {
                continue;
            }
            let t = ((c - w[0]).dot(d) / (len * len)).clamp(0.0, 1.0);
            let q = w[0] + d * t;
            let far = q.distance_squared(c);
            if far < near {
                near = far;
                best = Anchor {
                    at: q,
                    arc: self.arc[k] + t * len,
                    dir: Some(d / len),
                };
            }
        }
        self.tiles.insert(tile, best);
        best
    }
}

/// Kept from frame to frame: every drawing's curve and its tiles' anchors
/// (a drawing never changes once drawn; a new match's are found anew).
#[derive(Default)]
pub struct Anchors {
    curves: HashMap<u32, Curve>,
}

impl Anchors {
    fn anchor(&mut self, world: &World, id: u32, tile: (i32, i32)) -> Option<Anchor> {
        let drawing = world.drawing(id)?;
        let curve = self.curves.entry(id).or_insert_with(|| Curve::new(drawing));
        if curve.drawing != drawing {
            *curve = Curve::new(drawing);
        }
        Some(curve.anchor(tile))
    }
}

/// A tile of a line, for linking.
#[derive(Clone, Debug)]
struct Node {
    owner: PlayerId,
    kind: Kind,
    tile: (i32, i32),
    at: Vec2,
    /// The way its own line runs at it, if it has a line longer than a
    /// tile.
    dir: Option<Vec2>,
    /// The drawing it was laid along, and how far along it.
    drawing: Option<u32>,
    arc: f32,
    links: Vec<usize>,
}

type Key = (PlayerId, u8, (i32, i32));

/// Every shown wall tile's piece (see the module docs).
#[derive(Default)]
pub struct WallLines {
    nodes: Vec<Node>,
    shapes: HashMap<Key, Shape>,
}

/// A ghost's pretend drawing: no real one has this number.
const GHOST: u32 = u32::MAX - 1;

impl WallLines {
    /// The lines of the tiles `shown`: each tile laid along a line (bridges
    /// aside) with what it is, and every gate.
    pub fn new(
        world: &World,
        anchors: &mut Anchors,
        shown: impl Iterator<Item = (StructureView, Kind)>,
    ) -> WallLines {
        let mut nodes = Vec::new();
        for (s, kind) in shown {
            if kind == Kind::Gate {
                // Each of a gate's tiles, for walls to run up to.
                for x in 0..i32::from(s.width) {
                    for y in 0..i32::from(s.depth) {
                        let tile = (s.tile.0 + x, s.tile.1 + y);
                        nodes.push(Node {
                            owner: s.owner,
                            kind,
                            tile,
                            at: Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
                            dir: None,
                            drawing: None,
                            arc: 0.0,
                            links: Vec::new(),
                        });
                    }
                }
                continue;
            }
            let anchor = (s.drawing).and_then(|id| anchors.anchor(world, id, s.tile));
            // Not laid along a drawing: by the tile's own line.
            let own = || {
                let [x0, y0, x1, y1] = s.line?;
                Vec2::new((x1 - x0) as f32, (y1 - y0) as f32).try_normalize()
            };
            let centre = Vec2::new(s.tile.0 as f32 + 0.5, s.tile.1 as f32 + 0.5);
            nodes.push(Node {
                owner: s.owner,
                kind,
                tile: s.tile,
                at: anchor.map_or(centre, |a| a.at),
                dir: anchor.map_or_else(own, |a| a.dir.or_else(own)),
                drawing: anchor.and(s.drawing),
                arc: anchor.map_or(0.0, |a| a.arc),
                links: Vec::new(),
            });
        }
        let mut lines = WallLines {
            nodes,
            shapes: HashMap::new(),
        };
        lines.settle();
        lines
    }

    /// The piece tile `s` shows, if it is one of the tiles these lines
    /// were worked out for.
    pub fn shape(&self, s: &StructureView, kind: Kind) -> Option<&Shape> {
        self.shapes.get(&(s.owner, kind.class(), s.tile))
    }

    /// The pieces a wall about to be drawn along `path` over `tiles` would
    /// show for `owner`, tile by tile: as the finished wall, joined to the
    /// walls and gates already standing.
    pub fn ghost(
        &self,
        owner: PlayerId,
        kind: Kind,
        path: &DrawnPath,
        tiles: &[(i32, i32)],
    ) -> Vec<Option<Shape>> {
        let drawing = Drawing {
            owner,
            path: *path,
            width: 1,
            seed: 0,
        };
        let mut curve = Curve::along(drawing, path);
        // Only the standing tiles near enough to change a ghost tile's
        // piece: its neighbours, and theirs, whose links they count.
        const NEAR: i32 = 3;
        let ghost: HashSet<(i32, i32)> = tiles.iter().copied().collect();
        let near = |t: (i32, i32)| {
            (-NEAR..=NEAR).any(|dy| (-NEAR..=NEAR).any(|dx| ghost.contains(&(t.0 + dx, t.1 + dy))))
        };
        let mut nodes: Vec<Node> = (self.nodes.iter())
            .filter(|n| n.owner == owner && n.kind.class() == kind.class())
            .filter(|n| !ghost.contains(&n.tile) && near(n.tile))
            .map(|n| Node {
                links: Vec::new(),
                ..n.clone()
            })
            .collect();
        for &tile in tiles {
            let a = curve.anchor(tile);
            nodes.push(Node {
                owner,
                kind,
                tile,
                at: a.at,
                dir: a.dir,
                drawing: Some(GHOST),
                arc: a.arc,
                links: Vec::new(),
            });
        }
        let mut lines = WallLines {
            nodes,
            shapes: HashMap::new(),
        };
        lines.settle();
        (tiles.iter())
            .map(|&t| lines.shapes.get(&(owner, kind.class(), t)).cloned())
            .collect()
    }

    /// Link the tiles into chains and work out each one's piece.
    fn settle(&mut self) {
        let nodes = &mut self.nodes;
        // In a fixed order, so the same tiles always give the same lines.
        nodes.sort_by_key(|n| (n.owner, n.kind.class(), n.tile, n.kind == Kind::Gate));
        nodes.dedup_by_key(|n| (n.owner, n.kind.class(), n.tile));
        let index: HashMap<Key, usize> = (nodes.iter().enumerate())
            .map(|(i, n)| ((n.owner, n.kind.class(), n.tile), i))
            .collect();
        let touch = |a: (i32, i32), b: (i32, i32)| (a.0 - b.0).abs().max((a.1 - b.1).abs()) == 1;
        let link = |nodes: &mut Vec<Node>, a: usize, b: usize| {
            if a != b
                && !nodes[a].links.contains(&b)
                && nodes[a].links.len() < LINKS
                && nodes[b].links.len() < LINKS
            {
                nodes[a].links.push(b);
                nodes[b].links.push(a);
            }
        };
        // Along each drawing: its tiles in order, where they still touch.
        let mut along: Vec<(PlayerId, u8, u32, f32, usize)> = (nodes.iter().enumerate())
            .filter_map(|(i, n)| Some((n.owner, n.kind.class(), n.drawing?, n.arc, i)))
            .collect();
        along.sort_by(|a, b| {
            (a.0, a.1, a.2)
                .cmp(&(b.0, b.1, b.2))
                .then(a.3.total_cmp(&b.3))
        });
        for w in along.windows(2) {
            let same = (w[0].0, w[0].1, w[0].2) == (w[1].0, w[1].1, w[1].2);
            if same && touch(nodes[w[0].4].tile, nodes[w[1].4].tile) {
                link(nodes, w[0].4, w[1].4);
            }
        }
        // From each free end to the tile it touches: straight on from an
        // end, or failing that to its side; a lone tile to the nearest
        // beside it, and to another round the other side.
        for i in 0..nodes.len() {
            if nodes[i].kind == Kind::Gate {
                continue;
            }
            for _ in nodes[i].links.len()..2 {
                let n = &nodes[i];
                let way = |j: usize| (nodes[j].at - n.at).normalize_or(Vec2::X);
                // The way on from its one link; with none, any way.
                let on = n.links.first().map(|&p| -way(p));
                let least = if n.dir.is_some() { AHEAD } else { -APART };
                // Straight on is best; of two as good, the nearer.
                let score = |j: usize| {
                    on.map_or(0.0, |on| on.dot(way(j))) + 0.01 / nodes[j].at.distance(n.at).max(0.1)
                };
                let best = crate::wall_kit::NEIGHBOURS
                    .iter()
                    .filter_map(|&(dx, dy)| {
                        let key = (n.owner, n.kind.class(), (n.tile.0 + dx, n.tile.1 + dy));
                        index.get(&key).copied()
                    })
                    .filter(|&j| !n.links.contains(&j) && nodes[j].links.len() < LINKS)
                    .filter(|&j| on.is_none_or(|on| on.dot(way(j)) > least))
                    .max_by(|&a, &b| score(a).total_cmp(&score(b)));
                match best {
                    Some(j) => link(nodes, i, j),
                    None => break,
                }
            }
        }
        // Each tile's piece.
        self.shapes = (nodes.iter().enumerate())
            .filter(|(_, n)| n.kind != Kind::Gate)
            .map(|(i, n)| ((n.owner, n.kind.class(), n.tile), shape_of(nodes, i)))
            .collect();
    }
}

/// The piece node `i` shows, from its links.
fn shape_of(nodes: &[Node], i: usize) -> Shape {
    let n = &nodes[i];
    // The joint with each tile it is linked to, and who puts the post: a
    // wall at a gate; of two wall tiles, the one that comes first.
    let joints: Vec<(Vec2, Joint)> = (n.links.iter())
        .map(|&j| {
            let o = &nodes[j];
            let mine = o.kind == Kind::Gate || n.tile < o.tile;
            (
                (n.at + o.at) / 2.0,
                if mine { Joint::Post } else { Joint::Bare },
            )
        })
        .collect();
    let way = |j: Vec2| (j - n.at).normalize_or(Vec2::X);
    let (mut from, mut to, mut ends, arms) = match joints.len() {
        0 => {
            let dir = n.dir.unwrap_or(Vec2::X);
            (
                n.at - dir * 0.5,
                n.at + dir * 0.5,
                [Joint::Stop, Joint::Stop],
                Vec::new(),
            )
        }
        1 => {
            let (j, joint) = joints[0];
            let dir = (n.at - j).try_normalize().or(n.dir).unwrap_or(Vec2::X);
            (j, n.at + dir * 0.5, [joint, Joint::Stop], Vec::new())
        }
        _ => {
            // The two joints most nearly opposite are the tile's piece;
            // any more are arms.
            let mut pair = (0, 1);
            let mut least = f32::MAX;
            for a in 0..joints.len() {
                for b in a + 1..joints.len() {
                    let d = way(joints[a].0).dot(way(joints[b].0));
                    if d < least {
                        (pair, least) = ((a, b), d);
                    }
                }
            }
            let rest = |skip: &[usize]| {
                (joints.iter().enumerate())
                    .filter(|(k, _)| !skip.contains(k))
                    .map(|(_, &(j, joint))| (n.at, j, joint))
                    .collect::<Vec<_>>()
            };
            if least > HAIRPIN {
                let (a, joint) = joints[pair.0];
                // The first arm puts the post at the tile's point.
                (a, n.at, [joint, Joint::Bare], rest(&[pair.0]))
            } else {
                let ((a, ja), (b, jb)) = (joints[pair.0], joints[pair.1]);
                (a, b, [ja, jb], rest(&[pair.0, pair.1]))
            }
        }
    };
    // The way its line was drawn, so its front is told from its back.
    if let Some(dir) = n.dir
        && (to - from).dot(dir) < 0.0
    {
        (from, to) = (to, from);
        ends.swap(0, 1);
    }
    Shape {
        from,
        to,
        ends,
        arms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(tile: (i32, i32), drawing: Option<(u32, f32)>, dir: Option<Vec2>) -> Node {
        Node {
            owner: 0,
            kind: Kind::Wall,
            tile,
            at: Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
            dir,
            drawing: drawing.map(|d| d.0),
            arc: drawing.map_or(0.0, |d| d.1),
            links: Vec::new(),
        }
    }

    fn settled(nodes: Vec<Node>) -> WallLines {
        let mut lines = WallLines {
            nodes,
            shapes: HashMap::new(),
        };
        lines.settle();
        lines
    }

    fn shape(lines: &WallLines, tile: (i32, i32)) -> &Shape {
        &lines.shapes[&(0, 0, tile)]
    }

    /// Every piece end that isn't a stop meets another piece's end.
    fn unbroken(lines: &WallLines) {
        let ends: Vec<(Vec2, Joint)> = (lines.shapes.values())
            .flat_map(|s| {
                let arms = s.arms.iter().map(|a| (a.1, a.2));
                [(s.from, s.ends[0]), (s.to, s.ends[1])]
                    .into_iter()
                    .chain(arms)
            })
            .collect();
        for &(p, joint) in ends.iter().filter(|e| e.1 != Joint::Stop) {
            let meets = ends.iter().filter(|e| e.0.distance(p) < 1e-4).count();
            assert_eq!(meets, 2, "a joint at {p} of {joint:?}");
            // One post a joint.
            let posts = (ends.iter())
                .filter(|e| e.0.distance(p) < 1e-4 && e.1 == Joint::Post)
                .count();
            assert_eq!(posts, 1, "posts at {p}");
        }
    }

    #[test]
    fn a_straight_line_is_one_unbroken_run_with_a_stop_at_each_end() {
        let tiles: Vec<_> = (0..5).map(|x| (x, 3)).collect();
        let nodes = (tiles.iter().enumerate())
            .map(|(k, &t)| node(t, Some((7, k as f32)), Some(Vec2::X)))
            .collect();
        let lines = settled(nodes);
        unbroken(&lines);
        let stops = |t| {
            (shape(&lines, t).ends.iter())
                .filter(|&&e| e == Joint::Stop)
                .count()
        };
        assert_eq!(stops((0, 3)), 1);
        assert_eq!(stops((4, 3)), 1);
        assert_eq!(stops((2, 3)), 0);
        // The end tile reaches the edge of its tile; a middle one runs
        // joint to joint, a tile long.
        assert_eq!(shape(&lines, (0, 3)).from, Vec2::new(0.0, 3.5));
        let mid = shape(&lines, (2, 3));
        assert_eq!(
            (mid.from, mid.to),
            (Vec2::new(2.0, 3.5), Vec2::new(3.0, 3.5))
        );
    }

    #[test]
    fn two_lines_meeting_at_a_corner_join_with_no_end_posts_between() {
        // One line east along y = 0, another south from its end.
        let mut nodes: Vec<_> = (0..4)
            .map(|x| node((x, 0), Some((1, x as f32)), Some(Vec2::X)))
            .collect();
        nodes.extend((1..4).map(|y| node((3, y), Some((2, y as f32)), Some(Vec2::Y))));
        let lines = settled(nodes);
        unbroken(&lines);
        let stops: usize = (lines.shapes.values())
            .map(|s| s.ends.iter().filter(|&&e| e == Joint::Stop).count())
            .sum();
        assert_eq!(stops, 2, "only the wall's two far ends stop");
        // The corner tile runs from its joint with the tile before to its
        // joint with the tile below: across the corner.
        let corner = shape(&lines, (3, 0));
        assert_eq!(
            (corner.from, corner.to),
            (Vec2::new(3.0, 0.5), Vec2::new(3.5, 1.0))
        );
    }

    #[test]
    fn single_tiles_mended_into_a_gap_join_the_wall_either_side() {
        // A wall east with a gap at x = 2 and 3, each mended as a tile of
        // its own (its own drawing, a tile long: no way of its own).
        let mut nodes = vec![
            node((0, 0), Some((1, 0.0)), Some(Vec2::X)),
            node((1, 0), Some((1, 1.0)), Some(Vec2::X)),
            node((4, 0), Some((1, 4.0)), Some(Vec2::X)),
            node((5, 0), Some((1, 5.0)), Some(Vec2::X)),
        ];
        nodes.push(node((2, 0), Some((8, 0.0)), None));
        nodes.push(node((3, 0), Some((9, 0.0)), None));
        let lines = settled(nodes);
        unbroken(&lines);
        for x in 1..5 {
            assert!(
                !shape(&lines, (x, 0)).ends.contains(&Joint::Stop),
                "tile {x} stops"
            );
        }
    }

    #[test]
    fn steps_on_the_slant_run_corner_to_corner() {
        // Single tiles stepping down to the south-east.
        let nodes = (0..4)
            .map(|k| node((k, k), Some((10 + k as u32, 0.0)), None))
            .collect();
        let lines = settled(nodes);
        unbroken(&lines);
        let mid = shape(&lines, (1, 1));
        let dir = (mid.to - mid.from).normalize();
        assert!(dir.dot(Vec2::ONE.normalize()).abs() > 0.99, "{mid:?}");
    }

    #[test]
    fn a_wall_ending_on_another_makes_a_joint_with_an_arm() {
        // A wall east along y = 2, and one coming down onto its middle.
        let mut nodes: Vec<_> = (0..5)
            .map(|x| node((x, 2), Some((1, x as f32)), Some(Vec2::X)))
            .collect();
        nodes.extend((0..2).map(|y| node((2, y), Some((2, y as f32)), Some(Vec2::Y))));
        let lines = settled(nodes);
        unbroken(&lines);
        let tee = shape(&lines, (2, 2));
        assert_eq!(tee.arms.len(), 1);
        assert_eq!(
            (tee.from, tee.to),
            (Vec2::new(2.0, 2.5), Vec2::new(3.0, 2.5))
        );
        // The wall coming down has no end post at the meeting.
        assert!(!shape(&lines, (2, 1)).ends.contains(&Joint::Stop));
    }

    #[test]
    fn a_wall_runs_up_to_a_gate_and_stops_at_a_wall_that_is_not_there() {
        let mut nodes: Vec<_> = (0..3)
            .map(|x| node((x, 0), Some((1, x as f32)), Some(Vec2::X)))
            .collect();
        for x in 3..6 {
            nodes.push(Node {
                kind: Kind::Gate,
                ..node((x, 0), None, None)
            });
        }
        let lines = settled(nodes);
        let last = shape(&lines, (2, 0));
        assert_eq!(last.ends, [Joint::Bare, Joint::Post]);
        assert_eq!(last.to, Vec2::new(3.0, 0.5), "to the gate's edge");
        assert!(
            !lines.shapes.contains_key(&(0, 0, (4, 0))),
            "a gate shows no piece"
        );
        assert_eq!(shape(&lines, (0, 0)).ends[0], Joint::Stop);
    }

    #[test]
    fn a_curve_bends_at_its_joints_and_nowhere_else() {
        // Anchors on a quarter circle of radius 6 about (0, 0).
        let mut nodes = Vec::new();
        let mut last = None;
        for k in 0..=40 {
            let a = k as f32 / 40.0 * std::f32::consts::FRAC_PI_2;
            let p = Vec2::new(a.cos(), a.sin()) * 6.0;
            let tile = (p.x.floor() as i32, p.y.floor() as i32);
            if last != Some(tile) {
                let dir = Vec2::new(-a.sin(), a.cos());
                nodes.push(Node {
                    at: p,
                    ..node(tile, Some((3, a * 6.0)), Some(dir))
                });
                last = Some(tile);
            }
        }
        let lines = settled(nodes);
        unbroken(&lines);
        // No piece far from the circle.
        for s in lines.shapes.values() {
            for p in [s.from, s.to] {
                assert!((p.length() - 6.0).abs() < 0.6, "{p} is off the curve");
            }
        }
    }
}
