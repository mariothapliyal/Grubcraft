//! A Field Battle's deployment phase (docs/06-game-modes.md): before the
//! battle, each player places their regiments inside their own zone, without
//! seeing the enemy, until everyone is ready or time runs out.
//!
//! While it lasts, time stands still: nothing marches, fights or loses
//! heart, and only placing, formations and readiness are accepted.

use gc_fixed::{Fx, FxVec2};

use crate::{PlayerId, StateHasher, TICKS_PER_SECOND};

/// Longest a deployment phase lasts.
pub const DEPLOY_SECONDS: u32 = 90;
/// Players that can have a deployment zone.
pub const MAX_ZONES: usize = 8;

/// Where one player may place their regiments: a circle on the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    pub center: FxVec2,
    pub radius: Fx,
}

impl Zone {
    pub fn contains(&self, p: FxVec2) -> bool {
        let r = self.radius.raw().max(0) as u64;
        (p - self.center).length_sq_raw() <= r * r
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct Deployment {
    zones: [Option<Zone>; MAX_ZONES],
    ready: [bool; MAX_ZONES],
    /// The tick the battle starts on; 0 when there is no deployment.
    end: u32,
}

impl Deployment {
    pub fn active(&self, tick: u32) -> bool {
        tick < self.end
    }

    /// Give `player` a zone. Only on the first tick, before anything moves.
    pub fn set_zone(&mut self, player: PlayerId, zone: Zone, tick: u32) -> bool {
        let Some(slot) = self.zones.get_mut(player as usize) else {
            return false;
        };
        if tick != 0 || zone.radius <= Fx::ZERO {
            return false;
        }
        *slot = Some(zone);
        self.end = DEPLOY_SECONDS * TICKS_PER_SECOND;
        true
    }

    pub fn zone(&self, player: PlayerId) -> Option<Zone> {
        self.zones.get(player as usize).copied().flatten()
    }

    pub fn is_ready(&self, player: PlayerId) -> bool {
        self.ready.get(player as usize).copied().unwrap_or(false)
    }

    /// `player` is done placing. Once every player with a zone is, the
    /// battle starts on the next tick.
    pub fn set_ready(&mut self, player: PlayerId, tick: u32) -> bool {
        if !self.active(tick) || self.zone(player).is_none() || self.is_ready(player) {
            return false;
        }
        self.ready[player as usize] = true;
        let all = (0..MAX_ZONES).all(|p| self.zones[p].is_none() || self.ready[p]);
        if all {
            self.end = tick + 1;
        }
        true
    }

    /// Ticks left until the battle starts.
    pub fn ticks_left(&self, tick: u32) -> u32 {
        self.end.saturating_sub(tick)
    }

    /// Nothing at all when there was never a deployment, so worlds without
    /// one keep their hashes.
    pub fn hash_into(&self, h: &mut StateHasher) {
        if self.end == 0 {
            return;
        }
        h.u32(self.end);
        for (zone, &ready) in self.zones.iter().zip(&self.ready) {
            match zone {
                Some(z) => {
                    h.u8(1 + u8::from(ready));
                    h.i32(z.center.x.raw());
                    h.i32(z.center.y.raw());
                    h.i32(z.radius.raw());
                }
                None => h.u8(0),
            }
        }
    }
}
