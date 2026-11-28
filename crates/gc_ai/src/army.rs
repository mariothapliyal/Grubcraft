//! Raising and placing an army for a Field Battle (docs/06-game-modes.md).
//!
//! Until production arrives with M3, a Field Battle army is a fixed number
//! of regiments picked from its faction's roster by a preset mix, and set
//! out in a standard deployment: the line in front, archers behind it, fast
//! troops on the wings. The same code raises the AI's army and the player's,
//! so neither side starts better placed.

use gc_data::army;
use gc_sim::{Command, PlayerCommand, PlayerId, UnitId, UnitStats};
use glam::Vec2;
use serde::Deserialize;

pub use crate::deployment::{Deployment, deploy_clear_of};
use crate::perception::{Class, to_fx_vec, vec_angle};
use crate::personality::Faction;
use crate::terrain::Terrain;

/// Regiments in a Field Battle army.
pub const ARMY_REGIMENTS: usize = 10;

/// The army mixes a personality can build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub enum Preset {
    /// Half line, the rest split between fast troops and archers.
    Balanced,
    /// Heavy on fast troops.
    Aggressive,
    /// Heavy on line and archers.
    Defensive,
}

impl Preset {
    /// Shares of line, shock and ranged regiments, out of 12.
    fn shares(self) -> [u32; 3] {
        match self {
            Preset::Balanced => [6, 3, 3],
            Preset::Aggressive => [5, 5, 2],
            Preset::Defensive => [6, 2, 4],
        }
    }
}

const CLASSES: [Class; 3] = [Class::Line, Class::Shock, Class::Ranged];

/// Pick `size` regiments for an army of `faction` from the unit table
/// (`factions[i]` is unit `i`'s faction). Classes the faction doesn't have
/// are made up from the ones it does; types within a class take turns.
pub fn pick_army(
    units: &[UnitStats],
    factions: &[Faction],
    faction: Faction,
    preset: Preset,
    size: usize,
) -> Vec<UnitId> {
    pick_army_of(units, factions, faction, preset, size, |_| true)
}

/// [`pick_army`] from the types `usable` lets in.
fn pick_army_of(
    units: &[UnitStats],
    factions: &[Faction],
    faction: Faction,
    preset: Preset,
    size: usize,
    usable: impl Fn(usize) -> bool,
) -> Vec<UnitId> {
    let mut mine: Vec<UnitId> = (0..units.len())
        .filter(|&i| factions.get(i) == Some(&faction) && usable(i))
        .map(|i| i as UnitId)
        .collect();
    if mine.is_empty() {
        // A faction with no regiments of its own yet borrows everyone's.
        mine = (0..units.len())
            .filter(|&i| usable(i))
            .map(|i| i as UnitId)
            .collect();
    }
    let of_class = |c: Class| -> Vec<UnitId> {
        mine.iter()
            .copied()
            .filter(|&u| Class::of(&units[u as usize]) == c)
            .collect()
    };
    let types: [Vec<UnitId>; 3] = CLASSES.map(of_class);
    let shares = preset.shares();
    let mut counts = [0usize; 3];
    for c in 0..3 {
        if !types[c].is_empty() {
            counts[c] = (size * shares[c] as usize + 6) / 12;
        }
    }
    // Round to exactly `size`, adding to (or taking from) the line first.
    let order = [0usize, 1, 2];
    while counts.iter().sum::<usize>() < size {
        let c = order.iter().copied().find(|&c| !types[c].is_empty());
        match c {
            Some(c) => counts[c] += 1,
            None => break,
        }
    }
    while counts.iter().sum::<usize>() > size {
        let c = order
            .iter()
            .copied()
            .rev()
            .find(|&c| counts[c] > 0)
            .unwrap();
        counts[c] -= 1;
    }
    let mut army = Vec::with_capacity(size);
    for c in 0..3 {
        for k in 0..counts[c] {
            army.push(types[c][k % types[c].len()]);
        }
    }
    army
}

