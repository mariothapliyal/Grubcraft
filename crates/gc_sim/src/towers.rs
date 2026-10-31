//! Towers shoot (docs/26-fortifications.md, stage 1).
//!
//! A finished building with an attack ([`crate::structures::TowerAttack`])
//! looses a volley of its `shots` each reload at an enemy regiment its side
//! can see within its range (further downhill, as for regiments), through
//! the same volleys as regiments' arrows ([`Combat::loose`]). Its shots are
//! aimed one by one, so every one lands: how close the target stands makes
//! no odds, though a shield wall's front and trees still catch them. A
//! Choir tower needs power to shoot, and a hasted one reloads faster.
//!
//! Each reload it picks, nearest first within each:
//! 1. the regiment its owner told it to shoot ([`crate::Command::FocusTower`]),
//!    while that one is in range;
//! 2. siege engines and artillery;
//! 3. regiments attacking its side's buildings;
//! 4. anyone else.
//!
//! A tower that shoots is known to its target's side from then on, as in
//! fog of war a building is once seen.
//!
//! **Bolt and Blast towers** (stage 5, [`TowerShot`]). A bolt tower looses
//! one heavy shot that runs down a single file of soldiers (it goes no
//! deeper than the target's ranks), hits big soldiers (monsters, riders,
//! engines: heavy ones, [`big_to_bolt`]) [`BOLT_BIG_PERCENT`]
//! percent as hard, and shoots siege first, then big soldiers. A blast
//! tower lobs its shots into the block, so the more packed it is the more
//! they hurt, splashes [`SPLASH_PERCENT`] of that on each enemy regiment
//! within [`BLAST_SPLASH`] of its target's block, shoots siege first, then the
//! fullest regiment, and can't shoot anyone within [`BLAST_MIN_RANGE`] of
//! its walls. Neither is stopped by a shield wall. Their quirks
//! ([`Quirk`]):
//! - **Burns** (Ember Horoku Tower): its fire-pots leave burning ground
//!   ([`Pool`]) for [`FIRE_POOL_TICKS`], and set any growth there alight.
//! - **Collects** (Concord Collector's Bolt): each soldier it kills pays
//!   its owner [`COLLECT_GRUB`] Grub.
//! - **Raises** (Concord Corpse Catapult): every [`RAISE_AT`] soldiers its
//!   hits kill rise beside it as a regiment of that many of its unit (Bone
//!   Pikes), its owner's for [`RAISED_TICKS`].
//! - **Pierces** (Choir Prism Lance): its bolt runs on through up to
//!   [`MAX_PIERCED`] more enemy regiments in line behind its target.
//! - **Disorders** (Choir Chord Obelisk): every regiment its blast falls
//!   on is thrown into disorder.
//! - **Drags** (Brood Harpoon Maw): the regiment its bolt hits is dragged
//!   [`DRAG_TILES`] toward it and halted, unless it is locked in a fight
//!   or on a wall.
//! - **Acid** (Brood Acid Bloater): its blast leaves a pool of acid for
//!   [`ACID_POOL_TICKS`].
//!
//! A pool hurts every enemy regiment of its tower's owner touching it
//! once a second ([`FIRE_POOL_HP`] or [`ACID_POOL_HP`], and morale as
//! fire does), wherever its tower has gone.
//!
//! The arrow towers' quirks:
//! - **Tollgate** ([`TOLL`]): every [`TOLL_HP`] hit points its volleys take
//!   earn its owner a Grub, less upkeep as for any Grub gathered.
//! - **Bell Tower** ([`SILENCES`]): when an enemy regiment comes within
//!   [`BELL_REACH`] of it, it tolls, at most once in [`BELL_EVERY`]; for
//!   [`BELL_SILENCE`] after, its enemies can't use a power on anything
//!   within [`BELL_REACH`] of it (see [`Structures::silenced`]).

use gc_fixed::{Fx, FxVec2};

use crate::combat::{Block, Combat, Shot, TowerHit, Whose, placement};
use crate::conquest::Conquest;
use crate::events::Shooter;
use crate::nav::Nav;
use crate::powers::raise_beside;
use crate::regiments::{RegimentHandle, Regiments, hash_handle};
use crate::stances::Stances;
use crate::structures::{
    Breaker, Quirk, SILENCES, StructureHandle, Structures, TOLL, TowerAttack, TowerShot,
};
use crate::terrain::Terrain;
use crate::veins::within;
use crate::vision::Vision;
use crate::{Mass, PlayerId, StateHasher, TICKS_PER_SECOND, UnitStats, morale};

