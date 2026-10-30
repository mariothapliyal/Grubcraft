//! What a regiment is doing, what is holding it up, and why it wavers, as
//! words and pictures (docs/29, "Clear orders and feedback"). The selection
//! panel, the army tray's cards and the hover card all read these, so a
//! word or a picture is changed in one place.
//!
//! The game maps the simulation's own `Doing`, `Waiting` and morale
//! reasons onto these (the UI crate knows nothing of the simulation); the
//! reason bits sit at the same places as `gc_sim::morale::WHY_*`, and the
//! game checks that they stay so.
//! Docs: docs/08-ui-ux.md#visual-style.

use crate::draw::{DrawList, Rect};
use crate::hud::linen_light;
use crate::icons::{self, Icon};
use crate::ui::theme;

/// What a regiment is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Order {
    #[default]
    Holding,
    Marching,
    ForcedMarch,
    Attacking,
    Fighting,
    Braced,
    FallingBack,
    Running,
}

impl Order {
    /// The word for the panel and the tooltip.
    pub fn word(self) -> &'static str {
        match self {
            Order::Holding => "Holding",
            Order::Marching => "Marching",
            Order::ForcedMarch => "Forced march",
            Order::Attacking => "Attacking",
            Order::Fighting => "Fighting",
            Order::Braced => "Braced",
            Order::FallingBack => "Falling back",
            Order::Running => "Running",
        }
    }

    /// The small picture that goes with the word.
    pub fn icon(self) -> Icon {
        match self {
            Order::Holding => Icon::Hold,
            Order::Marching => Icon::Move,
            Order::ForcedMarch => Icon::Hurry,
            Order::Attacking => Icon::Attack,
            Order::Fighting => Icon::Fight,
            Order::Braced => Icon::Brace,
            Order::FallingBack => Icon::FallBack,
            Order::Running => Icon::Run,
        }
    }
}

/// What is holding a regiment up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    Pinned,
    DrivenBack,
    FriendInTheWay,
    FilingThrough,
    Trapped,
}

impl Wait {
    /// A few words to follow the order's.
    pub fn note(self) -> &'static str {
        match self {
            Wait::Pinned => "pinned",
            Wait::DrivenBack => "driven back",
            Wait::FriendInTheWay => "waiting: friend in the way",
            Wait::FilingThrough => "filing through",
            Wait::Trapped => "trapped",
        }
    }
}

/// The order and what holds it up, as one line: "Marching (pinned)".
pub fn order_text(order: Order, wait: Option<Wait>) -> String {
    match wait {
        Some(w) => format!("{} ({})", order.word(), w.note()),
        None => order.word().to_string(),
    }
}

/// What a selection is doing, as one thing to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Orders {
    /// Nothing to say.
    #[default]
    None,
    /// Several regiments, each doing something different.
    Mixed,
    /// One order, and what holds it up (one regiment, or all alike).
    One(Order, Option<Wait>),
}

impl Orders {
    /// Add another regiment's: the same order stays, otherwise it is mixed;
    /// a wait stays only if all share it.
    pub fn with(self, next: Orders) -> Orders {
        match (self, next) {
            (Orders::None, next) => next,
            (Orders::One(a, wa), Orders::One(b, wb)) if a == b => {
                Orders::One(a, if wa == wb { wa } else { None })
            }
            (Orders::One(..), Orders::One(..)) => Orders::Mixed,
            (Orders::Mixed, _) | (_, Orders::Mixed) => Orders::Mixed,
            (one, Orders::None) => one,
        }
    }

    /// The picture and line to show; `None` with nothing to say.
    pub fn shown(self) -> Option<(Icon, String)> {
        match self {
            Orders::None => None,
            Orders::Mixed => Some((Icon::Flag, "Mixed orders".to_string())),
            Orders::One(order, wait) => Some((order.icon(), order_text(order, wait))),
        }
    }
}

/// The small order picture tab: linen with an ink border, the order's
/// icon inside. Used on the army cards and over banners in the world.
pub fn draw_tab(draw: &mut DrawList, tab: Rect, order: Order) {
    draw.rect(tab, linen_light());
    draw.border(tab, 1, theme::INK);
    icons::icon(draw, tab.inset(1), order.icon(), theme::INK);
}

/// Reasons a regiment's nerve is shaken. The same bits as the
/// simulation's `WHY_*` constants.
pub mod why {
    pub const FLANK: u16 = 1 << 0;
    pub const REAR: u16 = 1 << 1;
    pub const PINNED: u16 = 1 << 2;
    pub const OUTNUMBERED: u16 = 1 << 3;
    pub const LOSSES: u16 = 1 << 4;
    pub const BLOODIED: u16 = 1 << 5;
    pub const ALLY_BROKE: u16 = 1 << 6;
    pub const GENERAL: u16 = 1 << 7;
    pub const AMBUSH: u16 = 1 << 8;
    pub const FIRE: u16 = 1 << 9;
    pub const WATER: u16 = 1 << 10;
    pub const CHAMPION: u16 = 1 << 11;
}

