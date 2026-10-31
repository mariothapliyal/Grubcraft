//! Drawing a wall or bridge by dragging (docs/26-fortifications.md, "Drawn
//! walls" and "Bridges"; docs/29-restart-plan.md). The player presses the
//! button, drags along the way the line should go and lets go: the tool
//! drops a control point every few tiles of mouse travel, snaps each to the
//! half-tile grid the simulation's [`DrawnPath`] is drawn on, and thins them
//! out to the few it takes. None of this knows the world: `wall_tool` has
//! the mouse, the preview and the orders.

use gc_sim::drawn::{DrawnPath, MAX_POINTS};
use glam::Vec2;

/// How far the mouse travels, in tiles, between two control points.
pub const SPACING: f32 = 3.0;
/// How far from where the button went down, in tiles, a press has to travel
/// to count as a drag and not a click.
pub const DRAG_REACH: f32 = 2.0;
/// The most points a drag keeps before it stops adding more.
const MOST_RAW: usize = 96;
/// A dropped point this near the end (in half tiles) is left out: it would
/// only make the end wobble.
const NEAR_END: i32 = 4;

/// A control point, in half tiles.
pub type Half = (i16, i16);

/// The half-tile point nearest ground point `p` (in tiles): the centre of a
/// tile or an edge of one.
pub fn half_of(p: Vec2) -> Half {
    let h = |v: f32| (v * 2.0).round().clamp(0.0, f32::from(i16::MAX)) as i16;
    (h(p.x), h(p.y))
}

/// The centre of tile `t`, in half tiles.
pub fn centre_half(t: (i32, i32)) -> Half {
    let c = |v: i32| (2 * v + 1).clamp(0, i32::from(i16::MAX)) as i16;
    (c(t.0), c(t.1))
}

/// A drag in progress: the control points dropped so far.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    /// The points dropped so far, the first the start.
    pub raw: Vec<Half>,
    /// Where on the ground the last was dropped, in tiles.
    pub last: Vec2,
    /// It has travelled far enough to be a drag, not a click.
    pub live: bool,
}

impl Drag {
    /// A drag that starts on `tile`.
    pub fn new(tile: (i32, i32)) -> Drag {
        Drag {
            raw: vec![centre_half(tile)],
            last: Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5),
            live: false,
        }
    }

    /// The mouse is now over ground point `at`: drop a point if it has
    /// moved [`SPACING`] tiles on from the last one.
    pub fn advance(&mut self, at: Vec2) {
        let start = Vec2::new(f32::from(self.raw[0].0), f32::from(self.raw[0].1)) / 2.0;
        if at.distance(start) >= DRAG_REACH {
            self.live = true;
        }
        if at.distance(self.last) >= SPACING && self.raw.len() < MOST_RAW {
            let h = half_of(at);
            if self.raw.last() != Some(&h) {
                self.raw.push(h);
            }
            self.last = at;
        }
    }
}

/// `points` thinned to at most `max`, evenly, keeping the first and last.
pub fn thin(points: &[Half], max: usize) -> Vec<Half> {
    if points.len() <= max || max < 2 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    (0..max)
        .map(|k| points[(k * last + (max - 1) / 2) / (max - 1)])
        .collect()
}

fn near(a: Half, b: Half) -> bool {
    let (dx, dy) = (i32::from(a.0 - b.0), i32::from(a.1 - b.1));
    dx * dx + dy * dy <= NEAR_END * NEAR_END
}