/// Hit points a Tollgate's volleys take for each Grub they earn.
pub const TOLL_HP: u32 = 4;
/// How far from a Bell Tower's walls an enemy sets it tolling, and its
/// silence reaches, in tiles.
pub const BELL_REACH: i32 = 10;
/// Ticks between a Bell Tower's tolls, at the least (20 s).
pub const BELL_EVERY: u16 = 20 * TICKS_PER_SECOND as u16;
/// Ticks a toll silences enemy powers for (5 s).
pub const BELL_SILENCE: u16 = 5 * TICKS_PER_SECOND as u16;
/// Whether soldiers of `stats` are big to a bolt: the heavy ones,
/// monsters, riders and engines (the same the computer players watch for
/// to build bolt towers).
pub(crate) fn big_to_bolt(stats: &UnitStats) -> bool {
    stats.mass == Mass::Heavy
}
/// How hard a bolt hits big soldiers, in percent.
pub const BOLT_BIG_PERCENT: u64 = 200;
/// How far from a blast's target, edge to edge, the regiments it splashes
/// stand at most.
pub const BLAST_SPLASH: Fx = Fx::from_int(2);
/// What a blast does to each regiment it splashes, in percent of what it
/// would do to it aimed.
pub const SPLASH_PERCENT: u64 = 50;
/// The most regiments one blast splashes.
pub const MAX_SPLASHED: usize = 4;
/// The most a packed block adds to a blast, in percent (a regiment's
/// volley stops at 125).
pub const BLAST_DENSITY_MAX: i64 = 175;
/// How near a blast tower's walls, in tiles, it can't shoot.
pub const BLAST_MIN_RANGE: i32 = 8;
/// Grub a Collector's Bolt earns for each soldier it kills.
pub const COLLECT_GRUB: u32 = 3;
/// Soldiers a Corpse Catapult's hits kill for each regiment it raises,
/// and how many soldiers that regiment has.
pub const RAISE_AT: u8 = 10;
/// How many raisings' worth of kills a Corpse Catapult keeps while there
/// is no room beside it to raise them.
const RAISE_KEPT: u16 = 3;
/// How long a raised regiment fights before it crumbles (30 s).
pub const RAISED_TICKS: u16 = 30 * TICKS_PER_SECOND as u16;
/// The most regiments behind its target a Prism Lance's bolt runs on
/// through.
pub const MAX_PIERCED: usize = 3;
/// How far a Harpoon Maw drags the regiment it hits, in tiles.
pub const DRAG_TILES: i32 = 3;
/// How long burning ground and an acid pool last.
pub const FIRE_POOL_TICKS: u16 = 8 * TICKS_PER_SECOND as u16;
pub const ACID_POOL_TICKS: u16 = 6 * TICKS_PER_SECOND as u16;
/// How far a pool spreads from where it landed, in tiles: it covers the
/// square this far round.
pub const POOL_REACH: Fx = Fx::from_int(2);
/// Hit points a pool takes off each regiment touching it, a second.
pub const FIRE_POOL_HP: u32 = 12;
pub const ACID_POOL_HP: u32 = 18;
/// Morale a regiment touching a pool loses a second.
pub const POOL_MORALE: i16 = 3;
/// The most pools on the field at once: past it a new one isn't laid.
pub const MAX_POOLS: usize = 64;
/// Percent of a tower's shots that land.
const ACCURACY: u8 = 100;
/// Ticks a tower with nobody to shoot waits before looking again.
const LOOK_EVERY: u16 = 5;

/// Burning ground or a pool of acid a tower's hit left (see the module
/// docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    /// The owner of the tower that left it: it hurts their enemies.
    pub owner: PlayerId,
    /// Acid; otherwise burning ground.
    pub acid: bool,
    /// Where it landed: it covers [`POOL_REACH`] round.
    pub at: FxVec2,
    /// Ticks left.
    pub left: u16,
}

impl Pool {
    fn block(&self) -> Block {
        Block::square(self.at, POOL_REACH, POOL_REACH)
    }
}

/// Which of a tower's targets it shoots first (lower first).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Priority {
    Siege,
    /// Big soldiers, to a bolt.
    Big,
    Besieger,
    Other,
}

