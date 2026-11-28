//! Taking command of a hero in first person (docs/46, PR A "Eyes").
//!
//! This stage is the view: the camera at the hero's own eye, the mouse
//! grabbed for looking, a crosshair and the hero's panel, and a key map where
//! nothing sends an order by accident. The hero is still moved by ordinary
//! orders (Tab frees the mouse and brings the order bar back); walking,
//! swinging and guarding are the sim stage that follows.

use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::CursorGrabMode;

use gc_platform::logln;
use gc_sim::RegimentHandle;
use gc_ui::Rect;
use gc_ui::ui::theme;

use crate::camera::FollowCamera;
use crate::{Ride, Running};

/// Radians the view turns for one raw mouse count at the standard speed.
const LOOK_PER_COUNT: f32 = 0.0022;
/// How high the eye sits, as a share of the model's height.
const EYE_OF_HEIGHT: f32 = 0.9;
/// The eye's height when the hero has no model to measure.
const EYE_FALLBACK: f32 = 1.4;

impl Running {
    /// Whether the view is at a hero's eye.
    pub fn in_person(&self) -> bool {
        self.ride
            .as_ref()
            .is_some_and(|ride| ride.camera.is_in_person())
    }

    /// The hero whose eye the view is at.
    pub(crate) fn embodied(&self) -> Option<RegimentHandle> {
        let ride = self.ride.as_ref()?;
        ride.camera.is_in_person().then_some(ride.regiment)
    }

    /// F6: step into the selected hero, or step back out.
    pub fn toggle_take_command(&mut self) {
        if self.in_person() {
            self.leave_ride();
            return;
        }
        if !self.menu.settings().take_command {
            return self.say("Turn on Take command (preview) in Settings first.".into());
        }
        self.director.stop();
        if !self.match_started || self.menu_open || self.showroom.is_some() || self.viewer.is_some()
        {
            return;
        }
        // `GC_TAKE_ANY_HERO` (a dev knob, like `GC_WATCH_FROM`): with nothing
        // selected F6 takes the first hero of either side, so a replay can be
        // watched from a hero's eye (a replay selects nobody of its own).
        let any = std::env::var_os("GC_TAKE_ANY_HERO")
            .and_then(|_| self.world.heroes().find_map(|hero| hero.regiment));
        let Some(h) = self.hero_selected().or(any) else {
            return self.say("Select your hero first, then press F6.".into());
        };
        let Some(r) = self.world.regiments.get(h) else {
            return;
        };
        let target = self.ride_target(&r);
        let height = (self.units.models.get(r.unit as usize).copied().flatten())
            .zip(self.art.as_ref())
            .map_or(EYE_FALLBACK, |(model, art)| {
                art.heights[model] * EYE_OF_HEIGHT
            });
        let fov = f32::from(self.menu.settings().eye_fov);
        let camera = FollowCamera::in_person(target, height, fov, |p| self.ground_at(p));
        self.ride = Some(Ride {
            regiment: h,
            camera,
        });
        self.person_free = false;
    }

