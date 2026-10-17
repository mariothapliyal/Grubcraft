//! The Marshal: a Conquest mind's army (docs/18-ai.md, "Marshal"). New
//! regiments gather at a rally point in front of home; once the army is
//! big enough it marches on the nearest enemy building it knows of, or
//! goes to look at an enemy start, and knocks down what it finds. It turns
//! back to defend when its own buildings are attacked, and falls back to
//! rebuild when an attack has cost it most of its army. Fighting itself is
//! the Captain's ([`crate::captain`]): whenever enemies come near, a
//! Captain takes the army through the battle on a look that holds only
//! that army and the enemies about it, so the battle layers play exactly
//! as they do in a Field Battle.

use gc_sim::economy::SUPPLY_LIMIT;
use gc_sim::structures::{Role, StructureHandle};
use gc_sim::{PlayerId, RegimentHandle, TICKS_PER_SECOND};
use glam::Vec2;

use super::Sense;
use crate::captain::{Captain, Ctx};
use crate::orders::{Act, Orders, ROUTINE, Want};
use crate::perception::{Perception, Seen, right_of, v2};

/// Ticks between the Marshal's looks.
const THINK_TICKS: u32 = TICKS_PER_SECOND;
/// Enemies within this many tiles of the army start a battle.
const ENGAGE: f32 = 36.0;
/// A battle takes in everyone within this many tiles of the army's middle.
const BATTLE_REACH: f32 = 60.0;
/// A battle ends after this many ticks with no enemy standing near.
const QUIET_TICKS: u32 = 8 * TICKS_PER_SECOND;
/// Enemies within this many tiles of its buildings are a threat to home.
const THREAT: f32 = 30.0;
/// The rally point stands this far from home towards the enemy.
const RALLY_OUT: f32 = 16.0;
/// Within this many tiles of its target the army goes for the buildings.
const AT_TARGET: f32 = 20.0;
/// A start counts as looked at once a regiment comes this near it.
const SCOUTED: f32 = 14.0;
/// Tiles between regiments marching abreast.
const ABREAST: f32 = 7.0;
/// An attack falls back once the army is down to this share of the
/// soldiers it set out with.
const FALL_BACK_SHARE: f32 = 0.35;
/// Reinforcements at the rally point set out to join the attack once there
/// are this many regiments.
const REINFORCE: usize = 3;
/// Supply in fighters it attacks with at the start, what it adds each
/// minute, and the most it waits for.
const FIRST_WAVE: f32 = 16.0;
const PER_MINUTE: f32 = 2.5;
const BIGGEST_WAVE: f32 = 70.0;

/// What the army is for now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    /// Gathering at the rally point.
    Gather,
    /// Going to meet enemies at its buildings.
    Defend(Vec2),
    /// Marching on an enemy building, or to look at a start.
    Attack(Vec2),
}

impl Goal {
    pub fn name(&self) -> &'static str {
        match self {
            Goal::Gather => "gathering",
            Goal::Defend(_) => "defending",
            Goal::Attack(_) => "attacking",
        }
    }
}

/// A fight under way: a Captain and the look it fights on.
struct Battle {
    captain: Captain,
    view: Perception,
    quiet_since: Option<u32>,
}

pub struct Marshal {
    next_think: u32,
    pub goal: Goal,
    /// Where new regiments gather.
    pub rally: Option<Vec2>,
    /// The regiments out on the attack (the rest wait at the rally point).
    out: Vec<RegimentHandle>,
    /// Soldiers it set out with.
    set_out_with: u32,
    /// Starts looked at and found empty, by index.
    scouted: Vec<usize>,
    battle: Option<Battle>,
    orders: Orders,
    wants: Vec<Want>,
}

/// What the Marshal needs from the mind besides the field.
pub struct Command<'a, 'b> {
    pub captain: Ctx<'a>,
    pub sense: &'b Sense<'b>,
}

impl Marshal {
    pub fn new(party: PlayerId, reaction_ticks: u32, orders_per_minute: u32) -> Marshal {
        Marshal {
            next_think: 0,
            goal: Goal::Gather,
            rally: None,
            out: Vec::new(),
            set_out_with: 0,
            scouted: Vec::new(),
            battle: None,
            orders: Orders::new(party, reaction_ticks, orders_per_minute),
            wants: Vec::new(),
        }
    }

    /// Whether a battle is under way.
    pub fn fighting(&self) -> bool {
        self.battle.is_some()
    }

    /// The Captain of the battle under way.
    pub fn captain(&self) -> Option<&Captain> {
        self.battle.as_ref().map(|b| &b.captain)
    }

