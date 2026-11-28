//! Hero abilities (docs/44-hero-abilities.md, heroes stage H2): two a
//! hero, a basic one and an ultimate, cast with [`crate::Command::Cast`].
//!
//! - **Ranks.** No skill points: the basic ability (slot 0) is rank 1 from
//!   level 1, rank 2 from level 4 and rank 3 from level 7; the ultimate
//!   (slot 1) opens at level 6 (see [`rank`]).
//! - **Casting.** An ability has a cooldown and a reach from the hero. A
//!   cast out of reach walks the hero into reach and goes off when it gets
//!   there; another order calls it off. A dead, stunned or cocooned hero
//!   can't cast. A cast that can't go off (no room to summon, too few
//!   corpses, a target gone) spends no cooldown and is told as
//!   [`crate::events::Event::CastRefused`].
//! - **Effects.** Harm reaches only the caster's enemies, help only the
//!   caster and allies. Ability damage ignores armour, takes soldiers from
//!   the back first and never draws on the combat dice; damage shared
//!   between regiments is shared evenly, the rest going in slot order.
//!   What lasts is kept on the regiments (their inspired, stunned and
//!   untouchable ticks), on the economy (a market crash, a symphony) or as
//!   a zone pulsing at a point (see [`Zone`]).
//!
//! The ability table is here, keyed by hero and slot ([`Kit`]); the
//! rosters' text is the screen's tooltip. Everything a cast leaves is in
//! the state hash.

use alloc::vec::Vec;

use gc_fixed::{Angle, Fx, FxVec2};

use crate::combat::Block;
use crate::hash::StateHasher;
use crate::heroes::Hero;
use crate::morale;
use crate::powers::{put_down, rings_round, spot_among};
use crate::regiments::{Regiments, hash_handle};
use crate::structures::Refusal;
use crate::{Formation, PlayerId, RegimentHandle, TICKS_PER_SECOND, UnitId, UnitStats, World};

/// The most zones (pulsing abilities) on the field at once.
pub const MAX_ZONES: usize = 32;
/// The levels the basic ability reaches ranks 1, 2 and 3 at.
pub const RANK_LEVELS: [u8; 3] = [1, 4, 7];
/// The level the ultimate opens at.
pub const ULTIMATE_LEVEL: u8 = 6;
/// How far from the hero a summoned regiment may stand, in tiles.
pub const SUMMON_TILES: i32 = 6;
/// A regiment of this many soldiers or fewer at full strength is a
/// monster (or a hero) to Maw of the Titan.
pub const MONSTER_MODELS: u16 = 2;
/// How far a walking cast's target may move before the hero is sent after
/// it again, in tiles.
const FOLLOW_TILES: i32 = 2;

/// Which hero an ability set is: one for each of the twelve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kit {
    Daimyo = 0,
    Ronin = 1,
    Onmyoji = 2,
    Queen = 3,
    Devourer = 4,
    Whisperer = 5,
    Maestro = 6,
    Refractor = 7,
    HollowBell = 8,
    Magnate = 9,
    Undertaker = 10,
    Usurer = 11,
}

/// What an ability is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Aim {
    /// Nothing: it works round the hero, or on the hero's side.
    Hero,
    /// A point on the field.
    Point,
    /// An enemy regiment.
    Regiment,
}

/// One ability, as the simulation knows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ability {
    /// As the roster names it.
    pub name: &'static str,
    /// At most 6 characters, for the command grid.
    pub short: &'static str,
    pub aim: Aim,
    /// How far from the hero its point or regiment may be, in tiles (0 for
    /// one aimed at nothing).
    pub range: u8,
    pub cooldown_secs: u16,
    /// Always on: never cast.
    pub passive: bool,
}

const fn active(name: &'static str, short: &'static str, aim: Aim, range: u8, cd: u16) -> Ability {
    Ability {
        name,
        short,
        aim,
        range,
        cooldown_secs: cd,
        passive: false,
    }
}

/// Each hero's name as its roster gives it, and its two abilities.
const KITS: [(&str, [Ability; 2]); 12] = [
    (
        "The Daimyo",
        [
            active("Rally", "Rally", Aim::Hero, 0, 40),
            active("Shogun's Banner", "Banner", Aim::Hero, 0, 120),
        ],
    ),
    (
        "The Ronin",
        [
            active("Iaijutsu Dash", "Dash", Aim::Point, 10, 14),
            active("Thousand Cuts", "Cuts", Aim::Hero, 0, 90),
        ],
    ),
    (
        "The Onmyoji",
        [
            active("Fire Spirit", "Spirit", Aim::Hero, 0, 40),
            active("Kitsune Storm", "Storm", Aim::Point, 24, 120),
        ],
    ),
    (
        "The Queen-in-Waiting",
        [
            active("Spawn Brood", "Spawn", Aim::Hero, 0, 40),
            active("Swarm Tide", "Tide", Aim::Hero, 0, 120),
        ],
    ),
    (
        "The Devourer",
        [
            active("Burrow Charge", "Burrow", Aim::Point, 16, 20),
            active("Maw of the Titan", "Maw", Aim::Point, 16, 120),
        ],
    ),
    (
        "The Whisperer",
        [
            active("Confusion Cloud", "Cloud", Aim::Point, 16, 30),
            active("Puppet Army", "Puppet", Aim::Hero, 0, 150),
        ],
    ),
    (
        "The Maestro",
        [
            active("Crescendo", "Cresc", Aim::Hero, 0, 35),
            active("Symphony", "Symph", Aim::Hero, 0, 180),
        ],
    ),
    (
        "The Refractor",
        [
            active("Split Beam", "Split", Aim::Hero, 0, 18),
            active("Prismatic Lance", "Lance", Aim::Point, 40, 120),
        ],
    ),
    (
        "The Hollow Bell",
        [
            active("Toll", "Toll", Aim::Regiment, 10, 20),
            active("Deafening Peal", "Peal", Aim::Hero, 0, 150),
        ],
    ),
    (
        "The Magnate",
        [
            Ability {
                name: "Compound Interest",
                short: "Intrst",
                aim: Aim::Hero,
                range: 0,
                cooldown_secs: 0,
                passive: true,
            },
            active("Market Crash", "Crash", Aim::Hero, 0, 180),
        ],
    ),
    (
        "The Undertaker",
        [
            active("Raise Dead", "Raise", Aim::Hero, 0, 40),
            active("Mass Grave", "Grave", Aim::Hero, 0, 150),
        ],
    ),
    (
        "The Usurer",
        [
            active("Leech Debt", "Leech", Aim::Regiment, 8, 15),
            active("Foreclosure", "Forecl", Aim::Regiment, 10, 120),
        ],
    ),
];

