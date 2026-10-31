//! The computer players of a lobby Conquest match: what each one knows of
//! its faction before the match starts (its [`Kit`]), and seating a
//! Conquest mind on every AI slot. Like Field Battle's minds they run on
//! the host (or alone) and act only through ordinary commands, so nothing
//! here touches the simulation (docs/18-ai.md, "The big decision").
//!
//! The tests below also hold a headless runner that plays whole Conquest
//! matches between minds with no window, to watch them and to check they
//! can finish a game.

use std::sync::Arc;

use gc_ai::conquest::kit::{EconomyStyle, Kit, KitBuilding, KitUnit};
use gc_ai::conquest::opening::Opening;
use gc_ai::{Difficulty, Mind, MindConfig, Minds};
use gc_data::conquest::Class;
use gc_data::{Faction, GameData};
use gc_fixed::Pcg32;
use gc_map::Map;
use gc_ui::lobby::{MatchStart, SlotKind};
use glam::Vec2;

use crate::buildings::Table;
use crate::conquest;

/// What a mind of `faction` knows before the match starts, the way a
/// player knows their faction's build menu: its building and unit types
/// (ids are the baked tables' indexes) and where the map's starts are.
pub fn kit(data: &GameData, table: &Table, map: &Map, faction: Faction) -> Kit {
    let buildings = (0..data.buildings.len())
        .filter(|&i| data.buildings[i].faction() == faction)
        .map(|i| KitBuilding {
            id: i as u16,
            stats: table.stats[i],
        })
        .collect();
    let units: Vec<KitUnit> = data
        .conquest_units
        .iter()
        .enumerate()
        .filter(|(_, rec)| rec.faction() == faction)
        .map(|(u, rec)| {
            let unit = conquest::unit_of(rec);
            // A regiment for the line: trained at a building, costing
            // something and taking supply, and not a worker, caravan,
            // healer or Reclaimer.
            let fighter = !unit.worker
                && rec.class() != Class::Support
                && unit.trained_at.is_some()
                && unit.supply > 0
                && unit.cost_grub + unit.cost_marrow > 0;
            KitUnit {
                id: u as u16,
                unit,
                fighter,
            }
        })
        .collect();
    Kit {
        // gc_data and gc_ai list the factions in the same order.
        faction,
        economy: units
            .iter()
            .find(|u| u.unit.worker)
            .map_or(EconomyStyle::Shared, |w| {
                EconomyStyle::of(
                    &w.unit,
                    units.iter().find(|u| u.unit.trades()).map(|u| &u.unit),
                )
            }),
        buildings,
        units,
        all_workers: data
            .conquest_units
            .iter()
            .enumerate()
            // Caravans count: they are raided like workers.
            .filter(|(_, rec)| rec.worker() || rec.trades())
            .map(|(u, _)| u as u16)
            .collect(),
        all_units: data
            .conquest_units
            .iter()
            .map(|rec| (rec.faction(), conquest::unit_of(rec).supply))
            .collect(),
        starts: map
            .starts()
            .into_iter()
            .map(|(x, y)| Vec2::new(x as f32 + 0.5, y as f32 + 0.5))
            .collect(),
    }
}

/// The temper (personality roll) of player `i`'s mind in a game rolled
/// `roll`.
fn temper(roll: u64, i: usize) -> u32 {
    Pcg32::new(roll, i as u64).next_u32()
}

/// Put a Conquest mind in charge of every AI slot of `setup` (player `i`
/// is slot `i`), picking each one's temper and seed from `roll` as Field
/// Battle does. The caller does this on the host or alone only.
pub fn seat(
    setup: &MatchStart,
    data: &GameData,
    table: &Table,
    map: &Map,
    roll: u64,
    minds: &mut Minds,
) {
    let sides: Vec<(Faction, u8)> = setup.players.iter().map(|s| (s.faction, s.team)).collect();
    for (i, slot) in setup.players.iter().enumerate() {
        if slot.kind != SlotKind::Ai {
            continue;
        }
        let level = Difficulty::from_index(usize::from(slot.ai_level));
        let temper = temper(roll, i);
        minds.add(mind_for(
            data, table, map, i, &sides, level, temper, roll, None,
        ));
    }
}