    /// Army orders sent so far.
    pub fn orders(&self) -> &Orders {
        &self.orders
    }

    /// Look, decide, and append the army's orders due now to `out`.
    pub fn tick(
        &mut self,
        c: &mut Command,
        home: Option<Vec2>,
        out: &mut Vec<gc_sim::PlayerCommand>,
    ) {
        let s = c.sense;
        let party = s.party;
        let view = s.view;
        if self.rally.is_none()
            && let Some(home) = home
        {
            self.rally = Some(rally_point(s, home));
        }
        let army: Vec<&Seen> = view
            .regiments
            .iter()
            .filter(|r| r.owner == party && s.kit.is_fighter(r.unit) && !r.broken())
            .collect();
        self.out.retain(|h| army.iter().any(|r| r.handle == *h));
        if !army.is_empty() {
            self.battle(c, &army);
        }
        if s.now >= self.next_think && self.battle.is_none() {
            self.next_think = s.now + THINK_TICKS;
            self.wants.clear();
            self.decide(c, &army, home);
            self.orders.decide(view, &mut self.wants);
        }
        self.orders.release(s.now, out);
    }

    /// Start, run or end a battle round the army.
    fn battle(&mut self, c: &mut Command, army: &[&Seen]) {
        let s = c.sense;
        let view = s.view;
        let party = s.party;
        let near = |p: Vec2, reach: f32| army.iter().any(|r| r.pos.distance(p) <= reach);
        let foe_near = view
            .regiments
            .iter()
            .any(|r| is_foe(s, r) && !r.broken() && near(r.pos, ENGAGE));
        if self.battle.is_none() {
            if !foe_near {
                return;
            }
            c.captain
                .journal
                .note(s.now, "Enemies close: forming for battle".to_string());
            self.battle = Some(Battle {
                captain: Captain::new(),
                view: Perception::new(),
                quiet_since: None,
            });
        }
        let Some(b) = self.battle.as_mut() else {
            return;
        };
        if foe_near {
            b.quiet_since = None;
        } else if b.quiet_since.is_none() {
            b.quiet_since = Some(s.now);
        }
        if b.quiet_since.is_some_and(|t| s.now >= t + QUIET_TICKS) {
            c.captain
                .journal
                .note(s.now, "No enemy near: marching on".to_string());
            self.battle = None;
            self.next_think = 0;
            return;
        }
        let mid = centroid(army);
        // The army, and whoever else stands about it. Regiments at the
        // rally point fight with it once it comes home.
        view.subset(&mut b.view, |r| {
            if r.owner == party {
                s.kit.is_fighter(r.unit) && r.pos.distance(mid) <= BATTLE_REACH
            } else {
                !s.kit.all_workers.contains(&r.unit) && r.pos.distance(mid) <= BATTLE_REACH
            }
        });
        if s.now < self.next_think {
            return;
        }
        self.next_think = s.now + c.captain.knobs.think_ticks.max(1);
        self.wants.clear();
        let mut ctx = Ctx {
            view: &b.view,
            terrain: c.captain.terrain,
            party,
            knobs: c.captain.knobs,
            persona: c.captain.persona,
            rng: &mut *c.captain.rng,
            journal: &mut *c.captain.journal,
            next_slip: &mut *c.captain.next_slip,
        };
        b.captain.think(&mut ctx, &mut self.wants);
        self.orders.decide(&b.view, &mut self.wants);
    }

