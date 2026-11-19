//! The online match kit's screen parts: the chat log and chat box, pings
//! (the ground marker, the minimap mark and the Alt-drag wheel), the
//! surrender vote banner and the pause banner. Docs:
//! docs/11-networking.md#the-match-kit, docs/08-ui-ux.md.
//!
//! Like [`alerts`](crate::alerts) this knows nothing of the network: the game
//! turns what `gc_net` tells it into the plain data here ([`KitData`]) each
//! frame, and gets clicks back as [`HudEvent`]s. The chat box's typing is a
//! little state machine ([`ChatBox`]) the game feeds with keys, so it is
//! tested here too.

use crate::draw::{CELL_H, CELL_W, Color, DrawList, Rect, fit, mix, text_width, wrap};
use crate::hud::{HudEvent, HudInput, SlotState, WIDGET_KIT, muted_ink, slot_colors};
use crate::ui::{Press, theme};

/// Longest chat line, in characters (`gc_net` cuts lines it carries to it).
pub const MAX_CHAT: usize = 120;
/// How many chat lines show at once.
pub const SHOWN_LINES: usize = 6;
/// A chat line shows this long, fading over the last [`FADE_MS`].
pub const LINE_LIFE_MS: u64 = 10_000;
const FADE_MS: u64 = 2_000;
/// A ping stays on the ground this long, fading over the last [`PING_FADE_MS`].
pub const PING_LIFE_MS: u64 = 6_000;
const PING_FADE_MS: u64 = 1_500;
/// Room kept by the game for old chat lines: older ones are dropped.
pub const KEPT_LINES: usize = 40;

/// Distance from the press, in UI pixels, beyond which a drag shows the ping
/// wheel; and inside which a release is "Look here".
pub const WHEEL_SHOW: i32 = 6;
pub const WHEEL_DEAD: i32 = 26;
/// How far the wheel's plates sit from its middle, across and up.
const WHEEL_X: i32 = 84;
const WHEEL_Y: i32 = 46;

const MARGIN: i32 = 8;
const ROW: i32 = CELL_H + 2;
const BOX_H: i32 = 26;
/// Text laid on the battlefield sits on a dark plate to stay readable.
const PLATE: Color = [20, 12, 6, 150];
const SOFT_INK: Color = [250, 244, 222, 255];

/// What a ping says. Pings go to the pinger's allies only; on the network
/// a kind is its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PingKind {
    /// "Look here": a plain marker.
    Look = 0,
    Attack = 1,
    Defend = 2,
    /// "Danger": enemies, or keep away.
    Danger = 3,
    OnMyWay = 4,
}

impl PingKind {
    pub const ALL: [PingKind; 5] = [
        PingKind::Look,
        PingKind::Attack,
        PingKind::Defend,
        PingKind::Danger,
        PingKind::OnMyWay,
    ];

    pub fn from_u8(v: u8) -> Option<PingKind> {
        PingKind::ALL.get(usize::from(v)).copied()
    }

    /// The words beside a ping's marker.
    pub fn label(self) -> &'static str {
        match self {
            PingKind::Look => "Look here",
            PingKind::Attack => "Attack here",
            PingKind::Defend => "Defend here",
            PingKind::Danger => "Danger",
            PingKind::OnMyWay => "On my way",
        }
    }

    /// The short word on the wheel's plate.
    fn plate(self) -> &'static str {
        match self {
            PingKind::Look => "Look",
            PingKind::Attack => "Attack",
            PingKind::Defend => "Defend",
            PingKind::Danger => "Danger",
            PingKind::OnMyWay => "On my way",
        }
    }

    /// The marker's colour: red to attack, blue to defend, amber for
    /// danger, green on the way, pale for a plain look.
    pub fn color(self) -> Color {
        match self {
            PingKind::Look => [244, 236, 204, 255],
            PingKind::Attack => [224, 84, 64, 255],
            PingKind::Defend => [98, 150, 214, 255],
            PingKind::Danger => [236, 176, 56, 255],
            PingKind::OnMyWay => [132, 188, 84, 255],
        }
    }
}

/// The ping a drag from the wheel's middle to `(dx, dy)` picks: near the
/// middle "Look here", else up Attack, right On my way, down Defend, left
/// Danger (whichever way the drag goes furthest).
pub fn wheel_pick(dx: i32, dy: i32) -> PingKind {
    if dx * dx + dy * dy < WHEEL_DEAD * WHEEL_DEAD {
        return PingKind::Look;
    }
    if dx.abs() >= dy.abs() {
        if dx > 0 {
            PingKind::OnMyWay
        } else {
            PingKind::Danger
        }
    } else if dy < 0 {
        PingKind::Attack
    } else {
        PingKind::Defend
    }
}

/// Who a chat line was said to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Team,
    All,
    /// "Ana: Attack here": a ping, noted in the log too.
    Ping,
}

/// One line of the chat log.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatLine {
    pub kind: LineKind,
    pub from: String,
    /// The speaker's colour.
    pub color: Color,
    pub text: String,
    /// When it arrived, on the clock the HUD's `now_ms` uses.
    pub born_ms: u64,
}

/// Add `line` to `lines`, forgetting the oldest beyond [`KEPT_LINES`].
pub fn push_line(lines: &mut Vec<ChatLine>, line: ChatLine) {
    lines.push(line);
    if lines.len() > KEPT_LINES {
        lines.remove(0);
    }
}

/// The lines to show at `now_ms` and how solid each is (0 to 1): the last
/// [`SHOWN_LINES`] that are still young, or with the chat box open
/// (`all`) the last ones whatever their age.
pub fn shown(lines: &[ChatLine], now_ms: u64, all: bool) -> Vec<(&ChatLine, f32)> {
    let mut out: Vec<(&ChatLine, f32)> = lines
        .iter()
        .filter_map(|l| {
            let age = now_ms.saturating_sub(l.born_ms);
            if all {
                return Some((l, 1.0));
            }
            if age >= LINE_LIFE_MS {
                return None;
            }
            let left = LINE_LIFE_MS - age;
            Some((l, (left as f32 / FADE_MS as f32).min(1.0)))
        })
        .collect();
    let extra = out.len().saturating_sub(SHOWN_LINES);
    out.drain(..extra);
    out
}

