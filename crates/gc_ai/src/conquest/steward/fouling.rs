//! Fouling the enemy's bridges (docs/39-water-engine.md, section 5, stage
//! 5a): a mind that has seen an enemy bridge over running water sends a
//! worker upstream of it to fell the bank woods into the river, or, with no
//! woods to fell, to tip a raft in, so the current carries the debris onto
//! the bridge's piers. How often, and whether it waits for a flood, is its
//! level's ([`Fouling`]).

use super::{Look, Steward};
use crate::conquest::Sense;
use crate::conquest::spymaster;
use crate::conquest::strategy::Fouling;
use crate::orders::Hands as Clicks;
use gc_sim::rivers::debris::RAFT_GRUB;
use gc_sim::structures::{Role, StructureHandle};
use gc_sim::terrain::Growth;
use gc_sim::{Command, TICKS_PER_SECOND};
use glam::Vec2;

/// How often it looks for an enemy bridge to foul.
const LOOK_EVERY: u32 = 10 * TICKS_PER_SECOND;
/// How far round an enemy bridge it looks for a place to put debris in.
const SEARCH: i32 = 40;
/// The furthest down the river, in river tiles, a place may be above the
/// bridge (debris put in further up may ground or jam elsewhere first).
const FURTHEST: usize = 48;
/// The nearest, in river tiles: a worker right by the enemy's bridge
/// would be seen and caught.
const NEAREST: usize = 6;
/// No place with an enemy fighter in sight within this many tiles.
const DANGER: f32 = 14.0;
/// Grub it keeps back before it pays for a raft.
const GRUB_KEPT: u32 = 200;

/// What it is doing to the enemy's bridges.
#[derive(Clone, Debug, Default)]
pub(super) struct Fouled {
    next_look: u32,
    /// Enemy bridge tiles it last sent debris at, and when.
    sent: Vec<(StructureHandle, u32)>,
}

/// How the debris goes in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Way {
    /// Felling the woods on land tile `at` into the river beside it.
    Fell,
    /// Tipping a raft into river tile `at`.
    Raft,
}

impl Steward {
    /// Send a worker to put debris into the river above an enemy bridge,
    /// if its level does and the time is right. What it decided, for the
    /// journal.
    pub(super) fn foul_bridges(
        &mut self,
        s: &Sense,
        clicks: &mut Clicks,
        look: &Look,
    ) -> Option<String> {
        let Fouling { every, calm } = s.plan.fouls?;
        if s.now < self.fouled.next_look || !clicks.free() {
            return None;
        }
        self.fouled.next_look = s.now + LOOK_EVERY;
        let rivers = s.world.flood().rivers();
        let flood = rivers.fronts().next().is_some() || rivers.flooding();
        if (!calm && !flood) || rivers.share_used(s.party) {
            return None;
        }
        let worker = s.kit.worker()?;
        let home = self.home?;
        let every = every * TICKS_PER_SECOND;
        (self.fouled.sent).retain(|&(_, t)| s.now < t + every);
        let bridge = (spymaster::enemy_sighted(s).into_iter())
            .filter(|b| b.role == Role::Bridge)
            .filter(|b| !self.fouled.sent.iter().any(|f| f.0 == b.handle))
            .map(|b| (b.handle, (b.at.x.floor() as i32, b.at.y.floor() as i32)))
            // Over running water.
            .filter(|&(_, (x, y))| s.world.down_river(x, y, 1).next().is_some())
            .min_by(|a, b| tile_distance(a.1, home).total_cmp(&tile_distance(b.1, home)))?;
        let grub = s.world.bank(s.party).map_or(0, |b| b.grub);
        let rafts = grub >= RAFT_GRUB + GRUB_KEPT;
        let fells = worker.unit.clearing.fells;
        let (way, at) = place(s, bridge.1, home, flood, fells, rafts)?;
        let w = self.builder(s, look, Vec2::new(at.0 as f32 + 0.5, at.1 as f32 + 0.5))?;
        let tile = (at.0 as i16, at.1 as i16);
        let (order, note) = match way {
            Way::Fell => (
                Command::Fell {
                    regiment: w,
                    tile,
                    size: (1, 1),
                },
                "Felling the bank woods into the river above the enemy's bridge",
            ),
            Way::Raft => (
                Command::Dump { regiment: w, tile },
                "Tipping a raft into the river above the enemy's bridge",
            ),
        };
        if !clicks.give(s.now, order) {
            return None;
        }
        self.sent.push((w, s.now));
        self.fouled.sent.push((bridge.0, s.now));
        let when = if flood {
            ", with a flood coming down"
        } else {
            ""
        };
        Some(format!("{note}{when}"))
    }
}

/// The place, nearest `home`, to put debris in that drifts onto river tile
/// `bridge`, between [`NEAREST`] and [`FURTHEST`] river tiles above it,
/// over no ford in low water (it would ground there), and with no enemy
/// fighter in sight near: a wooded bank to fell if the worker `fells`,
/// else, with Grub for `rafts`, a bank to tip a raft in.
fn place(
    s: &Sense,
    bridge: (i32, i32),
    home: Vec2,
    flood: bool,
    fells: bool,
    rafts: bool,
) -> Option<(Way, (i32, i32))> {
    let reaches = |x: i32, y: i32| {
        let mut down = s.world.down_river(x, y, FURTHEST);
        let mut steps = 0;
        loop {
            let (tile, ford) = down.next()?;
            steps += 1;
            if tile == bridge {
                return (steps >= NEAREST).then_some(());
            }
            if ford && !flood {
                return None;
            }
        }
    };
    let terrain = s.world.terrain();
    let wooded = |x, y| matches!(terrain.growth(x, y), Growth::Woods | Growth::Thicket);
    let mut best: Option<(f32, Way, (i32, i32))> = None;
    let (bx, by) = bridge;
    for y in by - SEARCH..=by + SEARCH {
        for x in bx - SEARCH..=bx + SEARCH {
            let way = if fells && wooded(x, y) && s.world.fells_into(x, y) {
                // The logs fall into a river tile beside it: every one
                // there must drift onto the bridge.
                let mut beside = NEIGHBOURS
                    .iter()
                    .map(|&(dx, dy)| (x + dx, y + dy))
                    .filter(|&(x, y)| s.world.down_river(x, y, 1).next().is_some())
                    .peekable();
                if beside.peek().is_none() || !beside.all(|(x, y)| reaches(x, y).is_some()) {
                    continue;
                }
                Way::Fell
            } else if rafts && s.world.bank_river(x, y) && reaches(x, y).is_some() {
                Way::Raft
            } else {
                continue;
            };
            let d = tile_distance((x, y), home);
            // Felling is free: it is picked over a raft not much nearer.
            let d = if way == Way::Fell { d * 0.75 } else { d };
            if best.as_ref().is_some_and(|b| b.0 <= d) {
                continue;
            }
            let at = [Vec2::new(x as f32 + 0.5, y as f32 + 0.5)];
            if spymaster::fighters_near(s, &at, DANGER).next().is_some() {
                continue;
            }
            best = Some((d, way, (x, y)));
        }
    }
    best.map(|b| (b.1, b.2))
}

/// The eight tiles round one.
const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
    (0, -1),
    (1, -1),
];

fn tile_distance((x, y): (i32, i32), to: Vec2) -> f32 {
    Vec2::new(x as f32 + 0.5, y as f32 + 0.5).distance(to)
}
