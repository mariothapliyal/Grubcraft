//! Drawing fog of war on the ground (docs/08-ui-ux.md, "Fog of war").
//!
//! The simulation says where the viewer's side can see, as circles
//! (`World::sight`). A few times a second this turns them into a grid with
//! one byte a tile for the renderer, which blurs the edges: 255 where the
//! side sees now, [`EXPLORED`] where it has seen before, [`UNEXPLORED`]
//! where it has not. What has been seen is remembered here only; it is not
//! part of the match and is forgotten when a new one starts.

use std::time::{Duration, Instant};

use gc_render::Renderer;
use gc_sim::{PlayerId, World};
use glam::Vec2;

/// How much of the ground shows on a tile seen before but not in sight now.
pub const EXPLORED: u8 = 140;
/// ... and on a tile never seen.
pub const UNEXPLORED: u8 = 51;
/// The grid is redrawn this often.
const EVERY: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct FogView {
    /// Tiles a side; 0 before the first update.
    tiles: usize,
    /// Tiles the side has seen at some point this match.
    explored: Vec<bool>,
    /// What the renderer is given.
    grid: Vec<u8>,
    /// `World::sight`'s circles, kept to save allocating.
    sight: Vec<(gc_fixed::FxVec2, gc_fixed::Fx)>,
    /// The circles in tiles as floats.
    circles: Vec<(Vec2, f32)>,
    last: Option<Instant>,
}

impl FogView {
    /// Forget what has been seen, for a new match.
    pub fn reset(&mut self) {
        self.explored.fill(false);
        self.last = None;
    }

    /// Draw the ground for `side` if the match is in fog and it is
    /// `viewing` through its eyes, else without fog. Call it every frame;
    /// it only redraws a few times a second.
    pub fn update(
        &mut self,
        now: Instant,
        renderer: &mut Renderer,
        world: &World,
        side: PlayerId,
        tiles: u32,
        viewing: bool,
    ) {
        if !(viewing && world.fog()) {
            renderer.set_fog(None);
            self.last = None;
            return;
        }
        if self.last.is_some_and(|t| now.duration_since(t) < EVERY) {
            return;
        }
        self.last = Some(now);
        world.sight(side, &mut self.sight);
        self.circles.clear();
        self.circles.extend(
            self.sight
                .iter()
                .map(|&(c, r)| (crate::from_fx_vec(c), crate::fx_to_f32(r))),
        );
        let circles = std::mem::take(&mut self.circles);
        self.paint(tiles as usize, &circles);
        self.circles = circles;
        renderer.set_fog(Some(&self.grid));
    }

    /// Fill `grid` for a map `tiles` across and the circles in sight
    /// (centre and radius, in tiles), and remember what they cover.
    fn paint(&mut self, tiles: usize, circles: &[(Vec2, f32)]) {
        if self.tiles != tiles {
            self.tiles = tiles;
            self.explored.clear();
            self.explored.resize(tiles * tiles, false);
            self.grid.clear();
            self.grid.resize(tiles * tiles, 0);
        }
        self.grid.fill(0);
        let last = tiles as f32 - 1.0;
        for &(c, r) in circles {
            // Tiles whose centres (x + 0.5, y + 0.5) lie inside the circle.
            let lo = |v: f32| (v - r - 0.5).ceil().clamp(0.0, last) as usize;
            let hi = |v: f32| (v + r - 0.5).floor().clamp(0.0, last) as usize;
            let (x0, x1, y0, y1) = (lo(c.x), hi(c.x), lo(c.y), hi(c.y));
            let r2 = r * r;
            for y in y0..=y1 {
                let dy = y as f32 + 0.5 - c.y;
                let row = y * tiles;
                for x in x0..=x1 {
                    let dx = x as f32 + 0.5 - c.x;
                    if dx * dx + dy * dy <= r2 {
                        self.grid[row + x] = 255;
                    }
                }
            }
        }
        for (cell, seen) in self.grid.iter_mut().zip(&mut self.explored) {
            if *cell == 255 {
                *seen = true;
            } else {
                *cell = if *seen { EXPLORED } else { UNEXPLORED };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_circle_lights_the_tiles_whose_centres_it_holds() {
        let mut f = FogView::default();
        f.paint(8, &[(Vec2::new(4.0, 4.0), 1.0)]);
        // Centres (3.5, 3.5) and the like are 0.71 away; (2.5, 3.5) is 1.58.
        let lit: Vec<usize> = (0..64).filter(|&i| f.grid[i] == 255).collect();
        assert_eq!(lit, vec![27, 28, 35, 36]);
        assert_eq!(f.grid[0], UNEXPLORED);
    }

    #[test]
    fn what_was_seen_is_remembered() {
        let mut f = FogView::default();
        f.paint(8, &[(Vec2::new(4.0, 4.0), 1.0)]);
        f.paint(8, &[(Vec2::new(1.0, 1.0), 1.0)]);
        assert_eq!(f.grid[0], 255);
        assert_eq!(f.grid[27], EXPLORED);
        assert_eq!(f.grid[63], UNEXPLORED);
        f.reset();
        f.paint(8, &[]);
        assert!(f.grid.iter().all(|&v| v == UNEXPLORED));
    }

    #[test]
    fn circles_off_the_edge_are_clipped() {
        let mut f = FogView::default();
        f.paint(
            8,
            &[
                (Vec2::new(-10.0, 4.0), 3.0),
                (Vec2::new(20.0, 4.0), 3.0),
                (Vec2::new(7.5, 7.5), 100.0),
            ],
        );
        assert!(f.grid.iter().all(|&v| v == 255));
    }
}