impl Kit {
    pub const ALL: [Kit; 12] = [
        Kit::Daimyo,
        Kit::Ronin,
        Kit::Onmyoji,
        Kit::Queen,
        Kit::Devourer,
        Kit::Whisperer,
        Kit::Maestro,
        Kit::Refractor,
        Kit::HollowBell,
        Kit::Magnate,
        Kit::Undertaker,
        Kit::Usurer,
    ];

    pub fn from_u8(n: u8) -> Option<Kit> {
        Kit::ALL.get(usize::from(n)).copied()
    }

    /// The hero whose roster name is `name`, if it is one of the twelve.
    pub fn named(name: &str) -> Option<Kit> {
        Kit::ALL.into_iter().find(|k| k.hero_name() == name)
    }

    /// The hero's name, as its roster gives it.
    pub fn hero_name(self) -> &'static str {
        KITS[self as usize].0
    }

    /// Its two abilities: the basic one, then the ultimate.
    pub fn abilities(self) -> &'static [Ability; 2] {
        &KITS[self as usize].1
    }

    /// The unit type its basic ability summons or raises, by roster name
    /// (the game looks its [`UnitId`] up).
    pub fn summons(self) -> Option<&'static str> {
        match self {
            Kit::Onmyoji => Some("Fire Spirit"),
            Kit::Queen => Some("Spawnlings"),
            Kit::Undertaker => Some("Bound Dead"),
            _ => None,
        }
    }
}

/// What a hero's kit needs from the match: which hero it is, and the unit
/// type it summons, if any (see [`Kit::summons`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeroKit {
    pub kit: Kit,
    pub summons: Option<UnitId>,
}

impl HeroKit {
    /// `kit`, or none, into the state hash.
    pub(crate) fn hash_into(kit: Option<HeroKit>, h: &mut StateHasher) {
        match kit {
            Some(k) => {
                h.u8(1 + k.kit as u8);
                h.u16(k.summons.unwrap_or(u16::MAX));
            }
            None => h.u8(0),
        }
    }
}

/// The rank of ability `slot` (0 basic, 1 ultimate) a hero of `level`
/// has: 0 while it is locked.
pub fn rank(slot: u8, level: u8) -> u8 {
    match slot {
        0 => RANK_LEVELS.iter().filter(|&&l| level >= l).count() as u8,
        1 => u8::from(level >= ULTIMATE_LEVEL),
        _ => 0,
    }
}

/// Whether hero `kit`'s ability `slot` summons or raises a regiment.
fn raises(kit: Kit, slot: u8) -> bool {
    matches!(
        (kit, slot),
        (Kit::Onmyoji, 0) | (Kit::Queen, _) | (Kit::Undertaker, _)
    )
}

/// Whether hero `kit`'s ability `slot` works only in Conquest: it moves
/// Grub, or raises the dead from the Tithe's graves, which a Field Battle
/// has none of.
pub fn needs_conquest(kit: Kit, slot: u8) -> bool {
    matches!(
        (kit, slot),
        (Kit::Maestro, 1) | (Kit::Magnate, _) | (Kit::Undertaker, _)
    )
}

/// Whether hero `kit`'s ability `slot` leaves a pulsing zone.
fn zoned(kit: Kit, slot: u8) -> bool {
    matches!(
        (kit, slot),
        (Kit::Ronin, 1) | (Kit::Onmyoji, 1) | (Kit::Usurer, 0) | (Kit::Undertaker, 1)
    )
}

/// The rank's value of three (rank 1, 2, 3; 0 counts as 1).
fn by_rank<T: Copy>(rank: u8, values: [T; 3]) -> T {
    values[usize::from(rank.clamp(1, 3)) - 1]
}

/// What a [`crate::Command::Cast`] is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CastTarget {
    /// Nothing ([`Aim::Hero`]).
    Hero,
    /// A point on the field, in tiles ([`Aim::Point`]).
    Point(FxVec2),
    /// An enemy regiment ([`Aim::Regiment`]).
    Regiment(RegimentHandle),
}

impl CastTarget {
    fn fits(self, aim: Aim) -> bool {
        matches!(
            (self, aim),
            (CastTarget::Hero, Aim::Hero)
                | (CastTarget::Point(_), Aim::Point)
                | (CastTarget::Regiment(_), Aim::Regiment)
        )
    }

    pub(crate) fn hash_into(self, h: &mut StateHasher) {
        match self {
            CastTarget::Hero => h.u8(0),
            CastTarget::Point(p) => {
                h.u8(1);
                h.i32(p.x.raw());
                h.i32(p.y.raw());
            }
            CastTarget::Regiment(r) => {
                h.u8(2);
                hash_handle(h, Some(r));
            }
        }
    }
}

/// A cast waiting for its hero to walk into reach.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pending {
    pub slot: u8,
    pub target: CastTarget,
    /// Where the hero was last sent to get there; none before it walks.
    pub(crate) toward: Option<FxVec2>,
}

/// What a zone does each pulse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pulse {
    /// Thousand Cuts: this much shared between the enemies round the hero.
    Cuts { damage: u16 },
    /// Kitsune Storm: this much, and 3 morale, to every enemy inside.
    Storm { damage: u16 },
    /// Leech Debt: this much off the regiment, healing the hero.
    Leech { damage: u16 },
    /// Mass Grave: the dead round the point join this regiment.
    Grave { raised: RegimentHandle },
}