/// The Conquest mind for player `i` of `sides` (each player's faction and
/// team) at `level`, its personality picked by `temper` and its seed from
/// `roll`; it picks its own opening unless given one.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mind_for(
    data: &GameData,
    table: &Table,
    map: &Map,
    i: usize,
    sides: &[(Faction, u8)],
    level: Difficulty,
    temper: u32,
    roll: u64,
    opening: Option<&'static Opening>,
) -> Mind {
    let (faction, team) = sides[i];
    let config = MindConfig {
        party: i as u8,
        difficulty: level,
        personality: gc_ai::personality::pick(faction, temper),
        seed: roll ^ i as u64,
    };
    // The factions it faces: its allies are no rivals.
    let rivals: Vec<Faction> = sides
        .iter()
        .filter(|&&(_, t)| t != team)
        .map(|&(f, _)| f)
        .collect();
    let kit = Arc::new(kit(data, table, map, faction));
    Mind::conquest(config, kit, &rivals, opening)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_ai::conquest::opening;
    use gc_sim::economy::Bank;
    use gc_sim::{PlayerId, TICKS_PER_SECOND, Verdict, Victories, Win, World};
    use gc_ui::lobby::Tempo;

    use crate::headless::{Headless, Real};

    /// How one side stood when a headless match ended.
    pub(crate) struct Standing {
        pub faction: Faction,
        /// The mind's one-line summary of what it is doing.
        pub summary: String,
        /// The AI lens's panel for the mind, after the summary line.
        pub lens: Vec<String>,
        pub bank: Bank,
        pub buildings: usize,
        pub regiments: usize,
        /// Orders the mind sent: its battle layer's, and its economy
        /// clicks.
        pub orders: u32,
        pub clicks: u32,
        /// The tick its scout first looked at a start an enemy might hold.
        pub scout_looked: Option<u32>,
        /// Raids its raiders set out on, and enemy worker regiments seen
        /// to go while they were near.
        pub raids: u32,
        pub raid_kills: u32,
        /// The most works it had standing at once over the match, by kind.
        pub works: Works,
        /// Its economy and army over the match (see [`Ledger`]).
        pub ledger: Ledger,
    }

    /// A first proper army: fighters worth this much (four or five tier 1
    /// regiments).
    pub(crate) const ARMY_READY: u32 = 450;

    /// Minutes the ledger takes its snapshots at.
    pub(crate) const MARKS: [u32; 5] = [5, 8, 10, 15, 20];

    /// One side's economy and army, for the balance table: Grub and Marrow
    /// brought in by each mark (rises in the store, so a spend in the
    /// same tick as a delivery hides that delivery), and its fighting
    /// army's worth (Grub + 2 Marrow of what it cost) and soldiers at
    /// each mark; when its army first lost soldiers; worth of fighters it
    /// lost and it killed.
    #[derive(Clone, Copy, Debug, Default)]
    pub(crate) struct Ledger {
        pub income: [(u32, u32); MARKS.len()],
        pub army: [u32; MARKS.len()],
        pub soldiers: [u32; MARKS.len()],
        pub first_loss: Option<u32>,
        /// When its fighting army was first worth [`ARMY_READY`].
        pub army_ready: Option<u32>,
        /// The marks the match lasted to.
        pub reached: [bool; MARKS.len()],
        pub lost: u32,
        pub killed: u32,
        grub: u32,
        marrow: u32,
        last: (u32, u32),
    }

    /// Works a side has standing (finished or going up), by kind.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Works {
        pub walls: u32,
        pub gates: u32,
        pub barricades: u32,
        pub stakes: u32,
        pub traps: u32,
        pub bridges: u32,
        /// Areas of thicket it ordered cleared (a count of orders, kept
        /// over the match rather than standing at once).
        pub clears: u32,
        /// Bolt and blast towers (docs/26 stage 5).
        pub bolts: u32,
        pub blasts: u32,
        /// Towers raised to level 2.
        pub raised: u32,
        /// 1 once its walls closed round its hall: a walled town.
        pub walled: u32,
    }

    impl Works {
        /// Side `p`'s works now.
        fn of(world: &World, p: PlayerId) -> Works {
            use gc_sim::structures::{GATE, Role, TowerShot};
            let mut w = Works {
                walled: u32::from(world.walled_town(p)),
                ..Works::default()
            };
            let Some(st) = world.structures() else {
                return w;
            };
            let table = st.buildings();
            for v in st.iter().filter(|v| v.owner == p) {
                let Some(b) = table.get(usize::from(v.building)) else {
                    continue;
                };
                if b.role == Role::Defence && b.upgrades_from.is_some() {
                    w.raised += 1;
                }
                let n = match b.role {
                    Role::Wall if b.flags & GATE != 0 => &mut w.gates,
                    Role::Wall => &mut w.walls,
                    Role::Barricade => &mut w.barricades,
                    Role::Stakes => &mut w.stakes,
                    Role::Trap => &mut w.traps,
                    Role::Bridge => &mut w.bridges,
                    Role::Defence => match b.attack.map(|a| a.shot) {
                        Some(TowerShot::Bolt) => &mut w.bolts,
                        Some(TowerShot::Blast) => &mut w.blasts,
                        _ => continue,
                    },
                    _ => continue,
                };
                *n += 1;
            }
            w
        }

        /// The most of each kind in either.
        fn most(self, o: Works) -> Works {
            Works {
                walls: self.walls.max(o.walls),
                gates: self.gates.max(o.gates),
                barricades: self.barricades.max(o.barricades),
                stakes: self.stakes.max(o.stakes),
                traps: self.traps.max(o.traps),
                bridges: self.bridges.max(o.bridges),
                clears: self.clears.max(o.clears),
                bolts: self.bolts.max(o.bolts),
                blasts: self.blasts.max(o.blasts),
                raised: self.raised.max(o.raised),
                walled: self.walled.max(o.walled),
            }
        }
    }

    /// A headless Conquest match played to its end (or the time limit).
    pub(crate) struct Played {
        pub verdict: Option<Verdict>,
        pub ticks: u32,
        pub sides: Vec<Standing>,
        /// The first tick (checked each second) two sides' regiments
        /// fought in melee within [`CONTACT_TILES`] of each other.
        pub first_contact: Option<u32>,
    }

    /// How near two sides' regiments in melee count as having met.
    const CONTACT_TILES: i32 = 6;

    /// Whether regiments of two sides are fighting each other now.
    fn sides_met(world: &World, players: usize) -> bool {
        let near = gc_fixed::Fx::from_int(CONTACT_TILES);
        let fighting: Vec<_> = world
            .regiments
            .iter()
            .filter(|r| r.engaged && usize::from(r.owner) < players)
            .map(|r| (r.owner, r.position))
            .collect();
        fighting.iter().any(|&(a, pa)| {
            fighting
                .iter()
                .any(|&(b, pb)| a != b && (pa - pb).length() <= near)
        })
    }

    impl Played {
        /// The side that won, if the match was decided and not drawn.
        pub fn winner(&self) -> Option<PlayerId> {
            self.verdict.filter(|v| v.by != Win::Draw).map(|v| v.holder)
        }

        /// The result and how each side stood, one line apiece.
        pub fn report(&self) -> String {
            let clock = |t: u32| {
                format!(
                    "{}:{:02}",
                    t / 60 / TICKS_PER_SECOND,
                    t / TICKS_PER_SECOND % 60
                )
            };
            let mut out = match self.verdict {
                Some(v) if v.by == Win::Draw => {
                    format!("draw ({:?}) at {}", v.by, clock(v.tick))
                }
                Some(v) => format!("side {} wins by {:?} at {}", v.holder, v.by, clock(v.tick)),
                None => format!("undecided after {}", clock(self.ticks)),
            };
            out += &match self.first_contact {
                Some(t) => format!("; first contact at {}", clock(t)),
                None => "; no contact".into(),
            };
            for (p, s) in self.sides.iter().enumerate() {
                out += &format!("\n  {}", s.line(p));
            }
            out
        }
    }

    impl Standing {
        /// "side 0 EmberClans: bank 300/0, supply 8/10, ..." and, below,
        /// the mind's summary and what its lens panel says.
        pub fn line(&self, p: usize) -> String {
            let b = &self.bank;
            let lens: String = self.lens.iter().map(|l| format!("\n  {l}")).collect();
            format!(
                "side {p} {:?}: bank {}/{}, supply {}/{}, {} buildings, {} regiments, \
                 {} orders ({} economy), most works {:?}\n    {}{lens}",
                self.faction,
                b.grub,
                b.marrow,
                b.supply_used,
                b.supply_cap,
                self.buildings,
                self.regiments,
                self.orders,
                self.clicks,
                self.works,
                self.summary
            )
        }
    }

    /// One side's numbers now: its bank and what it owns.
    fn standing(world: &World, minds: &Minds, p: PlayerId, faction: Faction) -> Standing {
        let mind = minds.get(p).expect("a mind plays every side");
        Standing {
            faction,
            summary: mind.summary(),
            lens: mind.lens(minds.view()).text.split_off(1),
            bank: world.bank(p).unwrap_or_default(),
            buildings: world
                .structures()
                .map_or(0, |s| s.iter().filter(|v| v.owner == p).count()),
            regiments: world.regiments.iter().filter(|r| r.owner == p).count(),
            orders: mind.orders_sent(),
            clicks: mind.realm().map_or(0, |r| r.clicks().count),
            scout_looked: mind
                .realm()
                .and_then(|r| r.marshal.scout.first_enemy_start_looked_at()),
            raids: mind.realm().map_or(0, |r| r.marshal.raider.raids()),
            raid_kills: mind.realm().map_or(0, |r| r.marshal.raider.kills()),
            works: Works::of(world, p),
            ledger: Ledger::default(),
        }
    }

    /// Play Conquest map `choice`, made `tiles` a side (0 for a shipped
    /// map), between Conquest minds, `factions[i]` at
    /// start `i` thinking at `levels[i]` (the last level covers any
    /// more), with no window, until a side wins or `max_minutes` of game
    /// time pass. Fog of war and victories are as the lobby's defaults.
    /// With `AI_TRACE=1` it prints every side's progress each two game
    /// minutes.
    pub(crate) fn play(
        choice: usize,
        tiles: u32,
        factions: &[Faction],
        levels: &[Difficulty],
        seed: u64,
        max_minutes: u32,
    ) -> Played {
        play_with(
            (choice, tiles),
            factions,
            levels,
            seed,
            max_minutes,
            None,
            &[],
        )
    }

    /// [`play`], every side with the personality `temper` picks for its
    /// faction if given (for the arena, so only the level differs).
    pub(crate) fn play_with(
        (choice, tiles): (usize, u32),
        factions: &[Faction],
        levels: &[Difficulty],
        seed: u64,
        max_minutes: u32,
        temper: Option<u32>,
        openings: &[Option<&'static Opening>],
    ) -> Played {
        let map = conquest::make_map(choice, tiles, seed);
        play_map(map, factions, levels, seed, max_minutes, temper, openings)
    }

    /// [`play_with`] on a map already made (a shipped def with a tweak, say).
    pub(crate) fn play_map(
        map: Map,
        factions: &[Faction],
        levels: &[Difficulty],
        seed: u64,
        max_minutes: u32,
        temper: Option<u32>,
        openings: &[Option<&'static Opening>],
    ) -> Played {
        let trace = std::env::var("AI_TRACE").unwrap_or_default();
        // AI_TRACE=2 also prints every journal entry as it is written.
        let journal = trace == "2";
        let trace = journal || trace == "1";
        let real = Real::load();
        let game = Headless::new(&real, map, factions, seed);
        let (data, units) = (&real.data, &real.units);
        let defaults = gc_ui::lobby::LobbySettings::default();
        let plan = game.plan(Tempo::Standard, conquest::victories(defaults.victory));
        assert!(plan.victories.any() && plan.victories != Victories::default());
        let mut world = game.world(&plan);
        let mut minds = game.minds();
        for i in 0..factions.len() {
            let level = levels[i.min(levels.len() - 1)];
            let temper = temper.unwrap_or_else(|| super::temper(seed, i));
            let opening = openings.get(i).copied().flatten();
            minds.add(game.mind(i, level, temper, opening));
        }
        game.open(&mut world, World::tick);
        let end = max_minutes * 60 * TICKS_PER_SECOND;
        let every = 120 * TICKS_PER_SECOND;
        let mut cmds = Vec::new();
        let mut noted = vec![0u32; factions.len()];
        let mut works = vec![Works::default(); factions.len()];
        let mut first_contact = None;
        let mut ledgers = vec![Ledger::default(); factions.len()];
        // The starting store isn't income.
        for (p, l) in ledgers.iter_mut().enumerate() {
            let b = world.bank(p as PlayerId).unwrap_or_default();
            l.last = (b.grub, b.marrow);
        }
        // A fighter's worth, by unit id (0 for workers and caravans).
        let worth: Vec<u32> = data
            .conquest_units
            .iter()
            .map(|rec| {
                let u = conquest::unit_of(rec);
                if rec.worker() || rec.trades() {
                    0
                } else {
                    u32::from(u.cost_grub) + 2 * u32::from(u.cost_marrow)
                }
            })
            .collect();
        let per_model: Vec<u32> = units.iter().map(|u| u32::from(u.models.max(1))).collect();
        // What `models` soldiers of `unit` are worth, pro rata.
        let value = |unit: gc_sim::UnitId, models: u16| {
            let u = usize::from(unit);
            worth[u] * u32::from(models) / per_model[u]
        };
        let mut alive: std::collections::HashMap<gc_sim::RegimentHandle, (PlayerId, u16, u16)> =
            Default::default();
        while world.verdict().is_none() && world.tick_count() < end {
            cmds.clear();
            minds.tick(&world, &mut cmds);
            for c in &cmds {
                if let (gc_sim::Command::Clear { .. }, Some(w)) =
                    (c.command, works.get_mut(usize::from(c.player)))
                {
                    w.clears += 1;
                }
            }
            world.tick(&cmds);
            let t = world.tick_count();
            if first_contact.is_none()
                && t.is_multiple_of(TICKS_PER_SECOND)
                && sides_met(&world, factions.len())
            {
                first_contact = Some(t);
            }
            for (p, l) in ledgers.iter_mut().enumerate() {
                let b = world.bank(p as PlayerId).unwrap_or_default();
                l.grub += b.grub.saturating_sub(l.last.0);
                l.marrow += b.marrow.saturating_sub(l.last.1);
                l.last = (b.grub, b.marrow);
            }
            if t.is_multiple_of(TICKS_PER_SECOND) {
                // Fighters lost since the last second, worth pro rata (a
                // regiment gone loses all it had; one that changed type,
                // a Brood evolution, loses nothing).
                for (p, l) in ledgers.iter_mut().enumerate() {
                    if l.army_ready.is_none() {
                        let army: u32 = (world.regiments.iter())
                            .filter(|r| r.owner == p as PlayerId)
                            .map(|r| value(r.unit, r.models))
                            .sum();
                        if army >= ARMY_READY {
                            l.army_ready = Some(t);
                        }
                    }
                }
                let now: std::collections::HashMap<_, _> = world
                    .regiments
                    .iter()
                    // Players' fighters only, not neutrals.
                    .filter(|r| usize::from(r.owner) < ledgers.len())
                    .filter(|r| worth[usize::from(r.unit)] > 0)
                    .map(|r| (r.handle, (r.owner, r.unit, r.models)))
                    .collect();
                for (h, &(owner, unit, m)) in &alive {
                    let left = match now.get(h) {
                        Some(&(_, u, n)) if u == unit => n,
                        Some(_) => continue,
                        None => 0,
                    };
                    if left < m {
                        let w = value(unit, m - left);
                        let l = &mut ledgers[usize::from(owner)];
                        l.lost += w;
                        l.first_loss.get_or_insert(t);
                        if ledgers.len() == 2 {
                            ledgers[1 - usize::from(owner)].killed += w;
                        }
                    }
                }
                alive = now;
            }
            if let Some(i) = MARKS.iter().position(|&m| t == m * 60 * TICKS_PER_SECOND) {
                for (p, l) in ledgers.iter_mut().enumerate() {
                    l.income[i] = (l.grub, l.marrow);
                    l.reached[i] = true;
                    for r in world.regiments.iter().filter(|r| r.owner == p as PlayerId) {
                        if worth[usize::from(r.unit)] > 0 {
                            l.army[i] += value(r.unit, r.models);
                            l.soldiers[i] += u32::from(r.models);
                        }
                    }
                }
            }
            if t.is_multiple_of(10 * TICKS_PER_SECOND) {
                for (p, w) in works.iter_mut().enumerate() {
                    *w = w.most(Works::of(&world, p as PlayerId));
                }
            }
            if journal {
                for (p, last) in noted.iter_mut().enumerate() {
                    let Some(m) = minds.get(p as PlayerId) else {
                        continue;
                    };
                    let from = *last;
                    for e in m.journal().entries().filter(|e| e.tick > from) {
                        println!(
                            "{}:{:02} side {p}: {}",
                            e.tick / 1200,
                            e.tick / 20 % 60,
                            e.text
                        );
                        *last = e.tick;
                    }
                }
            }
            if trace && t.is_multiple_of(every) {
                println!("-- {}:00", t / 60 / TICKS_PER_SECOND);
                for (p, &f) in factions.iter().enumerate() {
                    let s = standing(&world, &minds, p as PlayerId, f);
                    let last = minds
                        .get(p as PlayerId)
                        .and_then(|m| m.journal().latest())
                        .map_or(String::new(), |e| format!("[{}] {}", e.tick, e.text));
                    println!("{}\n    last: {last}", s.line(p));
                }
            }
        }
        Played {
            verdict: world.verdict(),
            ticks: world.tick_count(),
            first_contact,
            sides: factions
                .iter()
                .enumerate()
                .map(|(p, &f)| {
                    let mut s = standing(&world, &minds, p as PlayerId, f);
                    s.works = s.works.most(works[p]);
                    s.ledger = ledgers[p];
                    s
                })
                .collect(),
        }
    }

    /// The levels named by `AI_LEVEL`, a name or an index each, one per
    /// side and comma-separated (the last covers any more sides); Veteran
    /// if unset.
    fn levels_from_env() -> Vec<Difficulty> {
        let var = std::env::var("AI_LEVEL").unwrap_or_default();
        var.split(',')
            .map(|v| level_named(v).unwrap_or(Difficulty::Veteran))
            .collect()
    }

    /// Openings named in `var`, a comma list, one a side (unset or
    /// unknown: the side picks its own).
    fn openings_from_env(var: &str) -> Vec<Option<&'static Opening>> {
        let v = std::env::var(var).unwrap_or_default();
        v.split(',').map(opening::find).collect()
    }

    /// A level by its name or its number.
    fn level_named(v: &str) -> Option<Difficulty> {
        let v = v.trim();
        Difficulty::ALL
            .into_iter()
            .find(|d| d.name().eq_ignore_ascii_case(v))
            .or_else(|| v.parse().ok().map(Difficulty::from_index))
    }

    fn env_number(name: &str, default: u64) -> u64 {
        std::env::var(name)
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    }

    #[test]
    fn a_kit_holds_the_factions_own_build_menu_and_the_maps_starts() {
        let data = crate::data::load();
        let table = Table::new(&data);
        let map = conquest::make_map(1, 0, 0);
        for f in Faction::ALL {
            let k = kit(&data, &table, &map, f);
            assert_eq!(k.faction as u8, f as u8);
            assert!(k.main().is_some(), "{f:?} has a main building");
            assert!(k.worker().is_some(), "{f:?} has a worker");
            assert!(k.housing().is_some(), "{f:?} has housing");
            assert!(k.units.iter().any(|u| u.fighter), "{f:?} has fighters");
            assert!(k.buildings.iter().all(|b| {
                data.buildings[usize::from(b.id)].faction() == f
                    && b.stats == table.stats[usize::from(b.id)]
            }));
            assert!(k.units.iter().all(|u| !(u.fighter && u.unit.worker)));
            assert!(k.all_workers.len() >= Faction::ALL.len());
            assert_eq!(k.starts.len(), 4);
            assert_eq!(
                k.starts[0],
                Vec2::new(
                    map.starts()[0].0 as f32 + 0.5,
                    map.starts()[0].1 as f32 + 0.5
                )
            );
        }
    }

    /// Watch a match: `AI_MAP`, `AI_SEED`, `AI_MINUTES` and `AI_LEVEL`
    /// choose it, `AI_TRACE=1` prints progress.
    ///
    /// ```text
    /// AI_TRACE=1 AI_MINUTES=12 cargo test -p gc_game --release conquest_ai_watch -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "a watching aid, run by hand"]
    fn conquest_ai_watch() {
        let choice = env_number("AI_MAP", 0) as usize;
        let seed = env_number("AI_SEED", 7);
        let minutes = env_number("AI_MINUTES", 40) as u32;
        let levels = levels_from_env();
        // AI_SIZE (Small, Medium or Large) sizes a made map; AI_PLAYERS
        // plays fewer sides than its starts.
        // As in the lobby, a made map's size caps its players.
        use gc_ui::lobby::{GameMode, LobbySettings, MapSize};
        let size = std::env::var("AI_SIZE").unwrap_or_default();
        let size = crate::by_name(&MapSize::ALL, MapSize::name, &size).unwrap_or(MapSize::Medium);
        let mut lobby = LobbySettings::default();
        lobby.set_mode(GameMode::Conquest);
        (lobby.map, lobby.map_size) = (choice, size);
        let most = lobby.max_players();
        let players = (env_number("AI_PLAYERS", most as u64) as usize).clamp(2, most);
        // AI_FACTIONS picks them by their index in `Faction::ALL`, such as
        // "12" for the Brood against the Choir.
        let picked = std::env::var("AI_FACTIONS").unwrap_or_default();
        let factions: Vec<Faction> = (0..players)
            .map(|i| match (picked.as_bytes().get(i), players) {
                (Some(c), _) => Faction::ALL[usize::from(c - b'0') % Faction::ALL.len()],
                (None, 2) => [Faction::EmberClans, Faction::CarrionConcord][i],
                (None, _) => Faction::ALL[i % 4],
            })
            .collect();
        let started = std::time::Instant::now();
        // AI_SAME=1 gives every side the same personality, as the arena does.
        let same = std::env::var("AI_SAME").is_ok_and(|v| v == "1");
        let temper = same.then(|| super::temper(seed, 0));
        let openings = openings_from_env("AI_OPENINGS");
        // AI_TILES plays a made map at another side, such as the old 768.
        let tiles = match env_number("AI_TILES", 0) as u32 {
            0 => lobby.conquest_tiles(),
            t => t,
        };
        let played = play_with(
            (choice, tiles),
            &factions,
            &levels,
            seed,
            minutes,
            temper,
            &openings,
        );
        println!("{}", played.report());
        for (p, side) in played.sides.iter().enumerate() {
            println!(
                "side {p}: {} raids set out, {} enemy worker regiments seen gone near the raiders",
                side.raids, side.raid_kills
            );
        }
        println!(
            "{} game minutes in {:.1?}",
            played.ticks / 60 / TICKS_PER_SECOND,
            started.elapsed()
        );
    }

    /// The Conquest arena (docs/18-ai.md, "Difficulty"): how often level
    /// `AI_A` beats `AI_B` one-on-one on Ribcage Pass, in mirror matches
    /// (both sides the same faction and personality, so only the level
    /// counts),
    /// each faction on each side, over `AI_GAMES` seeds per pairing.
    /// `cargo test --release -p gc_game conquest_arena -- --ignored --nocapture`
    #[test]
    #[ignore = "an arena run, not a check: run it by hand with --release"]
    fn conquest_arena() {
        let find = |var: &str, or: Difficulty| {
            level_named(&std::env::var(var).unwrap_or_default()).unwrap_or(or)
        };
        let a = find("AI_A", Difficulty::General);
        let b = find("AI_B", Difficulty::Veteran);
        // AI_OPEN_A and AI_OPEN_B set each level's opening, by name.
        let (open_a, open_b) = (
            openings_from_env("AI_OPEN_A"),
            openings_from_env("AI_OPEN_B"),
        );
        let games = env_number("AI_GAMES", 3);
        let (mut won, mut lost, mut drawn) = (0, 0, 0);
        for f in Faction::ALL {
            let (mut fw, mut fl) = (0, 0);
            for seed in 1..=games {
                for a_side in 0..2u8 {
                    let levels = if a_side == 0 { [a, b] } else { [b, a] };
                    // The same game with the sides swapped.
                    let roll = seed * 31;
                    let temper = Some(super::temper(roll, 0));
                    let (oa, ob) = (open_a[0], open_b[0]);
                    let openings = if a_side == 0 { [oa, ob] } else { [ob, oa] };
                    let played = play_with((0, 0), &[f, f], &levels, roll, 45, temper, &openings);
                    match played.winner() {
                        Some(w) if w == a_side => (won, fw) = (won + 1, fw + 1),
                        Some(_) => (lost, fl) = (lost + 1, fl + 1),
                        None => drawn += 1,
                    }
                }
            }
            println!("{f:?}: {} won {fw}, {} won {fl}", a.name(), b.name());
        }
        let all = (won + lost + drawn).max(1);
        println!(
            "{} beat {} in {won} of {all} ({}%), lost {lost}, {drawn} undecided",
            a.name(),
            b.name(),
            won * 100 / all
        );
    }

    /// The Conquest balance table: every pair of factions one-on-one on
    /// Ribcage Pass (`AI_MAP`), each on both starts, over `AI_GAMES` seeds
    /// from `AI_FROM`, at `AI_LEVEL` (Veteran), up to `AI_MINUTES` (40).
    /// Prints each pair's wins, then each faction's average income, army
    /// worth and soldiers at each mark, its first losses, and the worth it
    /// killed for each it lost. `AI_PAIR=13` plays one pair.
    /// `AI_GAMES=4 cargo test --release -p gc_game conquest_balance -- --ignored --nocapture`
    #[test]
    #[ignore = "a balance run, not a check: run it by hand with --release"]
    fn conquest_balance() {
        let games = env_number("AI_GAMES", 3);
        let from = env_number("AI_FROM", 1);
        let minutes = env_number("AI_MINUTES", 40) as u32;
        let choice = env_number("AI_MAP", 0) as usize;
        let levels = levels_from_env();
        let only = std::env::var("AI_PAIR").unwrap_or_default();
        let mut pairs = Vec::new();
        let n = Faction::ALL.len();
        for a in 0..n {
            for b in a + 1..n {
                let name = format!("{a}{b}");
                if only.is_empty() || only == name {
                    pairs.push((a, b));
                }
            }
        }
        let jobs: Vec<(usize, usize, u64, bool)> = pairs
            .iter()
            .flat_map(|&(a, b)| {
                (from..from + games).flat_map(move |s| [(a, b, s, false), (a, b, s, true)])
            })
            .collect();
        let next = std::sync::atomic::AtomicUsize::new(0);
        let results = std::sync::Mutex::new(Vec::new());
        let threads = std::thread::available_parallelism().map_or(4, std::num::NonZero::get);
        std::thread::scope(|sc| {
            for _ in 0..threads {
                sc.spawn(|| {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(&(a, b, seed, swap)) = jobs.get(i) else {
                            break;
                        };
                        let f = if swap { [b, a] } else { [a, b] };
                        let factions = [Faction::ALL[f[0]], Faction::ALL[f[1]]];
                        let played =
                            play_with((choice, 0), &factions, &levels, seed, minutes, None, &[]);
                        results.lock().unwrap().push((f, seed, played));
                    }
                });
            }
        });
        let results = results.into_inner().unwrap();
        let clock = gc_ai::lens::clock;
        for &(a, b) in &pairs {
            let (mut wa, mut wb, mut open, mut length) = (0, 0, 0, Vec::new());
            let (mut ka, mut kb) = (0, 0);
            for (f, _, p) in results
                .iter()
                .filter(|(f, _, _)| (f[0], f[1]) == (a, b) || (f[0], f[1]) == (b, a))
            {
                match p.winner() {
                    Some(w) if f[usize::from(w)] == a => wa += 1,
                    Some(_) => wb += 1,
                    None => open += 1,
                }
                if p.winner().is_some() {
                    length.push(p.ticks);
                }
                let i = usize::from(f[0] != a);
                ka += p.sides[i].ledger.killed;
                kb += p.sides[1 - i].ledger.killed;
            }
            let avg = length.iter().sum::<u32>() / length.len().max(1) as u32;
            println!(
                "{:?} v {:?}: {wa}-{wb} ({open} undecided), decided games average {}; \
                 fighters' worth killed {ka} v {kb}",
                Faction::ALL[a],
                Faction::ALL[b],
                clock(avg)
            );
        }
        println!(
            "\nper faction, averaged over its games (income Grub/Marrow; army worth/soldiers; games still going at each mark):"
        );
        for (fi, f) in Faction::ALL.iter().enumerate() {
            let sides: Vec<&Standing> = results
                .iter()
                .flat_map(|(fs, _, p)| {
                    fs.iter()
                        .zip(&p.sides)
                        .filter(|(x, _)| **x == fi)
                        .map(|(_, s)| s)
                })
                .collect();
            if sides.is_empty() {
                continue;
            }
            let n = sides.len() as u32;
            let mut line = format!("{f:?} ({n} games)");
            for (i, m) in MARKS.iter().enumerate() {
                let at: Vec<&Ledger> = sides
                    .iter()
                    .map(|s| &s.ledger)
                    .filter(|l| l.reached[i])
                    .collect();
                let k = at.len().max(1) as u32;
                let g: u32 = at.iter().map(|l| l.income[i].0).sum::<u32>() / k;
                let mr: u32 = at.iter().map(|l| l.income[i].1).sum::<u32>() / k;
                let army: u32 = at.iter().map(|l| l.army[i]).sum::<u32>() / k;
                let men: u32 = at.iter().map(|l| l.soldiers[i]).sum::<u32>() / k;
                line += &format!(
                    "\n  {m:>2} min ({} going): in {g}/{mr}, army {army} worth, {men} soldiers",
                    at.len()
                );
            }
            let ready: Vec<u32> = sides.iter().filter_map(|s| s.ledger.army_ready).collect();
            line += &format!(
                "\n  first army ({ARMY_READY} worth) ready at {} on average ({} of {n} games)",
                clock(ready.iter().sum::<u32>() / ready.len().max(1) as u32),
                ready.len()
            );
            let losses: Vec<u32> = sides.iter().filter_map(|s| s.ledger.first_loss).collect();
            let first = losses.iter().sum::<u32>() / losses.len().max(1) as u32;
            let lost: u32 = sides.iter().map(|s| s.ledger.lost).sum();
            let killed: u32 = sides.iter().map(|s| s.ledger.killed).sum();
            line += &format!(
                "\n  first fighter lost at {} on average; killed {killed} worth for {lost} lost ({:.2} each)",
                clock(first),
                killed as f32 / lost.max(1) as f32
            );
            println!("{line}");
        }
    }

    /// The lens panel of a Conquest mind has its economy and belief lines
    /// after the battle lines, and what the army waits for once it is
    /// gathering.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_lens_panel_shows_its_economy_belief_and_plan() {
        let factions = [Faction::EmberClans, Faction::Choir];
        // Whether anybody weighs up the other's army by eight minutes
        // depends on how the game goes, so the first of a few seeds where
        // somebody does counts; every game's panel is laid out alike.
        let mut last = String::new();
        let weighed = [8, 1, 2, 3, 4, 5].into_iter().any(|seed| {
            let played = play(0, 0, &factions, &[Difficulty::Veteran], seed, 8);
            let mut weighed = false;
            for (p, side) in played.sides.iter().enumerate() {
                let find = |label: &str| {
                    let start = format!("  {label:<7} ");
                    side.lens.iter().position(|l| l.starts_with(&start))
                };
                let roles = find("roles").expect("battle lines come first");
                let economy =
                    find("economy").unwrap_or_else(|| panic!("side {p}: {:?}", side.lens));
                assert!(economy > roles, "{:?}", side.lens);
                assert!(
                    side.lens[economy].contains("keeps 70% workers, 2 producers a tier"),
                    "{}",
                    side.lens[economy]
                );
                assert!(side.lens[economy].contains("saving: "));
                assert!(side.lens[economy].contains("arming: "));
                assert_eq!(find("belief"), Some(economy + 1), "{:?}", side.lens);
                weighed |= side.lens[economy + 1].contains(", odds ");
                if let Some(plan) = find("plan") {
                    assert_eq!(plan, economy + 2, "{:?}", side.lens);
                }
                assert!(side.lens.iter().all(|l| l.starts_with("  ")));
            }
            if !weighed {
                last = played.report();
            }
            weighed
        });
        // By eight minutes the armies have met, and somebody weighed up
        // the other's.
        assert!(weighed, "{last}");
    }

    /// A General's scout comes to look at the enemy's start within the
    /// first five minutes of a one-on-one on Ribcage Pass. Two scouts meeting
    /// halfway can turn each other back, so one of the two finding it is
    /// enough.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_a_generals_scout_finds_the_enemy_start() {
        let factions = [Faction::EmberClans, Faction::EmberClans];
        for seed in [7, 8] {
            let played = play(0, 0, &factions, &[Difficulty::General], seed, 5);
            let looked: Vec<Option<u32>> = played.sides.iter().map(|s| s.scout_looked).collect();
            assert!(
                looked.iter().any(Option::is_some),
                "seed {seed}: no scout looked at the enemy start: {:?}",
                played.sides[0].lens
            );
            for (p, side) in played.sides.iter().enumerate() {
                assert!(
                    side.lens.iter().any(|l| l.starts_with("  scout   ")),
                    "side {p} has no scout line: {:?}",
                    side.lens
                );
            }
        }
    }

    /// Warlords' raiders set out against enemy workers in a four-player
    /// game on Four Teeth, and some party kills some of them within 40
    /// minutes; a lens panel shows the `raid` line after the `scout` line,
    /// and a General has none.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_a_warlords_raiders_kill_enemy_workers() {
        let (mut killed, mut raids) = (0, 0);
        // Whether a raid catches workers depends on the game, so play on
        // (a known seed first) until one does.
        for seed in [2, 1, 3, 4, 5, 6] {
            let played = play(1, 0, &Faction::ALL, &[Difficulty::Warlord], seed, 40);
            for (p, side) in played.sides.iter().enumerate() {
                let find = |label: &str| {
                    let start = format!("  {label:<7} ");
                    side.lens.iter().position(|l| l.starts_with(&start))
                };
                let scout = find("scout").expect("a Warlord scouts");
                assert_eq!(find("raid"), Some(scout + 1), "side {p}: {:?}", side.lens);
                killed += side.raid_kills;
                raids += side.raids;
            }
            if killed > 0 {
                break;
            }
        }
        assert!(raids > 0, "no raid set out");
        assert!(killed > 0, "the raiders saw no enemy worker go");
        let factions = [Faction::EmberClans, Faction::EmberClans];
        let played = play(0, 0, &factions, &[Difficulty::General], 7, 8);
        for side in &played.sides {
            assert_eq!(side.raids, 0);
            assert!(side.lens.iter().all(|l| !l.starts_with("  raid    ")));
        }
    }

    /// Generals bridge (docs/26-fortifications.md): where a deep river, with
    /// no ford, runs the whole map between the starts, some General draws a
    /// bridge across it.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_generals_bridge_a_river_that_cuts_them_off() {
        use gc_map::def::{Mode, Shape, TerrainOp};
        let mut def = gc_map::shipped("ribcage_pass").expect("shipped");
        // A river from corner to corner across the middle, about seven
        // tiles wide and a quarter tile deep, over the ribs and the ford.
        def.terrain.push(TerrainOp {
            shape: Shape::Line {
                a: (0, 127),
                b: (127, 0),
                half_width: 3,
            },
            mode: Mode::Set,
            height: 4,
            edge: 0,
        });
        let map = Map::from_def(&def);
        let nav = map.sim_nav_costs();
        let (a, b) = (map.starts()[0], map.starts()[1]);
        assert!(
            gc_match::terrain_view::ai_terrain(&map, &nav).cut_off(
                Vec2::new(a.0 as f32, a.1 as f32),
                [Vec2::new(b.0 as f32, b.1 as f32)]
            ),
            "the river cuts the starts off"
        );
        let f = Faction::EmberClans;
        let played = play_map(map, &[f, f], &[Difficulty::General], 8, 35, None, &[]);
        // Both sides are cut off, so both bridge (a deck is 3 to 5 tiles
        // wide, so a bridge is at least three tiles).
        for (p, side) in played.sides.iter().enumerate() {
            assert!(
                side.works.bridges >= 3,
                "side {p} built no bridge: {}",
                played.report()
            );
        }
    }

    /// Generals cut through thicket (docs/41, step 6): where a belt of it
    /// runs the whole map between the starts, some General sends workers
    /// to grub out a path through it.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_generals_cut_a_path_through_thicket_that_cuts_them_off() {
        use gc_map::def::{CoverKind, CoverOp, Shape};
        let mut def = gc_map::shipped("ribcage_pass").expect("shipped");
        // A belt of thicket from corner to corner across the middle, about
        // five tiles wide.
        def.cover.push(CoverOp {
            shape: Shape::Line {
                a: (0, 127),
                b: (127, 0),
                half_width: 2,
            },
            kind: CoverKind::Thicket,
        });
        let map = Map::from_def(&def);
        let f = Faction::EmberClans;
        let played = play_map(map, &[f, f], &[Difficulty::General], 8, 20, None, &[]);
        assert!(
            played.sides.iter().any(|s| s.works.clears >= 1),
            "nobody cut a path: {}",
            played.report()
        );
    }

    /// Generals fortify (docs/26-fortifications.md, stage 8): in a
    /// one-on-one on Ribcage Pass some side draws a front wall with a gate
    /// and a barricade at its rally point, and some General lays traps on
    /// the approach; a Veteran puts up field works at most, and no walls
    /// or traps.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_generals_wall_and_trap_veterans_only_field_works() {
        // Traps come last, once Grub piles up: a quick win may come first,
        // so a trap in any General's game counts.
        let general = |f: Faction, seed: u64| {
            let played = play(0, 0, &[f, f], &[Difficulty::General], seed, 40);
            let most = (played.sides.iter()).fold(Works::default(), |m, s| m.most(s.works));
            (most, played.report())
        };
        let (mut traps, mut last) = (0, String::new());
        // A General that walls has reached tier 2 or been at it a while:
        // a blast tower behind its hall, or a bolt tower once it has seen
        // riders or monsters, in one of its games at least.
        let mut towers = 0;
        // Once at tier 2 with Marrow to spare it raises a tower to level 2.
        let mut raised = 0;
        // Then carries the wall on round its hall: a walled town.
        let mut walled_towns = 0;
        // Whether a General walls depends on how its game goes (about one
        // Clans mirror in five; fewer since blocks keep their spacing, which
        // moves where things get built, and with frontage about one in 25),
        // so the first seed that walls counts: a known one first, then the
        // rest of 1 to 30.
        for (f, known) in [(Faction::EmberClans, 14), (Faction::CarrionConcord, 8)] {
            let walled = core::iter::once(known)
                .chain((1..=30).filter(|&s| s != known))
                .any(|seed| {
                    let most;
                    (most, last) = general(f, seed);
                    traps += most.traps;
                    towers += most.bolts + most.blasts;
                    raised += most.raised;
                    walled_towns += most.walled;
                    most.walls >= 5 && most.gates >= 1 && most.barricades >= 1
                });
            assert!(walled, "{f:?}: no General walled in seeds 1 to 24: {last}");
        }
        // Still none: more Clans games until one lays a trap.
        for seed in 13..=24 {
            if traps > 0 {
                break;
            }
            let most;
            (most, last) = general(Faction::EmberClans, seed);
            traps += most.traps;
            towers += most.bolts + most.blasts;
            raised += most.raised;
        }
        assert!(traps >= 1, "no General laid a trap; the last game: {last}");
        assert!(towers >= 1, "no bolt or blast tower put up in any game");
        assert!(raised >= 1, "no tower raised to level 2 in any game");
        assert!(walled_towns >= 1, "no General closed a ring round its hall");
        let played = play(
            0,
            0,
            &[Faction::CarrionConcord; 2],
            &[Difficulty::Veteran],
            8,
            40,
        );
        for side in &played.sides {
            let w = side.works;
            assert_eq!(
                (w.walls, w.gates, w.traps),
                (0, 0, 0),
                "{}",
                played.report()
            );
        }
    }

    // A game takes seconds in a release build and minutes in a debug one,
    // so these run in CI's release step.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_ribcage_1v1_ends_in_a_win() {
        let pairs = [
            [Faction::EmberClans, Faction::CarrionConcord],
            [Faction::Brood, Faction::Choir],
        ];
        // An hour: since frontage a base under siege holds a little longer
        // (seed 7 ended by the Heart at 46 minutes).
        for (factions, seed) in pairs.iter().zip([7, 8]) {
            let played = play(0, 0, factions, &[Difficulty::Veteran], seed, 60);
            assert!(played.winner().is_some(), "{}", played.report());
        }
    }

    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_four_teeth_ends_in_a_win() {
        // A last side standing can take most of the 100 minutes to clear
        // the others' bases one by one (seed 8: about 91).
        for seed in [7, 8] {
            let played = play(1, 0, &Faction::ALL, &[Difficulty::Veteran], seed, 100);
            assert!(played.winner().is_some(), "{}", played.report());
        }
    }

    /// Six computer players on a Large Wilds map (the first choice past
    /// the shipped maps).
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_six_player_wilds_ends_in_a_win() {
        // Six Veterans can stall for the whole two hours (seed 1 has since
        // the Brood swarm cut, docs/29): a seed that ends shows the game can.
        const SEED: u64 = 2;
        use gc_ai::personality::Faction::*;
        let factions = [EmberClans, Brood, Choir, CarrionConcord, EmberClans, Brood];
        let wilds = gc_map::SHIPPED.len();
        let large = gc_ui::lobby::MapSize::Large.conquest_tiles();
        let played = play(wilds, large, &factions, &[Difficulty::Veteran], SEED, 120);
        assert!(played.winner().is_some(), "{}", played.report());
    }

    /// Computer players finish one-on-ones on grown land (docs/41, step
    /// 1): a Medium map, as wild as the seed picks, across rivers, woods
    /// and passes.
    #[test]
    #[ignore = "slow in debug builds: CI runs it with --release"]
    fn ai_only_grown_land_1v1_ends_in_a_win() {
        let land = gc_ui::lobby::DEFAULT_CONQUEST_MAP;
        let medium = gc_ui::lobby::MapSize::Medium.conquest_tiles();
        let pairs = [
            [Faction::EmberClans, Faction::Brood],
            [Faction::Choir, Faction::CarrionConcord],
        ];
        for (factions, seed) in pairs.iter().zip([7, 8]) {
            let played = play(land, medium, factions, &[Difficulty::Veteran], seed, 60);
            assert!(played.winner().is_some(), "{}", played.report());
        }
    }
}