impl Conquest {
    /// One tick of towers: the volleys that came down this tick do what
    /// their towers' quirks do (Tollgates earn), pools burn, bells toll,
    /// and towers that have reloaded shoot. Played after the tick's
    /// volleys have landed.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn towers(
        &mut self,
        combat: &mut Combat,
        regs: &mut Regiments,
        units: &[UnitStats],
        nav: &Nav,
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        tick: u32,
    ) {
        for hit in combat.tower_hits() {
            self.tower_hit(hit, regs, units, nav);
        }
        if tick.is_multiple_of(TICKS_PER_SECOND) {
            let near = POOL_REACH + combat.max_radius();
            self.structures.burn_pools(regs, units, stances, near);
        }
        self.structures.age_pools();
        let s = &mut self.structures;
        s.tolled.clear();
        for i in 0..s.slots() {
            let Some(attack) = s.stats_of(i).attack.filter(|_| s.stands(i)) else {
                continue;
            };
            s.silence[i] = s.silence[i].saturating_sub(1);
            if !s.finished(i) || !s.powered(i) {
                continue;
            }
            // A haste speeds its reloads, not its bell.
            if s.flags_of(i) & SILENCES != 0 {
                s.bell[i] = s.bell[i].saturating_sub(1);
                if s.bell[i] == 0 && s.enemy_near(combat, regs, stances, vision, i) {
                    s.bell[i] = BELL_EVERY;
                    s.silence[i] = BELL_SILENCE;
                    s.tolled.push(s.handle(i));
                }
            }
            let steps = s.steps(i) as u16;
            s.reload[i] = s.reload[i].saturating_sub(steps);
            if s.reload[i] > 0 {
                continue;
            }
            // A regiment told to shoot that has fallen is forgotten.
            if s.focus[i].is_some_and(|h| regs.resolve(h).is_none()) {
                s.focus[i] = None;
            }
            let Some(j) = s.tower_target(
                combat,
                regs,
                &self.units,
                units,
                stances,
                terrain,
                vision,
                i,
            ) else {
                s.reload[i] = LOOK_EVERY;
                continue;
            };
            let shot = Shot {
                shooter: Shooter::Tower(s.handle(i)),
                owner: s.owner_of(i),
                from: s.centre(i),
                shots: u16::from(attack.shots),
                damage: attack.damage,
                accuracy: ACCURACY,
                kind: attack.shot,
            };
            if combat.loose(regs, j, shot, tick) {
                s.reload[i] = attack.reload_ticks;
                s.mark_seen(i, vision.allies_of(regs.owner[j]));
                if attack.quirk == Quirk::Pierces && attack.shot == TowerShot::Bolt {
                    s.pierce(
                        combat, regs, units, stances, terrain, vision, i, j, &attack, shot, tick,
                    );
                }
            } else {
                // Every volley is in the air: try again in a moment.
                s.reload[i] = LOOK_EVERY;
            }
        }
    }

    /// What a tower volley that came down does besides hurt: its tower's
    /// quirk, while the tower stands.
    fn tower_hit(&mut self, hit: &TowerHit, regs: &mut Regiments, units: &[UnitStats], nav: &Nav) {
        let s = &mut self.structures;
        let Some(i) = s.resolve(hit.tower) else {
            return;
        };
        let owner = s.owner_of(i);
        if s.flags_of(i) & TOLL != 0 {
            let total = u32::from(s.tally[i]) + hit.hp;
            s.tally[i] = (total % TOLL_HP) as u8;
            self.economy
                .earn_grub(owner, total / TOLL_HP, regs, &self.units);
        }
        let Some(attack) = s.stats_of(i).attack else {
            return;
        };
        match attack.quirk {
            Quirk::None | Quirk::Pierces => {}
            Quirk::Collects => {
                let grub = u32::from(hit.lost) * COLLECT_GRUB;
                self.economy.earn_grub(owner, grub, regs, &self.units);
            }
            Quirk::Raises(unit) => {
                // Kills past a raising carry over, a few raisings' worth at
                // most while there is no room to raise.
                let total = u16::from(s.tally[i]) + hit.lost;
                s.tally[i] = total.min(u16::from(RAISE_AT) * RAISE_KEPT) as u8;
                while s.tally[i] >= RAISE_AT
                    && raise_beside(
                        s,
                        regs,
                        nav,
                        units,
                        owner,
                        unit,
                        i,
                        RAISED_TICKS,
                        Some(RAISE_AT.into()),
                    )
                    .is_some()
                {
                    s.tally[i] -= RAISE_AT;
                }
            }
            Quirk::Disorders => {
                if let Some(j) = regs.resolve(hit.target) {
                    regs.disorder(j);
                }
            }
            // Only the regiment a bolt or blast was aimed at is dragged or
            // has a pool laid under it, not those splashed.
            Quirk::Drags => {
                if hit.aimed
                    && let Some(j) = regs.resolve(hit.target)
                {
                    drag(regs, units, nav, j, s.centre(i), s.block(i).radius());
                }
            }
            Quirk::Burns | Quirk::Acid => {
                if hit.aimed {
                    s.lay_pool(owner, attack.quirk == Quirk::Acid, hit.at);
                }
            }
            Quirk::Kindles => {
                if s.kindle.len() < s.kindle.capacity() {
                    s.kindle.push((hit.at.x.floor_int(), hit.at.y.floor_int()));
                }
            }
        }
    }
}