/// An ability pulsing for a while: at a point, or on the hero or a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Zone {
    owner: PlayerId,
    /// The hero that cast it.
    hero: RegimentHandle,
    pulse: Pulse,
    at: FxVec2,
    /// Its middle follows this regiment (the hero, or Leech Debt's
    /// target), and it ends when that is gone.
    on: Option<RegimentHandle>,
    /// How far it reaches, in tiles.
    tiles: u8,
    /// Ticks between pulses, and pulses left (Mass Grave: ticks left).
    every: u8,
    left: u16,
    /// Ticks to its next pulse.
    next: u8,
}

impl Zone {
    pub(crate) fn hash_into(&self, h: &mut StateHasher) {
        h.u8(self.owner);
        hash_handle(h, Some(self.hero));
        match self.pulse {
            Pulse::Cuts { damage } | Pulse::Storm { damage } | Pulse::Leech { damage } => {
                h.u8(match self.pulse {
                    Pulse::Cuts { .. } => 0,
                    Pulse::Storm { .. } => 1,
                    _ => 2,
                });
                h.u16(damage);
            }
            Pulse::Grave { raised } => {
                h.u8(3);
                hash_handle(h, Some(raised));
            }
        }
        h.i32(self.at.x.raw());
        h.i32(self.at.y.raw());
        hash_handle(h, self.on);
        h.u8(self.tiles);
        h.u8(self.every);
        h.u16(self.left);
        h.u8(self.next);
    }
}

/// `secs` seconds in ticks.
const fn secs(secs: u32) -> u16 {
    (secs * TICKS_PER_SECOND) as u16
}

/// Tenths of a second in ticks.
const fn tenths(tenths: u32) -> u16 {
    (tenths * TICKS_PER_SECOND / 10) as u16
}

/// Whether regiment `j`'s block reaches within `tiles` of `at`.
pub(crate) fn within(
    regs: &Regiments,
    units: &[UnitStats],
    j: usize,
    at: FxVec2,
    tiles: Fx,
) -> bool {
    let reach = tiles + Block::of(regs, units, j).radius();
    (regs.position[j] - at).length() <= reach
}

/// How far `p` is from the segment `from`–`to`.
fn off_line(p: FxVec2, from: FxVec2, to: FxVec2) -> Fx {
    let d = to - from;
    let len = d.dot(d);
    if len.raw() <= 0 {
        return (p - from).length();
    }
    let t = ((p - from).dot(d) / len).clamp(Fx::ZERO, Fx::ONE);
    (p - (from + d * t)).length()
}

/// `total` shared evenly between `n`, the first `total % n` getting one
/// more: the `k`th share.
fn share(total: u32, n: usize, k: usize) -> u32 {
    let n = n.max(1) as u32;
    total / n + u32::from((k as u32) < total % n)
}

impl Regiments {
    /// Slot `i` is inspired for `ticks`: `attack` more melee attack,
    /// reloading `reload` percent faster, and (if `unshaken`) losing no
    /// morale to casualties. Over another inspiration it takes the larger
    /// of each and the longer.
    pub(crate) fn inspire(&mut self, i: usize, ticks: u16, attack: u8, reload: u8, unshaken: bool) {
        if self.inspired_left[i] == 0 {
            self.inspired_attack[i] = 0;
            self.inspired_reload[i] = 0;
            self.unshaken[i] = false;
        }
        self.inspired_left[i] = self.inspired_left[i].max(ticks);
        self.inspired_attack[i] = self.inspired_attack[i].max(attack);
        self.inspired_reload[i] = self.inspired_reload[i].max(reload);
        self.unshaken[i] |= unshaken;
    }

    /// Slot `i` is stunned for `ticks`: held fast where it stands, striking
    /// and shooting nothing. It keeps its orders.
    pub(crate) fn stun(&mut self, i: usize, ticks: u16) {
        self.stunned_left[i] = self.stunned_left[i].max(ticks);
        self.trapped_left[i] = self.trapped_left[i].max(ticks);
    }

    /// Whether slot `i` is stunned.
    pub(crate) fn stunned(&self, i: usize) -> bool {
        self.stunned_left[i] > 0
    }

    /// Whether slot `i` can't strike, shoot or cast: stunned or in a
    /// cocoon.
    pub(crate) fn cannot_act(&self, i: usize) -> bool {
        self.stunned(i) || self.cocooned(i)
    }

    /// Melee attack slot `i`'s inspiration adds.
    pub(crate) fn attack_bonus(&self, i: usize) -> i32 {
        if self.inspired_left[i] > 0 {
            i32::from(self.inspired_attack[i])
        } else {
            0
        }
    }

    /// Ticks to reload `ticks`, inspired.
    pub(crate) fn inspired_reload(&self, i: usize, ticks: u16) -> u16 {
        if self.inspired_left[i] > 0 && self.inspired_reload[i] > 0 {
            (u32::from(ticks) * 100 / (100 + u32::from(self.inspired_reload[i]))) as u16
        } else {
            ticks
        }
    }

    /// Whether slot `i` loses no morale to casualties now.
    pub(crate) fn is_unshaken(&self, i: usize) -> bool {
        self.inspired_left[i] > 0 && self.unshaken[i]
    }

    /// The percent slot `i`'s next volley deals: more while a Split Beam
    /// has it charged.
    pub(crate) fn beam_percent(&self, i: usize) -> u64 {
        if self.volleys_up[i] == 0 {
            return 100;
        }
        100 + u64::from(self.volley_bonus[i])
    }

    /// Slot `i` marches `percent` faster for `ticks`: over a march, the
    /// larger and the longer.
    pub(crate) fn speed_up(&mut self, i: usize, ticks: u16, percent: u8) {
        if self.march_left[i] == 0 {
            self.march[i] = 0;
        }
        self.march[i] = self.march[i].max(percent);
        self.march_left[i] = self.march_left[i].max(ticks);
    }