/// The largest army in `preset`'s mix (see [`pick_army`]) whose regiments
/// cost at most `budget` points, `points[u]` being unit `u`'s cost, and no
/// more than `most` regiments. A type that costs 0 points is not for
/// Field Battles (a worker, a summoned troop) and is never picked.
pub fn pick_army_within(
    units: &[UnitStats],
    points: &[u32],
    factions: &[Faction],
    faction: Faction,
    preset: Preset,
    budget: u32,
    most: usize,
) -> Vec<UnitId> {
    let cost = |army: &[UnitId]| -> u32 {
        army.iter()
            .map(|&u| points.get(u as usize).copied().unwrap_or(u32::MAX))
            .fold(0, u32::saturating_add)
    };
    let usable = |u: usize| points.get(u).is_some_and(|&p| p > 0);
    let mut army: Vec<UnitId> = (1..=most)
        .map(|n| pick_army_of(units, factions, faction, preset, n, usable))
        .take_while(|army| cost(army) <= budget)
        .last()
        .unwrap_or_default();
    // The mix's next size can cost far more than is left (a dear type's
    // turn comes up), so fill what is left with the dearest of the
    // faction's own types that still fits, line first.
    let mut left = budget.saturating_sub(cost(&army));
    while army.len() < most {
        let Some(u) = (0..units.len())
            .filter(|&u| factions.get(u) == Some(&faction) && usable(u) && points[u] <= left)
            .max_by_key(|&u| (Class::of(&units[u]) == Class::Line, points[u]))
        else {
            break;
        };
        army.push(u as UnitId);
        left -= points[u];
    }
    army
}

/// Whether a unit type is a hero, for a Field Battle army.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HeroType {
    /// A regiment.
    #[default]
    No,
    /// A hero with an ability that works only in Conquest.
    Hero,
    /// A hero all of whose abilities work in a Field Battle: the computer
    /// players take these ([`pick_hero`]).
    Whole,
}

/// The part of a player's own `army` (entries as [`gc_data::army`] has
/// them) that can take the field: their faction's regiments (none that
/// cost 0 points) and heroes (`heroes` says which unit types are; each
/// once, at most [`army::MAX_HEROES`], a hero listed without a level at
/// level 1, a level on anything else dropped), in list order, up to
/// `budget` points (a hero's at its level) and `most` regiments. Anything
/// else is left at home, so a changed budget or a tampered list can't
/// field more than it should.
pub fn within_budget(
    army: &[u16],
    points: &[u32],
    factions: &[Faction],
    heroes: &[HeroType],
    faction: Faction,
    budget: u32,
    most: usize,
) -> Vec<u16> {
    let mut spent = 0u32;
    let mut out: Vec<u16> = Vec::new();
    for &e in army {
        let u = army::unit(e);
        let (Some(&first), Some(&f)) = (points.get(u as usize), factions.get(u as usize)) else {
            continue;
        };
        let hero = heroes.get(u as usize).is_some_and(|&h| h != HeroType::No);
        let e = match (hero, army::level(e)) {
            (true, 0) => army::entry(u, 1),
            (false, l) if l > 0 => continue,
            _ => e,
        };
        let p = army::points(e, first);
        let heroes_in = out.iter().filter(|&&o| army::level(o) > 0);
        if hero
            && (heroes_in.clone().count() >= army::MAX_HEROES
                || heroes_in.clone().any(|&o| army::unit(o) == u))
        {
            continue;
        }
        if f != faction || first == 0 || out.len() == most || spent + p > budget {
            continue;
        }
        spent += p;
        out.push(e);
    }
    out
}

/// The hero a computer player takes into a Field Battle, as an entry:
/// the first of `faction`'s [`HeroType::Whole`] heroes, at the highest
/// level that costs at most [`HERO_SHARE_PERCENT`] of `budget`; none if
/// even its first level costs more.
pub fn pick_hero(
    points: &[u32],
    factions: &[Faction],
    heroes: &[HeroType],
    faction: Faction,
    budget: u32,
) -> Option<u16> {
    let u = (0..heroes.len()).find(|&u| {
        heroes[u] == HeroType::Whole
            && factions.get(u) == Some(&faction)
            && points.get(u).is_some_and(|&p| p > 0)
    })?;
    let share = budget * HERO_SHARE_PERCENT / 100;
    (1..=army::MAX_LEVEL)
        .rev()
        .find(|&l| army::hero_points(points[u], l) <= share)
        .map(|l| army::entry(u as u16, l))
}

/// The most of its budget a computer player spends on its hero, in
/// percent.
pub const HERO_SHARE_PERCENT: u32 = 25;

/// Where one regiment of an army starts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub unit: UnitId,
    pub position: Vec2,
    pub facing: Vec2,
}

/// Space between regiments side by side, and between the rows, tiles.
pub(crate) const GAP: f32 = 2.0;
pub(crate) const ROW_GAP: f32 = 4.0;

/// Set `army` out in the standard deployment around `centre`, facing
/// `forward`: line regiments in a row, archers behind them, fast troops on
/// the wings. Where the ground won't hold that (the edge of the map, water,
/// cliffs, thicket), the deployment is moved, turned and folded into more
/// ranks until it fits; see [`crate::deployment`].
pub fn deploy(
    units: &[UnitStats],
    army: &[UnitId],
    centre: Vec2,
    forward: Vec2,
    terrain: &Terrain,
) -> Vec<Placement> {
    deploy_clear_of(units, army, centre, forward, terrain, &[]).placed
}

