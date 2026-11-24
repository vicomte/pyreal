//! Hide Interface: one key hides the HUD and every window for a clean picture, and brings them
//! back as they were.
//!
//! The hidden interface still runs each frame, with the pointer and the keys kept from it, and
//! what it drew is cut from the draw list. The game's boxes are drawn after the cut, so they show.

use pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;

use crate::art::Family;
use crate::draw::{with_alpha, Rect};
use crate::ui::game::{GameState, KeyBindingsView};
use crate::ui::input::vk;
use crate::ui::kit::Ctx;
use crate::ui::paint::{Align, Painter, TextStyle};

/// How long the notice saying how to show the interface stays, in seconds, before it fades.
const NOTICE_SECONDS: f64 = 4.0;

/// Follow this frame's input: the Hide Interface key hides or shows the interface. While it is
/// hidden, Escape, a window's or the chat's key, or the player's death shows it again. `hidden`
/// is when it was hidden, while it is.
pub fn follow(hidden: &mut Option<f64>, ctx: &mut Ctx<'_>, state: &GameState, now: f64) {
    let before = ctx.input.actions.len();
    ctx.input.actions.retain(|a| *a != HIDE_INTERFACE.0);
    for _ in ctx.input.actions.len()..before {
        *hidden = if hidden.is_some() { None } else { Some(now) };
    }
    if hidden.is_none() {
        return;
    }
    let escape = ctx.input.take_key(vk::ESCAPE);
    let asked = ctx.input.actions.iter().any(|a| {
        crate::ui::hud::window_for_action(*a).is_some() || crate::ui::chat::is_chat_action(*a)
    });
    if escape || asked || dead(state) {
        *hidden = None;
    }
}

/// The player has died: their health is at nothing.
fn dead(state: &GameState) -> bool {
    state.in_world && state.health.max > 0 && state.health.current == 0
}

/// The notice's words, naming the player's key for Hide Interface.
#[must_use]
pub fn notice(keys: Option<&KeyBindingsView>) -> String {
    let key = keys
        .and_then(|k| k.rows.iter().find(|r| r.action == HIDE_INTERFACE.0))
        .and_then(|r| r.keys.iter().find(|k| !k.is_empty()));
    match key {
        Some(key) => format!("The interface is hidden. Press {key} or Escape to show it."),
        None => "The interface is hidden. Press Escape to show it.".to_owned(),
    }
}

/// The notice near the top of the screen for a few seconds after the interface is hidden.
pub fn draw_notice(p: &mut Painter<'_>, keys: Option<&KeyBindingsView>, since: f64, now: f64) {
    let age = now - since;
    let a = crate::ui::narrow(((NOTICE_SECONDS - age) / 0.5).clamp(0.0, 1.0));
    if a <= 0.0 {
        return;
    }
    let k = p.scale;
    let style = TextStyle::new(Family::Body, 18.0, with_alpha(0xFFFF_FFFF, a))
        .edge(with_alpha(0xFF00_0000, a));
    let r = Rect::new(0.0, 60.0 * k, p.screen.0, 26.0 * k);
    p.text_in(&style, r, Align::Centre, &notice(keys));
}

/// What the hidden interface would have had of the frame, kept from it while it runs: the
/// pointer, the buttons and the keys; and how far the draw list, the pad's targets and the drop
/// places had got, to cut what it drew.
#[derive(Debug)]
pub struct Blind {
    mouse: (f32, f32),
    down: [bool; 3],
    pressed: [bool; 3],
    released: [bool; 3],
    wheel: f32,
    double: bool,
    captured: bool,
    keys: Vec<usize>,
    chars: Vec<char>,
    quads: usize,
    backdrops: usize,
    drops: usize,
    nav: [usize; 6],
    want: Option<Rect>,
    screen_cancel: Option<Rect>,
}

impl Blind {
    /// Keep the pointer, the buttons and the keys from what is drawn next.
    pub fn begin(p: &Painter<'_>, ctx: &mut Ctx<'_>) -> Self {
        let drops = ctx.drops.len();
        let i = &mut ctx.input;
        let nav = &i.nav;
        let blind = Self {
            mouse: i.mouse,
            down: i.down,
            pressed: i.pressed,
            released: i.released,
            wheel: i.wheel,
            double: i.double,
            captured: i.captured,
            keys: std::mem::take(&mut i.keys),
            chars: std::mem::take(&mut i.chars),
            quads: p.list.quads.len(),
            backdrops: p.list.backdrops.len(),
            drops,
            nav: [
                nav.targets.len(),
                nav.hidden.len(),
                nav.layers.len(),
                nav.tab_bars.len(),
                nav.homes.len(),
                nav.lists.len(),
            ],
            want: nav.want,
            screen_cancel: nav.screen_cancel,
        };
        let i = &mut ctx.input;
        i.mouse = (-1.0e6, -1.0e6);
        i.down = [false; 3];
        i.pressed = [false; 3];
        i.released = [false; 3];
        i.wheel = 0.0;
        i.double = false;
        blind
    }

    /// Cut what was drawn since [`Self::begin`] and give the input back.
    pub fn end(self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        let list = &mut *p.list;
        list.quads.truncate(self.quads);
        list.anchored.retain(|a| a.quads.end <= self.quads);
        list.backdrops.truncate(self.backdrops);
        ctx.drops.truncate(self.drops);
        let i = &mut ctx.input;
        let nav = &mut i.nav;
        let [targets, hidden, layers, tab_bars, homes, lists] = self.nav;
        nav.targets.truncate(targets);
        nav.hidden.truncate(hidden);
        nav.layers.truncate(layers);
        nav.tab_bars.truncate(tab_bars);
        nav.homes.truncate(homes);
        nav.lists.truncate(lists);
        nav.want = self.want;
        nav.screen_cancel = self.screen_cancel;
        i.mouse = self.mouse;
        i.down = self.down;
        i.pressed = self.pressed;
        i.released = self.released;
        i.wheel = self.wheel;
        i.double = self.double;
        i.keys = self.keys;
        i.chars = self.chars;
        i.captured = self.captured;
    }
}
