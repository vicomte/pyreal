//! Effects ending soon: each beneficial effect in its last stretch, and every harmful effect that
//! runs out, as its own icon with its time left under the status row; and a notice at the top of
//! the screen when a beneficial effect enters that stretch. A HUD element of its own ("ending"),
//! so the HUD Layout editor moves it.

use super::{time_left, Hud};
use crate::art::AcArt as _;
use crate::art::Family;
use crate::draw::{Rect, WHITE};
use crate::ui::game::Effect;
use crate::ui::kit::Ctx;
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::panels::info::Ask;

/// How long before its end an effect counts as ending soon, as the player chose; or never.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EndingSoon {
    Off,
    HalfMinute,
    #[default]
    OneMinute,
    TwoMinutes,
    FiveMinutes,
}

impl EndingSoon {
    /// Every choice, in the order Settings lists them.
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::HalfMinute,
        Self::OneMinute,
        Self::TwoMinutes,
        Self::FiveMinutes,
    ];

    /// Its name in Settings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::HalfMinute => "Last 30 Seconds",
            Self::OneMinute => "Last Minute",
            Self::TwoMinutes => "Last 2 Minutes",
            Self::FiveMinutes => "Last 5 Minutes",
        }
    }

    /// Its word in the settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::HalfMinute => "30",
            Self::OneMinute => "60",
            Self::TwoMinutes => "120",
            Self::FiveMinutes => "300",
        }
    }

    /// The choice a settings file's word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == word.trim())
    }

    /// The seconds left from which an effect counts as ending; `None` when off.
    #[must_use]
    pub const fn seconds(self) -> Option<f64> {
        match self {
            Self::Off => None,
            Self::HalfMinute => Some(30.0),
            Self::OneMinute => Some(60.0),
            Self::TwoMinutes => Some(120.0),
            Self::FiveMinutes => Some(300.0),
        }
    }

    /// How the notice says the time: "ends in one minute".
    const fn phrase(self) -> &'static str {
        match self {
            Self::Off | Self::OneMinute => "one minute",
            Self::HalfMinute => "30 seconds",
            Self::TwoMinutes => "two minutes",
            Self::FiveMinutes => "five minutes",
        }
    }
}

/// The most icons the row shows; the ones ending first.
pub const MAX_SHOWN: usize = 8;

/// How long after one notice the next may come; effects entering their last stretch meanwhile
/// share it.
const NOTICE_GAP: f64 = 4.0;

/// Below this many seconds the countdown turns red.
const URGENT: f64 = 10.0;

/// An effect the row shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub spell: u32,
    pub name: String,
    pub ac_icon: u32,
    pub harmful: bool,
    pub remaining: f64,
}

/// What the row keeps across frames.
#[derive(Debug, Default)]
pub struct EffectsEnding {
    /// How long before its end an effect shows, as the player chose.
    pub threshold: EndingSoon,
    /// Each timed effect's seconds left last frame, by layered enchantment id.
    last: std::collections::HashMap<u32, f64>,
    /// Effects that entered their last stretch and wait for a notice, by layered id and name.
    pending: Vec<(u32, String)>,
    /// When the last notice was given.
    noticed_at: Option<f64>,
    shown: Vec<Shown>,
}

impl EffectsEnding {
    /// What the row shows this frame, ending first.
    #[must_use]
    pub fn shown(&self) -> &[Shown] {
        &self.shown
    }

    /// Follow the effects in force at time `now`: what the row shows, and the notice to give, if
    /// any. A beneficial effect is noticed when it passes into its last stretch, so one first seen
    /// there (cast short, or met at log-in) and one refreshed within it are not. One a stronger
    /// spell of its category supersedes is left out; when it takes over again it counts as first
    /// seen.
    pub fn step(&mut self, effects: &[Effect], now: f64) -> Option<String> {
        let Some(limit) = self.threshold.seconds() else {
            self.last.clear();
            self.pending.clear();
            self.shown.clear();
            return None;
        };
        let mut last = std::collections::HashMap::new();
        self.shown.clear();
        for e in effects {
            let Some(left) = e.remaining.filter(|r| *r > 0.0 && !e.superseded) else {
                continue;
            };
            last.insert(e.layered, left);
            let ending = left <= limit;
            if ending
                && !e.harmful
                && self.last.get(&e.layered).is_some_and(|was| *was > limit)
                && !self.pending.iter().any(|(l, _)| *l == e.layered)
            {
                self.pending.push((e.layered, e.name.clone()));
            }
            if ending || e.harmful {
                self.shown.push(Shown {
                    spell: e.spell,
                    name: e.name.clone(),
                    ac_icon: e.ac_icon,
                    harmful: e.harmful,
                    remaining: left,
                });
            }
        }
        // One refreshed past the stretch, or gone, before its notice is not noticed.
        self.pending
            .retain(|(s, _)| last.get(s).is_some_and(|left| *left <= limit));
        self.last = last;
        self.shown
            .sort_by(|a, b| a.remaining.total_cmp(&b.remaining));
        self.shown.truncate(MAX_SHOWN);
        if self.pending.is_empty() || self.noticed_at.is_some_and(|t| now - t < NOTICE_GAP) {
            return None;
        }
        self.noticed_at = Some(now);
        let mut names: Vec<String> = Vec::new();
        for (_, n) in self.pending.drain(..) {
            // Two casters' copies of one spell are named once.
            if !names.contains(&n) {
                names.push(n);
            }
        }
        Some(notice(&names, self.threshold))
    }
}