/// A key the chat box understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChatKey {
    /// A typed character (from the key event's text).
    Char(char),
    Backspace,
    Enter,
    Escape,
    /// Switch between team and everyone.
    Tab,
}

/// What a key did to the chat box besides editing it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatAction {
    None,
    /// Enter on a line with words in it: say it (`team`, else everyone).
    Send {
        team: bool,
        text: String,
    },
    /// The box closed without saying anything.
    Closed,
}

/// The chat box: closed, or open with a line being typed.
#[derive(Clone, Debug, Default)]
pub struct ChatBox {
    open: bool,
    team: bool,
    text: String,
}

impl ChatBox {
    /// Open an empty box, for the team or for everyone.
    pub fn open(&mut self, team: bool) {
        self.open = true;
        self.team = team;
        self.text.clear();
    }

    pub fn close(&mut self) {
        self.open = false;
        self.text.clear();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Whether the line goes to the team (else to everyone).
    pub fn team(&self) -> bool {
        self.team
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// One key while the box is open. Every key goes here while it is, so
    /// no hotkey fires.
    pub fn key(&mut self, key: ChatKey) -> ChatAction {
        if !self.open {
            return ChatAction::None;
        }
        match key {
            ChatKey::Char(c) => {
                if (' '..='~').contains(&c) && self.text.chars().count() < MAX_CHAT {
                    self.text.push(c);
                }
                ChatAction::None
            }
            ChatKey::Backspace => {
                self.text.pop();
                ChatAction::None
            }
            ChatKey::Tab => {
                self.team = !self.team;
                ChatAction::None
            }
            ChatKey::Escape => {
                self.close();
                ChatAction::Closed
            }
            ChatKey::Enter => {
                let text = self.text.trim().to_string();
                let team = self.team;
                self.close();
                if text.is_empty() {
                    ChatAction::Closed
                } else {
                    ChatAction::Send { team, text }
                }
            }
        }
    }
}

/// A live ping, for its markers.
#[derive(Clone, Debug, PartialEq)]
pub struct PingMark<'a> {
    /// Comes back in alerts and is the badge's id.
    pub id: u32,
    pub from: &'a str,
    /// The pinger's colour.
    pub color: Color,
    pub choice: PingKind,
    /// Where it is on the map, in tiles.
    pub at_tiles: [f32; 2],
    /// Where it is on the screen, in UI pixels (may be off it); `None` when
    /// it is behind the camera.
    pub at_screen: Option<[f32; 2]>,
    /// How long ago, in milliseconds.
    pub age_ms: u64,
}

/// The surrender vote of this player's team.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteBanner<'a> {
    pub by: &'a str,
    pub yes: u8,
    pub needed: u8,
    /// Whole seconds left.
    pub secs: u32,
    /// This player has said yes.
    pub voted: bool,
}

/// A pause under way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PauseBanner<'a> {
    pub who: &'a str,
    pub secs: u32,
    /// This player may carry on now.
    pub can_resume: bool,
    /// This player's pauses left.
    pub pauses_left: Option<u8>,
}

/// The ping wheel while Alt and the left button are held: where the press
/// was and where the mouse is now, in UI pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wheel {
    pub centre: (i32, i32),
    pub mouse: (i32, i32),
}

impl Wheel {
    /// What letting go now would ping.
    pub fn pick(&self) -> PingKind {
        wheel_pick(self.mouse.0 - self.centre.0, self.mouse.1 - self.centre.1)
    }

    fn showing(&self) -> bool {
        let (dx, dy) = (self.mouse.0 - self.centre.0, self.mouse.1 - self.centre.1);
        dx * dx + dy * dy >= WHEEL_SHOW * WHEEL_SHOW
    }
}

/// What the kit shows this frame. Everything defaults to nothing, so a
/// match without the kit (offline) draws nothing.
#[derive(Clone, Debug, Default)]
pub struct KitData<'a> {
    /// Side of the map, in tiles, for the minimap marks.
    pub map_tiles: f32,
    pub chat: &'a [ChatLine],
    /// The chat box, while it is open.
    pub chat_box: Option<&'a ChatBox>,
    pub pings: &'a [PingMark<'a>],
    pub vote: Option<VoteBanner<'a>>,
    pub pause: Option<PauseBanner<'a>>,
    pub wheel: Option<Wheel>,
}

/// A tooltip a kit button asks for; the HUD draws it once the mouse rests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitTip {
    pub id: u32,
    pub anchor: Rect,
    pub name: String,
    pub key: String,
    pub text: String,
}

/// Widget ids (within [`WIDGET_KIT`]) and tooltip ids of the vote buttons.
const VOTE_YES: u32 = 0;
const VOTE_NO: u32 = 1;
const TIP_YES: u32 = 120;
const TIP_NO: u32 = 121;

/// The vote banner's panel and its Yes and No buttons: the top right of
/// the `field`, clear of the network lines at the top left.
pub fn vote_rects(field: Rect) -> (Rect, Rect, Rect) {
    let w = 520.min(field.w - 2 * MARGIN).max(0);
    let panel = Rect::new(field.right() - MARGIN - w, field.y + MARGIN, w, 34);
    let yes = Rect::new(panel.right() - 128, panel.y + 6, 56, 22);
    let no = Rect::new(panel.right() - 66, panel.y + 6, 56, 22);
    (panel, yes, no)
}

/// The chat box: the bottom-left of the `field`.
pub fn chat_box_rect(field: Rect) -> Rect {
    let w = (field.w / 2)
        .clamp(360, 760)
        .min(field.w - 2 * MARGIN)
        .max(0);
    Rect::new(
        field.x + MARGIN,
        (field.bottom() - MARGIN - BOX_H).max(field.y),
        w,
        BOX_H,
    )
}