/// The path of a drag: the points dropped, then the end (half tiles),
/// thinned to [`MAX_POINTS`]. Smooth between its points; the ends are
/// corners (and a bridge has no others). `closed` makes it a smooth loop
/// back to the start. `None` if there is nothing to draw (fewer than two
/// places, or three for a loop).
pub fn path(raw: &[Half], end: Half, closed: bool) -> Option<DrawnPath> {
    let mut pts: Vec<Half> = raw.to_vec();
    // A point dropped right by the end only makes it wobble.
    while !closed && pts.len() > 1 && near(pts[pts.len() - 1], end) {
        pts.pop();
    }
    pts.push(end);
    pts.dedup();
    if closed {
        // The way back to the start closes the loop.
        while pts.len() > 1 && near(pts[pts.len() - 1], pts[0]) {
            pts.pop();
        }
        if pts.len() < 3 {
            return None;
        }
        let mut loop_pts = thin(&pts, MAX_POINTS - 1);
        loop_pts.push(loop_pts[0]);
        return DrawnPath::new(&loop_pts, |_| false);
    }
    if pts.len() < 2 {
        return None;
    }
    DrawnPath::new(&thin(&pts, MAX_POINTS), |_| false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_snap_to_the_half_tile_grid() {
        // The middle of tile 4 and the edge of tile 5.
        assert_eq!(half_of(Vec2::new(4.5, 5.0)), (9, 10));
        assert_eq!(half_of(Vec2::new(4.2, 5.3)), (8, 11));
        assert_eq!(centre_half((4, 5)), (9, 11));
        assert_eq!(half_of(Vec2::new(-3.0, 0.0)), (0, 0));
    }

    #[test]
    fn a_point_drops_every_few_tiles_of_travel() {
        let mut drag = Drag::new((10, 10));
        // Dragging along a row, half a tile a step.
        for step in 1..=40 {
            drag.advance(Vec2::new(10.5 + step as f32 * 0.5, 10.5));
        }
        // 20 tiles at 3 apiece: the start and six more.
        assert_eq!(drag.raw.len(), 7, "{:?}", drag.raw);
        for pair in drag.raw.windows(2) {
            let gap = f32::from(pair[1].0 - pair[0].0) / 2.0;
            assert!((2.5..=3.5).contains(&gap), "{gap}");
        }
        assert!(drag.live);
    }

    #[test]
    fn a_short_press_is_a_click_not_a_drag() {
        let mut drag = Drag::new((10, 10));
        drag.advance(Vec2::new(11.0, 10.8));
        assert!(!drag.live);
        assert_eq!(drag.raw.len(), 1);
        drag.advance(Vec2::new(13.0, 10.5));
        assert!(drag.live);
    }

    #[test]
    fn a_long_drag_is_thinned_to_the_most_points() {
        let raw: Vec<Half> = (0..60).map(|k| (1 + 6 * k, 20)).collect();
        let end = (1 + 6 * 60, 20);
        let path = path(&raw, end, false).expect("a path");
        assert_eq!(path.points().len(), MAX_POINTS);
        // The ends are kept.
        assert_eq!(path.points()[0], raw[0]);
        assert_eq!(*path.points().last().unwrap(), end);
        // Evenly: the gaps differ by a point or so.
        let gaps: Vec<i16> = path.points().windows(2).map(|w| w[1].0 - w[0].0).collect();
        let (lo, hi) = (gaps.iter().min().unwrap(), gaps.iter().max().unwrap());
        assert!(hi - lo <= 6, "{gaps:?}");
    }

    #[test]
    fn thinning_keeps_both_ends_and_order() {
        let pts: Vec<Half> = (0..30).map(|k| (k, 0)).collect();
        let t = thin(&pts, 12);
        assert_eq!(t.len(), 12);
        assert_eq!((t[0], t[11]), (pts[0], pts[29]));
        assert!(t.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(thin(&pts[..5], 12), pts[..5].to_vec());
    }

    #[test]
    fn a_straight_drag_is_a_path_the_simulation_accepts() {
        let mut drag = Drag::new((10, 10));
        for step in 1..=30 {
            drag.advance(Vec2::new(10.5 + step as f32 * 0.5, 10.5));
        }
        let end = centre_half((25, 10));
        let p = path(&drag.raw, end, false).expect("a path");
        assert!(p.check().is_ok());
        assert!(p.bends_gently());
        assert!(!p.has_corners());
        assert_eq!(p.ends(), ((10, 10), (25, 10)));
        // The wall tiles run along the row.
        assert!(p.wall_tiles().iter().all(|t| t.1 == 10));
    }

    #[test]
    fn a_point_dropped_beside_the_end_is_left_out() {
        let raw = vec![(21, 21), (27, 21), (33, 21)];
        let p = path(&raw, (34, 21), false).expect("a path");
        assert_eq!(p.points(), &[(21, 21), (27, 21), (34, 21)]);
        // Nowhere to go: no path.
        assert!(path(&[(21, 21)], (21, 21), false).is_none());
    }

    #[test]
    fn a_ring_drag_closes_into_a_smooth_loop() {
        // Round a square, 10 tiles a side.
        let raw = vec![(21, 21), (41, 21), (41, 41), (21, 41)];
        let p = path(&raw, (21, 29), true).expect("a loop");
        assert!(p.closed());
        assert_eq!(p.points().first(), p.points().last());
        assert!(!p.corner(0));
        assert!(p.check().is_ok());
        // One place makes no loop.
        assert!(path(&[(21, 21)], (41, 21), true).is_none());
        let long: Vec<Half> = (0..40).map(|k| (21 + k * 4, 21 + (k % 7) * 3)).collect();
        let big = path(&long, (200, 60), true).expect("a loop");
        assert!(big.points().len() <= MAX_POINTS);
        assert!(big.closed());
    }
}
