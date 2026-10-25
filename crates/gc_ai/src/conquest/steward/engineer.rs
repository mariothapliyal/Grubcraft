//! The Engineer: the works a Conquest mind puts up against its enemies
//! (docs/18-ai.md, "Engineer"; docs/26-fortifications.md, stage 8). It
//! is part of the Steward, spending the economy's attention, a spare
//! worker, and only the money left over after what the Steward is saving
//! for, every couple of seconds:
//!
//! - a **front wall** across the way the nearest enemy comes, with a gate
//!   in the middle, drawn a few tiles at a time out from the gate, and the
//!   stretches by the gate raised a level once it is rich;
//! - a **barricade** in front of its rally point, mid-game;
//! - **stakes** planted by spearmen and archers waiting at the rally point
//!   once it has seen enemy riders or monsters;
//! - a few hidden **traps** on the approach once Grub piles up;
//! - a **bridge** over the narrowest water on the way to its enemies when
//!   water cuts it off from all of them.
//!
//! Its level says which of these it does ([`Fortify`]); its personality's
//! caution how soon and how much. Manning the walls and assaulting the
//! enemy's are the Marshal's.

use gc_sim::structures::{GATE, GROWN, LINE, Role, StructureHandle};
use gc_sim::{BuildingId, Command, Mass, TICKS_PER_SECOND};
use glam::Vec2;

use super::{Look, Sense, Steward};
use crate::conquest::kit::{EconomyStyle, KitBuilding};
use crate::conquest::placing;
use crate::conquest::strategy::Fortify;
use crate::orders::Hands as Clicks;

/// Ticks between its looks.
const THINK_TICKS: u32 = 2 * TICKS_PER_SECOND;
/// When it starts on the front wall without tier 2, ticks: 10 minutes for
/// the boldest, 6 for the most cautious.
const WALL_AFTER_BOLD: u32 = 10 * 60 * TICKS_PER_SECOND;
const WALL_AFTER_CAUTIOUS: u32 = 6 * 60 * TICKS_PER_SECOND;
/// How far out from home the front wall goes, tiles: as far as fits, from
/// the first to the last.
const WALL_OUT: [i32; 5] = [16, 14, 12, 10, 8];
/// Tiles of wall each side of the gate, and a piece drawn at a time.
const WALL_HALF: i32 = 8;
const PIECE: i32 = 3;
/// The share of a line's tiles (percent) that must fit for it to be the
/// front wall's line.
const LINE_FITS: usize = 60;
/// Grub it keeps in hand beyond what it saves for, before it spends on
/// works.
const RESERVE: u32 = 150;
/// Grub in hand beyond its saving from which it raises the wall.
const RICH: u32 = 500;
/// Stretches it raises: those either side of the gate.
const RAISES: u8 = 2;
/// How long to wait before looking again for a front wall line where none
/// fitted (a Choir's power fields grow), ticks.
const LINE_RETRY: u32 = 60 * TICKS_PER_SECOND;
/// The barricade: tiles across, and how far in front of the rally point.
const BARRICADE_HALF: i32 = 2;
const BARRICADE_OUT: f32 = 4.0;
/// Stakes: planted by regiments this near the rally point, at most this
/// many lines.
const STAKES_NEAR: f32 = 8.0;
const STAKE_LINES: usize = 2;
/// Traps: laid from this many tiles out, once it has this much Grub to
/// spare; two, or three for a cautious mind.
const TRAPS_OUT: f32 = 22.0;
const TRAP_GRUB: u32 = 300;
const TRAPS: u8 = 2;
/// Longest bridge it draws, tiles.
const BRIDGE_LONGEST: i32 = 16;

/// The front wall it has planned.
#[derive(Clone, Debug)]
struct Front {
    /// The pieces still to draw, nearest the gate first, each from tile
    /// to tile.
    pieces: Vec<((i32, i32), (i32, i32))>,
    /// Where the gate goes and its turn, until laid.
    gate: Option<((i32, i32), u8)>,
    /// The wall tiles either side of the gate: the stretches it raises.
    by_gate: [(i32, i32); 2],
}