/// The text of the vote banner.
pub fn vote_text(v: &VoteBanner) -> String {
    if v.voted {
        format!("You said yes: {} of {}, {} s", v.yes, v.needed, v.secs)
    } else {
        format!(
            "{} asks to surrender: {} of {} yes, {} s",
            v.by, v.yes, v.needed, v.secs
        )
    }
}

/// The lines of the pause banner.
pub fn pause_lines(p: &PauseBanner) -> Vec<String> {
    let mut lines = vec![format!("Paused by {}: {} s", p.who, p.secs)];
    if p.can_resume {
        lines.push("P: carry on".into());
    }
    match p.pauses_left {
        Some(0) => lines.push("You have no pauses left".into()),
        Some(1) => lines.push("You have 1 pause left".into()),
        Some(n) => lines.push(format!("You have {n} pauses left")),
        None => {}
    }
    lines
}

fn faded(c: Color, solid: f32) -> Color {
    [
        c[0],
        c[1],
        c[2],
        (f32::from(c[3]) * solid.clamp(0.0, 1.0)) as u8,
    ]
}

/// A colour lightened so it reads on the dark plate.
fn bright(c: Color) -> Color {
    mix(c, [255, 255, 255, 255], 0.35)
}

/// A line's header: "[Team] Ana: ", "[All] Ana: " or "Ana: ", with the
/// name cut to a sensible length.
fn header(line: &ChatLine) -> (&'static str, String) {
    let tag = match line.kind {
        LineKind::Team => "[Team] ",
        LineKind::All => "[All] ",
        LineKind::Ping => "",
    };
    (tag, format!("{}: ", fit(&line.from, 16)))
}

/// Draw the kit, report clicks, note in `over` each rectangle that takes
/// the mouse from the battlefield, and return the tooltip a hovered
/// button wants. `field` is the screen between the bars, `minimap` the
/// minimap's rectangle.
#[allow(clippy::too_many_arguments)]
pub(crate) fn draw(
    draw: &mut DrawList,
    field: Rect,
    minimap: Rect,
    input: HudInput,
    press: &mut Press,
    kit: &KitData,
    over: &mut Vec<Rect>,
    events: &mut Vec<HudEvent>,
) -> Option<KitTip> {
    for p in kit.pings {
        if let Some(at) = p.at_screen {
            ping_marker(draw, field, p, at, input.now_ms);
        }
        minimap_mark(draw, minimap, kit.map_tiles, p, input.now_ms);
    }
    chat_log(draw, field, kit, input.now_ms);
    if let Some(p) = &kit.pause {
        pause_banner(draw, field, p);
    }
    let tip = kit
        .vote
        .as_ref()
        .and_then(|v| vote_banner(draw, field, input, press, v, over, events));
    if let Some(b) = kit.chat_box {
        chat_box(draw, field, b, input.now_ms);
        over.push(chat_box_rect(field));
    }
    if let Some(w) = &kit.wheel
        && w.showing()
    {
        wheel(draw, w);
    }
    tip
}

/// A pulsing ring on the ground, a flag over it and "Name: Label" above.
fn ping_marker(draw: &mut DrawList, field: Rect, p: &PingMark, at: [f32; 2], now_ms: u64) {
    if p.age_ms >= PING_LIFE_MS {
        return;
    }
    let (cx, cy) = (at[0] as i32, at[1] as i32);
    // Wholly in view, with room for the flag and words, or the edge badge
    // has it.
    if !field.inset(30).contains(cx, cy) {
        return;
    }
    let solid = ((PING_LIFE_MS - p.age_ms) as f32 / PING_FADE_MS as f32).min(1.0);
    let color = p.choice.color();
    // Two rings, a half-beat apart, spreading and thinning out.
    for k in 0..2u64 {
        let phase = ((now_ms + k * 450) % 900) as f32 / 900.0;
        let r = 8.0 + 22.0 * phase;
        let a = solid * (1.0 - phase);
        for step in 0..20 {
            let t = step as f32 / 20.0 * std::f32::consts::TAU;
            let (x, y) = (cx as f32 + r * t.cos(), cy as f32 + r * 0.5 * t.sin());
            draw.rect(Rect::new(x as i32 - 1, y as i32 - 1, 3, 3), faded(color, a));
        }
    }
    // The foot, the pole and the flag.
    draw.rect(Rect::new(cx - 3, cy - 2, 7, 5), faded(theme::INK, solid));
    draw.rect(Rect::new(cx - 2, cy - 1, 5, 3), faded(color, solid));
    draw.rect(Rect::new(cx - 1, cy - 34, 3, 33), faded(theme::INK, solid));
    for row in 0..10 {
        let w = 14 - row;
        draw.rect(Rect::new(cx + 2, cy - 34 + row, w, 1), faded(color, solid));
    }
    let label = format!("{}: {}", fit(p.from, 16), p.choice.label());
    let w = text_width(&label, 1) + 10;
    let plate = Rect::new(cx - w / 2, cy - 34 - CELL_H - 8, w, CELL_H + 4);
    draw.rect(plate, faded(PLATE, solid));
    let name = format!("{}: ", fit(p.from, 16));
    let x = plate.x + 5;
    draw.text(x, plate.y + 2, 1, faded(bright(p.color), solid), &name);
    draw.text(
        x + text_width(&name, 1),
        plate.y + 2,
        1,
        faded(color, solid),
        p.choice.label(),
    );
}

