//! The online match kit in the game (docs/11-networking.md, "The match kit"):
//! what the keys and the mouse do for chat, pings, pause and surrender.
//! `gc_ui::match_kit` draws it and `online` keeps the chat lines and live
//! pings; this is the glue to `gc_net::Lan` and to the battle.
//!
//! Keys: Enter chats to the team, Shift+Enter to everyone (Tab switches while
//! typing, Enter sends, Esc closes); Alt+left-click pings "Look here" and
//! Alt+left-drag opens the ping wheel; P pauses and carries on; the Esc
//! menu's Surrender asks the team to give up.

use gc_sim::{Command, PlayerCommand};
use gc_ui::SurrenderItem;
use gc_ui::match_kit::{ChatAction, ChatKey, wheel_pick};
use winit::event::{ElementState, KeyEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

use crate::Running;
use crate::online::PingPress;

/// What to say when a ping or chat is tried offline.
const OFFLINE_PINGS: &str = "Pings are for network battles.";

impl Running {
    /// Whether the chat box is open: every key then goes to it.
    pub fn chat_is_open(&self) -> bool {
        self.online.as_ref().is_some_and(|o| o.chat_box.is_open())
    }

    /// Enter (`team`) or Shift+Enter (everyone): open the chat box, in a
    /// network battle that is still on.
    pub fn open_chat(&mut self, team: bool) {
        let Some(o) = self.online.as_mut().filter(|o| o.ended.is_none()) else {
            return;
        };
        o.chat_box.open(team);
        // Keys held down as the box opens would otherwise never be let go.
        self.input.arrows = [false; 4];
        self.input.turn = [false; 2];
    }

    /// A key while the chat box is open. Nothing else sees it.
    pub fn chat_key(&mut self, event: &KeyEvent) {
        if event.state != ElementState::Pressed {
            return;
        }
        let named = match event.physical_key {
            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) if !event.repeat => {
                Some(ChatKey::Enter)
            }
            PhysicalKey::Code(KeyCode::Escape) if !event.repeat => Some(ChatKey::Escape),
            PhysicalKey::Code(KeyCode::Tab) if !event.repeat => Some(ChatKey::Tab),
            PhysicalKey::Code(KeyCode::Backspace) => Some(ChatKey::Backspace),
            _ => None,
        };
        let mut keys: Vec<ChatKey> = Vec::new();
        match named {
            Some(k) => keys.push(k),
            // Typed text, as the menu's boxes get it, but not Ctrl or Cmd
            // chords.
            None if !self.input.command => {
                if let Some(text) = &event.text {
                    keys.extend(text.chars().map(ChatKey::Char));
                }
            }
            None => {}
        }
        for key in keys {
            let action = match self.online.as_mut() {
                Some(o) => o.chat_box.key(key),
                None => return,
            };
            if let ChatAction::Send { team, text } = action {
                self.menu.net().chat(team, &text);
            }
        }
    }

    /// Alt+left pressed on the battlefield: the start of a ping. Online it
    /// notes where; offline there is nobody to ping, and it says so.
    pub fn begin_ping(&mut self) {
        if self.online.as_ref().is_none_or(|o| o.ended.is_some()) {
            self.say(OFFLINE_PINGS.into());
            return;
        }
        let Some(at) = self.ground_under_cursor() else {
            return;
        };
        let screen = self.ui_cursor();
        if let Some(o) = &mut self.online {
            o.ping_press = Some(PingPress {
                screen,
                at: at.to_array(),
            });
        }
    }

    /// Whether an Alt+left press is waiting to become a ping.
    pub fn ping_pending(&self) -> bool {
        self.online.as_ref().is_some_and(|o| o.ping_press.is_some())
    }

    /// The left button came up after [`Running::begin_ping`]: send the ping
    /// the drag picked (a plain click is "Look here").
    pub fn finish_ping(&mut self) {
        let Some(press) = self.online.as_mut().and_then(|o| o.ping_press.take()) else {
            return;
        };
        let now = self.ui_cursor();
        let choice = if self.input.cursor.is_some() {
            wheel_pick(now.0 - press.screen.0, now.1 - press.screen.1)
        } else {
            gc_ui::match_kit::PingKind::Look
        };
        self.menu.net().ping(choice, press.at);
    }

    /// P in a network battle: pause, or carry on (the host's rules decide
    /// whether that is allowed; the banner says who paused).
    pub fn pause_key(&mut self) {
        if self.online.as_ref().is_some_and(|o| o.ended.is_none()) {
            self.menu.net().toggle_pause();
        }
    }

    /// The Esc menu's Surrender item for this match.
    pub fn surrender_offer(&self) -> SurrenderItem {
        let Some(battle) = &self.battle else {
            return SurrenderItem::Hidden;
        };
        if self.watching.is_some() {
            SurrenderItem::Hidden
        } else if self.world.deploying() {
            SurrenderItem::Off("The armies are still deploying: nobody can surrender yet.")
        } else if self.world.verdict().is_some() || battle.outcome.is_some() {
            SurrenderItem::Off("The battle is already decided.")
        } else if self.world.has_conceded(self.sandbox.side) {
            SurrenderItem::Off("Your side has already surrendered.")
        } else {
            SurrenderItem::Ready
        }
    }

    /// The Esc menu's Surrender, confirmed. In a network battle that calls
    /// the team's vote (or votes yes in one that is open); offline every
    /// party on the player's side concedes at once.
    pub fn surrender_now(&mut self) {
        if self.surrender_offer() != SurrenderItem::Ready {
            return;
        }
        if self.online.is_some() {
            self.menu.net().surrender(true);
        } else {
            let side = self.sandbox.side;
            for party in 0..self.parties.len() as gc_sim::PlayerId {
                if party == side || !self.world.hostile(side, party) {
                    self.human_orders.push(PlayerCommand {
                        player: party,
                        command: Command::Concede,
                    });
                }
            }
        }
        self.close_menu();
    }
}