/// The reasons, most serious first, with their short words.
const REASONS: [(u16, &str); 12] = [
    (why::REAR, "rear attack"),
    (why::FLANK, "flanked"),
    (why::GENERAL, "general fell"),
    (why::ALLY_BROKE, "ally broke"),
    (why::OUTNUMBERED, "outnumbered"),
    (why::AMBUSH, "ambushed"),
    (why::LOSSES, "heavy losses"),
    (why::BLOODIED, "bloodied"),
    (why::PINNED, "pinned"),
    (why::FIRE, "burning"),
    (why::WATER, "flooded"),
    (why::CHAMPION, "champion lost"),
];

/// How many reasons are ever shown.
pub const MAX_REASONS: usize = 2;

/// The words for the most serious reasons in `bits`, at most
/// [`MAX_REASONS`], most serious first.
pub fn reasons(bits: u16) -> impl Iterator<Item = &'static str> {
    REASONS
        .iter()
        .filter(move |(bit, _)| bits & bit != 0)
        .map(|&(_, word)| word)
        .take(MAX_REASONS)
}

/// "Why: rear attack, outnumbered"; `None` when there is no reason to give.
pub fn why_text(bits: u16) -> Option<String> {
    let mut words = reasons(bits);
    let mut text = String::from("Why: ");
    text.push_str(words.next()?);
    for word in words {
        text.push_str(", ");
        text.push_str(word);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_order_has_its_word_and_a_picture() {
        let all = [
            Order::Holding,
            Order::Marching,
            Order::ForcedMarch,
            Order::Attacking,
            Order::Fighting,
            Order::Braced,
            Order::FallingBack,
            Order::Running,
        ];
        let words: Vec<_> = all.iter().map(|o| o.word()).collect();
        assert_eq!(
            words,
            [
                "Holding",
                "Marching",
                "Forced march",
                "Attacking",
                "Fighting",
                "Braced",
                "Falling back",
                "Running"
            ]
        );
        assert!(all.iter().all(|o| o.icon() != Icon::None));
    }

    #[test]
    fn a_wait_follows_the_order_in_brackets() {
        assert_eq!(order_text(Order::Marching, None), "Marching");
        assert_eq!(
            order_text(Order::Marching, Some(Wait::FriendInTheWay)),
            "Marching (waiting: friend in the way)"
        );
        assert_eq!(
            order_text(Order::Fighting, Some(Wait::Pinned)),
            "Fighting (pinned)"
        );
        assert_eq!(Wait::DrivenBack.note(), "driven back");
        assert_eq!(Wait::FilingThrough.note(), "filing through");
        assert_eq!(Wait::Trapped.note(), "trapped");
    }

    #[test]
    fn orders_fold_into_one_thing_to_show() {
        let march = Orders::One(Order::Marching, None);
        let pinned = Orders::One(Order::Marching, Some(Wait::Pinned));
        assert_eq!(Orders::None.with(march), march);
        assert_eq!(march.with(Orders::None), march);
        assert_eq!(march.with(march), march);
        assert_eq!(pinned.with(pinned), pinned);
        assert_eq!(pinned.with(march), march, "a wait only if all share it");
        assert_eq!(march.with(Orders::One(Order::Holding, None)), Orders::Mixed);
        assert_eq!(Orders::Mixed.with(march), Orders::Mixed);
        assert_eq!(march.with(Orders::Mixed), Orders::Mixed);
        assert_eq!(Orders::None.shown(), None);
        assert_eq!(Orders::Mixed.shown().unwrap().1, "Mixed orders");
        assert_eq!(pinned.shown().unwrap().1, "Marching (pinned)");
    }

    #[test]
    fn no_bits_no_reasons() {
        assert_eq!(reasons(0).count(), 0);
        assert_eq!(why_text(0), None);
    }

    #[test]
    fn each_bit_has_a_word() {
        let words: Vec<_> = (0..12)
            .map(|i| reasons(1 << i).next().expect("a word"))
            .collect();
        assert_eq!(
            words,
            [
                "flanked",
                "rear attack",
                "pinned",
                "outnumbered",
                "heavy losses",
                "bloodied",
                "ally broke",
                "general fell",
                "ambushed",
                "burning",
                "flooded",
                "champion lost"
            ]
        );
    }

    #[test]
    fn at_most_two_reasons_the_most_serious_first() {
        let all = 0x0fff;
        assert_eq!(reasons(all).collect::<Vec<_>>(), ["rear attack", "flanked"]);
        assert_eq!(
            why_text(why::FLANK | why::OUTNUMBERED | why::FIRE | why::CHAMPION).as_deref(),
            Some("Why: flanked, outnumbered")
        );
        // Order of the bits does not matter, only their seriousness.
        assert_eq!(
            why_text(why::CHAMPION | why::GENERAL | why::REAR).as_deref(),
            Some("Why: rear attack, general fell")
        );
        assert_eq!(
            why_text(why::BLOODIED | why::LOSSES).as_deref(),
            Some("Why: heavy losses, bloodied")
        );
        assert_eq!(why_text(why::WATER).as_deref(), Some("Why: flooded"));
        // Bits we do not know are ignored.
        assert_eq!(why_text(1 << 15), None);
    }
}