#[derive(Clone, Debug, Default)]
pub(super) struct Engineer {
    next_think: u32,
    front: Option<Front>,
    /// No line fitted: look again from then.
    next_line: u32,
    raised: u8,
    /// The rally point it has barricaded, or tried to.
    barricaded: Option<Vec2>,
    /// Regiments sent to plant stakes.
    staked: Vec<gc_sim::RegimentHandle>,
    traps: u8,
    /// Whether it has looked for (and drawn, if it could) a bridge.
    bridged: bool,
}

impl Steward {
    /// The Engineer's look: put up whatever works are due, with what
    /// Grub is spare, sending what it has trained to `rally`.
    pub(super) fn fortify(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        rally: Option<Vec2>,
    ) {
        if s.plan.fortifies == Fortify::None || s.now < self.engineer.next_think {
            return;
        }
        self.engineer.next_think = s.now + THINK_TICKS;
        let (Some(home), Some(bank)) = (self.home, s.world.bank(s.party)) else {
            return;
        };
        if self.arming || !clicks.free() {
            return;
        }
        let spare = bank.grub.saturating_sub(self.ledger.saving.0 + RESERVE);
        let Some(toward) = enemy_way(s, home) else {
            return;
        };
        let full = s.plan.fortifies == Fortify::Full;
        let late = s.now >= wall_after(s.caution);
        // Most pressing first: a way to the enemy, then field works, then
        // the wall, then traps. One order a look.
        if full && !self.engineer.bridged && self.bridge(s, clicks, look, home, spare) {
            return;
        }
        if let Some(rally) = rally {
            if (look.tier >= 2 || late) && self.barricade(s, clicks, look, rally, toward, spare)
            {
                return;
            }
            if self.stakes(s, clicks, look, rally) {
                return;
            }
        }
        if full && (look.tier >= 2 || late) {
            if self.engineer.front.is_none() && s.now >= self.engineer.next_line {
                self.engineer.front = plan_front(s, look, self, home, toward);
                if self.engineer.front.is_none() {
                    self.engineer.next_line = s.now + LINE_RETRY;
                }
            }
            if self.wall(s, clicks, look, spare) {
                return;
            }
        }
        if full && self.traps(s, clicks, look, home, toward, spare) {
            return;
        }
    }