    /// Grab the mouse for looking while in first person with nothing else
    /// asking for it, and let it go otherwise (the menu, the chat, the
    /// guide, another window, Tab's order bar). Run every frame.
    pub fn sync_look_grab(&mut self) {
        let want = self.in_person()
            && !self.person_free
            && self.input.focused
            && !self.menu_open
            && self.guide.is_none()
            && !self.chat_is_open()
            && self.match_started;
        if want == self.look_grabbed {
            return;
        }
        self.look_grabbed = want;
        if want {
            // Locked where the platform has it, else confined to the
            // window; the raw motion events are read either way.
            let grabbed = self
                .window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined));
            if let Err(e) = grabbed {
                logln!("grubcraft: the mouse could not be grabbed: {e}");
            }
            self.window.set_cursor_visible(false);
            self.input.cursor = None;
            self.hud.reset();
        } else {
            let _ = self.window.set_cursor_grab(CursorGrabMode::None);
            self.window.set_cursor_visible(true);
        }
    }

    /// Raw mouse motion, in counts: turn the view while the mouse is grabbed.
    pub fn person_look(&mut self, dx: f32, dy: f32) {
        if !self.look_grabbed {
            return;
        }
        let settings = self.menu.settings();
        let per_count = LOOK_PER_COUNT * settings.look_scale();
        let up = if settings.look_invert_y { dy } else { -dy };
        if let Some(ride) = &mut self.ride {
            ride.camera.look(dx * per_count, up * per_count);
        }
    }

    /// A key while in first person. True when it was handled (or swallowed),
    /// so it goes no further. While the mouse is free (Tab) every key but
    /// F6 and Tab is the ordinary game's, so the hero can be given orders.
    pub fn person_key(&mut self, event: &KeyEvent) -> bool {
        let PhysicalKey::Code(code) = event.physical_key else {
            return false;
        };
        let fresh = event.state == ElementState::Pressed && !event.repeat;
        match code {
            KeyCode::F6 => {
                if fresh {
                    self.toggle_take_command();
                }
                return true;
            }
            KeyCode::Tab => {
                if fresh {
                    self.person_free = !self.person_free;
                    self.hud.reset();
                }
                return true;
            }
            _ => {}
        }
        // A key let go is always the ordinary game's, so nothing sticks down.
        if !self.look_grabbed || event.state == ElementState::Released {
            return false;
        }
        match code {
            // The menu, the pause and chat, the function keys (F1 help and
            // the faction powers among them) and the modifiers are as ever.
            KeyCode::Escape
            | KeyCode::KeyP
            | KeyCode::Pause
            | KeyCode::Enter
            | KeyCode::NumpadEnter
            | KeyCode::F1
            | KeyCode::F2
            | KeyCode::F3
            | KeyCode::F4
            | KeyCode::F5
            | KeyCode::F7
            | KeyCode::F8
            | KeyCode::F9
            | KeyCode::F10
            | KeyCode::F11
            | KeyCode::F12 => false,
            KeyCode::ShiftLeft
            | KeyCode::ShiftRight
            | KeyCode::ControlLeft
            | KeyCode::ControlRight
            | KeyCode::AltLeft
            | KeyCode::AltRight
            | KeyCode::SuperLeft
            | KeyCode::SuperRight => false,
            // The hero's two abilities, aimed with the crosshair.
            // (with Ctrl they stay the group-save keys)
            KeyCode::Digit1 | KeyCode::Digit2 if self.input.ctrl_cmd => false,
            KeyCode::Digit1 | KeyCode::Digit2 => {
                if fresh && let Some(h) = self.embodied() {
                    self.hero_press(h, u8::from(code == KeyCode::Digit2));
                }
                true
            }
            // Everything else is swallowed: no order may leave by accident.
            _ => {
                if fresh {
                    self.say("Orders: Tab frees the mouse. F6 steps out of first person.".into());
                }
                true
            }
        }
    }

    /// The first-person screen: a crosshair, the hero's panel and a line of
    /// what the keys do.
    pub fn run_person_hud(&mut self) {
        let (screen, scale) = self.ui_screen();
        self.hud.draw.clear();
        let (cx, cy) = (screen.0 / 2, screen.1 / 2);
        let cross = |draw: &mut gc_ui::DrawList, r: Rect| {
            draw.rect(Rect::new(r.x - 1, r.y - 1, r.w + 2, r.h + 2), theme::INK);
            draw.rect(r, theme::LINEN);
        };
        for r in [
            Rect::new(cx - 9, cy - 1, 6, 2),
            Rect::new(cx + 4, cy - 1, 6, 2),
            Rect::new(cx - 1, cy - 9, 2, 6),
            Rect::new(cx - 1, cy + 4, 2, 6),
        ] {
            cross(&mut self.hud.draw, r);
        }
        if let Some(h) = self.embodied()
            && let Some(panel) = crate::hud::hero_panel(&self.world, h, &self.units.names, |p| {
                crate::color_of(&self.parties, p)
            })
        {
            let mut over = Vec::new();
            gc_ui::hero::draw(&mut self.hud.draw, screen.1, &panel, &mut over);
        }
        let text = "1 and 2: abilities (click to aim)    Tab: orders    F6: leave";
        let w = gc_ui::draw::text_width(text, 1);
        let h = gc_ui::draw::text_height(1);
        let pad = 8;
        let panel = Rect::new((screen.0 - w) / 2 - pad, 12, w + 2 * pad, h + pad);
        self.hud.draw.shadow(panel, 3);
        self.hud.draw.boxed(panel, theme::LINEN, 2);
        self.hud
            .draw
            .text(panel.x + pad, panel.y + pad / 2, 1, theme::INK, text);
        crate::hud::upload_ui(
            &mut self.renderer,
            &mut self.ui_quads,
            &self.hud.draw,
            scale,
        );
    }
}