/// Regiment slot `j` is dragged [`DRAG_TILES`] toward `to` (a tower's
/// centre, its walls `radius` out), stopping short of them, and halted
/// where its block fits; not while it is locked in a fight or on a wall.
fn drag(regs: &mut Regiments, units: &[UnitStats], nav: &Nav, j: usize, to: FxVec2, radius: Fx) {
    if regs.engaged[j].is_some() || regs.on_wall[j] || regs.models[j] == 0 {
        return;
    }
    let from = regs.position[j];
    let gap = (to - from).length();
    let keep = radius + Block::of(regs, units, j).radius() + Fx::ONE;
    let pull = Fx::from_int(DRAG_TILES).min(gap - keep);
    if pull <= Fx::ZERO {
        return;
    }
    let way = (to - from).normalized();
    let owner = regs.owner[j];
    let at = nav.settle(owner, from + way * pull, regs.berth(units, j));
    // Never through a wall: only onto ground it could walk to from here.
    if !nav.walkable(owner, at) || !nav.clear_line(owner, from, at) {
        return;
    }
    regs.position[j] = at;
    regs.halt(j);
}

/// Whether regiment slot `j` is an enemy of `owner`'s that their side can
/// see.
fn foe_in_sight(
    owner: PlayerId,
    regs: &Regiments,
    stances: &Stances,
    vision: &Vision,
    j: usize,
) -> bool {
    regs.alive[j] && stances.hostile(owner, regs.owner[j]) && vision.sees(owner, j, regs.owner[j])
}

impl Structures {
    /// The regiment tower slot `i` shoots now, if any (see the module's
    /// order).
    #[allow(clippy::too_many_arguments)]
    fn tower_target(
        &self,
        combat: &Combat,
        regs: &Regiments,
        kinds: &[crate::ConquestUnit],
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        i: usize,
    ) -> Option<usize> {
        let owner = self.owner_of(i);
        let attack = self.stats_of(i).attack?;
        let at = self.centre(i);
        let foot = self.footprint(i);
        let blast = attack.shot == TowerShot::Blast;
        // Further at a target in torchlight (see [`Vision::reach_at`]); a
        // blast can't come down close by.
        let shootable = |j: usize| {
            foe_in_sight(owner, regs, stances, vision, j)
                && terrain.in_range(vision.reach_at(attack.range, j), at, regs.position[j])
                && !(blast && within(foot, regs.position[j], BLAST_MIN_RANGE))
        };
        let focus = self.focus[i].and_then(|h| regs.resolve(h));
        if let Some(j) = focus.filter(|&j| shootable(j)) {
            return Some(j);
        }
        let reach = terrain.reach(vision.reach_any(attack.range)) + combat.max_step();
        // Nearest first, but a blast goes for the fullest block first.
        let mut best: Option<(Priority, u16, u64, usize)> = None;
        combat.near_enemies(at, reach, owner, |j| {
            if !shootable(j) {
                return;
            }
            let siege = kinds
                .get(usize::from(regs.unit[j]))
                .is_some_and(|k| matches!(k.breaker, Breaker::Siege | Breaker::Artillery));
            let big = attack.shot == TowerShot::Bolt
                && units
                    .get(usize::from(regs.unit[j]))
                    .is_some_and(big_to_bolt);
            let priority = if siege {
                Priority::Siege
            } else if big {
                Priority::Big
            } else if (self.besieging(regs, j)).is_some_and(|b| !stances.hostile(owner, b)) {
                Priority::Besieger
            } else {
                Priority::Other
            };
            let fewer = if blast { u16::MAX - regs.models[j] } else { 0 };
            let key = (priority, fewer, (regs.position[j] - at).length_sq_raw(), j);
            if best.is_none_or(|b| key < b) {
                best = Some(key);
            }
        });
        best.map(|(_, _, _, j)| j)
    }