    /// Draw the next piece of the front wall, or lay its gate, or raise a
    /// stretch by the gate. Whether it gave an order.
    fn wall(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look, spare: u32) -> bool {
        let Some(wall) = wall_type(s) else {
            return false;
        };
        let Some(front) = self.engineer.front.as_mut() else {
            return false;
        };
        // The gate first: the wall is drawn up to it.
        if let Some((tile, turn)) = front.gate {
            let Some(gate) = gate_type(s) else {
                front.gate = None;
                return false;
            };
            if spare < u32::from(gate.stats.cost_grub) {
                return false;
            }
            if s.world.can_lay(s.party, gate.id, tile, turn).is_err() {
                front.gate = None;
                return false;
            }
            let at = Vec2::new(tile.0 as f32 + 0.5, tile.1 as f32 + 0.5);
            let Some(w) = self.builder(s, look, at) else {
                return false;
            };
            let order = Command::Build {
                regiment: w,
                building: gate.id,
                tile: (tile.0 as i16, tile.1 as i16),
                turn,
            };
            if clicks.give(s.now, order) {
                self.sent.push((w, s.now));
                if let Some(f) = self.engineer.front.as_mut() {
                    f.gate = None;
                }
                return true;
            }
            return false;
        }
        while let Some(&(from, to)) = front.pieces.first() {
            let Ok(tiles) = s.world.wall_fits(s.party, wall.id, from, to) else {
                front.pieces.remove(0);
                continue;
            };
            // Walled already, or not all of it fits: on to the next.
            if tiles.is_empty() || tiles.iter().any(|t| t.fits.is_err()) {
                front.pieces.remove(0);
                continue;
            }
            if spare < u32::from(wall.stats.cost_grub) * tiles.len() as u32 {
                return false;
            }
            let worker = if wall.stats.flags & GROWN != 0 {
                None
            } else {
                let at = Vec2::new(from.0 as f32, from.1 as f32);
                match self.builder(s, look, at) {
                    Some(w) => Some(w),
                    None => return false,
                }
            };
            let order = Command::BuildWall {
                regiment: worker,
                building: wall.id,
                from: (from.0 as i16, from.1 as i16),
                to: (to.0 as i16, to.1 as i16),
            };
            if !clicks.give(s.now, order) {
                return false;
            }
            if let Some(w) = worker {
                self.sent.push((w, s.now));
            }
            if let Some(f) = self.engineer.front.as_mut() {
                f.pieces.remove(0);
            }
            return true;
        }
        // All drawn: raise the stretches by the gate once rich.
        if self.engineer.raised >= RAISES || spare < RICH {
            return false;
        }
        let k = usize::from(self.engineer.raised);
        let tile = front.by_gate[k];
        self.engineer.raised += 1;
        let Some(h) = own_wall_at(s, tile) else {
            return false;
        };
        let Ok(raise) = s.world.wall_level(s.party, h) else {
            return false;
        };
        if !raise.afford || spare < raise.grub {
            return false;
        }
        let grown = s
            .kit
            .buildings
            .iter()
            .any(|b| b.id == raise.to && b.stats.flags & GROWN != 0);
        let worker = if grown {
            None
        } else {
            self.builder(s, look, Vec2::new(tile.0 as f32, tile.1 as f32))
        };
        if !grown && worker.is_none() {
            self.engineer.raised -= 1;
            return false;
        }
        let order = Command::RaiseWall {
            structure: h,
            regiment: worker,
        };
        if clicks.give(s.now, order) {
            if let Some(w) = worker {
                self.sent.push((w, s.now));
            }
            return true;
        }
        self.engineer.raised -= 1;
        false
    }

