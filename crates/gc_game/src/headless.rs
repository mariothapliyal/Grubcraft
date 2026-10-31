//! A Conquest match with no window, for tests: the game data, the map and
//! everything a world is made with, then the world, its opening and the
//! computer players' minds, made the way the game makes them. The AI
//! tests, the soak, the scripted golden match and the stress suite's
//! AI-against-walls all start their matches here.

use gc_ai::conquest::opening::Opening;
use gc_ai::{Difficulty, Mind, Minds};
use gc_data::{Faction, GameData};
use gc_map::Map;
use gc_replay::ConquestRecord;
use gc_sim::{Capacities, MatchSetup, PlayerCommand, TickStats, UnitStats, Victories, World};
use gc_ui::lobby::Tempo;

use crate::buildings::Table;
use crate::conquest;
use crate::conquest_ai::mind_for;

/// A headless Conquest match on `map` between `factions` (player `i`
/// plays `factions[i]`, each on a team of their own), rolled from `seed`.
pub(crate) struct Headless {
    pub data: GameData<'static>,
    pub table: Table,
    pub map: Map,
    pub factions: Vec<Faction>,
    pub seed: u64,
    /// The units' battle stats, by unit id.
    pub units: Vec<UnitStats>,
    nav: Vec<u8>,
    teams: Vec<u8>,
}

impl Headless {
    pub fn new(map: Map, factions: &[Faction], seed: u64) -> Headless {
        let data = crate::data::load();
        Headless {
            table: Table::new(&data),
            units: conquest::unit_stats(&data),
            nav: map.sim_nav_costs(),
            teams: (0..factions.len() as u8).collect(),
            factions: factions.to_vec(),
            data,
            map,
            seed,
        }
    }

    /// The battle setup the world is made with (generals on, still sky).
    pub fn setup(&self) -> MatchSetup<'_> {
        MatchSetup {
            seed: self.seed,
            capacities: Capacities::default(),
            units: &self.units,
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
        conquest::plan(
            &self.table,
            &self.data,
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

    /// The opening orders for `world`, fresh from [`Headless::world`]:
    /// give them in its first tick and check the tick with
    /// [`Headless::opened`].
    pub fn opening(&self, world: &World) -> Vec<PlayerCommand> {
        conquest::opening(&self.data, &self.table, &self.map, world, &self.factions)
    }

    /// Check that the first tick, given `opening`, carried out every order.
    pub fn opened(stats: TickStats, opening: &[PlayerCommand]) {
        assert_eq!(
            stats.commands_applied as usize,
            opening.len(),
            "opening orders refused"
        );
    }

    /// Each player's faction and team.
    pub fn sides(&self) -> Vec<(Faction, u8)> {
        self.factions
            .iter()
            .copied()
            .zip(self.teams.clone())
            .collect()
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
            &self.data,
            &self.table,
            &self.map,
            i,
            &self.sides(),
            level,
            temper,
            self.seed,
            opening,
        )
    }
}
