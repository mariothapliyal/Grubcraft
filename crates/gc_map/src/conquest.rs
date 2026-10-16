//! Conquest map recipes (random maps step 2): each a small program that
//! writes a whole Conquest map description ([`MapDef`]) from a seed.
//!
//! A recipe writes the first player's share: the ground around its start
//! and the economy a Conquest match needs (the start, two seeps and a
//! Marrow vein in the base, a natural expansion of two seeps, a hamlet,
//! contested seeps and veins guarded by creep camps, a Shrine, Bloodseep
//! sites and frontier slots). The loader turns that share round for the
//! other players (`Rot2` for two, `Rot4` for four), so every start gets
//! the same ground and the same economy. The Heart, and on a Warfield the
//! Old Mouth and the Bazaar, are placed once in the middle.
//!
//! Whether a map is fair is the checker's call (`gc_mapcheck`), which
//! rerolls a seed whose map fails; the recipes keep their random choices
//! inside ranges where most seeds pass.

use gc_fixed::Pcg32;

use crate::def::{
    CoverKind, CoverOp, Kind, MapDef, Mode, Noise, Object, Pos, SetPiece, SetPieceKind, Shape,
    SizeClass, Symmetry,
};
use crate::recipe::Writer;

/// The kinds of Conquest map a seed can make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConquestRecipe {
    /// Each start on a high fang with cliff sides and two ramps down;
    /// rock spines split the open ground into lanes, and the players meet
    /// at wading places.
    Teeth,
    /// A wide shallow bowl in the middle, the Heart on a raised tongue in
    /// it, the starts on its rim and hills round the edge.
    Bowl,
    /// Low wet ground: wadeable pools everywhere, a few deep meres, the
    /// starts on dry islands and reeds to hide in.
    Fen,
}

impl ConquestRecipe {
    pub const ALL: [ConquestRecipe; 3] = [
        ConquestRecipe::Teeth,
        ConquestRecipe::Bowl,
        ConquestRecipe::Fen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ConquestRecipe::Teeth => "Teeth",
            ConquestRecipe::Bowl => "Bowl",
            ConquestRecipe::Fen => "Fen",
        }
    }
}

/// The sizes the Conquest recipes make: two players on a Duel or a
/// Skirmish map, four on a Warfield.
pub const CONQUEST_SIZES: [SizeClass; 3] =
    [SizeClass::Duel, SizeClass::Skirmish, SizeClass::Warfield];

/// Ground level and the water line (sixteenths of a tile), as the shipped
/// maps have them.
const BASE: i32 = 40;
const WATER: i32 = 24;
/// Water this far under the line is a ford, and this far too deep to wade.
const FORD: i32 = WATER - 3;
const DEEP: i32 = WATER - 14;
/// A fen's dry ground: half a tile above the water.
const FEN_BASE: i32 = WATER + 8;

/// The description a recipe makes from a seed, or `None` for a size it
/// doesn't make (see [`CONQUEST_SIZES`]).
pub fn describe(recipe: ConquestRecipe, size: SizeClass, seed: u64) -> Option<MapDef> {
    let frame = Frame::of(size)?;
    let mut rng = Pcg32::new(seed, 0x0063_6f6e);
    let base = if recipe == ConquestRecipe::Fen {
        FEN_BASE
    } else {
        BASE
    };
    let mut share = Share {
        w: Writer::new(&mut rng, frame.n),
        f: frame,
        base,
        objects: Vec::new(),
        features: Vec::new(),
    };
    // Rolling ground first; the objects next, so the ground the recipe
    // shapes after can keep clear of them; then the recipe's shape and
    // its cover.
    let noise = Noise {
        seed: share.w.rng.next_u32(),
        amplitude: share.w.range(12, 24),
        scale: share.w.range(32, 48),
    };
    share.economy();
    match recipe {
        ConquestRecipe::Teeth => share.teeth(),
        ConquestRecipe::Bowl => share.bowl(),
        ConquestRecipe::Fen => share.fen(),
    }
    let Share { w, f, objects, .. } = share;
    Some(MapDef {
        name: format!("{} {size:?} {seed}", recipe.name()),
        size,
        symmetry: f.symmetry,
        players: f.players,
        base,
        water: WATER,
        noise: Some(noise),
        terrain: w.terrain,
        cover: w.cover,
        objects,
        set_pieces: w.set_pieces,
    })
}