    /// A Prism Lance's bolt, loosed from tower slot `i` at regiment slot
    /// `j` as `shot`, runs on: the same bolt is loosed at each enemy
    /// regiment its side can see in line behind `j` within the tower's
    /// reach, nearest first, up to [`MAX_PIERCED`].
    #[allow(clippy::too_many_arguments)]
    fn pierce(
        &self,
        combat: &mut Combat,
        regs: &Regiments,
        units: &[UnitStats],
        stances: &Stances,
        terrain: &Terrain,
        vision: &Vision,
        i: usize,
        j: usize,
        attack: &TowerAttack,
        shot: Shot,
        tick: u32,
    ) {
        let owner = self.owner_of(i);
        let at = self.centre(i);
        let way = regs.position[j] - at;
        let first = way.length();
        if first == Fx::ZERO {
            return;
        }
        let dir = way.normalized();
        let reach = terrain.reach(vision.reach_any(attack.range)) + combat.max_step();
        let mut behind: [(Fx, usize); MAX_PIERCED] = [(Fx::ZERO, usize::MAX); MAX_PIERCED];
        let mut n = 0;
        let place = placement!(regs, stances);
        combat.each_near(&place, at, reach, Whose::Enemies(owner), |k| {
            let p = regs.position[k];
            let rel = p - at;
            let along = rel.dot(dir);
            if k == j || along <= first {
                return;
            }
            // How far its centre stands off the line.
            let off = (rel - dir * along).length();
            if off > Block::of(regs, units, k).radius()
                || !vision.sees(owner, k, regs.owner[k])
                || !terrain.in_range(vision.reach_at(attack.range, k), at, p)
            {
                return;
            }
            let key = (along, k);
            if n < MAX_PIERCED {
                behind[n] = key;
                n += 1;
            } else if let Some(worst) = behind.iter_mut().max().filter(|w| key < **w) {
                *worst = key;
            }
        });
        let behind = &mut behind[..n];
        behind.sort_unstable();
        for &(_, k) in behind.iter() {
            if !combat.loose(regs, k, shot, tick) {
                break;
            }
        }
    }

    /// Lay a pool where a tower of `owner`'s hit landed at `at` (acid, or
    /// burning ground), while there is room; burning ground sets the growth
    /// it covers alight (see [`Structures::kindle`]).
    pub(crate) fn lay_pool(&mut self, owner: PlayerId, acid: bool, at: FxVec2) {
        if self.pools.len() >= MAX_POOLS {
            return;
        }
        let left = if acid {
            ACID_POOL_TICKS
        } else {
            FIRE_POOL_TICKS
        };
        let pool = Pool {
            owner,
            acid,
            at,
            left,
        };
        if !acid {
            let r = POOL_REACH.ceil_int();
            let (cx, cy) = (at.x.floor_int(), at.y.floor_int());
            for y in cy - r..=cy + r {
                for x in cx - r..=cx + r {
                    if self.kindle.len() < self.kindle.capacity() {
                        self.kindle.push((x, y));
                    }
                }
            }
        }
        self.pools.push(pool);
    }

