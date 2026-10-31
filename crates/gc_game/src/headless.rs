//! A Conquest match with no window, for tests: the game data
//! ([`Real`], loaded once and shared), the map and everything a world is
//! made with, then the world, its opening and the computer players' minds,
//! made with the game's own Conquest plan, opening and minds (on a still
//! sky, every player on a team of their own). The AI tests, the soak, the
//! scripted golden match and the stress suite start their matches here.

use gc_ai::conquest::opening::Opening;
use gc_ai::{Difficulty, Mind, Minds};
use gc_data::{Faction, GameData};
use gc_map::Map;
use gc_replay::ConquestRecord;
use gc_sim::{
    Capacities, ConquestUnit, MatchSetup, PlayerCommand, TickStats, UnitStats, Victories, World,
};
use gc_ui::lobby::Tempo;

use crate::buildings::Table;
use crate::conquest;
use crate::conquest_ai::mind_for;

/// The real game data, as a Conquest world reads it.
pub(crate) struct Real {
    pub data: GameData<'static>,
    pub table: Table,
    /// The units' battle stats, by unit id.
    pub units: Vec<UnitStats>,
    pub conquest_units: Vec<ConquestUnit>,
}

impl Real {
    pub fn load() -> Real {
        let data = crate::data::load();
        Real {
            table: Table::new(&data),
            units: conquest::unit_stats(&data),
            conquest_units: conquest::units_of(&data),
            data,
        }
    }

    /// The Conquest unit called `name`.
    pub fn unit(&self, name: &str) -> u16 {
        (self.data.conquest_units.iter())
            .position(|u| u.name() == name)
            .unwrap_or_else(|| panic!("no unit called {name}")) as u16
    }

    /// The Ember Clans' spearmen, swordsmen and archers.
    pub fn ember_line(&self) -> [u16; 3] {
        ["Yari Ashigaru", "Katana Samurai", "Yumi Ashigaru"].map(|n| self.unit(n))
    }

    /// The building called `name`.
    pub fn building(&self, name: &str) -> u16 {
        self.table.named(name)
    }
}

/// A headless Conquest match on `map` between `factions` (player `i`
/// plays `factions[i]`, each on a team of their own), rolled from `seed`,
/// played with `real`'s data.
pub(crate) struct Headless<'a> {
    pub real: &'a Real,
    pub map: Map,
    factions: Vec<Faction>,
    seed: u64,
    nav: Vec<u8>,
    teams: Vec<u8>,
    /// Each player's faction and team.
    sides: Vec<(Faction, u8)>,
}

impl<'a> Headless<'a> {
    pub fn new(real: &'a Real, map: Map, factions: &[Faction], seed: u64) -> Headless<'a> {
        Headless {
            real,
            nav: map.sim_nav_costs(),
            teams: (0..factions.len() as u8).collect(),
            sides: factions.iter().copied().zip(0..).collect(),
            factions: factions.to_vec(),
            map,
            seed,
        }
    }

    /// The battle setup the world is made with (generals on, still sky).
    pub fn setup(&self) -> MatchSetup<'_> {
        MatchSetup {
            seed: self.seed,
            capacities: Capacities::default(),
            units: &self.real.units,
            map_tiles: self.map.tiles,
            nav: &self.nav,
            teams: &self.teams,
            ground: self.map.ground(),
            champions: false,
            sky: gc_sim::Sky::STILL,
            generals: true,
        }
    }

    /// The Conquest plan at `tempo`, won by `victories`.
    pub fn plan(&self, tempo: Tempo, victories: Victories) -> ConquestRecord {
        let real = self.real;
        conquest::plan(
            &real.table,
            &real.data,
            &self.map,
            &self.factions,
            tempo,
            victories,
        )
    }

    /// A fresh Conquest world played to `plan`.
    pub fn world(&self, plan: &ConquestRecord) -> World {
        let mut world = World::new(&self.setup());
        assert!(world.conquest(&plan.setup()), "the Conquest setup refused");
        world
    }

    /// Give `world`, fresh from [`Headless::world`], its opening orders in
    /// its first tick through `tick` (the caller's own, timed or recorded)
    /// and check every one was carried out.
    pub fn open(
        &self,
        world: &mut World,
        tick: impl FnOnce(&mut World, &[PlayerCommand]) -> TickStats,
    ) {
        let real = self.real;
        let opening = conquest::opening(&real.data, &real.table, &self.map, world, &self.factions);
        let stats = tick(world, &opening);
        assert_eq!(
            stats.commands_applied as usize,
            opening.len(),
            "opening orders refused"
        );
    }

    /// No minds yet, reading this map's terrain.
    pub fn minds(&self) -> Minds {
        Minds::new(gc_match::terrain_view::ai_terrain(&self.map, &self.nav))
    }

    /// The mind for player `i` at `level`, its personality picked by
    /// `temper`; it picks its own opening unless given one.
    pub fn mind(
        &self,
        i: usize,
        level: Difficulty,
        temper: u32,
        opening: Option<&'static Opening>,
    ) -> Mind {
        mind_for(
            &self.real.data,
            &self.real.table,
            &self.map,
            i,
            &self.sides,
            level,
            temper,
            self.seed,
            opening,
        )
    }
}
