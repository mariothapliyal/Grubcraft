//! Conquest's command card (docs/08-ui-ux.md, "Command card"): what the
//! grid offers for a selected building, for workers and in their build
//! menu, and carrying it out. The orders themselves are the simulation's
//! (`Command::Build`, `Train`, `Upgrade`, `CallOff`, `Untrain`, `Rally`,
//! `Gather` and `Construct`).

use std::fmt::Write;

use gc_data::ConquestUnitRecord;
use gc_sim::{BuildingId, Command, PlayerCommand, RegimentHandle, StructureView, UnitId};
use gc_ui::hud::{GRID_SLOTS, SlotState};
use glam::Vec2;

use crate::hud::Armed;
use crate::{Running, from_fx_vec, fx_vec, sounds};

/// A worker's Build button: F, the special slot.
pub const SLOT_BUILD: usize = 7;
/// A building's last slot (V): its tier upgrade, or calling it off.
const SLOT_LAST: usize = GRID_SLOTS - 1;
/// A right-click this close to a seep or vein (in tiles) sends workers there.
const NODE_REACH: f32 = 1.5;

/// What one slot of the card does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open the workers' build menu.
    OpenBuild,
    /// Choose where a worker lays this building.
    Lay(BuildingId),
    /// Queue this regiment at the selected building.
    Train(UnitId),
    /// Start the selected building's tier upgrade into this.
    Upgrade(BuildingId),
    /// Call off the selected building or its upgrade.
    CallOff,
}

/// The grid as Conquest fills it: an action per slot. With `whole`, the card
/// is the whole grid (a building, or the build menu); otherwise its actions
/// go over the battle grid.
#[derive(Clone, Copy, Debug)]
pub struct Card {
    pub slots: [Option<Action>; GRID_SLOTS],
    pub whole: bool,
}

impl Running {
    /// The Conquest card for what is selected, if it has one.
    pub fn conquest_card(&self) -> Option<Card> {
        if !self.conquest_units || self.watching.is_some() {
            return None;
        }
        let mut slots = [None; GRID_SLOTS];
        if let Some(s) = self
            .picked_structure()
            .and_then(|h| self.world.structures()?.get(h))
        {
            if !s.finished() || s.upgrading.is_some() {
                slots[SLOT_LAST] = Some(Action::CallOff);
            } else {
                let root = self.buildings.root(s.building);
                let trains = (self.data.conquest_units.iter().enumerate())
                    .filter(|(_, u)| u.trained_at == root)
                    .map(|(i, _)| Action::Train(i as UnitId));
                for (slot, a) in slots[..SLOT_LAST].iter_mut().zip(trains) {
                    *slot = Some(a);
                }
                slots[SLOT_LAST] = self.buildings.upgrade_of(s.building).map(Action::Upgrade);
            }
            return Some(Card { slots, whole: true });
        }
        let worker = self.selected_workers().next()?;
        if !self.hud.build_menu {
            slots[SLOT_BUILD] = Some(Action::OpenBuild);
            return Some(Card {
                slots,
                whole: false,
            });
        }
        let faction = self.world.regiments.get(worker).and_then(|r| {
            self.data
                .conquest_units
                .get(usize::from(r.unit))
                .map(ConquestUnitRecord::faction)
        })?;
        for (slot, b) in slots.iter_mut().zip(self.buildings.buildable(faction)) {
            *slot = Some(Action::Lay(b));
        }
        Some(Card { slots, whole: true })
    }

