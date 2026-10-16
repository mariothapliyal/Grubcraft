//! The showroom (`--showroom`): chosen soldier models standing in a row on
//! an empty field, seen from a fixed camera, for reviewing them. The layout
//! and the clip cycle are plain functions, tested below.

use glam::Vec2;

use crate::camera::{FOV_Y, MIN_DISTANCE, PITCH};
use crate::models::{Action, Clips, Pose};

/// Models in a row before the next row starts behind it.
pub const ROW_LEN: usize = 8;
/// A model's slot in the row: at least this wide, in tiles, ...
pub const SLOT_MIN: f32 = 1.5;
/// ... or this many times its height.
pub const SLOT_PER_HEIGHT: f32 = 1.1;
/// The gap between rows, in heights of the tallest model in the front row.
pub const ROW_GAP: f32 = 1.5;
/// How far a model reaches in front of and behind its spot, for fitting
/// the view.
const HALF_DEPTH: f32 = 0.5;
/// The whole arrangement fills this much of the window at most.
const FILL: f32 = 1.0 / 1.1;
/// Closest the camera comes (a lone model fills the window from near), and
/// the furthest, short of Commander View.
const NEAREST: f32 = 4.0;
const FURTHEST: f32 = 170.0;
/// Seconds each clip plays in the default cycle.
pub const CLIP_SECONDS: f32 = 4.0;
/// Where the models stand: the middle of the map.
pub const CENTRE: Vec2 = Vec2::splat(crate::MAP_TILES as f32 * 0.5);
/// The way the models face: towards the camera, which sits south of the
/// focus when the view is not turned.
pub const FACING: Vec2 = Vec2::Y;

const _: () = assert!(FURTHEST < crate::COMMANDER_IN);
const _: () = assert!(NEAREST < MIN_DISTANCE);

/// What a model shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clip {
    Idle,
    Walk,
    Attack,
    Shoot,
    Cheer,
    Die,
}

impl Clip {
    /// The cycle's order: idle, walk, attack, then fall.
    const CYCLE: [Clip; 4] = [Clip::Idle, Clip::Walk, Clip::Attack, Clip::Die];

    pub fn parse(name: &str) -> Option<Clip> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "idle" => Clip::Idle,
            "walk" => Clip::Walk,
            "attack" => Clip::Attack,
            "shoot" => Clip::Shoot,
            "cheer" => Clip::Cheer,
            "die" => Clip::Die,
            _ => return None,
        })
    }
}

/// Which clips play over time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    /// Every clip in turn, `CLIP_SECONDS` each, and round again.
    Cycle,
    /// One clip, looping (a fall plays once and holds).
    Hold(Clip),
}

impl Show {
    /// The clip playing `time` seconds in, and how many seconds into it.
    pub fn at(self, time: f32) -> (Clip, f32) {
        let time = time.max(0.0);
        match self {
            Show::Hold(clip) => (clip, time),
            Show::Cycle => {
                let round = time % (CLIP_SECONDS * Clip::CYCLE.len() as f32);
                let k = ((round / CLIP_SECONDS) as usize).min(Clip::CYCLE.len() - 1);
                (Clip::CYCLE[k], round - k as f32 * CLIP_SECONDS)
            }
        }
    }
}

/// The pose of `model` `time` seconds into the show, and whether to lay it
/// flat (a fall on a model with no death clip).
pub fn pose(clips: &Clips, model: usize, show: Show, time: f32) -> Option<(Pose, bool)> {
    let (clip, t) = show.at(time);
    let action = match clip {
        Clip::Die => return clips.death(model, t),
        Clip::Idle => Action::Idle,
        Clip::Walk => Action::Walk,
        Clip::Attack => Action::Attack,
        Clip::Shoot => Action::Shoot,
        Clip::Cheer => Action::Cheer,
    };
    clips.pose(model, action, t).map(|p| (p, false))
}

/// The models `spec` names: `all`, or names separated by commas, matched
/// without regard to case. `names` are the model names in pack order. An
/// unknown name comes back in `Err`.
pub fn choose(spec: &str, names: &[&str]) -> Result<Vec<usize>, String> {
    if spec.trim().eq_ignore_ascii_case("all") {
        return Ok((0..names.len()).collect());
    }
    spec.split(',')
        .map(|n| {
            let n = n.trim();
            names
                .iter()
                .position(|m| m.eq_ignore_ascii_case(n))
                .ok_or_else(|| n.to_string())
        })
        .collect()
}