    /// A barricade across the front of the rally point, once. Whether it
    /// gave an order.
    fn barricade(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        rally: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> bool {
        if self
            .engineer
            .barricaded
            .is_some_and(|b| b.distance(rally) < 4.0)
        {
            return false;
        }
        let Some(b) = works_type(s, Role::Barricade) else {
            return false;
        };
        let n = 2 * BARRICADE_HALF + 1;
        if spare < u32::from(b.stats.cost_grub) * n as u32 {
            return false;
        }
        self.engineer.barricaded = Some(rally);
        // Out in front, then a little further, then nearer.
        for out in [BARRICADE_OUT, BARRICADE_OUT + 2.0, BARRICADE_OUT - 2.0] {
            let (from, to) = across(rally + toward * out, toward, BARRICADE_HALF);
            let fits = s
                .world
                .wall_fits(s.party, b.id, from, to)
                .is_ok_and(|t| t.len() as i32 == n && t.iter().all(|t| t.fits.is_ok()));
            if !fits {
                continue;
            }
            let Some(w) = self.builder(s, look, rally) else {
                return false;
            };
            let order = Command::BuildWall {
                regiment: Some(w),
                building: b.id,
                from: (from.0 as i16, from.1 as i16),
                to: (to.0 as i16, to.1 as i16),
            };
            if clicks.give(s.now, order) {
                self.sent.push((w, s.now));
                return true;
            }
            return false;
        }
        false
    }

    /// Against enemy riders or monsters it has seen, a spear or archer
    /// regiment waiting at the rally point plants a line of stakes, up to
    /// [`STAKE_LINES`]. Whether it gave an order.
    fn stakes(&mut self, s: &Sense, clicks: &mut Clicks, look: &Look, rally: Vec2) -> bool {
        if self.engineer.staked.len() >= STAKE_LINES || !heavy_foes(s) {
            return false;
        }
        let Some(b) = works_type(s, Role::Stakes) else {
            return false;
        };
        let Some(r) = look.army.iter().find(|r| {
            !r.moving
                && !r.engaged
                && r.pos.distance(rally) <= STAKES_NEAR
                && !self.engineer.staked.contains(&r.handle)
                && s.world.plant_check(r.handle, b.id).is_ok()
        }) else {
            return false;
        };
        let order = Command::PlantStakes {
            regiment: r.handle,
            building: b.id,
        };
        if clicks.give(s.now, order) {
            self.engineer.staked.push(r.handle);
            return true;
        }
        false
    }

    /// A hidden trap on the approach, up to [`TRAPS`] (one more for a
    /// cautious mind), once Grub piles up. The Brood's nests take a Gnawer
    /// each, so it lays none. Whether it gave an order.
    fn traps(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        home: Vec2,
        toward: Vec2,
        spare: u32,
    ) -> bool {
        let most = TRAPS + u8::from(s.caution > 0.5);
        if self.engineer.traps >= most || spare < TRAP_GRUB || s.kit.economy == EconomyStyle::Veins
        {
            return false;
        }
        let Some(b) = works_type(s, Role::Trap) else {
            return false;
        };
        let k = f32::from(self.engineer.traps);
        let side = Vec2::new(-toward.y, toward.x);
        // Spread across the way in and a little further out each time.
        let at = home + toward * (TRAPS_OUT + 3.0 * k) + side * (k - 1.0) * 3.0;
        self.engineer.traps += 1;
        let tile = (at.x.floor() as i32, at.y.floor() as i32);
        if s.world.can_lay(s.party, b.id, tile, 0).is_err() {
            return false;
        }
        let Some(w) = self.builder(s, look, at) else {
            self.engineer.traps -= 1;
            return false;
        };
        let order = Command::Build {
            regiment: w,
            building: b.id,
            tile: (tile.0 as i16, tile.1 as i16),
            turn: 0,
        };
        if clicks.give(s.now, order) {
            self.sent.push((w, s.now));
            return true;
        }
        self.engineer.traps -= 1;
        false
    }

    /// When water cuts it off from every enemy start, bridge the narrowest
    /// crossing near the straight way to the nearest. Looks once. Whether
    /// it gave an order.
    fn bridge(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
        home: Vec2,
        spare: u32,
    ) -> bool {
        let Some(b) = works_type(s, Role::Bridge) else {
            self.engineer.bridged = true;
            return false;
        };
        let enemies: Vec<Vec2> = enemy_starts(s, home).collect();
        if enemies.is_empty() || enemies.iter().any(|&e| s.terrain.joined(home, e)) {
            self.engineer.bridged = true;
            return false;
        }
        let Some(&target) = enemies
            .iter()
            .min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
        else {
            return false;
        };
        let Some((from, to)) = narrowest_crossing(s, b.id, home, target) else {
            self.engineer.bridged = true;
            return false;
        };
        let len = (to.0 - from.0).abs().max((to.1 - from.1).abs()) as u32 + 1;
        if spare < u32::from(b.stats.cost_grub) * len {
            return false;
        }
        let Some(w) = self.builder(s, look, Vec2::new(from.0 as f32, from.1 as f32)) else {
            return false;
        };
        let order = Command::BuildWall {
            regiment: Some(w),
            building: b.id,
            from: (from.0 as i16, from.1 as i16),
            to: (to.0 as i16, to.1 as i16),
        };
        if clicks.give(s.now, order) {
            self.sent.push((w, s.now));
            self.engineer.bridged = true;
            return true;
        }
        false
    }
}

/// When a mind with `caution` starts on its wall without tier 2.
fn wall_after(caution: f32) -> u32 {
    let c = caution.clamp(0.0, 1.0);
    let span = (WALL_AFTER_BOLD - WALL_AFTER_CAUTIOUS) as f32;
    WALL_AFTER_BOLD - (span * c) as u32
}

/// The other starts on the map, those far enough from `home` to be
/// someone else's.
fn enemy_starts<'a>(s: &'a Sense, home: Vec2) -> impl Iterator<Item = Vec2> + 'a {
    s.kit
        .starts
        .iter()
        .copied()
        .filter(move |p| p.distance(home) > 30.0)
}