/// A pulsing square on the minimap where the ping is.
fn minimap_mark(draw: &mut DrawList, minimap: Rect, map_tiles: f32, p: &PingMark, now_ms: u64) {
    if p.age_ms >= PING_LIFE_MS || map_tiles <= 0.0 {
        return;
    }
    let x = minimap.x + (p.at_tiles[0] / map_tiles * minimap.w as f32) as i32;
    let y = minimap.y + (p.at_tiles[1] / map_tiles * minimap.h as f32) as i32;
    if !minimap.inset(1).contains(x, y) {
        return;
    }
    let solid = ((PING_LIFE_MS - p.age_ms) as f32 / PING_FADE_MS as f32).min(1.0);
    let pulse = ((now_ms % 900) as f32 / 900.0 * 6.0) as i32;
    let r = 3 + pulse;
    draw.border(
        Rect::new(x - r, y - r, 2 * r + 1, 2 * r + 1),
        1,
        faded(p.choice.color(), solid),
    );
    draw.rect(Rect::new(x - 2, y - 2, 5, 5), faded(theme::INK, solid));
    draw.rect(
        Rect::new(x - 1, y - 1, 3, 3),
        faded(p.choice.color(), solid),
    );
}

/// The recent chat lines, bottom-left, above the chat box when it is open.
fn chat_log(draw: &mut DrawList, field: Rect, kit: &KitData, now_ms: u64) {
    let open = kit.chat_box.is_some();
    let lines = shown(kit.chat, now_ms, open);
    if lines.is_empty() {
        return;
    }
    let width = chat_box_rect(field).w;
    let max_chars = ((width - 12) / CELL_W).max(8) as usize;
    // Each line wrapped; the oldest are dropped if the lot would be tall.
    let mut rows: Vec<(Vec<String>, &ChatLine, f32)> = lines
        .iter()
        .map(|&(l, solid)| {
            let (tag, who) = header(l);
            let head = tag.len() + who.len();
            let whole = format!("{tag}{who}{}", l.text);
            let mut wrapped = wrap(&whole, max_chars);
            // The header always opens the first row, even when wrapped
            // before the words after it.
            if wrapped
                .first()
                .is_none_or(|r| r.len() < head.min(whole.len()))
            {
                wrapped = vec![whole];
            }
            (wrapped, l, solid)
        })
        .collect();
    let tall =
        |rows: &[(Vec<String>, &ChatLine, f32)]| rows.iter().map(|r| r.0.len()).sum::<usize>();
    let max_rows = ((field.h / 2) / ROW).max(2) as usize;
    while rows.len() > 1 && tall(&rows) > max_rows {
        rows.remove(0);
    }
    let count = tall(&rows) as i32;
    let base = if open {
        chat_box_rect(field).y - 4
    } else {
        field.bottom() - MARGIN
    };
    let top = (base - count * ROW).max(field.y);
    let mut y = top;
    for (wrapped, line, solid) in &rows {
        let (tag, who) = header(line);
        for (i, row) in wrapped.iter().enumerate() {
            let plate_w = text_width(row, 1) + 12;
            draw.rect(
                Rect::new(field.x + MARGIN, y, plate_w, ROW),
                faded(PLATE, *solid),
            );
            let mut x = field.x + MARGIN + 6;
            let ty = y + 1;
            let mut rest = row.as_str();
            if i == 0 {
                // The tag, the name and the words, each in its colour.
                for (part, color) in [(tag, theme::WELD_HI), (who.as_str(), bright(line.color))] {
                    if let Some(p) = rest.strip_prefix(part) {
                        draw.text(x, ty, 1, faded(color, *solid), part);
                        x += text_width(part, 1);
                        rest = p;
                    }
                }
            }
            let ink = if line.kind == LineKind::Ping {
                bright(PingKind::Look.color())
            } else {
                SOFT_INK
            };
            draw.text(x, ty, 1, faded(ink, *solid), rest);
            y += ROW;
        }
    }
}

/// The chat box: who it speaks to, the line with a caret, and the keys.
fn chat_box(draw: &mut DrawList, field: Rect, b: &ChatBox, now_ms: u64) {
    let r = chat_box_rect(field);
    draw.boxed(r, theme::FIELD_CLOTH, 2);
    let who = if b.team() { "[Team] " } else { "[All] " };
    let who_color = if b.team() { theme::WOAD } else { theme::MADDER };
    draw.text_bold(r.x + 8, r.y + (r.h - CELL_H) / 2, 1, who_color, who);
    let hint = "Enter send  Tab team/all  Esc cancel";
    let hint_w = text_width(hint, 1) + 12;
    let show_hint = r.w > 2 * hint_w + 80;
    let right = if show_hint {
        r.right() - hint_w
    } else {
        r.right() - 8
    };
    if show_hint {
        draw.text(right + 4, r.y + (r.h - CELL_H) / 2, 1, muted_ink(), hint);
    }
    let x0 = r.x + 8 + text_width(who, 1);
    let room = ((right - x0 - CELL_W) / CELL_W).max(1) as usize;
    // Long lines show their end, where the caret is.
    let text = b.text();
    let count = text.chars().count();
    let shown: String = text.chars().skip(count.saturating_sub(room)).collect();
    let y = r.y + (r.h - CELL_H) / 2;
    draw.text(x0, y, 1, theme::INK, &shown);
    if (now_ms / 500).is_multiple_of(2) {
        let cx = x0 + text_width(&shown, 1);
        draw.rect(Rect::new(cx, y + 1, 2, CELL_H - 2), theme::INK);
    }
    if count > MAX_CHAT - 20 {
        let n = format!("{count}/{MAX_CHAT}");
        draw.text(
            right - text_width(&n, 1) - 8,
            y,
            1,
            if count >= MAX_CHAT {
                theme::BAD_INK
            } else {
                muted_ink()
            },
            &n,
        );
    }
}