    /// Slot `i`'s hit points left, every soldier's.
    pub(crate) fn hp_left(&self, i: usize) -> u32 {
        let m = crate::formation::MAX_MODELS as usize;
        (self.hp[i * m..][..usize::from(self.models[i])].iter())
            .map(|&h| u32::from(h))
            .sum()
    }
}

impl Hero {
    /// Ticks until ability `slot` can be cast again at `now`: 0 when
    /// ready.
    pub fn cooling(&self, slot: u8, now: u32) -> u32 {
        (self.ready_at.get(usize::from(slot))).map_or(0, |&at| at.saturating_sub(now))
    }

    /// Its ability `slot`, if it has a kit.
    pub fn ability(&self, slot: u8) -> Option<&'static Ability> {
        self.kit?.kit.abilities().get(usize::from(slot))
    }
}

impl World {
    /// Whether `player` could have hero regiment `hero` cast its ability
    /// `slot` now, at some target: its own hero, an active ability its
    /// level has opened, not cooling down, the hero free to act.
    pub fn can_cast(
        &self,
        player: PlayerId,
        hero: RegimentHandle,
        slot: u8,
    ) -> Result<(), Refusal> {
        let h = self.heroes.of_regiment(hero).ok_or(Refusal::Unknown)?;
        let i = self.regiments.resolve(hero).ok_or(Refusal::Unknown)?;
        if !self.stances.can_command(player, self.regiments.owner[i]) {
            return Err(Refusal::Unknown);
        }
        if h.ability(slot).is_none_or(|a| a.passive) {
            return Err(Refusal::Unknown);
        }
        if self.conquest.is_none() && h.kit.is_some_and(|k| needs_conquest(k.kit, slot)) {
            return Err(Refusal::NoBuildings);
        }
        if rank(slot, h.level) == 0 {
            return Err(Refusal::Locked);
        }
        if h.cooling(slot, self.tick) > 0 {
            return Err(Refusal::Cooling);
        }
        if self.regiments.cannot_act(i) {
            return Err(Refusal::Stunned);
        }
        Ok(())
    }

    /// Whether `player` could have hero regiment `hero` cast its ability
    /// `slot` at `target` now ([`World::can_cast`], and a target of the
    /// ability's kind), and if not, why. Reach is not checked: a cast out
    /// of reach walks the hero there.
    pub fn cast_check(
        &self,
        player: PlayerId,
        hero: RegimentHandle,
        slot: u8,
        target: CastTarget,
    ) -> Result<(), Refusal> {
        self.can_cast(player, hero, slot)?;
        let h = self.hero_of(hero).ok_or(Refusal::Unknown)?;
        let a = h.ability(slot).ok_or(Refusal::Unknown)?;
        if !target.fits(a.aim) {
            return Err(Refusal::Unknown);
        }
        if let CastTarget::Regiment(t) = target {
            let foe = self
                .regiments
                .owner(t)
                .is_some_and(|o| self.stances.hostile(player, o));
            if !foe || !self.faces(player, t) {
                return Err(Refusal::Unseen);
            }
            let foreclosure = h.kit.is_some_and(|k| k.kit == Kit::Usurer) && slot == 1;
            if foreclosure && self.regiments.resolve(t).is_some_and(|j| self.sturdy(j)) {
                return Err(Refusal::Sturdy);
            }
        }
        Ok(())
    }

    /// Whether slot `j` stands at half strength or more: too sturdy for
    /// Foreclosure.
    fn sturdy(&self, j: usize) -> bool {
        u32::from(self.regiments.models[j]) * 2 >= u32::from(self.regiments.full_models[j])
    }

    /// [`crate::Command::Cast`]: this tick if in reach, else the hero walks
    /// there and casts on arrival.
    pub(crate) fn cast(
        &mut self,
        player: PlayerId,
        hero: RegimentHandle,
        slot: u8,
        target: CastTarget,
    ) -> bool {
        if self.cast_check(player, hero, slot, target).is_err() {
            return false;
        }
        let Some(k) = self.heroes.index_of(hero) else {
            return false;
        };
        // It goes off (or the hero sets out) when the heroes cast this
        // tick, after the tick's events are cleared, so it is told. A cast
        // it was walking to is replaced, and so is that walk.
        let walking = self
            .heroes
            .get(k)
            .pending
            .is_some_and(|p| p.toward.is_some());
        self.heroes.get_mut(k).pending = Some(Pending {
            slot,
            target,
            toward: None,
        });
        if walking && let Some(i) = self.regiments.resolve(hero) {
            self.regiments.target[i] = None;
        }
        true
    }

    /// Heroes casting, once a tick, before the tick's dead are counted:
    /// the Magnate's interest, casts walking into reach, and the zones.
    pub(crate) fn abilities(&mut self) {
        if self.heroes.iter().all(Hero::dead) && self.heroes.zones().is_empty() {
            return;
        }
        if self.tick > 0 && self.tick.is_multiple_of(60 * TICKS_PER_SECOND) {
            self.interest();
        }
        // The zones pulse before the casts, so a zone cast now waits its
        // full time for its second pulse.
        self.pulse(None);
        for k in 0..self.heroes.iter().count() {
            self.go_cast(k, false);
        }
    }

    /// Heroes casting after the tick's dead are counted: the casts that
    /// summon or raise (their regiments must not take the slot of one wiped
    /// out this tick before its dead are counted, and raising wants this
    /// tick's corpses), and Mass Grave's regiments growing by the tick's
    /// dead.
    pub(crate) fn abilities_late(&mut self) {
        if self.heroes.iter().all(|h| h.pending.is_none()) && self.heroes.zones.is_empty() {
            return;
        }
        for k in 0..self.heroes.iter().count() {
            self.go_cast(k, true);
        }
        self.grow_graves();
    }

