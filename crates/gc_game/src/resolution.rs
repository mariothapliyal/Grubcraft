//! The 3D world's resolution, chosen as the game runs: full while the GPU
//! keeps up with the frame cap, lower only while it cannot, so a machine with
//! integrated graphics keeps a steady frame rate instead of stuttering, and a
//! machine with room to spare never sees a difference. The interface is
//! always drawn at full resolution; the world is scaled up and sharpened
//! under it (`Renderer::set_world_scale`).
//!
//! It reads the GPU's own time for each frame (`Renderer::gpu_times`), half
//! a second at a time. Where the GPU cannot time its passes the world stays
//! at full resolution.

use gc_ui::settings::GraphicsPreset;

/// Frames measured before each decision (about half a second at 60 fps).
pub const WINDOW_FRAMES: u32 = 30;
/// How many windows in a row must show room to spare before the
/// resolution goes back up, so it does not flicker between two steps.
const CALM_WINDOWS: u32 = 4;
/// The most the scale falls, and rises, in one window.
const MOST_DOWN: f32 = 0.15;
const MOST_UP: f32 = 0.05;

/// What a preset asks of the resolution: the lowest scale it goes to, and
/// how much of each frame's time the GPU should be busy for at most.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub floor: f32,
    pub busy: f32,
}

/// What `preset` asks for; `None` for a fixed full resolution (High).
pub fn aim(preset: GraphicsPreset) -> Option<Aim> {
    match preset {
        GraphicsPreset::High => None,
        // Lower only when the frames fall behind.
        GraphicsPreset::Balanced => Some(Aim {
            floor: 0.7,
            busy: 0.85,
        }),
        // Keep the GPU half idle, for the battery's sake and a cooler laptop.
        GraphicsPreset::Battery => Some(Aim {
            floor: 0.5,
            busy: 0.5,
        }),
    }
}

#[derive(Clone, Debug)]
pub struct AutoResolution {
    scale: f32,
    calm: u32,
}

impl Default for AutoResolution {
    fn default() -> Self {
        AutoResolution {
            scale: 1.0,
            calm: 0,
        }
    }
}

impl AutoResolution {
    #[cfg(test)]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// After a window of frames whose GPU work averaged `gpu_ms`, with
    /// `budget_ms` a frame to draw in (the frame cap's, or the screen's):
    /// the scale to draw the world at next. Pixel work goes with the square
    /// of the scale, but not all of a frame's work is pixels, so each step
    /// is kept small and the next window shows what it did.
    pub fn update(&mut self, gpu_ms: f32, budget_ms: f32, aim: Option<Aim>) -> f32 {
        let Some(aim) = aim.filter(|_| gpu_ms > 0.0 && budget_ms > 0.0) else {
            self.scale = 1.0;
            self.calm = 0;
            return self.scale;
        };
        let target = budget_ms * aim.busy;
        if gpu_ms > target {
            self.calm = 0;
            let fit = self.scale * (target / gpu_ms).sqrt();
            self.scale = fit.max(self.scale - MOST_DOWN);
        } else if gpu_ms < target * 0.7 && self.scale < 1.0 {
            // Room to spare: a step up once it has lasted, if that step
            // would still fit.
            self.calm += 1;
            let up = (self.scale + MOST_UP).min(1.0);
            let after = gpu_ms * (up / self.scale).powi(2);
            if self.calm >= CALM_WINDOWS && after < target {
                self.scale = up;
                self.calm = 0;
            }
        } else {
            self.calm = 0;
        }
        self.scale = self.scale.clamp(aim.floor, 1.0);
        self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BALANCED: Option<Aim> = Some(Aim {
        floor: 0.7,
        busy: 0.85,
    });

    #[test]
    fn a_gpu_that_keeps_up_stays_at_full_resolution() {
        let mut r = AutoResolution::default();
        for _ in 0..100 {
            assert_eq!(r.update(12.0, 16.7, BALANCED), 1.0);
        }
    }

    #[test]
    fn a_gpu_that_falls_behind_draws_the_world_smaller_but_not_below_the_floor() {
        let mut r = AutoResolution::default();
        let first = r.update(20.0, 16.7, BALANCED);
        assert!(first < 1.0 && first >= 0.85, "{first}");
        for _ in 0..20 {
            r.update(40.0, 16.7, BALANCED);
        }
        assert_eq!(r.scale(), 0.7);
    }

    #[test]
    fn it_climbs_back_slowly_once_there_is_room() {
        let mut r = AutoResolution::default();
        r.update(25.0, 16.7, BALANCED);
        let low = r.scale();
        // Three calm windows are not enough to move.
        for _ in 0..CALM_WINDOWS - 1 {
            assert_eq!(r.update(6.0, 16.7, BALANCED), low);
        }
        assert!(r.update(6.0, 16.7, BALANCED) > low);
        for _ in 0..200 {
            r.update(6.0, 16.7, BALANCED);
        }
        assert_eq!(r.scale(), 1.0);
    }

    #[test]
    fn high_and_untimed_gpus_draw_at_full_resolution() {
        let mut r = AutoResolution::default();
        r.update(40.0, 16.7, BALANCED);
        assert_eq!(r.update(40.0, 16.7, None), 1.0);
        r.update(40.0, 16.7, BALANCED);
        assert_eq!(r.update(0.0, 16.7, BALANCED), 1.0);
    }
}