/// The surrender vote banner at the top right, under the top bar; returns a tooltip for the
/// button the mouse rests on.
fn vote_banner(
    draw: &mut DrawList,
    field: Rect,
    input: HudInput,
    press: &mut Press,
    v: &VoteBanner,
    over: &mut Vec<Rect>,
    events: &mut Vec<HudEvent>,
) -> Option<KitTip> {
    let (panel, yes, no) = vote_rects(field);
    draw.shadow(panel, 3);
    draw.boxed(panel, theme::LINEN, 2);
    draw.border(panel.inset(3), 1, theme::MADDER);
    over.push(panel);
    let text = vote_text(v);
    // Cut to the room the buttons leave.
    let room = if v.voted {
        panel.w - 20
    } else {
        yes.x - panel.x - 16
    };
    let text = fit(&text, (room / CELL_W).max(4) as usize);
    draw.text_bold(
        panel.x + 10,
        panel.y + (panel.h - CELL_H) / 2,
        1,
        theme::INK,
        &text,
    );
    if v.voted {
        return None;
    }
    let mut tip = None;
    for (id, r, label, vote) in [(VOTE_YES, yes, "Yes", true), (VOTE_NO, no, "No", false)] {
        let hot = r.contains(input.mouse.0, input.mouse.1);
        let (clicked, down) = press.widget(
            WIDGET_KIT + id,
            r,
            input.mouse,
            input.left_pressed,
            input.left_released,
        );
        if clicked {
            events.push(HudEvent::SurrenderVote(vote));
        }
        let (mut back, _, ink) = slot_colors(SlotState::Ready, hot);
        if down {
            back = mix(back, theme::INK, 0.15);
        }
        draw.boxed(r, back, 2);
        draw.text_centered(r, 1, ink, label);
        if hot {
            tip = Some(if vote {
                KitTip {
                    id: TIP_YES,
                    anchor: r,
                    name: "Vote yes".into(),
                    key: String::new(),
                    text: "Your team gives up the battle once enough of you say yes. It counts as a defeat for the team.".into(),
                }
            } else {
                KitTip {
                    id: TIP_NO,
                    anchor: r,
                    name: "Vote no".into(),
                    key: String::new(),
                    text: "Keep fighting. If too few say yes before the time runs out, the match goes on.".into(),
                }
            });
        }
    }
    tip
}

/// "Paused by Ana: 47 s" in the middle of the field.
fn pause_banner(draw: &mut DrawList, field: Rect, p: &PauseBanner) {
    let lines = pause_lines(p);
    let chars = lines.iter().map(|l| l.chars().count()).max().unwrap_or(1) as i32;
    let w = (chars * CELL_W + 48).min(field.w);
    let h = lines.len() as i32 * ROW + 24;
    let r = Rect::new(
        field.x + (field.w - w) / 2,
        field.y + (field.h - h) / 2,
        w,
        h,
    );
    draw.shadow(r, 4);
    draw.boxed(r, theme::LINEN, 2);
    draw.border(r.inset(3), 1, theme::WELD);
    for (i, l) in lines.iter().enumerate() {
        let row = Rect::new(r.x, r.y + 12 + i as i32 * ROW, r.w, ROW);
        if i == 0 {
            draw.text_centered(row, 1, theme::MADDER, l);
            draw.text_centered(
                Rect::new(row.x + 1, row.y, row.w, row.h),
                1,
                theme::MADDER,
                l,
            );
        } else {
            draw.text_centered(row, 1, theme::INK, l);
        }
    }
}