/// The way the nearest other start lies from `home`, as a unit vector.
fn enemy_way(s: &Sense, home: Vec2) -> Option<Vec2> {
    let toward = enemy_starts(s, home).min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))?;
    (toward - home).try_normalize()
}

/// The tiles from and to of a straight line `half` tiles each side of
/// `mid`, across `toward` (on whichever axis is nearer across it).
fn across(mid: Vec2, toward: Vec2, half: i32) -> ((i32, i32), (i32, i32)) {
    let (x, y) = (mid.x.floor() as i32, mid.y.floor() as i32);
    if toward.x.abs() >= toward.y.abs() {
        ((x, y - half), (x, y + half))
    } else {
        ((x - half, y), (x + half, y))
    }
}

/// The front wall's line: across the way the enemy comes, as far out from
/// home as [`LINE_FITS`] of its tiles fit, off the lanes to its seeps and
/// veins, with a gate in the middle where the faction has one.
fn plan_front(
    s: &Sense,
    look: &Look,
    steward: &Steward,
    home: Vec2,
    toward: Vec2,
) -> Option<Front> {
    let wall = wall_type(s)?;
    let gate = gate_type(s);
    let lanes = steward.lanes(s, look);
    let upright = toward.x.abs() >= toward.y.abs();
    for out in WALL_OUT {
        // Out along the way to the enemy, on the axis nearer it.
        let mid = home + toward / toward.x.abs().max(toward.y.abs()) * out as f32;
        let (from, to) = across(mid, toward, WALL_HALF);
        let Ok(tiles) = s.world.wall_fits(s.party, wall.id, from, to) else {
            continue;
        };
        let clear = |t: (i32, i32)| {
            !lanes
                .iter()
                .any(|&(a, b)| placing::crosses([t.0, t.1, 1, 1], a, b))
        };
        let fit = tiles
            .iter()
            .filter(|t| t.fits.is_ok() && clear(t.tile))
            .count();
        if tiles.is_empty() || fit * 100 < tiles.len() * LINE_FITS {
            continue;
        }
        let (mx, my) = (mid.x.floor() as i32, mid.y.floor() as i32);
        // Along the line, `d` tiles from its middle.
        let at = |d: i32| if upright { (mx, my + d) } else { (mx + d, my) };
        let gap = if gate.is_some() { 2 } else { 0 };
        let mut pieces = Vec::new();
        for sign in [-1, 1] {
            let mut d = gap;
            while d <= WALL_HALF {
                let end = (d + PIECE - 1).min(WALL_HALF);
                let (a, b) = (at(sign * d), at(sign * end));
                if clear(a) && clear(b) {
                    pieces.push((a, b));
                }
                d = end + 1;
            }
        }
        // Nearest the gate first, side by side.
        pieces.sort_by_key(|&(a, _)| {
            let d = if upright { a.1 - my } else { a.0 - mx };
            d.abs()
        });
        // A gate (1 by 3) along the line: as it stands across an upright
        // line, turned a quarter across a level one.
        let gate_at = gate.map(|_| {
            if upright {
                ((mx, my - 1), 0)
            } else {
                ((mx - 1, my), 1)
            }
        });
        return Some(Front {
            pieces,
            gate: gate_at,
            by_gate: [at(-gap), at(gap)],
        });
    }
    None
}

/// The faction's first wall: drawn, a tile a piece, upgraded from nothing.
fn wall_type<'k>(s: &Sense<'k>) -> Option<&'k KitBuilding> {
    s.kit.buildings.iter().find(|b| {
        b.stats.role == Role::Wall
            && b.stats.flags & LINE != 0
            && b.stats.flags & GATE == 0
            && b.stats.upgrades_from.is_none()
    })
}