/// Where the first player's share sits on a map of one size.
#[derive(Clone, Copy)]
struct Frame {
    n: i32,
    start: Pos,
    centre: Pos,
    symmetry: Symmetry,
    players: u8,
}

impl Frame {
    fn of(size: SizeClass) -> Option<Frame> {
        // (tiles, how far the start is in from the north-west corner, symmetry, players)
        let (n, inset, symmetry, players) = match size {
            SizeClass::Duel => (128, 22, Symmetry::Rot2, 2),
            SizeClass::Skirmish => (192, 30, Symmetry::Rot2, 2),
            SizeClass::Warfield => (256, 48, Symmetry::Rot4, 4),
            SizeClass::Grand | SizeClass::Field(_) => return None,
        };
        Some(Frame {
            n,
            start: (inset, inset),
            centre: (n / 2, n / 2),
            symmetry,
            players,
        })
    }

    /// The point `u` thousandths of the way from the start to the centre
    /// and `v` tiles to the side of that line (positive is north-east).
    fn at(&self, u: i32, v: i32) -> Pos {
        let (sx, sy) = self.start;
        let (cx, cy) = self.centre;
        let side = v * 707 / 1000;
        (
            sx + (cx - sx) * u / 1000 + side,
            sy + (cy - sy) * u / 1000 - side,
        )
    }

    /// `t` tiles on a Warfield, scaled to this map's size.
    fn scaled(&self, t: i32) -> i32 {
        t * self.n / 256
    }

    /// A tile and its turned copies, as the loader makes them.
    fn copies(&self, p: Pos) -> Vec<Pos> {
        let turns = 4 / self.symmetry.copies() as u32;
        let mut out = vec![p];
        let mut q = p;
        for _ in 1..self.symmetry.copies() {
            for _ in 0..turns {
                q = (self.n - 1 - q.1, q.0);
            }
            out.push(q);
        }
        out
    }
}

/// The first player's share being written.
struct Share<'a> {
    w: Writer<'a>,
    f: Frame,
    /// The recipe's ground level before its stamps.
    base: i32,
    objects: Vec<Object>,
    /// Ground already shaped (hills, pools, meres, spines): the centre and
    /// how far it reaches, kept clear of each other.
    features: Vec<(Pos, i32)>,
}

fn dist2(a: Pos, b: Pos) -> i32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dy * dy
}