    /// Compound Interest: each standing Magnate's player banks a share of
    /// their Grub.
    fn interest(&mut self) {
        let Some(c) = &mut self.conquest else {
            return;
        };
        for k in 0..self.heroes.iter().count() {
            let h = *self.heroes.get(k);
            if h.dead() || h.kit.map(|k| k.kit) != Some(Kit::Magnate) {
                continue;
            }
            let r = rank(0, h.level);
            let (grub, _) = c.economy.store(h.owner);
            let gain = (grub * u32::from(r) / 100).min(15 * u32::from(r));
            c.economy.refund(h.owner, gain, 0);
        }
    }

    /// Hero `k`'s waiting cast: off if it is in reach, else the hero walks
    /// on; called off if it can no longer be cast. A cast that summons or
    /// raises goes off only `late` (see [`World::abilities_late`]), the
    /// others only early; the hero walks early.
    fn go_cast(&mut self, k: usize, late: bool) {
        let h = *self.heroes.get(k);
        let Some(p) = h.pending else {
            return;
        };
        let call_off = |w: &mut World| w.heroes.get_mut(k).pending = None;
        let (Some(r), Some(a)) = (h.regiment, h.ability(p.slot)) else {
            return call_off(self);
        };
        let Some(i) = self.regiments.resolve(r) else {
            return call_off(self);
        };
        // Whoever commands the hero's side now (an ally it was handed to).
        let by = self.stances.commander(h.owner);
        if let Err(why) = self.cast_check(by, r, p.slot, p.target) {
            // It can no longer go off (the target hid, the hero was
            // stunned): told, as a refusal at the cast is.
            self.events.cast_refused(r, p.slot, why);
            return call_off(self);
        }
        let to = match p.target {
            CastTarget::Hero => self.regiments.position[i],
            CastTarget::Point(at) => at,
            CastTarget::Regiment(t) => match self.regiments.resolve(t) {
                Some(j) => self.regiments.position[j],
                None => return call_off(self),
            },
        };
        let reach = Fx::from_int(i32::from(a.range));
        let summons = h.kit.is_some_and(|kit| raises(kit.kit, p.slot));
        if (to - self.regiments.position[i]).length() <= reach {
            if summons != late {
                return;
            }
            call_off(self);
            match self.cast_now(k, i, p.slot, p.target) {
                Ok(()) => {
                    let ready = self.tick + u32::from(a.cooldown_secs) * TICKS_PER_SECOND;
                    self.heroes.get_mut(k).ready_at[usize::from(p.slot)] = ready;
                    // Walked there to cast: it stops where it cast from.
                    if p.toward.is_some() && self.regiments.resolve(r) == Some(i) {
                        self.regiments.target[i] = None;
                    }
                    self.events.cast(r, p.slot, to);
                }
                Err(why) => self.events.cast_refused(r, p.slot, why),
            }
            return;
        }
        if late {
            return;
        }
        // Out of reach: on towards it, again when the target has moved.
        let follow = Fx::from_int(FOLLOW_TILES);
        if let Some(toward) = p.toward
            && (to - toward).length() <= follow
        {
            if self.regiments.target[i].is_some() {
                return;
            }
            // It got as near as it can, and that is not near enough.
            self.events.cast_refused(r, p.slot, Refusal::Rough);
            return call_off(self);
        }
        let step = crate::orders::Step::Move {
            target: to,
            facing: None,
            files: 0,
            pace: Fx::ZERO,
            hunt: false,
            brigade: 0,
        };
        if !self.march(r, step) {
            return call_off(self);
        }
        self.heroes.get_mut(k).pending = Some(Pending {
            toward: Some(to),
            ..p
        });
    }

    /// A move, attack or work order for regiment `h` calls off its
    /// waiting cast.
    pub(crate) fn call_off_cast(&mut self, h: RegimentHandle) {
        if let Some(k) = self.heroes.index_of(h) {
            self.heroes.get_mut(k).pending = None;
        }
    }