/// Where the models stand and what the camera sees.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// Where each model stands, in the order asked for.
    pub positions: Vec<Vec2>,
    /// The ground point at the middle of the arrangement.
    pub focus: Vec2,
    /// How high above the ground the camera looks: half the tallest model.
    pub lift: f32,
    /// The arrangement's corners on the ground: slots across, and a model's
    /// depth front and back.
    low: Vec2,
    high: Vec2,
    /// Height of the tallest model.
    top: f32,
}

/// A model's slot across the row.
pub fn slot(height: f32) -> f32 {
    SLOT_MIN.max(SLOT_PER_HEIGHT * height)
}

/// Stand models of the given `heights` in rows of `ROW_LEN`, the first row
/// nearest the camera, centred on `centre`.
pub fn layout(heights: &[f32], centre: Vec2) -> Layout {
    let rows = heights.chunks(ROW_LEN);
    let gap = ROW_GAP
        * heights
            .iter()
            .take(ROW_LEN)
            .fold(0.0, |a: f32, &h| a.max(h));
    let mut positions = Vec::with_capacity(heights.len());
    let mut widest = 0.0_f32;
    let n_rows = rows.len();
    for (r, row) in rows.enumerate() {
        let width: f32 = row.iter().map(|&h| slot(h)).sum();
        widest = widest.max(width);
        let mut x = -width * 0.5;
        for &h in row {
            positions.push(Vec2::new(x + slot(h) * 0.5, -(r as f32) * gap));
            x += slot(h);
        }
    }
    // Rows run from z = 0 away from the camera; centre the lot on `centre`.
    let (front, back) = (HALF_DEPTH, -((n_rows.max(1) - 1) as f32) * gap - HALF_DEPTH);
    let shift = centre - Vec2::new(0.0, (front + back) * 0.5);
    for p in &mut positions {
        *p += shift;
    }
    let top = heights.iter().fold(0.0, |a: f32, &h| a.max(h));
    Layout {
        positions,
        focus: centre,
        lift: top * 0.5,
        low: Vec2::new(-widest * 0.5, back) + shift,
        high: Vec2::new(widest * 0.5, front) + shift,
        top,
    }
}

impl Layout {
    /// How far from the focus the camera must be, looking from the south
    /// at the usual pitch, for the whole arrangement (feet to the top of
    /// the tallest, slot edges, front to back) to fit a window of this
    /// `aspect` with a tenth to spare.
    pub fn distance(&self, aspect: f32) -> f32 {
        let (sin, cos) = PITCH.sin_cos();
        let half = (FOV_Y * 0.5).tan();
        let mut need = NEAREST;
        for x in [self.low.x, self.high.x] {
            for z in [self.low.y, self.high.y] {
                for y in [-self.lift, self.top - self.lift] {
                    // The corner relative to the focus: across the screen,
                    // up it, and how far it lies nearer than the focus.
                    let (q_x, q_z) = (x - self.focus.x, z - self.focus.y);
                    let across = q_x.abs();
                    let up = (y * cos - q_z * sin).abs();
                    let nearer = y * sin + q_z * cos;
                    need = need
                        .max(across / (half * aspect * FILL) + nearer)
                        .max(up / (half * FILL) + nearer);
                }
            }
        }
        need.min(FURTHEST)
    }
}

/// The models on show: which, where and doing what.
pub struct Showroom {
    pub models: Vec<usize>,
    pub layout: Layout,
    pub show: Show,
}

impl Showroom {
    pub fn new(models: Vec<usize>, heights: &[f32], show: Show) -> Showroom {
        let tall: Vec<f32> = models.iter().map(|&m| heights[m]).collect();
        Showroom {
            layout: layout(&tall, CENTRE),
            models,
            show,
        }
    }

    /// Hold the camera on the arrangement.
    pub fn aim(&self, camera: &mut crate::RtsCamera, ground: f32) {
        camera.focus = self.layout.focus;
        camera.focus_height = ground + self.layout.lift;
        camera.distance = self.layout.distance(camera.aspect);
        camera.home_yaw = 0.0;
        camera.yaw = 0.0;
    }