/// The four plates and the middle one of the ping wheel, the pick lit.
fn wheel(draw: &mut DrawList, w: &Wheel) {
    let (cx, cy) = w.centre;
    let pick = w.pick();
    draw.rect(
        Rect::new(
            cx - WHEEL_DEAD,
            cy - WHEEL_DEAD,
            2 * WHEEL_DEAD,
            2 * WHEEL_DEAD,
        ),
        [20, 12, 6, 70],
    );
    let plates = [
        (PingKind::Look, (0, 0)),
        (PingKind::Attack, (0, -WHEEL_Y)),
        (PingKind::OnMyWay, (WHEEL_X, 0)),
        (PingKind::Defend, (0, WHEEL_Y)),
        (PingKind::Danger, (-WHEEL_X, 0)),
    ];
    for (choice, (dx, dy)) in plates {
        let label = choice.plate();
        let pw = text_width(label, 1) + 16;
        let r = Rect::new(cx + dx - pw / 2, cy + dy - 12, pw, 24);
        let lit = choice == pick;
        let fill = if lit { choice.color() } else { theme::LINEN };
        draw.shadow(r, 2);
        draw.boxed(r, fill, 2);
        draw.text_centered(r, 1, theme::INK, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::draw::SOLID;

    const FIELD: Rect = Rect::new(0, 26, 1280, 566);
    const MAP: Rect = Rect::new(8, 600, 150, 150);
    const ANA: Color = [200, 60, 50, 255];

    fn line(kind: LineKind, from: &str, text: &str, born_ms: u64) -> ChatLine {
        ChatLine {
            kind,
            from: from.into(),
            color: ANA,
            text: text.into(),
            born_ms,
        }
    }

    /// Everything drawn as text, a space where the glyphs leave a gap.
    fn words(d: &DrawList) -> String {
        let mut s = String::new();
        let mut last: Option<(u16, u16)> = None;
        for q in d.quads.iter().filter(|q| q.glyph != SOLID) {
            if let Some((x, y)) = last
                && (q.pos[1] != y || q.pos[0] > x + CELL_W as u16)
            {
                s.push(' ');
            }
            s.push((q.glyph as u8 + 32) as char);
            last = Some((q.pos[0], q.pos[1]));
        }
        s
    }

    struct Run {
        draw: DrawList,
        events: Vec<HudEvent>,
        over: Vec<Rect>,
        tip: Option<KitTip>,
    }

    fn run(kit: &KitData, press: &mut Press, input: HudInput) -> Run {
        let mut r = Run {
            draw: DrawList::default(),
            events: Vec::new(),
            over: Vec::new(),
            tip: None,
        };
        r.tip = draw(
            &mut r.draw,
            FIELD,
            MAP,
            input,
            press,
            kit,
            &mut r.over,
            &mut r.events,
        );
        press.end_frame(input.left_released);
        r
    }

    fn at_ms(now_ms: u64) -> HudInput {
        HudInput {
            now_ms,
            ..HudInput::default()
        }
    }

    fn click_at(kit: &KitData, p: (i32, i32)) -> Vec<HudEvent> {
        let mut press = Press::default();
        let down = HudInput {
            mouse: p,
            left_pressed: true,
            ..HudInput::default()
        };
        run(kit, &mut press, down);
        let up = HudInput {
            mouse: p,
            left_released: true,
            ..HudInput::default()
        };
        run(kit, &mut press, up).events
    }

    fn type_in(b: &mut ChatBox, s: &str) {
        for c in s.chars() {
            assert_eq!(b.key(ChatKey::Char(c)), ChatAction::None);
        }
    }

    #[test]
    fn the_chat_box_types_edits_and_sends() {
        let mut b = ChatBox::default();
        assert_eq!(b.key(ChatKey::Char('x')), ChatAction::None);
        assert_eq!(b.text(), "", "closed: keys do nothing");
        b.open(true);
        assert!(b.is_open() && b.team());
        type_in(&mut b, "helo");
        b.key(ChatKey::Backspace);
        type_in(&mut b, "lo!");
        assert_eq!(b.text(), "hello!");
        // Control characters and what the font can't show are not typed.
        b.key(ChatKey::Char('\u{7}'));
        b.key(ChatKey::Char('é'));
        assert_eq!(b.text(), "hello!");
        assert_eq!(
            b.key(ChatKey::Enter),
            ChatAction::Send {
                team: true,
                text: "hello!".into()
            }
        );
        assert!(!b.is_open());
    }

    #[test]
    fn tab_switches_between_team_and_all_and_keeps_the_words() {
        let mut b = ChatBox::default();
        b.open(false);
        type_in(&mut b, "gg");
        assert!(!b.team());
        b.key(ChatKey::Tab);
        assert!(b.team());
        b.key(ChatKey::Tab);
        assert_eq!(
            b.key(ChatKey::Enter),
            ChatAction::Send {
                team: false,
                text: "gg".into()
            }
        );
    }

    #[test]
    fn escape_and_empty_enter_close_without_sending() {
        let mut b = ChatBox::default();
        b.open(true);
        type_in(&mut b, "never mind");
        assert_eq!(b.key(ChatKey::Escape), ChatAction::Closed);
        assert!(!b.is_open() && b.text().is_empty());
        b.open(true);
        type_in(&mut b, "   ");
        assert_eq!(b.key(ChatKey::Enter), ChatAction::Closed);
        b.open(false);
        assert_eq!(b.key(ChatKey::Enter), ChatAction::Closed);
    }

    #[test]
    fn a_line_is_cut_at_120_characters() {
        let mut b = ChatBox::default();
        b.open(true);
        type_in(&mut b, &"a".repeat(200));
        assert_eq!(b.text().chars().count(), MAX_CHAT);
        b.key(ChatKey::Backspace);
        b.key(ChatKey::Char('b'));
        assert!(b.text().ends_with('b') && b.text().len() == MAX_CHAT);
    }

    #[test]
    fn the_log_shows_the_last_six_young_lines_and_fades_them() {
        let lines: Vec<ChatLine> = (0..9)
            .map(|i| line(LineKind::All, "Ana", &format!("m{i}"), 1_000 + i * 100))
            .collect();
        let s = shown(&lines, 2_000, false);
        assert_eq!(s.len(), SHOWN_LINES);
        assert_eq!(s[0].0.text, "m3");
        assert!(s.iter().all(|&(_, a)| a == 1.0));
        // Fading over the last two seconds, gone after ten.
        let s = shown(&lines, 1_000 + LINE_LIFE_MS - 500, false);
        assert!(s.iter().all(|&(_, a)| a > 0.0 && a < 1.0), "{s:?}");
        assert!(shown(&lines, 1_000 + LINE_LIFE_MS + 900, false).is_empty());
        // The chat box open shows them whatever their age.
        assert_eq!(shown(&lines, 99_000, true).len(), SHOWN_LINES);
    }

    #[test]
    fn old_lines_are_forgotten_past_the_keep() {
        let mut lines = Vec::new();
        for i in 0..KEPT_LINES + 5 {
            push_line(&mut lines, line(LineKind::All, "Ana", &i.to_string(), 0));
        }
        assert_eq!(lines.len(), KEPT_LINES);
        assert_eq!(lines[0].text, "5");
    }

    #[test]
    fn chat_lines_read_like_the_spec_above_the_bottom_bar() {
        let chat = [
            line(LineKind::Team, "Ana", "push left", 0),
            line(LineKind::All, "Bo", "good luck", 0),
            line(LineKind::Ping, "Ana", "Attack here", 0),
        ];
        let kit = KitData {
            chat: &chat,
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(500));
        let w = words(&r.draw);
        for want in [
            "[Team] Ana: push left",
            "[All] Bo: good luck",
            "Ana: Attack here",
        ] {
            assert!(w.contains(want), "{want:?} in {w:?}");
        }
        // Lower left, above the bottom bar, and the click-through is left alone.
        assert!(r.over.is_empty());
        let bottom = r
            .draw
            .quads
            .iter()
            .map(|q| i32::from(q.pos[1]) + i32::from(q.size[1]))
            .max()
            .unwrap();
        assert!(bottom <= FIELD.bottom(), "{bottom}");
        assert!(
            r.draw
                .quads
                .iter()
                .all(|q| i32::from(q.pos[0]) < FIELD.w / 2 + 40),
            "the log keeps to the left half"
        );
        // The name is tinted by the player's colour (brightened a little).
        let name_glyph = r
            .draw
            .quads
            .iter()
            .find(|q| q.glyph == u32::from(b'A' - 32))
            .unwrap();
        assert!(name_glyph.color[0] > name_glyph.color[2], "reddish");
        // After ten seconds they are gone.
        let r = run(&kit, &mut Press::default(), at_ms(LINE_LIFE_MS + 5_000));
        assert!(words(&r.draw).is_empty());
    }

    #[test]
    fn a_long_line_wraps_instead_of_running_off() {
        let chat = [line(LineKind::Team, "Ana", &"word ".repeat(24), 0)];
        let kit = KitData {
            chat: &chat,
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(0));
        let right = r
            .draw
            .quads
            .iter()
            .filter(|q| q.glyph != SOLID)
            .map(|q| i32::from(q.pos[0]) + i32::from(q.size[0]))
            .max()
            .unwrap();
        assert!(right <= FIELD.w / 2 + 40, "{right}");
        let rows: std::collections::BTreeSet<_> = r
            .draw
            .quads
            .iter()
            .filter(|q| q.glyph != SOLID)
            .map(|q| q.pos[1])
            .collect();
        assert!(rows.len() >= 2, "{rows:?}");
    }

    #[test]
    fn the_open_chat_box_shows_its_text_who_it_is_for_and_the_old_lines() {
        let chat = [line(LineKind::All, "Bo", "old news", 0)];
        let mut b = ChatBox::default();
        b.open(true);
        type_in(&mut b, "attack now");
        let kit = KitData {
            chat: &chat,
            chat_box: Some(&b),
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(99_000));
        let w = words(&r.draw);
        assert!(w.contains("[Team]") && w.contains("attack now"), "{w:?}");
        assert!(w.contains("old news"), "all lines show while typing");
        assert!(r.over.contains(&chat_box_rect(FIELD)));
        // The log stands above the box.
        let log_y = r
            .draw
            .quads
            .iter()
            .filter(|q| q.glyph == u32::from(b'o' - 32))
            .map(|q| i32::from(q.pos[1]))
            .min()
            .unwrap();
        assert!(log_y < chat_box_rect(FIELD).y);
        b.key(ChatKey::Tab);
        let kit = KitData {
            chat_box: Some(&b),
            ..KitData::default()
        };
        assert!(words(&run(&kit, &mut Press::default(), at_ms(0)).draw).contains("[All]"));
    }

    #[test]
    fn the_caret_blinks() {
        let mut b = ChatBox::default();
        b.open(true);
        let kit = KitData {
            chat_box: Some(&b),
            ..KitData::default()
        };
        let solid = |ms| {
            run(&kit, &mut Press::default(), at_ms(ms))
                .draw
                .quads
                .iter()
                .filter(|q| q.glyph == SOLID)
                .count()
        };
        assert_eq!(solid(0), solid(500) + 1);
    }

    #[test]
    fn the_wheel_picks_by_direction_and_the_middle_is_look() {
        assert_eq!(wheel_pick(0, 0), PingKind::Look);
        assert_eq!(wheel_pick(10, -10), PingKind::Look);
        assert_eq!(wheel_pick(0, -60), PingKind::Attack);
        assert_eq!(wheel_pick(60, 5), PingKind::OnMyWay);
        assert_eq!(wheel_pick(-5, 60), PingKind::Defend);
        assert_eq!(wheel_pick(-60, -10), PingKind::Danger);
        // Diagonals go to the longer way.
        assert_eq!(wheel_pick(50, -30), PingKind::OnMyWay);
        assert_eq!(wheel_pick(-30, -50), PingKind::Attack);
    }

    #[test]
    fn the_wheel_shows_after_a_drag_with_the_pick_lit() {
        let at = |mouse| KitData {
            wheel: Some(Wheel {
                centre: (400, 300),
                mouse,
            }),
            ..KitData::default()
        };
        // A click that has not moved: nothing drawn.
        let r = run(&at((401, 300)), &mut Press::default(), at_ms(0));
        assert!(r.draw.quads.is_empty());
        let r = run(&at((400, 240)), &mut Press::default(), at_ms(0));
        let w = words(&r.draw);
        for p in ["Look", "Attack", "On my way", "Defend", "Danger"] {
            assert!(w.contains(p), "{p} in {w:?}");
        }
        let lit = PingKind::Attack.color();
        assert!(
            r.draw
                .quads
                .iter()
                .any(|q| q.glyph == SOLID && q.color == lit)
        );
        assert!(
            !r.draw
                .quads
                .iter()
                .any(|q| q.glyph == SOLID && q.color == PingKind::Danger.color())
        );
    }

    fn mark(
        from: &str,
        choice: PingKind,
        at_screen: Option<[f32; 2]>,
        age_ms: u64,
    ) -> PingMark<'_> {
        PingMark {
            id: 1,
            from,
            color: ANA,
            choice,
            at_tiles: [128.0, 64.0],
            at_screen,
            age_ms,
        }
    }

    #[test]
    fn a_ping_marks_the_ground_the_minimap_and_says_who_and_what() {
        let pings = [mark("Ana", PingKind::Attack, Some([640.0, 300.0]), 100)];
        let kit = KitData {
            map_tiles: 256.0,
            pings: &pings,
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(100));
        assert!(words(&r.draw).contains("Ana: Attack here"));
        let red = PingKind::Attack.color();
        let on_ground = |d: &DrawList| {
            d.quads.iter().any(|q| {
                q.glyph == SOLID
                    && q.color == red
                    && !MAP.contains(i32::from(q.pos[0]), i32::from(q.pos[1]))
            })
        };
        assert!(on_ground(&r.draw));
        // The minimap's half-way across, a quarter down.
        let mid = (MAP.x + MAP.w / 2, MAP.y + MAP.h / 4);
        assert!(
            r.draw.quads.iter().any(|q| q.glyph == SOLID
                && q.color == red
                && (i32::from(q.pos[0]) - mid.0).abs() <= 3
                && (i32::from(q.pos[1]) - mid.1).abs() <= 3),
            "a dot on the minimap"
        );
    }

    #[test]
    fn a_ping_off_screen_has_no_ground_marker_but_still_the_minimap_dot() {
        let pings = [mark("Ana", PingKind::Look, Some([-400.0, 300.0]), 100)];
        let kit = KitData {
            map_tiles: 256.0,
            pings: &pings,
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(100));
        assert!(!words(&r.draw).contains("Ana"));
        assert!(!r.draw.quads.is_empty(), "the minimap mark");
        assert!(r.draw.quads.iter().all(|q| {
            MAP.inset(-8)
                .contains(i32::from(q.pos[0]), i32::from(q.pos[1]))
        }));
    }

    #[test]
    fn a_ping_fades_and_ends_after_six_seconds() {
        let at = |age| {
            let pings = [mark("Ana", PingKind::Danger, Some([640.0, 300.0]), age)];
            let kit = KitData {
                map_tiles: 256.0,
                pings: &pings,
                ..KitData::default()
            };
            run(&kit, &mut Press::default(), at_ms(age)).draw
        };
        let alpha = |d: &DrawList| d.quads.iter().map(|q| q.color[3]).max().unwrap_or(0);
        assert_eq!(alpha(&at(1_000)), 255);
        let fading = alpha(&at(PING_LIFE_MS - 300));
        assert!(fading < 255 && fading > 0, "{fading}");
        assert!(at(PING_LIFE_MS).quads.is_empty());
    }

    fn vote(voted: bool) -> VoteBanner<'static> {
        VoteBanner {
            by: "Ana",
            yes: 1,
            needed: 2,
            secs: 23,
            voted,
        }
    }

    #[test]
    fn the_vote_banner_asks_and_the_buttons_answer() {
        let kit = KitData {
            vote: Some(vote(false)),
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(0));
        let w = words(&r.draw);
        assert!(
            w.contains("Ana asks to surrender: 1 of 2 yes, 23 s"),
            "{w:?}"
        );
        assert!(w.contains("Yes") && w.contains("No"));
        let (panel, yes, no) = vote_rects(FIELD);
        assert!(r.over.contains(&panel));
        assert!(panel.y >= FIELD.y && panel.bottom() < FIELD.y + 60);
        let mid = |r: Rect| (r.x + r.w / 2, r.y + r.h / 2);
        assert_eq!(
            click_at(&kit, mid(yes)),
            vec![HudEvent::SurrenderVote(true)]
        );
        assert_eq!(
            click_at(&kit, mid(no)),
            vec![HudEvent::SurrenderVote(false)]
        );
        assert!(click_at(&kit, (panel.x + 5, panel.y + 5)).is_empty());
    }

    #[test]
    fn a_press_that_slides_off_a_vote_button_does_not_vote() {
        let kit = KitData {
            vote: Some(vote(false)),
            ..KitData::default()
        };
        let (_, yes, _) = vote_rects(FIELD);
        let mut press = Press::default();
        run(
            &kit,
            &mut press,
            HudInput {
                mouse: (yes.x + 3, yes.y + 3),
                left_pressed: true,
                ..HudInput::default()
            },
        );
        let r = run(
            &kit,
            &mut press,
            HudInput {
                mouse: (10, 400),
                left_released: true,
                ..HudInput::default()
            },
        );
        assert!(r.events.is_empty());
    }

    #[test]
    fn once_you_voted_the_banner_says_so_and_has_no_buttons() {
        let kit = KitData {
            vote: Some(vote(true)),
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(0));
        let w = words(&r.draw);
        assert!(w.contains("You said yes: 1 of 2, 23 s"), "{w:?}");
        assert!(!w.contains("Yes") && !w.ends_with("No"));
        let (_, yes, _) = vote_rects(FIELD);
        assert!(click_at(&kit, (yes.x + 3, yes.y + 3)).is_empty());
    }

    #[test]
    fn the_vote_buttons_ask_for_tooltips() {
        let kit = KitData {
            vote: Some(vote(false)),
            ..KitData::default()
        };
        let (_, yes, no) = vote_rects(FIELD);
        let tip = |r: Rect| {
            run(
                &kit,
                &mut Press::default(),
                HudInput {
                    mouse: (r.x + 4, r.y + 4),
                    ..HudInput::default()
                },
            )
            .tip
        };
        let t = tip(yes).unwrap();
        assert_eq!((t.name.as_str(), t.anchor), ("Vote yes", yes));
        assert!(t.text.contains("defeat"));
        assert_eq!(tip(no).unwrap().name, "Vote no");
        assert!(
            run(&kit, &mut Press::default(), at_ms(0)).tip.is_none(),
            "none when the mouse is elsewhere"
        );
    }

    #[test]
    fn the_pause_banner_tells_who_how_long_and_what_you_can_do() {
        let p = PauseBanner {
            who: "Ana",
            secs: 47,
            can_resume: true,
            pauses_left: Some(2),
        };
        let kit = KitData {
            pause: Some(p),
            ..KitData::default()
        };
        let r = run(&kit, &mut Press::default(), at_ms(0));
        let w = words(&r.draw);
        for want in [
            "Paused by Ana: 47 s",
            "P: carry on",
            "You have 2 pauses left",
        ] {
            assert!(w.contains(want), "{want:?} in {w:?}");
        }
        // In the middle of the field.
        let xs: Vec<i32> = r
            .draw
            .quads
            .iter()
            .filter(|q| q.glyph != SOLID)
            .map(|q| i32::from(q.pos[0]))
            .collect();
        let mid = xs.iter().sum::<i32>() / xs.len() as i32;
        assert!((mid - FIELD.w / 2).abs() < 80, "{mid}");
        // Not yours to end yet; singular and none.
        let lines = |can_resume, left| {
            pause_lines(&PauseBanner {
                can_resume,
                pauses_left: left,
                ..p
            })
        };
        assert_eq!(lines(false, Some(1)).len(), 2);
        assert!(!lines(false, Some(1)).iter().any(|l| l.contains("carry on")));
        assert_eq!(lines(false, Some(1))[1], "You have 1 pause left");
        assert_eq!(lines(true, Some(0))[2], "You have no pauses left");
        assert_eq!(lines(true, None).len(), 2);
    }

    #[test]
    fn with_nothing_to_show_nothing_is_drawn() {
        let r = run(&KitData::default(), &mut Press::default(), at_ms(0));
        assert!(r.draw.quads.is_empty() && r.over.is_empty() && r.events.is_empty());
    }
}