    /// Hero `k`, in regiment slot `i`, casts ability `slot` at `target`.
    fn cast_now(
        &mut self,
        k: usize,
        i: usize,
        slot: u8,
        target: CastTarget,
    ) -> Result<(), Refusal> {
        let h = *self.heroes.get(k);
        let kit = h.kit.ok_or(Refusal::Unknown)?;
        // No room for its zone: refused before anything happens.
        if zoned(kit.kit, slot) && self.heroes.zones.len() >= MAX_ZONES {
            return Err(Refusal::NoRoom);
        }
        let r = rank(slot, h.level);
        let (p, at) = (h.owner, self.regiments.position[i]);
        let point = match target {
            CastTarget::Point(to) => to,
            _ => at,
        };
        let foe = match target {
            CastTarget::Regiment(t) => self.regiments.resolve(t),
            _ => None,
        };
        let hero = self.regiments.handle(i);
        match (kit.kit, slot) {
            (Kit::Daimyo, 0) => {
                let gain = by_rank(r, [15, 25, 35]);
                for j in self.round(p, true, at, 12) {
                    morale::rally_now(&mut self.regiments, j);
                    morale::shift(&mut self.regiments, j, gain);
                }
            }
            (Kit::Daimyo, _) => {
                let regs = &mut self.regiments;
                for j in 0..regs.alive.len() {
                    if regs.alive[j] && regs.owner[j] == p {
                        morale::shift(regs, j, 25);
                        regs.speed_up(j, secs(20), 20);
                        regs.steadfast_left[j] = regs.steadfast_left[j].max(secs(20));
                    }
                }
            }
            (Kit::Ronin, 0) => {
                let damage = by_rank(r, [60, 100, 140]);
                let to = self.blink(i, point)?;
                for j in self.on_line(p, at, to, Fx::ONE) {
                    self.hurt(j, damage);
                }
            }
            (Kit::Ronin, _) => {
                self.regiments.untouchable_left[i] = secs(3);
                self.add_zone(Zone {
                    owner: p,
                    hero,
                    pulse: Pulse::Cuts { damage: 45 },
                    at,
                    on: Some(hero),
                    tiles: 4,
                    every: 7,
                    left: 8,
                    next: 0,
                })?;
            }
            (Kit::Onmyoji, 0) => {
                let lasts = secs(by_rank(r, [30, 40, 50]));
                self.summon(k, i, 1, lasts, None)?;
            }
            (Kit::Onmyoji, _) => {
                self.add_zone(Zone {
                    owner: p,
                    hero,
                    pulse: Pulse::Storm { damage: 20 },
                    at: point,
                    on: None,
                    tiles: 8,
                    every: TICKS_PER_SECOND as u8,
                    left: 10,
                    next: 0,
                })?;
            }
            (Kit::Queen, 0) => {
                self.summon(k, i, by_rank(r, [1, 2, 3]), secs(30), None)?;
            }
            (Kit::Queen, _) => {
                for j in self.round(p, true, at, 20) {
                    if self.regiments.owner[j] == p {
                        self.regiments.inspire(j, secs(15), 20, 0, true);
                    }
                }
                // As many as there is room for.
                let _ = self.summon(k, i, 4, secs(30), None);
            }
            (Kit::Devourer, 0) => {
                let damage = by_rank(r, [60, 90, 120]);
                let to = self.blink(i, point)?;
                for j in self.round(p, false, to, 3) {
                    self.regiments.disorder(j);
                    self.hurt(j, damage);
                }
            }
            (Kit::Devourer, _) => {
                let mut culled = Vec::new();
                for j in self.round(p, false, point, 5) {
                    let big = self.regiments.full_models[j] <= MONSTER_MODELS
                        || self.regiments.hero_level[j] > 0;
                    if big {
                        self.hurt(j, 200);
                    } else {
                        culled.push(j);
                    }
                }
                let n = culled.len();
                for (m, &j) in culled.iter().enumerate() {
                    let take = share(12, n, m) as usize;
                    if self.regiments.alive[j] {
                        self.regiments.kill_last(j, take);
                    }
                }
            }
            (Kit::Whisperer, 0) => {
                let ticks = secs(by_rank(r, [4, 5, 6]));
                for j in self.round(p, false, point, 5) {
                    self.regiments.disorder(j);
                    self.regiments.stun(j, ticks);
                }
            }
            (Kit::Whisperer, _) => {
                for j in self.round(p, false, at, 20) {
                    if self.regiments.morale_state[j].broken() {
                        self.regiments.stun(j, secs(10));
                    }
                }
            }
            (Kit::Maestro, 0) => {
                let (reload, attack) = (by_rank(r, [15, 25, 35]), by_rank(r, [10, 15, 20]));
                for j in self.round(p, true, at, 12) {
                    self.regiments.inspire(j, secs(10), attack, reload, false);
                }
            }
            (Kit::Maestro, _) => {
                let c = self.conquest.as_mut().ok_or(Refusal::NoBuildings)?;
                c.structures.symphony(p, secs(30));
            }
            (Kit::Refractor, 0) => {
                self.regiments.volleys_up[i] = 3;
                self.regiments.volley_bonus[i] = by_rank(r, [50, 100, 150]);
            }
            (Kit::Refractor, _) => self.lance(p, at, point),
            (Kit::HollowBell, 0) => {
                let j = foe.ok_or(Refusal::Unseen)?;
                self.regiments.stun(j, tenths(by_rank(r, [15, 25, 35])));
            }
            (Kit::HollowBell, _) => {
                for j in self.round(p, false, at, 14) {
                    morale::shake(&mut self.regiments, j, 30, morale::WHY_HERO);
                }
            }
            (Kit::Magnate, 0) => return Err(Refusal::Unknown),
            (Kit::Magnate, _) => {
                let stances = &self.stances;
                let c = self.conquest.as_mut().ok_or(Refusal::NoBuildings)?;
                for q in 0..crate::MAX_PARTIES as PlayerId {
                    if stances.hostile(p, q) {
                        c.economy.crash(q, secs(30));
                    }
                }
            }
            (Kit::Undertaker, 0) => {
                let most = by_rank(r, [6, 10, 14]);
                self.summon(k, i, 1, secs(45), Some((8, most)))?;
            }
            (Kit::Undertaker, _) => {
                let raised = self.summon(k, i, 1, secs(60), Some((10, u16::MAX)))?;
                // It rises short, and grows to its unit's full strength.
                if let Some(g) = self.regiments.resolve(raised) {
                    let full = self.units[usize::from(self.regiments.unit[g])].models;
                    self.regiments.full_models[g] = full.min(crate::formation::MAX_MODELS);
                }
                self.add_zone(Zone {
                    owner: p,
                    hero,
                    pulse: Pulse::Grave { raised },
                    at,
                    on: None,
                    tiles: 10,
                    every: 1,
                    left: secs(15),
                    next: 1,
                })?;
            }
            (Kit::Usurer, 0) => {
                let j = foe.ok_or(Refusal::Unseen)?;
                self.add_zone(Zone {
                    owner: p,
                    hero,
                    pulse: Pulse::Leech {
                        damage: by_rank(r, [15, 25, 35]),
                    },
                    at: self.regiments.position[j],
                    on: Some(self.regiments.handle(j)),
                    tiles: 0,
                    every: TICKS_PER_SECOND as u8,
                    left: 3,
                    next: 0,
                })?;
            }
            (Kit::Usurer, _) => {
                let j = foe.ok_or(Refusal::Unseen)?;
                if self.sturdy(j) {
                    return Err(Refusal::Sturdy);
                }
                let models = self.regiments.models[j];
                self.regiments.kill_last(j, usize::from(models / 3).max(1));
            }
        }
        // The zones' first pulses land with the cast.
        self.pulse(Some(hero));
        Ok(())
    }

