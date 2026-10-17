//! The Conquest layers of a mind (docs/18-ai.md, "Inside a mind"): the
//! [`Steward`](steward::Steward) runs the economy, production and building;
//! the [`Marshal`](marshal::Marshal) runs the army and hands each battle to
//! a [`Captain`](crate::Captain). The Sovereign's choice of intents and the
//! Spymaster's beliefs come in the next stage; for now the Marshal's few
//! rules (gather, attack when strong enough, defend home, fall back when
//! beaten) stand in for them.
//!
//! A Conquest mind plays by the same rules as any other: it reads what its
//! side sees (fog of war), and acts only through the commands a player's
//! clicks would send, with its level's reaction time and attention.

pub mod clicks;
pub mod kit;
pub mod marshal;
pub mod placing;
pub mod steward;

use std::sync::Arc;

use gc_sim::{PlayerCommand, PlayerId, World};

use crate::difficulty::Knobs;
use crate::perception::Perception;
use clicks::Clicks;
use kit::Kit;
use marshal::Marshal;
use steward::Steward;

/// What the Conquest layers read at each look.
pub struct Sense<'a> {
    pub world: &'a World,
    /// What its side sees.
    pub view: &'a Perception,
    pub kit: &'a Kit,
    pub party: PlayerId,
    pub now: u32,
    /// Its level's reaction time, in ticks.
    pub reaction: u32,
    /// The map's size, in tiles.
    pub tiles: f32,
    /// The tick from which it expands once its seeps are full.
    pub expand_after: u32,
}

/// A Conquest mind's economy and army.
pub struct Realm {
    kit: Arc<Kit>,
    pub steward: Steward,
    pub marshal: Marshal,
    clicks: Clicks,
}

impl Realm {
    pub fn new(kit: Arc<Kit>, party: PlayerId, knobs: &Knobs) -> Realm {
        Realm {
            kit,
            steward: Steward::new(),
            marshal: Marshal::new(party, knobs.reaction_ticks, knobs.orders_per_minute),
            clicks: Clicks::new(party, knobs.reaction_ticks, knobs.orders_per_minute),
        }
    }

    pub fn kit(&self) -> &Kit {
        &self.kit
    }

    /// Economy orders sent so far.
    pub fn clicks(&self) -> &Clicks {
        &self.clicks
    }

    /// Look and append the orders due now to `out`.
    pub fn tick(
        &mut self,
        world: &World,
        captain: crate::captain::Ctx,
        expand_after: u32,
        out: &mut Vec<PlayerCommand>,
    ) {
        let kit = Arc::clone(&self.kit);
        let sense = Sense {
            world,
            view: captain.view,
            kit: &kit,
            party: captain.party,
            now: captain.view.tick,
            reaction: captain.knobs.reaction_ticks,
            tiles: captain.terrain.tiles() as f32,
            expand_after,
        };
        self.clicks.refresh(sense.now);
        self.steward
            .think(&sense, &mut self.clicks, self.marshal.rally);
        self.clicks.release(sense.now, out);
        let home = self.steward.home;
        let mut c = marshal::Command {
            captain,
            sense: &sense,
        };
        self.marshal.tick(&mut c, home, out);
    }

    /// One line on what it is doing.
    pub fn summary(&self) -> String {
        let l = &self.steward.ledger;
        let fight = match self.marshal.captain() {
            Some(c) => format!(
                ", in battle ({}, {}{})",
                c.doctrine.name(),
                c.posture.name(),
                if c.finishing { ", finishing" } else { "" }
            ),
            None => String::new(),
        };
        let at = match self.marshal.goal {
            marshal::Goal::Attack(p) | marshal::Goal::Defend(p) | marshal::Goal::Heart(p) => {
                format!(" at ({:.0}, {:.0})", p.x, p.y)
            }
            marshal::Goal::Gather => String::new(),
        };
        format!(
            "tier {}, {}/{} workers, {} producing, army {}{at}{fight}",
            l.tier,
            l.workers,
            l.want_workers,
            l.producers,
            self.marshal.goal.name()
        )
    }
}