impl Share<'_> {
    /// An object of the first player's share; `owned` ones belong to it.
    fn put(&mut self, kind: Kind, at: Pos, owned: bool, label: &str) {
        self.objects.push(Object {
            kind,
            at,
            owner: owned.then_some(0),
            label: label.into(),
            once: false,
        });
    }

    /// An object placed once, not turned.
    fn once(&mut self, kind: Kind, at: Pos) {
        self.objects.push(Object {
            kind,
            at,
            owner: None,
            label: String::new(),
            once: true,
        });
    }

    /// Whether ground stamped at `at` reaching `r` tiles (and its turned
    /// copies) stays clear of every object and every feature already
    /// shaped, turned copies included.
    fn clear(&self, at: Pos, r: i32) -> bool {
        let spots = self.f.copies(at);
        let apart = |p: Pos, d: i32| spots.iter().all(|&s| dist2(p, s) >= d * d);
        let objects = self.objects.iter().all(|o| {
            if o.once {
                apart(o.at, r)
            } else {
                self.f.copies(o.at).into_iter().all(|p| apart(p, r))
            }
        });
        objects
            && self
                .features
                .iter()
                .all(|&(p, fr)| self.f.copies(p).into_iter().all(|q| apart(q, r + fr)))
    }

    /// A random spot in the share, `u` and `v` within the ranges (see
    /// [`Frame::at`]), clear by `r` (see [`Share::clear`]); `None` if
    /// none is found in a few dozen tries.
    fn spot(&mut self, u: (i32, i32), v: (i32, i32), r: i32) -> Option<Pos> {
        for _ in 0..32 {
            let at = self.f.at(self.w.range(u.0, u.1), self.w.range(v.0, v.1));
            let inside = (8..self.f.n - 8).contains(&at.0) && (8..self.f.n - 8).contains(&at.1);
            if inside && self.clear(at, r) {
                return Some(at);
            }
        }
        None
    }

    /// The economy: the same for every recipe, with a little jitter.
    fn economy(&mut self) {
        let s = self.f.start;
        let jitter = |w: &mut Writer, (x, y): Pos| (x + w.range(-1, 1), y + w.range(-1, 1));
        self.put(Kind::Start, s, true, "");
        // The base: two seeps and a vein within a dozen tiles.
        let a = jitter(&mut self.w, (s.0 - 9, s.1 + 8));
        let b = jitter(&mut self.w, (s.0 + 8, s.1 - 9));
        let v = jitter(&mut self.w, (s.0 + 8, s.1 + 8));
        self.put(Kind::GrubSeep, a, true, "");
        self.put(Kind::GrubSeep, b, true, "");
        self.put(Kind::MarrowVein, v, true, "");
        // The natural expansion, out of the base to the east.
        let e1 = jitter(&mut self.w, (s.0 + 32, s.1 - 6));
        let e2 = jitter(&mut self.w, (s.0 + 34, s.1 + 6));
        self.put(Kind::GrubSeep, e1, true, "");
        self.put(Kind::GrubSeep, e2, true, "");
        // A hamlet on the way out to the south-west.
        let f = self.f;
        let village = f.at(self.w.range(420, 480), -f.scaled(24));
        self.put(Kind::Village, village, false, "");
        // Contested nodes, each with a camp of creeps between it and the middle.
        let seep = f.at(self.w.range(640, 700), f.scaled(28));
        let vein = f.at(self.w.range(620, 680), -f.scaled(30));
        self.put(Kind::GrubSeep, seep, false, "");
        self.put(
            Kind::CreepCamp,
            (seep.0 + 4, seep.1 + 2),
            false,
            "Bile Toads",
        );
        self.put(Kind::MarrowVein, vein, false, "");
        self.put(
            Kind::CreepCamp,
            (vein.0 + 2, vein.1 + 4),
            false,
            "Marrow Golems",
        );
        // Bloodseep sites (three at least on the map) and frontier slots
        // (four to eight).
        let bloodseep = f.at(self.w.range(820, 860), f.scaled(18));
        self.put(Kind::Bloodseep, bloodseep, false, "");
        match f.symmetry {
            Symmetry::Rot2 => {
                let other = f.at(self.w.range(820, 860), -f.scaled(22));
                self.put(Kind::Bloodseep, other, false, "");
                let n = f.n;
                self.put(Kind::FrontierSlot, (n - n / 8, n / 8), false, "");
                let far = f.at(self.w.range(560, 620), f.scaled(84));
                self.put(Kind::FrontierSlot, far, false, "");
            }
            _ => {
                let edge = f.at(self.w.range(380, 420), -f.scaled(64));
                self.put(Kind::FrontierSlot, edge, false, "");
            }
        }
        // The middle.
        let (cx, cy) = f.centre;
        self.once(Kind::Heart, (cx, cy));
        if f.symmetry == Symmetry::Rot4 {
            self.once(Kind::OldMouth, (cx + 4, cy - 1));
            self.once(Kind::Bazaar, (cx, cy - 12));
        }
        // A Shrine off to the north-east, wherever there is room.
        let shrine = self
            .spot((250, 450), (f.scaled(36), f.scaled(60)), 4)
            .unwrap_or_else(|| f.at(300, f.scaled(52)));
        self.put(Kind::Shrine, shrine, false, "");
    }

    /// Mark ground shaped at `at` out to `r` tiles, so later features keep
    /// clear of it.
    fn claim(&mut self, at: Pos, r: i32) {
        self.features.push((at, r));
    }

    /// A hill a line can hold, clear of objects, named as a set piece.
    fn hill(&mut self, u: (i32, i32), v: (i32, i32), label: &str) {
        let r = self.w.range(3, 4);
        let edge = self.w.range(10, 12);
        let wide = self.f.scaled(60);
        let Some(at) = self
            .spot(u, v, r + edge / 2 + 2)
            .or_else(|| self.spot((250, 900), (-wide, wide), r + edge / 2 + 2))
        else {
            return;
        };
        self.claim(at, r + edge);
        let height = self.base + self.w.range(40, 52);
        self.w
            .stamp(Shape::Disc { at, r }, Mode::Raise, height, edge);
        self.w.set_pieces.push(SetPiece {
            kind: SetPieceKind::High,
            at,
            label: label.into(),
        });
    }

    /// A few clumps of `kind` clear of the objects.
    fn clumps(&mut self, kind: CoverKind, count: i32) {
        for _ in 0..count {
            let r = self.w.range(4, 8);
            let half = self.f.scaled(60);
            if let Some(at) = self.spot((250, 950), (-half, half), r + 3) {
                self.w.cover.push(CoverOp {
                    shape: Shape::Disc { at, r },
                    kind,
                });
            }
        }
    }

    /// The wading place where the players meet: a pool round the Heart's
    /// island on two-player maps, a stream between neighbours on four.
    fn wading_place(&mut self) {
        let f = self.f;
        let (cx, cy) = f.centre;
        let at = match f.symmetry {
            Symmetry::Rot2 => {
                let r = self.w.range(6, 8);
                self.w
                    .stamp(Shape::Disc { at: f.centre, r }, Mode::Set, FORD, 4);
                self.w
                    .stamp(Shape::Disc { at: f.centre, r: 2 }, Mode::Set, BASE + 4, 2);
                self.claim(f.centre, r + 4);
                (cx, cy + 3)
            }
            _ => {
                let (y0, y1) = (f.scaled(60), f.scaled(96));
                let a = (cx, y0);
                let b = (cx, y1);
                self.w.stamp(
                    Shape::Line {
                        a,
                        b,
                        half_width: 3,
                    },
                    Mode::Lower,
                    FORD - 1,
                    3,
                );
                for y in (y0..=y1).step_by(6) {
                    self.claim((cx, y), 6);
                }
                (cx, (y0 + y1) / 2)
            }
        };
        self.w.set_pieces.push(SetPiece {
            kind: SetPieceKind::Ford,
            at,
            label: "The wading place".into(),
        });
    }

    /// Fangs: a high plateau with cliff sides for the start, two ramps
    /// down, rock spines between the lanes.
    fn teeth(&mut self) {
        let f = self.f;
        let s = f.start;
        let r = f.scaled(20).max(14);
        self.w
            .stamp(Shape::Disc { at: s, r }, Mode::Set, BASE + 56, 1);
        self.claim(s, r + 6);
        // The ramps, east (to the natural expansion) and south.
        for at in [(s.0 + r, s.1), (s.0, s.1 + r)] {
            self.w
                .stamp(Shape::Disc { at, r: 5 }, Mode::Set, BASE + 16, 5);
        }
        self.wading_place();
        self.hill(
            (420, 680),
            (-f.scaled(36), f.scaled(36)),
            "Hill of the Back Molar",
        );
        // Rock spines that split the open ground into lanes.
        for _ in 0..self.w.range(2, 3) {
            self.spine();
        }
        self.clumps(CoverKind::Wood, 2);
        self.clumps(CoverKind::Grass, 2);
    }

    /// A short wall of rock across open ground, clear of the objects and
    /// of the ground already shaped.
    fn spine(&mut self) {
        let f = self.f;
        for _ in 0..32 {
            let half = self.w.range(f.scaled(10), f.scaled(16));
            let mid = f.at(
                self.w.range(380, 780),
                self.w.range(-f.scaled(48), f.scaled(48)),
            );
            let (a, b) = if self.w.range(0, 1) == 0 {
                ((mid.0 - half, mid.1), (mid.0 + half, mid.1))
            } else {
                ((mid.0, mid.1 - half), (mid.0, mid.1 + half))
            };
            let inside = |(x, y): Pos| (8..f.n - 8).contains(&x) && (8..f.n - 8).contains(&y);
            let along = (0..=4).map(|k| (a.0 + (b.0 - a.0) * k / 4, a.1 + (b.1 - a.1) * k / 4));
            let points: Vec<Pos> = along.collect();
            if points.iter().all(|&p| inside(p) && self.clear(p, 7)) {
                for &p in &points {
                    self.claim(p, 4);
                }
                self.w.stamp(
                    Shape::Line {
                        a,
                        b,
                        half_width: 2,
                    },
                    Mode::Raise,
                    BASE + 80,
                    2,
                );
                return;
            }
        }
    }

    /// A wide bowl in the middle with the Heart on a raised tongue, the
    /// starts on its rim.
    fn bowl(&mut self) {
        let f = self.f;
        // The bowl reaches a third of the way out to each start.
        let (r, edge) = (f.n / 6, f.n / 8);
        self.w.stamp(
            Shape::Disc { at: f.centre, r },
            Mode::Lower,
            BASE - 14,
            edge,
        );
        let tongue = self.w.range(3, 4);
        self.w.stamp(
            Shape::Disc {
                at: f.centre,
                r: tongue,
            },
            Mode::Set,
            BASE + 6,
            6,
        );
        self.w.set_pieces.push(SetPiece {
            kind: SetPieceKind::High,
            at: f.centre,
            label: "The tongue".into(),
        });
        let s = f.start;
        self.w
            .stamp(Shape::Disc { at: s, r: 14 }, Mode::Set, BASE + 10, 10);
        self.hill(
            (480, 680),
            (-f.scaled(48), -f.scaled(8)),
            "The west rim hill",
        );
        self.hill((480, 680), (f.scaled(8), f.scaled(48)), "The east rim hill");
        self.clumps(CoverKind::Wood, 3);
        self.clumps(CoverKind::Grass, 2);
    }

    /// Low wet ground: wadeable pools, a deep mere or two, the starts on
    /// dry islands.
    fn fen(&mut self) {
        let f = self.f;
        let s = f.start;
        self.w
            .stamp(Shape::Disc { at: s, r: 16 }, Mode::Set, BASE + 4, 6);
        self.hill((420, 680), (-f.scaled(36), f.scaled(36)), "The mound");
        let half = f.scaled(56);
        let mut named = false;
        for _ in 0..4 + f.n / 48 {
            let r = self.w.range(4, 7);
            let Some(at) = self.spot((250, 920), (-half, half), r + 3) else {
                continue;
            };
            self.claim(at, r + 2);
            self.w.stamp(Shape::Disc { at, r }, Mode::Set, FORD, 3);
            if !named && r >= 5 {
                self.w.set_pieces.push(SetPiece {
                    kind: SetPieceKind::Ford,
                    at,
                    label: "The reed ford".into(),
                });
                named = true;
            }
        }
        // Channels between the pools, and a deep mere or two no one wades.
        for _ in 0..self.w.range(2, 3) {
            let len = self.w.range(6, 10);
            // The channel reaches 1.12 × len from its middle, and its banks
            // a few tiles more.
            let reach = len * 9 / 8 + 5;
            let Some(mid) = self.spot((300, 880), (-half, half), reach + 1) else {
                continue;
            };
            self.claim(mid, reach);
            let (a, b) = if self.w.range(0, 1) == 0 {
                (
                    (mid.0 - len, mid.1 - len / 2),
                    (mid.0 + len, mid.1 + len / 2),
                )
            } else {
                (
                    (mid.0 - len / 2, mid.1 - len),
                    (mid.0 + len / 2, mid.1 + len),
                )
            };
            let half_width = self.w.range(1, 2);
            self.w
                .stamp(Shape::Line { a, b, half_width }, Mode::Set, FORD, 3);
        }
        for _ in 0..self.w.range(1, 2) {
            let r = self.w.range(3, 4);
            if let Some(at) = self.spot((300, 850), (-half, half), r + 5) {
                self.claim(at, r + 3);
                self.w.stamp(Shape::Disc { at, r }, Mode::Lower, DEEP, 3);
            }
        }
        self.clumps(CoverKind::Grass, 5);
        self.clumps(CoverKind::Wood, 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_recipe_size_and_seed_give_the_same_description() {
        for recipe in ConquestRecipe::ALL {
            for size in CONQUEST_SIZES {
                for seed in 1..=3 {
                    let a = describe(recipe, size, seed).unwrap().to_ron();
                    assert_eq!(a, describe(recipe, size, seed).unwrap().to_ron());
                    assert_ne!(a, describe(recipe, size, seed + 1).unwrap().to_ron());
                }
            }
        }
    }

    #[test]
    fn grand_and_field_sizes_are_not_made() {
        assert!(describe(ConquestRecipe::Teeth, SizeClass::Grand, 1).is_none());
        assert!(describe(ConquestRecipe::Bowl, SizeClass::Field(256), 1).is_none());
    }
}