    /// The selected regiments that are workers.
    pub fn selected_workers(&self) -> impl Iterator<Item = RegimentHandle> + '_ {
        self.selected.iter().copied().filter(|&h| {
            self.world.regiments.get(h).is_some_and(|r| {
                self.data
                    .conquest_units
                    .get(usize::from(r.unit))
                    .is_some_and(ConquestUnitRecord::worker)
            })
        })
    }

    /// Why `action` can't be done now, if it can't: its price or the tier
    /// it needs.
    fn blocked(&self, action: Action) -> Option<&'static str> {
        let side = self.sandbox.side;
        let bank = self.world.bank(side)?;
        let tier = self.world.structures().map_or(0, |s| s.tier_of(side));
        let (grub, marrow, needs) = match action {
            Action::Lay(b) | Action::Upgrade(b) => {
                let s = self.buildings.stats.get(usize::from(b))?;
                (s.cost_grub, s.cost_marrow, s.tier)
            }
            Action::Train(u) => {
                let u = self.data.conquest_units.get(usize::from(u))?;
                (u.cost_grub, u.cost_marrow, u.tier)
            }
            Action::OpenBuild | Action::CallOff => return None,
        };
        // An upgrade is to the tier it makes, so the one below will do.
        let needs = match action {
            Action::Upgrade(_) => needs.saturating_sub(1),
            _ => needs,
        };
        if tier < needs {
            Some("Your tier is too low for that.")
        } else if bank.grub < u32::from(grub) || bank.marrow < u32::from(marrow) {
            Some("You can't afford that.")
        } else {
            None
        }
    }

    /// Each slot of `card` as the grid shows it: its name into `labels`,
    /// its price into `details`, and whether it can be pressed.
    pub fn describe_card(
        &self,
        card: &Card,
        labels: &mut [String; GRID_SLOTS],
        details: &mut [String; GRID_SLOTS],
    ) -> [SlotState; GRID_SLOTS] {
        let mut states = [SlotState::Off; GRID_SLOTS];
        for (i, action) in card.slots.iter().enumerate() {
            let (label, detail) = (&mut labels[i], &mut details[i]);
            label.clear();
            detail.clear();
            let Some(action) = *action else {
                continue;
            };
            let price = |d: &mut String, grub: u16, marrow: u16| {
                let _ = match marrow {
                    0 => write!(d, "{grub}"),
                    m => write!(d, "{grub}/{m}"),
                };
            };
            match action {
                Action::OpenBuild => {
                    label.push_str("Build");
                    detail.push('B');
                }
                Action::Lay(b) | Action::Upgrade(b) => {
                    if let Some(s) = self.buildings.stats.get(usize::from(b)) {
                        price(detail, s.cost_grub, s.cost_marrow);
                    }
                    match action {
                        Action::Upgrade(_) => {
                            let tier = self
                                .buildings
                                .stats
                                .get(usize::from(b))
                                .map_or(0, |s| s.tier);
                            let _ = write!(label, "Tier {tier}");
                        }
                        _ => label.push_str(short(self.buildings.name(b))),
                    }
                }
                Action::Train(u) => {
                    if let Some(r) = self.data.conquest_units.get(usize::from(u)) {
                        price(detail, r.cost_grub, r.cost_marrow);
                    }
                    label.push_str(short(
                        self.units
                            .names
                            .get(usize::from(u))
                            .map_or("", String::as_str),
                    ));
                }
                Action::CallOff => {
                    label.push_str("Cancel");
                    detail.push_str("75%");
                }
            }
            let armed = matches!(action, Action::Lay(b) if self.hud.armed == Some(Armed::Lay(b)));
            states[i] = if armed {
                SlotState::Armed
            } else if self.blocked(action).is_some() {
                SlotState::Off
            } else {
                SlotState::Ready
            };
        }
        states
    }

    /// Whether the grid key for `slot` belongs to the Conquest card now.
    pub fn card_takes(&self, slot: usize) -> bool {
        self.conquest_card()
            .is_some_and(|c| c.whole || c.slots[slot].is_some())
    }

    /// Carry out the card's slot `slot`; false if the card has nothing there
    /// and the battle grid should have it.
    pub fn card_press(&mut self, slot: usize) -> bool {
        let Some(card) = self.conquest_card() else {
            return false;
        };
        let Some(action) = card.slots[slot] else {
            return card.whole;
        };
        if let Some(why) = self.blocked(action) {
            self.say(why.into());
            return true;
        }
        let structure = self.picked_structure();
        match action {
            Action::OpenBuild => self.hud.build_menu = true,
            Action::Lay(b) => {
                self.hud.armed = (self.hud.armed != Some(Armed::Lay(b))).then_some(Armed::Lay(b));
                self.say(format!("Where should the {} go?", self.buildings.name(b)));
            }
            Action::Train(unit) => {
                if let Some(structure) = structure {
                    self.order(Command::Train { structure, unit });
                }
            }
            Action::Upgrade(_) => {
                if let Some(structure) = structure {
                    self.order(Command::Upgrade { structure });
                }
            }
            Action::CallOff => {
                if let Some(structure) = structure {
                    self.order(Command::CallOff { structure });
                }
            }
        }
        true
    }

    /// The selected building takes the last regiment off its queue.
    pub fn untrain(&mut self) {
        if let Some(structure) = self.picked_structure()
            && self.watching.is_none()
        {
            self.order(Command::Untrain { structure });
        }
    }

    /// The click after choosing a building to lay: the selected worker
    /// nearest the spot lays it there. Shift keeps the building chosen.
    pub fn finish_laying(&mut self, building: BuildingId) {
        let Some((stats, tile, why)) = self.building_placement(building) else {
            return;
        };
        if let Some(why) = why {
            self.say(why.into());
            return;
        }
        let spot = Vec2::new(
            tile.0 as f32 + f32::from(stats.width) / 2.0,
            tile.1 as f32 + f32::from(stats.depth) / 2.0,
        );
        let regiments = &self.world.regiments;
        let Some(regiment) = self.selected_workers().min_by(|&a, &b| {
            let d = |h| {
                regiments
                    .position(h)
                    .map_or(f32::MAX, |p| from_fx_vec(p).distance(spot))
            };
            d(a).total_cmp(&d(b))
        }) else {
            return;
        };
        self.order(Command::Build {
            regiment,
            building,
            tile: (tile.0 as i16, tile.1 as i16),
        });
        if !self.input.shift {
            self.hud.armed = None;
            self.hud.build_menu = false;
        }
    }

    /// A right-click in Conquest that means more than a move: a selected
    /// building's rally point, or workers sent to a seep, a vein or one of
    /// your buildings. Returns whether it was one.
    pub fn conquest_right_click(&mut self, at: Vec2) -> bool {
        if !self.conquest_units || self.watching.is_some() {
            return false;
        }
        if let Some(structure) = self.picked_structure() {
            self.order(Command::Rally {
                structure,
                at: fx_vec(at),
            });
            return true;
        }
        let workers: Vec<RegimentHandle> = self.selected_workers().collect();
        if workers.is_empty() {
            return false;
        }
        let side = self.sandbox.side;
        let mend = self.building_under_cursor(|o| o == side).filter(|&h| {
            self.world
                .structures()
                .and_then(|s| s.get(h))
                .is_some_and(needs_hands)
        });
        let node = mend.is_none().then(|| self.node_near(at)).flatten();
        if mend.is_none() && node.is_none() {
            return false;
        }
        for regiment in workers {
            self.order(match (mend, node) {
                (Some(structure), _) => Command::Construct {
                    regiment,
                    structure,
                },
                (None, Some(node)) => Command::Gather { regiment, node },
                (None, None) => unreachable!("checked above"),
            });
        }
        true
    }

    /// The seep or vein nearest `at`, if one is within reach.
    fn node_near(&self, at: Vec2) -> Option<u16> {
        self.world
            .economy()?
            .nodes()
            .map(|n| (n.index, from_fx_vec(n.at).distance(at)))
            .filter(|&(_, d)| d <= NODE_REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    }

    /// Give `command` as the side you command.
    fn order(&mut self, command: Command) {
        self.human_orders.push(PlayerCommand {
            player: self.sandbox.side,
            command,
        });
        self.ui_sound(sounds::UiSound::Move);
    }
}

/// A building still going up, or damaged: workers have work there.
fn needs_hands(s: StructureView) -> bool {
    !s.finished() || s.hp < s.max_hp
}

/// A name short enough for a grid slot: its first word.
fn short(name: &str) -> &str {
    name.split(' ').next().unwrap_or(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_names_are_first_words() {
        assert_eq!(short("Yumi Ashigaru"), "Yumi");
        assert_eq!(short("Hands"), "Hands");
        assert_eq!(short(""), "");
    }
}