/// The commands that put a deployed army on the field for `party`.
pub fn spawn_commands(party: PlayerId, placements: &[Placement], out: &mut Vec<PlayerCommand>) {
    spawn_army(party, placements, &[], out);
}

/// [`spawn_commands`] for an army with heroes: a placement whose unit is
/// one of `heroes` (entries as [`gc_data::army`] has them, each hero
/// once) is put down as that hero at its level, the heroes first, in the
/// order of `heroes` (the first is the side's general).
pub fn spawn_army(
    party: PlayerId,
    placements: &[Placement],
    heroes: &[u16],
    out: &mut Vec<PlayerCommand>,
) {
    let is_hero = |p: &Placement| {
        heroes
            .iter()
            .any(|&e| army::level(e) > 0 && army::unit(e) == p.unit)
    };
    let first =
        (heroes.iter()).filter_map(|&e| placements.iter().find(|p| p.unit == army::unit(e)));
    let rest = placements.iter().filter(|p| !is_hero(p));
    out.extend(first.chain(rest).map(|p| {
        let (position, facing) = (to_fx_vec(p.position), vec_angle(p.facing));
        let hero = (heroes.iter()).find(|&&e| army::level(e) > 0 && army::unit(e) == p.unit);
        PlayerCommand {
            player: party,
            command: match hero {
                Some(&e) => Command::SpawnHero {
                    unit: p.unit,
                    position,
                    facing,
                    level: army::level(e),
                },
                None => Command::SpawnRegiment {
                    unit: p.unit,
                    position,
                    facing,
                },
            },
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_sim::scenario::DRILL_UNITS;

    fn factions() -> Vec<Faction> {
        // Spears, fast troops and archers for the Clans; pikes for the Concord.
        vec![
            Faction::EmberClans,
            Faction::EmberClans,
            Faction::EmberClans,
            Faction::CarrionConcord,
        ]
    }

    #[test]
    fn presets_mix_the_classes() {
        let f = factions();
        let balanced = pick_army(&DRILL_UNITS, &f, Faction::EmberClans, Preset::Balanced, 12);
        assert_eq!(balanced.len(), 12);
        let n = |a: &[UnitId], u: UnitId| a.iter().filter(|&&x| x == u).count();
        assert_eq!(
            (n(&balanced, 0), n(&balanced, 1), n(&balanced, 2)),
            (6, 3, 3)
        );
        let defensive = pick_army(&DRILL_UNITS, &f, Faction::EmberClans, Preset::Defensive, 10);
        assert_eq!(defensive.len(), 10);
        assert!(n(&defensive, 2) > n(&defensive, 1));
    }

    #[test]
    fn a_faction_with_one_type_fields_all_of_it() {
        let army = pick_army(
            &DRILL_UNITS,
            &factions(),
            Faction::CarrionConcord,
            Preset::Aggressive,
            ARMY_REGIMENTS,
        );
        assert_eq!(army, vec![3; ARMY_REGIMENTS]);
    }

    #[test]
    fn armies_are_raised_to_the_budget() {
        let f = factions();
        let points = [400, 600, 300, 400];
        let cost = |a: &[UnitId]| a.iter().map(|&u| points[u as usize]).sum::<u32>();
        for budget in [2_000, 4_000, 8_000] {
            let army = pick_army_within(
                &DRILL_UNITS,
                &points,
                &f,
                Faction::EmberClans,
                Preset::Balanced,
                budget,
                40,
            );
            assert!(cost(&army) <= budget, "{budget}: {}", cost(&army));
            // One more regiment of the mix would not have fitted.
            let next = pick_army(
                &DRILL_UNITS,
                &f,
                Faction::EmberClans,
                Preset::Balanced,
                army.len() + 1,
            );
            assert!(cost(&next) > budget, "{budget}: room left");
        }
        let pikes = pick_army_within(
            &DRILL_UNITS,
            &points,
            &f,
            Faction::CarrionConcord,
            Preset::Balanced,
            4_000,
            40,
        );
        assert_eq!(pikes, vec![3; 10]);
        let capped = pick_army_within(
            &DRILL_UNITS,
            &points,
            &f,
            Faction::CarrionConcord,
            Preset::Balanced,
            4_000,
            6,
        );
        assert_eq!(capped.len(), 6);
    }

    /// When the mix's next size would bring in a dear type, what is left
    /// buys line regiments instead of lying unspent.
    #[test]
    fn points_left_over_buy_more_line() {
        let points = [400, 1_500, 300, 400];
        let army = pick_army_within(
            &DRILL_UNITS,
            &points,
            &factions(),
            Faction::EmberClans,
            Preset::Balanced,
            4_000,
            40,
        );
        let cost: u32 = army.iter().map(|&u| points[u as usize]).sum();
        // The mix stops at five (3,000 points); two spears fill it up.
        assert_eq!(army.len(), 7, "{army:?}");
        assert_eq!(cost, 3_800);
        assert_eq!(army.iter().filter(|&&u| u == 0).count(), 5);
    }

    #[test]
    fn a_players_army_is_held_to_the_budget_and_their_faction() {
        let f = factions();
        let points = [400, 600, 300, 400];
        // A Concord regiment and an unknown type are dropped, then the
        // budget runs out at the second 600.
        let built = [0, 3, 1, 99, 2, 1, 0];
        let fielded = within_budget(&built, &points, &f, &[], Faction::EmberClans, 1_800, 40);
        assert_eq!(fielded, vec![0, 1, 2, 0]);
        assert_eq!(
            within_budget(&built, &points, &f, &[], Faction::EmberClans, 8_000, 2),
            vec![0, 1]
        );
    }

    /// A hero is fielded once, at its level's points; a level on a
    /// regiment is dropped, and a hero without one comes at level 1.
    #[test]
    fn a_players_heroes_are_held_to_their_levels_points() {
        let f = factions();
        let points = [400, 600, 300, 400];
        let heroes = [HeroType::No, HeroType::Hero, HeroType::No, HeroType::No];
        let hero = |l| army::entry(1, l);
        // Level 5: 960 points.
        let built = [hero(5), 0, hero(2), army::entry(0, 3), 2];
        assert_eq!(
            within_budget(&built, &points, &f, &heroes, Faction::EmberClans, 1_700, 40),
            vec![hero(5), 0, 2]
        );
        assert_eq!(
            within_budget(
                &[1, 0],
                &points,
                &f,
                &heroes,
                Faction::EmberClans,
                4_000,
                40
            ),
            vec![hero(1), 0]
        );
        assert_eq!(
            within_budget(
                &[hero(10)],
                &points,
                &f,
                &heroes,
                Faction::EmberClans,
                1_000,
                40
            ),
            Vec::<u16>::new(),
            "1,410 points"
        );
    }

    #[test]
    fn the_computer_takes_its_first_hero_at_the_level_a_quarter_of_its_points_buys() {
        let f = factions();
        let points = [400, 600, 300, 400];
        use HeroType::{No, Whole};
        let heroes = [No, Whole, No, Whole];
        let pick = |budget| pick_hero(&points, &f, &heroes, Faction::EmberClans, budget);
        assert_eq!(pick(2_000), None, "500 points won't buy one");
        assert_eq!(pick(4_000), Some(army::entry(1, 5)), "960 of 1,000");
        assert_eq!(pick(8_000), Some(army::entry(1, 10)));
        assert_eq!(
            pick_hero(&points, &f, &heroes, Faction::CarrionConcord, 4_000),
            Some(army::entry(3, 10))
        );
        // Placed as the hero at its level.
        let at = Placement {
            unit: 1,
            position: Vec2::ZERO,
            facing: Vec2::Y,
        };
        let mut out = Vec::new();
        let line = Placement { unit: 0, ..at };
        spawn_army(0, &[line, at], &[army::entry(1, 5)], &mut out);
        assert!(
            matches!(
                out[0].command,
                Command::SpawnHero {
                    unit: 1,
                    level: 5,
                    ..
                }
            ),
            "the hero first"
        );
        assert!(matches!(
            out[1].command,
            Command::SpawnRegiment { unit: 0, .. }
        ));
    }

    #[test]
    fn deployment_puts_archers_behind_and_nobody_on_top_of_anyone() {
        let army = pick_army(
            &DRILL_UNITS,
            &factions(),
            Faction::EmberClans,
            Preset::Balanced,
            12,
        );
        let centre = Vec2::new(100.0, 150.0);
        let forward = Vec2::new(0.0, -1.0);
        let placed = deploy(&DRILL_UNITS, &army, centre, forward, &Terrain::open(256));
        assert_eq!(placed.len(), 12);
        for p in &placed {
            let ahead = (p.position - centre).dot(forward);
            if p.unit == 2 {
                assert!(ahead < -4.0, "archers behind the line: {p:?}");
            } else {
                assert!(ahead.abs() < 0.01, "{p:?}");
            }
        }
        for (i, a) in placed.iter().enumerate() {
            for b in &placed[i + 1..] {
                assert!(a.position.distance(b.position) > 4.0, "{a:?} {b:?}");
            }
        }
    }
}