/// The faction's first gate, if it has one (the Choir's walls let its own
/// through).
fn gate_type<'k>(s: &Sense<'k>) -> Option<&'k KitBuilding> {
    s.kit.buildings.iter().find(|b| {
        b.stats.role == Role::Wall && b.stats.flags & GATE != 0 && b.stats.upgrades_from.is_none()
    })
}

/// The faction's works of `role`: bridge, stakes, barricade or trap.
fn works_type<'k>(s: &Sense<'k>, role: Role) -> Option<&'k KitBuilding> {
    s.kit.buildings.iter().find(|b| b.stats.role == role)
}

/// Its own wall tile at `tile`, if one stands there.
fn own_wall_at(s: &Sense, tile: (i32, i32)) -> Option<StructureHandle> {
    let st = s.world.structures()?;
    let table = st.buildings();
    st.iter()
        .filter(|v| v.owner == s.party && v.tile == tile && v.finished())
        .find(|v| {
            table
                .get(usize::from(v.building))
                .is_some_and(|b| b.role == Role::Wall && b.flags & GATE == 0)
        })
        .map(|v| v.handle)
}

/// Whether it has seen enemy riders or monsters (heavy regiments, which
/// stakes stop).
fn heavy_foes(s: &Sense) -> bool {
    crate::conquest::spymaster::enemy_army(s).any(|r| s.view.unit(r).stats.mass == Mass::Heavy)
}

/// The shortest bridge (from bank to bank) that fits over the water on
/// the way from `home` to `target`: crossings straight along that way and
/// along each axis, from points either side of it.
fn narrowest_crossing(
    s: &Sense,
    bridge: BuildingId,
    home: Vec2,
    target: Vec2,
) -> Option<((i32, i32), (i32, i32))> {
    let way = (target - home).try_normalize()?;
    let side = Vec2::new(-way.y, way.x);
    let len = home.distance(target);
    let mut best: Option<(i32, ((i32, i32), (i32, i32)))> = None;
    for k in -6..=6 {
        let start = home + side * (k as f32 * 3.0);
        // Where this walk first meets water it can't wade.
        let Some(wet) = (0..len as i32).map(|d| start + way * d as f32).find(|&p| {
            s.terrain.water(p) && s.terrain.blocked(p)
        }) else {
            continue;
        };
        let axes = [way, Vec2::new(way.x.signum(), 0.0), Vec2::new(0.0, way.y.signum())];
        for dir in axes.into_iter().filter(|d| d.length() > 0.5) {
            let dir = dir.normalize();
            let Some((from, to)) = banks(s, wet, dir) else {
                continue;
            };
            let n = (to.0 - from.0).abs().max((to.1 - from.1).abs());
            if n > BRIDGE_LONGEST || best.is_some_and(|(b, _)| b <= n) {
                continue;
            }
            let fits = s
                .world
                .wall_fits(s.party, bridge, from, to)
                .is_ok_and(|t| !t.is_empty() && t.iter().all(|t| t.fits.is_ok()));
            if fits {
                best = Some((n, (from, to)));
            }
        }
    }
    best.map(|(_, line)| line)
}

/// The last land tile before the water at `wet` going back along `dir`,
/// and the first land tile after it going on: a bridge's two ends.
fn banks(s: &Sense, wet: Vec2, dir: Vec2) -> Option<((i32, i32), (i32, i32))> {
    let tile = |p: Vec2| (p.x.floor() as i32, p.y.floor() as i32);
    let land = |p: Vec2| !s.terrain.water(p) && !s.terrain.blocked(p);
    let back = (1..=BRIDGE_LONGEST).map(|d| wet - dir * d as f32).find(|&p| land(p))?;
    let on = (1..=BRIDGE_LONGEST).map(|d| wet + dir * d as f32).find(|&p| land(p))?;
    Some((tile(back), tile(on)))
}