    /// Stand the models in this frame's drawing, `time` seconds in.
    pub fn push(
        &self,
        art: &mut crate::Art,
        ground: impl Fn(Vec2) -> f32,
        time: f32,
        color: [u8; 3],
    ) {
        for (&model, &pos) in self.models.iter().zip(&self.layout.positions) {
            let Some((pose, flat)) = pose(&art.clips, model, self.show, time) else {
                continue;
            };
            let at = glam::Vec3::new(pos.x, ground(pos), pos.y);
            let lod = art.view.lod(art.heights[model], at);
            art.buckets[model * gc_assets::model::LODS + lod].push(gc_render::SoldierInstance {
                pos: pos.to_array(),
                facing: FACING.to_array(),
                frames: pose.rows,
                flags: [(pose.blend * 255.0) as u8, 0, u8::from(flat) * 255, 0],
                color: [color[0], color[1], color[2], 255],
                lift: 0.0,
                scale: 1.0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::RtsCamera;

    const HEIGHTS: [f32; 5] = [2.1, 1.0, 0.8, 1.6, 2.4];

    #[test]
    fn names_match_without_case_or_spaces() {
        let names = ["Yari Ashigaru", "Yumi Ashigaru", "Bone Pikes"];
        assert_eq!(choose("all", &names), Ok(vec![0, 1, 2]));
        assert_eq!(choose(" ALL ", &names), Ok(vec![0, 1, 2]));
        assert_eq!(choose("bone pikes , yari ashigaru", &names), Ok(vec![2, 0]));
        assert_eq!(
            choose("Yari Ashigaru,Spearman", &names),
            Err("Spearman".into())
        );
        assert_eq!(choose("", &names), Err(String::new()));
    }

    #[test]
    fn the_real_models_can_all_be_chosen_by_name() {
        let models = crate::models::load().unwrap();
        let names: Vec<&str> = models
            .models
            .iter()
            .map(gc_assets::model::ModelRecord::name)
            .collect();
        assert_eq!(choose("all", &names).unwrap().len(), names.len());
        for (k, n) in names.iter().enumerate() {
            assert_eq!(choose(&n.to_uppercase(), &names), Ok(vec![k]));
        }
    }

    #[test]
    fn slots_do_not_overlap() {
        let l = layout(&HEIGHTS, Vec2::new(100.0, 80.0));
        assert_eq!(l.positions.len(), HEIGHTS.len());
        for (a, b) in l.positions.iter().zip(l.positions.iter().skip(1)) {
            assert_eq!(a.y, b.y);
        }
        for i in 0..HEIGHTS.len() - 1 {
            let apart = l.positions[i + 1].x - l.positions[i].x;
            let wanted = (slot(HEIGHTS[i]) + slot(HEIGHTS[i + 1])) * 0.5;
            assert!((apart - wanted).abs() < 1e-4, "{i}: {apart} vs {wanted}");
        }
        // Small models still get the minimum, tall ones a bit more than
        // their height.
        assert_eq!(slot(0.8), SLOT_MIN);
        assert!((slot(2.4) - 2.64).abs() < 1e-5);
        // Centred on the middle asked for.
        let (first, last) = (l.positions[0].x, l.positions[4].x);
        let ends = (first - slot(HEIGHTS[0]) * 0.5 + last + slot(HEIGHTS[4]) * 0.5) * 0.5;
        assert!((ends - 100.0).abs() < 1e-3);
        assert_eq!(l.focus, Vec2::new(100.0, 80.0));
    }

    #[test]
    fn nine_models_wrap_to_a_second_row() {
        let heights = [2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.2];
        let l = layout(&heights, Vec2::new(50.0, 50.0));
        let front = l.positions[0].y;
        assert!(l.positions[..8].iter().all(|p| p.y == front));
        // The ninth stands behind (further from the camera, which is south:
        // smaller z) by 1.5 tallest in the front row, centred in its own row.
        let back = l.positions[8];
        assert!((front - back.y - ROW_GAP * 2.0).abs() < 1e-4);
        let row_two = layout(&[1.2], Vec2::new(50.0, 50.0));
        assert!((back.x - row_two.positions[0].x).abs() < 1e-4);
        // Sixteen make two full rows; seventeen make a third.
        let many = vec![1.0; 17];
        let l = layout(&many, Vec2::ZERO);
        let mut zs: Vec<f32> = l.positions.iter().map(|p| p.y).collect();
        zs.dedup();
        assert_eq!(zs.len(), 3);
        // The arrangement is centred front to back on the focus.
        let mid = (l.positions[0].y + l.positions[16].y) * 0.5;
        assert!(mid.abs() < 1e-4, "{mid}");
    }

    #[test]
    fn everything_fits_the_view_with_a_margin() {
        let cases: [(&[f32], f32); 5] = [
            (&HEIGHTS, 16.0 / 9.0),
            (&HEIGHTS, 1.0),
            (&HEIGHTS, 0.5),
            (&[1.0; 17], 16.0 / 9.0),
            (&[2.0; 17], 21.0 / 9.0),
        ];
        for (heights, aspect) in cases {
            let l = layout(heights, Vec2::new(128.0, 128.0));
            let mut cam = RtsCamera::new(256.0, l.focus);
            cam.aspect = aspect;
            cam.focus_height = 10.0 + l.lift;
            cam.distance = l.distance(aspect);
            assert!(cam.distance < crate::COMMANDER_IN);
            let m = cam.view_proj();
            let mut widest = 0.0_f32;
            for (p, &h) in l.positions.iter().zip(heights) {
                for (up, half) in [(0.0, slot(h) * 0.5), (h, slot(h) * 0.5)] {
                    for dx in [-half, half] {
                        let v = m * glam::Vec4::new(p.x + dx, 10.0 + up, p.y, 1.0);
                        let (x, y) = (v.x / v.w, v.y / v.w);
                        assert!(x.abs() <= 1.0 / 1.1 + 1e-3, "x {x} aspect {aspect}");
                        assert!(y.abs() <= 1.0 / 1.1 + 1e-3, "y {y} aspect {aspect}");
                        widest = widest.max(x.abs()).max(y.abs());
                    }
                }
            }
            // And the view is not wasted: something comes close to the margin.
            assert!(
                widest > 0.7,
                "{widest} for {} models at {aspect}",
                heights.len()
            );
        }
    }

    #[test]
    fn a_lone_model_is_seen_close_and_a_crowd_stays_out_of_commander_view() {
        let one = layout(&[2.0], CENTRE).distance(16.0 / 9.0);
        assert!((NEAREST..MIN_DISTANCE).contains(&one));
        let crowd = layout(&[3.0; 200], CENTRE).distance(1.0);
        assert!(crowd <= FURTHEST && FURTHEST < crate::COMMANDER_IN);
    }

    #[test]
    fn the_cycle_runs_idle_walk_attack_fall_and_round() {
        let cycle = Show::Cycle;
        assert_eq!(cycle.at(0.0), (Clip::Idle, 0.0));
        assert_eq!(cycle.at(3.5), (Clip::Idle, 3.5));
        assert_eq!(cycle.at(4.0), (Clip::Walk, 0.0));
        assert_eq!(cycle.at(8.5), (Clip::Attack, 0.5));
        assert_eq!(cycle.at(12.0), (Clip::Die, 0.0));
        assert_eq!(cycle.at(15.75), (Clip::Die, 3.75));
        assert_eq!(cycle.at(16.0), (Clip::Idle, 0.0));
        assert_eq!(cycle.at(36.5), (Clip::Walk, 0.5));
        assert_eq!(cycle.at(-1.0), (Clip::Idle, 0.0));
    }

    #[test]
    fn a_held_clip_keeps_going_and_a_fall_holds_its_last_frame() {
        assert_eq!(Show::Hold(Clip::Attack).at(21.0), (Clip::Attack, 21.0));
        assert_eq!(Clip::parse(" Shoot "), Some(Clip::Shoot));
        assert_eq!(Clip::parse("run"), None);

        let models = crate::models::load().unwrap();
        let clips = Clips::new(&models);
        let m = models.by_name("Bone Pikes").unwrap();
        let die = Show::Hold(Clip::Die);
        let start = pose(&clips, m, die, 0.0).unwrap().0;
        let end = pose(&clips, m, die, 50.0).unwrap().0;
        assert_ne!(start.rows, end.rows);
        assert_eq!(end, pose(&clips, m, die, 500.0).unwrap().0);
        // The cycle's fall starts where the clip does and ends held, too.
        assert_eq!(pose(&clips, m, Show::Cycle, 12.0).unwrap().0, start);
        assert_eq!(pose(&clips, m, Show::Cycle, 15.9).unwrap().0.rows, end.rows);
        // Every model has a pose in every clip.
        for m in 0..models.models.len() {
            for clip in Clip::CYCLE.into_iter().chain([Clip::Shoot, Clip::Cheer]) {
                assert!(pose(&clips, m, Show::Hold(clip), 1.3).is_some());
            }
        }
    }
}