    /// Choose the army's goal and march it there.
    fn decide(&mut self, c: &mut Command, army: &[&Seen], home: Option<Vec2>) {
        let s = c.sense;
        let Some(rally) = self.rally else { return };
        // Home first.
        if let Some(threat) = threat_to_home(s) {
            if !matches!(self.goal, Goal::Defend(_)) {
                c.captain.journal.note(
                    s.now,
                    "Enemies at our buildings: going to defend".to_string(),
                );
            }
            self.goal = Goal::Defend(threat);
            self.out = army.iter().map(|r| r.handle).collect();
        } else if matches!(self.goal, Goal::Defend(_)) {
            self.goal = Goal::Gather;
            self.out.clear();
        }
        let soldiers = |hs: &[RegimentHandle]| -> u32 {
            hs.iter()
                .filter_map(|h| s.view.get(*h))
                .map(|r| u32::from(r.models))
                .sum()
        };
        match self.goal {
            Goal::Gather => {
                let supply: f32 = army
                    .iter()
                    .filter_map(|r| s.kit.unit(r.unit))
                    .map(|u| f32::from(u.unit.supply))
                    .sum();
                let minutes = s.now as f32 / (60.0 * TICKS_PER_SECOND as f32);
                let boldness = 1.3 - 0.6 * c.captain.persona.aggression();
                let wave = ((FIRST_WAVE + PER_MINUTE * minutes) * boldness).min(BIGGEST_WAVE);
                let capped = s
                    .world
                    .bank(s.party)
                    .is_some_and(|b| b.supply_used + 8 >= b.supply_cap.clamp(1, SUPPLY_LIMIT));
                if (supply >= wave || (capped && supply >= FIRST_WAVE))
                    && let Some(target) = self.target(s, centroid(army))
                {
                    self.out = army.iter().map(|r| r.handle).collect();
                    self.set_out_with = soldiers(&self.out);
                    self.goal = Goal::Attack(target);
                    c.captain.journal.note(
                        s.now,
                        format!(
                            "Attacking with {} regiments ({supply:.0} supply)",
                            army.len()
                        ),
                    );
                }
            }
            Goal::Attack(_) => {
                let left = soldiers(&self.out);
                if (left as f32) < FALL_BACK_SHARE * self.set_out_with as f32 {
                    c.captain.journal.note(
                        s.now,
                        "The attack has cost too much: falling back".to_string(),
                    );
                    self.goal = Goal::Gather;
                    self.out.clear();
                } else {
                    let mid = centroid_of(s.view, &self.out).unwrap_or(rally);
                    match self.target(s, mid) {
                        Some(t) => self.goal = Goal::Attack(t),
                        None => {
                            c.captain
                                .journal
                                .note(s.now, "Nothing left to find: coming home".to_string());
                            self.goal = Goal::Gather;
                            self.out.clear();
                            self.scouted.clear();
                        }
                    }
                }
            }
            Goal::Defend(_) => {}
        }
        // Those out march on the goal; the rest wait at the rally point,
        // and join an attack once there are enough of them.
        let waiting: Vec<&Seen> = army
            .iter()
            .copied()
            .filter(|r| !self.out.contains(&r.handle))
            .collect();
        if let Goal::Attack(_) = self.goal
            && waiting.len() >= REINFORCE
            && waiting.iter().all(|r| r.pos.distance(rally) <= 20.0)
        {
            self.out.extend(waiting.iter().map(|r| r.handle));
            self.set_out_with += waiting.iter().map(|r| u32::from(r.models)).sum::<u32>();
        }
        let (going, staying): (Vec<&Seen>, Vec<&Seen>) =
            army.iter().partition(|r| self.out.contains(&r.handle));
        let face = home.map_or(Vec2::Y, |h| (rally - h).normalize_or(Vec2::Y));
        march(s, &staying, rally, face, &mut self.wants, 1);
        match self.goal {
            Goal::Gather => {}
            Goal::Defend(at) => {
                let mid = centroid(&going);
                march(
                    s,
                    &going,
                    at,
                    (at - mid).normalize_or(face),
                    &mut self.wants,
                    2,
                );
            }
            Goal::Attack(at) => {
                let mid = centroid(&going);
                if mid.distance(at) > AT_TARGET {
                    march(
                        s,
                        &going,
                        at,
                        (at - mid).normalize_or(face),
                        &mut self.wants,
                        3,
                    );
                } else {
                    self.knock_down(s, &going, at);
                }
            }
        }
    }

    /// Where to attack: the enemy building it knows of nearest `from`,
    /// else the nearest enemy start it hasn't looked at.
    fn target(&mut self, s: &Sense, from: Vec2) -> Option<Vec2> {
        let seen = enemy_buildings(s);
        if let Some(b) = seen
            .iter()
            .min_by(|a, b| a.1.distance(from).total_cmp(&b.1.distance(from)))
        {
            return Some(b.1);
        }
        let home = s.view.regiments.iter().filter(|r| r.owner == s.party);
        // Look at starts as regiments pass them.
        for (i, &st) in s.kit.starts.iter().enumerate() {
            if !self.scouted.contains(&i) && home.clone().any(|r| r.pos.distance(st) <= SCOUTED) {
                self.scouted.push(i);
            }
        }
        s.kit
            .starts
            .iter()
            .enumerate()
            .filter(|(i, _)| !self.scouted.contains(i))
            .map(|(_, &p)| p)
            .min_by(|a, b| a.distance(from).total_cmp(&b.distance(from)))
    }