    /// One second of pools: each enemy regiment of a pool's owner touching
    /// it is hurt, by the worst pool it touches only (pools don't stack).
    /// `near` is how far from a pool's centre, along either axis, a
    /// regiment's centre can be and still touch it. A regiment the pools
    /// wipe out is gone at once.
    fn burn_pools(
        &mut self,
        regs: &mut Regiments,
        units: &[UnitStats],
        stances: &Stances,
        near: Fx,
    ) {
        if self.pools.is_empty() {
            return;
        }
        for j in 0..regs.alive.len() {
            if !regs.alive[j] || regs.models[j] == 0 {
                continue;
            }
            let at = regs.position[j];
            let close = |p: &&Pool| {
                (p.at.x - at.x).abs() <= near
                    && (p.at.y - at.y).abs() <= near
                    && stances.hostile(p.owner, regs.owner[j])
            };
            if !self.pools.iter().any(|p| close(&p)) {
                continue;
            }
            let block = Block::of(regs, units, j);
            let worst = (self.pools.iter())
                .filter(close)
                .filter(|p| block.touches(&p.block(), Fx::ZERO))
                .map(|p| if p.acid { ACID_POOL_HP } else { FIRE_POOL_HP })
                .max();
            let Some(hp) = worst else {
                continue;
            };
            regs.take_damage(j, hp);
            if regs.models[j] == 0 {
                regs.despawn(RegimentHandle {
                    index: j as u32,
                    generation: regs.generation[j],
                });
            } else {
                morale::shake(regs, j, POOL_MORALE, morale::WHY_FIRE);
            }
        }
    }

    /// Pools dry up.
    fn age_pools(&mut self) {
        self.pools.retain_mut(|p| {
            p.left = p.left.saturating_sub(1);
            p.left > 0
        });
    }

    /// The pools on the field now.
    pub fn pools(&self) -> &[Pool] {
        &self.pools
    }

    /// The owner of the building regiment slot `r` is attacking, if it is.
    fn besieging(&self, regs: &Regiments, r: usize) -> Option<PlayerId> {
        match self.job_of(regs, r)? {
            (s, crate::structures::Work::Attack) => self.owner(s),
            _ => None,
        }
    }

    /// Whether an enemy regiment building slot `i`'s side can see stands
    /// within [`BELL_REACH`] of its walls.
    fn enemy_near(
        &self,
        combat: &Combat,
        regs: &Regiments,
        stances: &Stances,
        vision: &Vision,
        i: usize,
    ) -> bool {
        let owner = self.owner_of(i);
        let foot = self.footprint(i);
        let search = Fx::from_int(BELL_REACH) + self.block(i).radius() + combat.max_step();
        let mut near = false;
        combat.near_enemies(self.centre(i), search, owner, |j| {
            near |= foe_in_sight(owner, regs, stances, vision, j)
                && within(foot, regs.position[j], BELL_REACH);
        });
        near
    }

    /// Whether a power `player` uses on something at `at` is silenced: an
    /// enemy Bell Tower within [`BELL_REACH`] of it has tolled lately.
    pub(crate) fn silenced(&self, stances: &Stances, player: PlayerId, at: FxVec2) -> bool {
        self.standing().any(|i| {
            self.silence[i] > 0
                && stances.hostile(player, self.owner_of(i))
                && within(self.footprint(i), at, BELL_REACH)
        })
    }

    /// Tower slot `i` shoots regiment `target` first while it is in range;
    /// `None` lets it choose again. The caller has checked the tower is the
    /// issuer's.
    pub(crate) fn set_focus(&mut self, i: usize, target: Option<RegimentHandle>) {
        self.focus[i] = target;
    }

    /// The regiment tower `h` was told to shoot, if any and still there.
    pub fn focus_of(&self, h: StructureHandle) -> Option<RegimentHandle> {
        self.resolve(h).and_then(|i| self.focus[i])
    }

    /// Ticks of tower `h`'s silence left: 0 when it isn't silencing.
    pub fn silence_left(&self, h: StructureHandle) -> u16 {
        self.resolve(h).map_or(0, |i| self.silence[i])
    }

    pub(crate) fn hash_tower(&self, i: usize, h: &mut StateHasher) {
        h.u16(self.reload[i]);
        hash_handle(h, self.focus[i]);
        let flags = self.flags_of(i);
        let raises = self
            .stats_of(i)
            .attack
            .is_some_and(|a| matches!(a.quirk, Quirk::Raises(_)));
        if flags & TOLL != 0 || raises {
            h.u8(self.tally[i]);
        }
        if flags & SILENCES != 0 {
            h.u16(self.bell[i]);
            h.u16(self.silence[i]);
        }
    }

    /// The pools, for the state hash: nothing while there are none, so
    /// worlds without them hash as they did.
    pub(crate) fn hash_pools(&self, h: &mut StateHasher) {
        for p in &self.pools {
            h.u8(p.owner);
            h.u8(u8::from(p.acid));
            h.i32(p.at.x.raw());
            h.i32(p.at.y.raw());
            h.u16(p.left);
        }
    }
}
