//! How a Conquest mind's level plays the long game (docs/18-ai.md,
//! "Difficulty"): the economy it keeps up and how it picks its fights. The
//! battle side of a level is in [`crate::difficulty::Knobs`].

use crate::difficulty::Difficulty;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Strategy {
    /// Share of the workers its nodes take that it keeps, percent: a
    /// Recruit leaves seeps half worked.
    pub workers_percent: u8,
    /// Production buildings it keeps per tier.
    pub producers_per_tier: u8,
    /// Weighs its army against the enemy's, as far as it knows it, before
    /// it sets out, and goes for the enemy it has the best odds against.
    pub weighs_odds: bool,
    /// Trains towards what does well against the enemy army it has seen.
    pub counters: bool,
    /// Counts only the enemies near the target, so it strikes a base
    /// whose army is away.
    pub opportunist: bool,
    /// How soon it takes its first expansion, percent of its temper's
    /// time.
    pub expand_percent: u8,
}

impl Strategy {
    pub fn of(level: Difficulty) -> Strategy {
        match level {
            Difficulty::Recruit => Strategy {
                workers_percent: 60,
                producers_per_tier: 1,
                weighs_odds: false,
                counters: false,
                opportunist: false,
                expand_percent: 100,
            },
            Difficulty::Veteran => Strategy {
                workers_percent: 70,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: false,
                opportunist: false,
                expand_percent: 100,
            },
            Difficulty::General => Strategy {
                workers_percent: 85,
                producers_per_tier: 2,
                weighs_odds: true,
                counters: true,
                opportunist: false,
                expand_percent: 100,
            },
            Difficulty::Warlord => Strategy {
                workers_percent: 100,
                producers_per_tier: 3,
                weighs_odds: true,
                counters: true,
                opportunist: true,
                expand_percent: 70,
            },
        }
    }
}