/// The notice for effects entering their last stretch: "Strength Self VI ends in one minute."
fn notice(names: &[String], threshold: EndingSoon) -> String {
    let when = threshold.phrase();
    match names {
        [one] => format!("{one} ends in {when}."),
        [a, b] => format!("{a} and {b} end in {when}."),
        [a, rest @ ..] => format!("{a} and {} other effects end in {when}.", rest.len()),
        [] => String::new(),
    }
}

/// A time left as the tooltip says it.
fn ends_in(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = seconds.max(0.0).ceil() as u64;
    match s {
        1 => "Ends in one second.".to_owned(),
        2..=90 => format!("Ends in {s} seconds."),
        _ => format!("Ends in {} minutes.", s.div_ceil(60)),
    }
}

impl Hud {
    /// The row: under the status row's icons, from their right edge leftwards. While the HUD
    /// Layout window is open and nothing is ending, empty frames stand in so it can be moved.
    pub(super) fn effects_ending(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, editing: bool) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let size = 32.0 * k;
        let pitch = size + 8.0 * k;
        let y = 100.0 * k;
        let right = sw - 260.0 * k;
        if self.ending.shown.is_empty() {
            if editing && self.ending.threshold != EndingSoon::Off {
                for i in 0..3 {
                    #[allow(clippy::cast_precision_loss)]
                    let x = right - size - pitch * i as f32;
                    p.fill(Rect::new(x, y, size, size), 0x8040_4040);
                }
            }
            return;
        }
        let white = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let red = TextStyle::new(Family::Numerals, 12.0, 0xFFFF_6A5A).edge(0xFF20_0000);
        // The list under the status row's icons, when open, takes the pointer.
        let pointer = self.effects_open.is_none();
        let shown = self.ending.shown.clone();
        for (i, e) in shown.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(right - size - pitch * i as f32, y, size, size);
            if let Some(icon) = p.art.ac_component(e.ac_icon) {
                p.sprite(&icon, r, WHITE);
            } else {
                p.fill(r, 0x8040_4040);
            }
            let rim = if e.harmful { 0xFFE0_4040 } else { 0xFF60_C060 };
            p.fill(Rect::new(r.x, r.bottom() - 3.0 * k, r.w, 3.0 * k), rim);
            let style = if e.remaining <= URGENT { &red } else { &white };
            p.text_in(
                style,
                Rect::new(r.x - 4.0 * k, r.bottom(), r.w + 8.0 * k, 16.0 * k),
                Align::Centre,
                &time_left(e.remaining),
            );
            if pointer && ctx.over(&r) {
                self.tip(
                    e.name.clone(),
                    vec![ends_in(e.remaining), "Right-click for details.".into()],
                );
                if ctx.input.right_clicked(&r) {
                    self.asks.push(Ask::Effect(e.spell));
                }
            }
        }
    }

    /// The floating notices showing, oldest first.
    #[must_use]
    pub fn notice_lines(&self) -> Vec<&str> {
        self.notices.iter().map(|(t, _)| t.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{ends_in, notice, EndingSoon};

    #[test]
    fn a_notice_names_one_two_or_the_first_of_many_effects() {
        let names = |n: &[&str]| n.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let t = EndingSoon::OneMinute;
        assert_eq!(
            notice(&names(&["Strength Self VI"]), t),
            "Strength Self VI ends in one minute."
        );
        assert_eq!(
            notice(&names(&["Strength Self VI", "Armor Self VI"]), t),
            "Strength Self VI and Armor Self VI end in one minute."
        );
        assert_eq!(
            notice(&names(&["A", "B", "C", "D"]), EndingSoon::HalfMinute),
            "A and 3 other effects end in 30 seconds."
        );
    }

    #[test]
    fn the_setting_s_words_read_back_and_an_unknown_one_is_none() {
        for s in EndingSoon::ALL {
            assert_eq!(EndingSoon::from_word(s.word()), Some(s));
        }
        assert_eq!(EndingSoon::from_word("45"), None);
        assert_eq!(EndingSoon::Off.seconds(), None);
    }

    #[test]
    fn the_tooltip_says_seconds_then_minutes() {
        assert_eq!(ends_in(0.4), "Ends in one second.");
        assert_eq!(ends_in(44.2), "Ends in 45 seconds.");
        assert_eq!(ends_in(150.0), "Ends in 3 minutes.");
    }
}
