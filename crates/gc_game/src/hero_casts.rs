//! A real hero's two abilities on the command grid (docs/44): A casts the
//! basic one, S the ultimate. One that needs a place or a foe is aimed with
//! the next click, like a faction power.

use gc_sim::abilities::{Aim, CastTarget};
use gc_sim::{Command, RegimentHandle};

use crate::hud::Armed;
use crate::{Running, buildings, fx_vec, viewer};

impl Running {
    /// The one selected regiment, when it is a hero of your side with a kit
    /// of abilities.
    pub fn hero_selected(&self) -> Option<RegimentHandle> {
        let &[h] = self.selected.regiments() else {
            return None;
        };
        let hero = self.world.hero_of(h)?;
        (hero.kit.is_some()
            && self
                .world
                .stances()
                .can_command(self.sandbox.side, hero.owner))
        .then_some(h)
    }

    /// What each of hero `h`'s two abilities does, from its roster.
    pub fn ability_tips(&self, h: RegimentHandle) -> [&'static str; 2] {
        let none = "A hero ability.";
        let Some(kit) = self.world.hero_of(h).and_then(|hero| hero.kit) else {
            return [none; 2];
        };
        let name = kit.kit.hero_name();
        let entry = viewer::CATALOG.iter().find(|c| c.name == name);
        [0, 1].map(|i| entry.and_then(|c| c.abilities.get(i)).map_or(none, |a| a.2))
    }

    /// Hero `h`'s ability `slot` pressed: cast one that needs no aim, aim
    /// one that does, or say why it can't be cast. Pressing an aimed one
    /// again puts it down.
    pub fn hero_press(&mut self, h: RegimentHandle, slot: u8) {
        if self.hud.armed == Some(Armed::Cast(slot)) {
            self.hud.armed = None;
            return;
        }
        let Some(a) = self.world.hero_of(h).and_then(|hero| hero.ability(slot)) else {
            return;
        };
        if a.passive {
            self.say(format!("{} works by itself", a.name));
            return;
        }
        if a.aim == Aim::Hero {
            return self.send_cast(h, slot, CastTarget::Hero);
        }
        // A locked or cooling ability says so at once, before any aiming.
        if let Err(why) = self.world.can_cast(self.sandbox.side, h, slot) {
            return self.say(buildings::why(why).into());
        }
        self.hud.armed = Some(Armed::Cast(slot));
        let what = if a.aim == Aim::Point {
            "a place"
        } else {
            "an enemy regiment"
        };
        self.say(format!(
            "{}: click {what} - Esc or right-click to cancel",
            a.name
        ));
    }

    /// The click after aiming ability `slot`: cast it at what was clicked.
    /// A click on nothing it can aim at leaves it aimed.
    pub fn finish_cast(&mut self, slot: u8) {
        let Some(h) = self.embodied().or_else(|| self.hero_selected()) else {
            self.hud.armed = None;
            return;
        };
        let Some(a) = self.world.hero_of(h).and_then(|hero| hero.ability(slot)) else {
            self.hud.armed = None;
            return;
        };
        let target = match a.aim {
            Aim::Point => self
                .ground_under_cursor()
                .map(|p| CastTarget::Point(fx_vec(p))),
            Aim::Regiment => self.enemy_under_cursor().map(CastTarget::Regiment),
            // Cast at once when pressed, never aimed.
            Aim::Hero => None,
        };
        if let Some(target) = target {
            self.hud.armed = None;
            self.send_cast(h, slot, target);
        }
    }

    /// Put down an ability being aimed once its hero is no longer selected.
    pub fn drop_stale_cast(&mut self) {
        if matches!(self.hud.armed, Some(Armed::Cast(_)))
            && (self.embodied().or_else(|| self.hero_selected()).is_none()
                || self.watching.is_some())
        {
            self.hud.armed = None;
        }
    }

    fn send_cast(&mut self, h: RegimentHandle, slot: u8, target: CastTarget) {
        match self.world.cast_check(self.sandbox.side, h, slot, target) {
            Ok(()) => self.orders([Command::Cast {
                regiment: h,
                ability: slot,
                target,
            }]),
            Err(why) => self.say(buildings::why(why).into()),
        }
    }
}