    /// The living regiments hostile to `owner` (or friendly, if
    /// `friends`) whose blocks reach within `tiles` of `at`, in slot order.
    fn round(&self, owner: PlayerId, friends: bool, at: FxVec2, tiles: i32) -> Vec<usize> {
        let (regs, tiles) = (&self.regiments, Fx::from_int(tiles));
        (0..regs.alive.len())
            .filter(|&j| {
                regs.alive[j]
                    && regs.models[j] > 0
                    && self.stances.hostile(owner, regs.owner[j]) != friends
                    && within(regs, &self.units, j, at, tiles)
            })
            .collect()
    }

    /// The enemies of `owner` whose blocks reach within `half` of the
    /// line from `from` to `to`, in slot order.
    fn on_line(&self, owner: PlayerId, from: FxVec2, to: FxVec2, half: Fx) -> Vec<usize> {
        let regs = &self.regiments;
        (0..regs.alive.len())
            .filter(|&j| {
                regs.alive[j]
                    && regs.models[j] > 0
                    && self.stances.hostile(owner, regs.owner[j])
                    && off_line(regs.position[j], from, to)
                        <= half + Block::of(regs, &self.units, j).radius()
            })
            .collect()
    }

    /// Regiment slot `j` takes `hp` of ability damage, and is gone if that
    /// leaves nobody.
    fn hurt(&mut self, j: usize, hp: u32) {
        if !self.regiments.alive[j] || self.regiments.models[j] == 0 {
            return;
        }
        crate::combat::hurt(&mut self.regiments, &self.units, &self.nav, j, hp);
        if self.regiments.models[j] == 0 {
            self.regiments.vanish_from_fight(j);
            match &mut self.conquest {
                Some(c) => c.remove(j, &mut self.regiments),
                None => {
                    self.regiments.despawn(self.regiments.handle(j));
                }
            }
        }
    }

    /// Hero slot `i` blinks to `to`: where its block fits, along a line
    /// open to it on its own side's grid (its own gates open, enemy gates
    /// and walls shut). Out of any fight. Where it landed.
    fn blink(&mut self, i: usize, to: FxVec2) -> Result<FxVec2, Refusal> {
        let (p, from) = (self.regiments.owner[i], self.regiments.position[i]);
        let stats = &self.units[usize::from(self.regiments.unit[i])];
        let berth = Regiments::berth_of(stats, Formation::Line, stats.files, stats.models);
        if !self.nav.walkable(p, to) || !self.nav.clear_line(p, from, to) {
            return Err(Refusal::Rough);
        }
        let to = self.nav.settle(p, to, berth);
        if !self.nav.fits(p, to, berth) || !self.nav.clear_line(p, from, to) {
            return Err(Refusal::Rough);
        }
        let facing = if to == from {
            self.regiments.facing[i]
        } else {
            Angle::of_vec(to - from)
        };
        let h = self.regiments.handle(i);
        self.regiments
            .place(h, to, facing, 0)
            .ok_or(Refusal::Rough)?;
        self.regiments.vanish_from_fight(i);
        self.regiments.target[i] = None;
        Ok(to)
    }

    /// Prismatic Lance: 250 shared along a 2-tile-wide line 40 tiles long
    /// from `from` towards `toward`, between the enemy regiments and
    /// buildings on it, half as much again to a building.
    fn lance(&mut self, owner: PlayerId, from: FxVec2, toward: FxVec2) {
        let way = (toward - from).normalized();
        if way == FxVec2::ZERO {
            return;
        }
        let to = from + way * Fx::from_int(40);
        let hit = self.on_line(owner, from, to, Fx::ONE);
        let stances = &self.stances;
        let built: Vec<usize> = self.conquest.as_ref().map_or_else(Vec::new, |c| {
            let st = &c.structures;
            (0..st.slots())
                .filter(|&s| {
                    let f = st.footprint(s);
                    let half = Fx::from_int(f[2].max(f[3])) / Fx::from_int(2);
                    st.stands(s)
                        && stances.hostile(owner, st.owner_of(s))
                        && off_line(st.centre(s), from, to) <= Fx::ONE + half
                })
                .collect()
        });
        let n = hit.len() + built.len();
        for (m, &j) in hit.iter().enumerate() {
            self.hurt(j, share(250, n, m));
        }
        let Some(c) = &mut self.conquest else {
            return;
        };
        for (m, &s) in built.iter().enumerate() {
            let hp = share(250, n, hit.len() + m) * 3 / 2;
            if c.structures.stands(s) {
                let scaled = hp * crate::structures::HP_SCALE;
                c.structures
                    .wear_down(s, scaled, &mut self.nav, &self.regiments);
            }
        }
    }

    /// Hero `k` (slot `i`) puts `count` regiments of its kit's unit down
    /// beside it for `lasts` ticks: or, with `raise` (tiles, most), one
    /// risen from the corpses within that many tiles, at most that strong.
    /// The first one put down; refused if there is none.
    fn summon(
        &mut self,
        k: usize,
        i: usize,
        count: u16,
        lasts: u16,
        raise: Option<(u8, u16)>,
    ) -> Result<RegimentHandle, Refusal> {
        let h = *self.heroes.get(k);
        let unit = h.kit.and_then(|k| k.summons).ok_or(Refusal::Unknown)?;
        let (p, at) = (h.owner, self.regiments.position[i]);
        let mut first = None;
        for _ in 0..count {
            let put = match (raise, &mut self.conquest) {
                // A Field Battle keeps no graves to raise from.
                (Some(_), None) => Err(Refusal::NoCorpses),
                (Some((tiles, most)), Some(c)) => c.raise_up_to(
                    &mut self.regiments,
                    &self.nav,
                    &self.units,
                    &self.stances,
                    (p, unit),
                    (tiles, at, most),
                    lasts,
                ),
                (None, _) => summon_beside(
                    &mut self.regiments,
                    &self.nav,
                    &self.units,
                    p,
                    unit,
                    at,
                    lasts,
                ),
            };
            let Ok(r) = put else {
                if first.is_none() {
                    return put;
                }
                break;
            };
            self.vision.start(
                &self.regiments,
                &self.units,
                &self.terrain,
                r.index as usize,
            );
            first.get_or_insert(r);
        }
        first.ok_or(Refusal::Crowded)
    }