    /// At the target: every regiment goes for one of the nearest enemy
    /// buildings, towers first, spread over the few nearest.
    fn knock_down(&mut self, s: &Sense, going: &[&Seen], at: Vec2) {
        let mut targets = enemy_buildings(s);
        targets.retain(|t| t.1.distance(at) <= AT_TARGET * 2.0);
        if targets.is_empty() {
            return;
        }
        targets.sort_by(|a, b| {
            let key =
                |t: &(StructureHandle, Vec2, Role)| (t.2 != Role::Defence, t.1.distance(at) as i32);
            key(a).cmp(&key(b))
        });
        targets.truncate(3);
        for (k, r) in going.iter().enumerate() {
            if r.engaged {
                continue;
            }
            // Keep at one it has already been sent to while it stands.
            let current = s.world.siege_target(r.handle);
            let t = match current.filter(|h| targets.iter().any(|t| t.0 == *h)) {
                Some(h) => h,
                None => targets[k % targets.len()].0,
            };
            self.wants.push(Want {
                regiment: r.handle,
                act: Act::Siege(t),
                priority: 2,
                group: 0,
            });
        }
    }
}

/// An enemy regiment worth fighting: not a worker.
fn is_foe(s: &Sense, r: &Seen) -> bool {
    s.view.hostile(s.party, r.owner) && !s.kit.all_workers.contains(&r.unit)
}

/// The rally point: in front of home, towards the nearest other start.
fn rally_point(s: &Sense, home: Vec2) -> Vec2 {
    let toward = s
        .kit
        .starts
        .iter()
        .filter(|p| p.distance(home) > 30.0)
        .min_by(|a, b| a.distance(home).total_cmp(&b.distance(home)))
        .copied()
        .unwrap_or(home + Vec2::Y);
    let p = home + (toward - home).normalize_or(Vec2::Y) * RALLY_OUT;
    let tiles = s.tiles;
    p.clamp(Vec2::splat(4.0), Vec2::splat(tiles - 4.0))
}

/// Every enemy building its side has seen and still stands, with its role.
fn enemy_buildings(s: &Sense) -> Vec<(StructureHandle, Vec2, Role)> {
    let Some(st) = s.world.structures() else {
        return Vec::new();
    };
    let table = st.buildings();
    st.iter()
        .filter(|v| s.view.hostile(s.party, v.owner) && v.seen_by(s.party))
        .filter_map(|v| {
            let role = table.get(usize::from(v.building))?.role;
            Some((v.handle, v2(v.centre()), role))
        })
        .collect()
}

/// The middle of the enemies it sees near its buildings, if any.
fn threat_to_home(s: &Sense) -> Option<Vec2> {
    let st = s.world.structures()?;
    let mine: Vec<Vec2> = st
        .iter()
        .filter(|v| v.owner == s.party)
        .map(|v| v2(v.centre()))
        .collect();
    let near: Vec<&Seen> = s
        .view
        .regiments
        .iter()
        .filter(|r| is_foe(s, r) && !r.broken())
        .filter(|r| mine.iter().any(|b| b.distance(r.pos) <= THREAT))
        .collect();
    (!near.is_empty()).then(|| centroid(&near))
}

/// March `regiments` abreast to `to`, facing `face`, at the pace of the
/// slowest.
fn march(s: &Sense, regiments: &[&Seen], to: Vec2, face: Vec2, wants: &mut Vec<Want>, group: u32) {
    if regiments.is_empty() {
        return;
    }
    let pace = regiments
        .iter()
        .map(|r| s.view.pace(r))
        .fold(f32::MAX, f32::min);
    let right = right_of(face);
    let n = regiments.len() as f32;
    // Keep each regiment's place in the line by how far right it stands.
    let mut order: Vec<&&Seen> = regiments.iter().collect();
    order.sort_by(|a, b| a.pos.dot(right).total_cmp(&b.pos.dot(right)));
    let tiles = s.tiles;
    for (i, r) in order.into_iter().enumerate() {
        let across = (i as f32 - (n - 1.0) / 2.0) * ABREAST;
        let spot = (to + right * across).clamp(Vec2::splat(2.0), Vec2::splat(tiles - 2.0));
        wants.push(Want {
            regiment: r.handle,
            act: Act::Move {
                to: spot,
                facing: Some(face),
                pace,
            },
            priority: ROUTINE,
            group: u32::MAX - group,
        });
    }
}

fn centroid(rs: &[&Seen]) -> Vec2 {
    if rs.is_empty() {
        return Vec2::ZERO;
    }
    rs.iter().map(|r| r.pos).sum::<Vec2>() / rs.len() as f32
}

fn centroid_of(view: &Perception, hs: &[RegimentHandle]) -> Option<Vec2> {
    let rs: Vec<&Seen> = hs.iter().filter_map(|h| view.get(*h)).collect();
    (!rs.is_empty()).then(|| centroid(&rs))
}