    /// Start zone `z`; refused with the field's zones all taken.
    fn add_zone(&mut self, z: Zone) -> Result<(), Refusal> {
        (self.heroes.add_zone(z))
            .then_some(())
            .ok_or(Refusal::NoRoom)
    }

    /// Every zone pulses when its time comes, and ends when its pulses
    /// (or what it follows) are gone; with `first`, only the zones hero
    /// `first` cast this tick pulse, at once (a cast's first pulse).
    fn pulse(&mut self, first: Option<RegimentHandle>) {
        let mut zones = core::mem::take(&mut self.heroes.zones);
        for z in &mut zones {
            if first.is_some_and(|h| z.hero != h || z.next != 0) {
                continue;
            }
            if let Some(on) = z.on {
                match self.regiments.resolve(on) {
                    Some(j) => z.at = self.regiments.position[j],
                    None => z.left = 0,
                }
            }
            if z.left == 0 {
                continue;
            }
            if z.next > 0 && first.is_none() {
                z.next -= 1;
                if z.next > 0 {
                    continue;
                }
            }
            z.next = z.every;
            z.left -= 1;
            self.pulse_once(z);
        }
        zones.retain(|z| z.left > 0);
        // Zones cast while these pulsed (none can be) would be lost.
        debug_assert!(self.heroes.zones.is_empty());
        self.heroes.zones = zones;
    }

    /// One pulse of zone `z`.
    fn pulse_once(&mut self, z: &mut Zone) {
        match z.pulse {
            Pulse::Cuts { damage } => {
                let foes = self.round(z.owner, false, z.at, i32::from(z.tiles));
                for (m, &j) in foes.iter().enumerate() {
                    self.hurt(j, share(u32::from(damage), foes.len(), m));
                }
            }
            Pulse::Storm { damage } => {
                let foes = self.round(z.owner, false, z.at, i32::from(z.tiles));
                for j in foes {
                    morale::shake(&mut self.regiments, j, 3, morale::WHY_HERO);
                    self.hurt(j, u32::from(damage));
                }
            }
            Pulse::Leech { damage } => {
                let (Some(j), Some(i)) = (
                    z.on.and_then(|r| self.regiments.resolve(r)),
                    self.regiments.resolve(z.hero),
                ) else {
                    z.left = 0;
                    return;
                };
                let before = self.regiments.hp_left(j);
                self.hurt(j, u32::from(damage));
                let after = if self.regiments.alive[j] {
                    self.regiments.hp_left(j)
                } else {
                    0
                };
                let full = self.hero_full_hp(i);
                let drained = before.saturating_sub(after).min(u32::from(u16::MAX)) as u16;
                self.regiments.mend(i, drained, full);
            }
            // It grows after the tick's dead are counted (`grow_graves`).
            Pulse::Grave { raised } => {
                if self.regiments.resolve(raised).is_none() {
                    z.left = 0;
                }
            }
        }
    }

    /// Each Mass Grave's regiment grows by a soldier for each soldier who
    /// died this tick within its reach, up to full strength.
    fn grow_graves(&mut self) {
        let graves: Vec<(RegimentHandle, FxVec2, u8)> = (self.heroes.zones.iter())
            .filter(|z| z.left > 0)
            .filter_map(|z| match z.pulse {
                Pulse::Grave { raised } => Some((raised, z.at, z.tiles)),
                _ => None,
            })
            .collect();
        for (raised, at, tiles) in graves {
            let Some(g) = self.regiments.resolve(raised) else {
                continue;
            };
            let regs = &self.regiments;
            let reach = u64::from(Fx::from_int(i32::from(tiles)).raw().unsigned_abs()).pow(2);
            let dead: u32 = (0..regs.deaths.len())
                .filter(|&j| regs.deaths[j] > 0 && j != g)
                .filter(|&j| (regs.position[j] - at).length_sq_raw() <= reach)
                .map(|j| u32::from(regs.deaths[j]))
                .sum();
            let hp = self.units[usize::from(regs.unit[g])].hp_per_model;
            for _ in 0..dead {
                if self.regiments.models[g] >= self.regiments.full_models[g] {
                    break;
                }
                self.regiments.add_soldier(g, hp);
            }
        }
    }

    /// Hero slot `i`'s soldier's hit points at full health.
    fn hero_full_hp(&self, i: usize) -> u16 {
        let stats = &self.units[usize::from(self.regiments.unit[i])];
        crate::heroes::full_hp(&self.regiments, stats, i)
    }
}

/// A regiment of `unit` of `player`'s put down for `lasts` ticks at the
/// nearest free ground within [`SUMMON_TILES`] of `at` that a straight
/// line from `at` reaches (so not through a wall).
fn summon_beside(
    regs: &mut Regiments,
    nav: &crate::nav::Nav,
    units: &[UnitStats],
    player: PlayerId,
    unit: UnitId,
    at: FxVec2,
    lasts: u16,
) -> Result<RegimentHandle, Refusal> {
    let stats = *units.get(usize::from(unit)).ok_or(Refusal::Unknown)?;
    let near = Fx::from_int(SUMMON_TILES);
    let spots =
        rings_round(at, 0).filter(|&s| (s - at).length() <= near && nav.clear_line(player, at, s));
    let spot = spot_among(regs, nav, &stats, player, spots).ok_or(Refusal::Crowded)?;
    if (spot - at).length() > near || !nav.clear_line(player, at, spot) {
        return Err(Refusal::Crowded);
    }
    // Facing out from the hero.
    let facing = if spot == at {
        Angle::ZERO
    } else {
        Angle::of_vec(spot - at)
    };
    put_down(regs, units, player, unit, (spot, facing), None, lasts).ok_or(Refusal::NoRoom)
}
