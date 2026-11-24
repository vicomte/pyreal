//! The quest tracker: Journal pages, contracts and quest timers the player pins, listed in a small
//! HUD panel with each timer's time left, a notice when a pinned timer is ready, and at login a
//! notice when the house's rent is owed.
//!
//! Pins are kept per world and character in a small text file in the client's settings folder
//! ([`file_name`]), one per line. Each timer's ready time is kept on the wall clock, so a pinned
//! timer counts on across a restart, a timer heard in chat too.

use std::path::{Path, PathBuf};

use pyreal_client_contract::journal::{JournalAction, JournalPage};
use pyreal_client_contract::UiRequest;

use super::{JournalTab, WindowId, Windows};
use crate::art::Family;
use crate::draw::{with_alpha, Rect};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The most pins kept.
pub const MAX_PINS: usize = 10;

/// How long a notice stays, in seconds, before it fades.
const NOTICE_SECONDS: f64 = 6.0;

/// How soon a rent owed is due for the panel to list it, in seconds.
const RENT_SOON: i64 = 7 * 86_400;

/// The panel's width, in layout units.
const WIDTH: f32 = 250.0;

/// A row's height, in layout units.
const ROW: f32 = 20.0;

/// What the HUD Layout window and the Journal's button tips call the panel.
pub const TITLE: &str = "Quest Tracker";

/// Something pinned to the tracker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pin {
    /// A notebook page: when it was written (0 when not known) and its name.
    Page { made: u64, name: String },
    /// A contract, by its id.
    Contract(u32),
    /// A quest timer the world sends, by the quest's name.
    Quest(String),
    /// A quest timer heard in chat, by its title.
    Heard(String),
}

impl Pin {
    /// The pin for notebook page `page`; none for a page with neither a name nor a date.
    #[must_use]
    pub fn page(page: &JournalPage) -> Option<Self> {
        (page.made != 0 || !page.name().is_empty()).then(|| Self::Page {
            made: page.made,
            name: page.name().to_owned(),
        })
    }

    /// Whether this pin and `other` are the same thing: a page by its date when it has one,
    /// else by its name; names ignore case.
    #[must_use]
    pub fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Page { made: a, name: x }, Self::Page { made: b, name: y }) => {
                if *a != 0 || *b != 0 {
                    a == b
                } else {
                    x.eq_ignore_ascii_case(y)
                }
            }
            (Self::Contract(a), Self::Contract(b)) => a == b,
            (Self::Quest(a), Self::Quest(b)) | (Self::Heard(a), Self::Heard(b)) => {
                a.eq_ignore_ascii_case(b)
            }
            _ => false,
        }
    }

    /// Whether `page` is the page this pin names.
    fn is_page(&self, page: &JournalPage) -> bool {
        Pin::page(page).is_some_and(|p| self.same(&p))
    }
}

/// A pin and what is known of its timer.
#[derive(Debug, Clone, PartialEq)]
struct Pinned {
    pin: Pin,
    /// When the timer is ready, in Unix seconds; none without a running timer.
    ready: Option<u64>,
    /// The ready notice was given.
    told: bool,
    /// A page's timer stamp when `ready` was taken from it, on the game's clock.
    stamp: f64,
    /// `ready` was taken this session, so a page's stamp is on this session's clock.
    fresh: bool,
}

/// One row of the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedRow {
    pub name: String,
    /// Where it is from, as the Timers tab says.
    pub from: &'static str,
    /// The time left, Ready, or a contract's progress.
    pub right: String,
    /// The timer is ready.
    pub ready: bool,
    /// A timer is counting down.
    pub running: bool,
    /// The pin, none for the rent.
    pub pin: Option<Pin>,
}

/// The character's pins, their file, and the notices waiting.
#[derive(Debug, Default)]
pub struct Tracker {
    pins: Vec<Pinned>,
    path: Option<PathBuf>,
    loaded_for: Option<String>,
    /// The notices showing, with when each came.
    notices: Vec<(String, f64)>,
    /// The rent was looked at this session.
    rent_seen: bool,
    /// The rows as last drawn, with where.
    drawn: Vec<(Rect, Option<Pin>)>,
}

/// The file, in the settings folder, a character's pins are kept in.
#[must_use]
pub fn file_name(address: &str, name: &str) -> String {
    let safe = |t: &str| -> String {
        t.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    format!("horizon-tracker-{}-{}.txt", safe(address), safe(name))
}

/// The pins as the file keeps them: `kind key ready told stamp name`, tab separated, `ready` 0
/// when no timer runs.
fn to_text(pins: &[Pinned]) -> String {
    pins.iter()
        .map(|p| {
            let (kind, key, name) = match &p.pin {
                Pin::Page { made, name } => ("page", *made, name.as_str()),
                Pin::Contract(id) => ("contract", u64::from(*id), ""),
                Pin::Quest(name) => ("quest", 0, name.as_str()),
                Pin::Heard(name) => ("heard", 0, name.as_str()),
            };
            format!(
                "{kind}\t{key}\t{}\t{}\t{}\t{}\n",
                p.ready.unwrap_or(0),
                u8::from(p.told),
                p.stamp,
                name.replace(['\t', '\n'], " ")
            )
        })
        .collect()
}

/// The pins a file holds; a line it cannot read is passed over.
fn parse(text: &str) -> Vec<Pinned> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.splitn(6, '\t');
            let kind = f.next()?;
            let key: u64 = f.next()?.parse().ok()?;
            let ready: u64 = f.next()?.parse().ok()?;
            let told = f.next()? == "1";
            let stamp: f64 = f.next()?.parse().ok()?;
            let name = f.next().unwrap_or("").to_owned();
            let pin = match kind {
                "page" => Pin::Page { made: key, name },
                "contract" => Pin::Contract(u32::try_from(key).ok()?),
                "quest" => Pin::Quest(name),
                "heard" => Pin::Heard(name),
                _ => return None,
            };
            Some(Pinned {
                pin,
                ready: (ready != 0).then_some(ready),
                told,
                stamp,
                fresh: false,
            })
        })
        .take(MAX_PINS)
        .collect()
}

/// A time on the game's `clock` as Unix seconds, with the wall clock at `wall`.
fn on_wall(at: f64, clock: f64, wall: u64) -> u64 {
    let left = at - clock;
    if left > 0.0 && left.is_finite() {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let left = left.ceil().min(1e12) as u64;
        wall.saturating_add(left)
    } else {
        wall
    }
}

/// The notice at login for a rent owed, due `left` seconds from now.
#[must_use]
pub fn rent_notice(left: i64) -> String {
    if left <= 0 {
        return "Your house's rent is overdue.".to_owned();
    }
    let (n, unit) = match left {
        86_400.. => (left / 86_400, "day"),
        3_600.. => (left / 3_600, "hour"),
        _ => ((left / 60).max(1), "minute"),
    };
    let s = if n == 1 { "" } else { "s" };
    format!("Your house's rent is due in {n} {unit}{s}.")
}

/// What a pin's thing is now: its name, where it is from, its timer on the game's clock (none
/// without one), its progress, and for a page its number and stamp. None while it is not there.
struct Live {
    name: String,
    from: &'static str,
    timer: Option<f64>,
    status: String,
    page: Option<(u32, f64)>,
    /// Not there now: only the ready time kept from an earlier session is known.
    kept: bool,
}

fn live(pin: &Pin, state: &GameState) -> Option<Live> {
    let j = &state.journal;
    match pin {
        Pin::Page { .. } => j.pages.iter().find(|p| pin.is_page(p)).map(|p| Live {
            name: if p.name().is_empty() {
                format!("Page {}", p.page_number)
            } else {
                p.name().to_owned()
            },
            from: "Notebook page",
            timer: (p.timer_running && p.timer_stamp > 0.0).then_some(p.timer_stamp),
            status: String::new(),
            page: Some((p.page_number, p.timer_stamp)),
            kept: false,
        }),
        Pin::Contract(id) => state
            .contracts
            .iter()
            .find(|c| c.contract_id == *id)
            .map(|c| Live {
                name: c.name.clone(),
                from: "Contract",
                timer: c.repeat_remaining.map(|left| state.clock + left),
                status: c.status.clone(),
                page: None,
                kept: false,
            }),
        Pin::Quest(name) => j
            .quests
            .iter()
            .find(|q| q.name.eq_ignore_ascii_case(name))
            .map(|q| Live {
                name: q.title.clone(),
                from: "Quest",
                timer: q.ready_at.filter(|_| q.min_delta > 0),
                status: String::new(),
                page: None,
                kept: false,
            }),
        Pin::Heard(title) => j
            .heard
            .iter()
            .find(|h| h.title.eq_ignore_ascii_case(title))
            .map(|h| Live {
                name: h.title.clone(),
                from: "Heard in chat",
                timer: Some(h.ready_at),
                status: String::new(),
                page: None,
                kept: false,
            }),
    }
}

/// What pin `p` names now, or for a timer heard in an earlier session, which still counts down,
/// its kept ready time.
fn seen(p: &Pinned, state: &GameState) -> Option<Live> {
    live(&p.pin, state).or_else(|| match &p.pin {
        Pin::Heard(title) if p.ready.is_some() => Some(Live {
            name: title.clone(),
            from: "Heard in chat",
            timer: None,
            status: String::new(),
            page: None,
            kept: true,
        }),
        _ => None,
    })
}

impl Tracker {
    /// Read the pins kept for character `name` on the world at `address` in `dir`, once per
    /// character. With no folder, pins last only while the client runs.
    pub fn load_for(&mut self, dir: Option<&Path>, address: &str, name: &str) {
        let key = format!("{address}|{name}");
        if name.is_empty() || self.loaded_for.as_deref() == Some(key.as_str()) {
            return;
        }
        self.path = dir.map(|d| d.join(file_name(address, name)));
        self.pins = self
            .path
            .as_ref()
            .and_then(|p| pyreal_client_runtime::platform::files::read_to_string(p).ok())
            .map(|t| parse(&t))
            .unwrap_or_default();
        self.loaded_for = Some(key);
        self.notices.clear();
        self.rent_seen = false;
    }

    /// Out of the world: the next login looks at the rent again, and each page's stamp is from
    /// an earlier session.
    pub fn forget_session(&mut self) {
        self.rent_seen = false;
        self.notices.clear();
        for p in &mut self.pins {
            p.fresh = false;
        }
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(path, to_text(&self.pins)));
        if let Err(e) = written {
            tracing::warn!("the quest tracker's pins were not kept: {e}");
        }
    }

    /// Whether `pin` is pinned.
    #[must_use]
    pub fn is_pinned(&self, pin: &Pin) -> bool {
        self.pins.iter().any(|p| p.pin.same(pin))
    }

    /// Whether another pin fits.
    #[must_use]
    pub fn has_room(&self) -> bool {
        self.pins.len() < MAX_PINS
    }

    /// Pin `pin`, and keep it. A timer already ready gives no notice.
    pub fn pin(&mut self, pin: Pin, state: &GameState) {
        if self.is_pinned(&pin) || !self.has_room() {
            return;
        }
        let mut p = Pinned {
            pin,
            ready: None,
            told: false,
            stamp: 0.0,
            fresh: true,
        };
        if let Some(l) = live(&p.pin, state) {
            p.ready = l.timer.map(|at| on_wall(at, state.clock, state.wall));
            p.stamp = l.page.map_or(0.0, |(_, s)| s);
        }
        p.told = p.ready.is_some_and(|r| r <= state.wall);
        self.pins.push(p);
        self.save();
    }

    /// Take `pin` off, and keep that.
    pub fn unpin(&mut self, pin: &Pin) {
        let before = self.pins.len();
        self.pins.retain(|p| !p.pin.same(pin));
        if self.pins.len() != before {
            self.save();
        }
    }

    /// Pin `pin`, or take it off when it is pinned.
    pub fn toggle(&mut self, pin: Pin, state: &GameState) {
        if self.is_pinned(&pin) {
            self.unpin(&pin);
        } else {
            self.pin(pin, state);
        }
    }

    /// The notices showing, with when each came.
    #[must_use]
    pub fn notices(&self) -> &[(String, f64)] {
        &self.notices
    }

    /// The rows as last drawn, with where each stood.
    #[must_use]
    pub fn drawn(&self) -> &[(Rect, Option<Pin>)] {
        &self.drawn
    }

    fn notice(&mut self, text: String, now: f64) {
        if !self.notices.iter().any(|(t, _)| *t == text) {
            self.notices.push((text, now));
        }
        let over = self.notices.len().saturating_sub(3);
        self.notices.drain(..over);
    }

    /// Bring each pinned timer's ready time up to date, give a notice for each that is now
    /// ready, and at login one for the rent owed. `now` is the interface's clock.
    pub fn follow(&mut self, state: &GameState, now: f64) {
        self.notices.retain(|(_, at)| now - at < NOTICE_SECONDS);
        let wall = state.wall;
        let mut changed = false;
        let mut ready_now = Vec::new();
        for p in &mut self.pins {
            let Some(l) = seen(p, state) else {
                continue;
            };
            if let Pin::Page { made, name } = &mut p.pin {
                if *made != 0 && *name != l.name && l.page.is_some() {
                    l.name.clone_into(name);
                    changed = true;
                }
            }
            let stamp = l.page.map_or(0.0, |(_, s)| s);
            // A page's stamp read back from an earlier session is not on this session's clock.
            let stale = l.page.is_some() && !p.fresh && p.ready.is_some() && stamp == p.stamp;
            if !stale && !l.kept {
                p.fresh = true;
                let ready = l.timer.map(|at| on_wall(at, state.clock, wall));
                let moved = match (p.ready, ready) {
                    (Some(a), Some(b)) => a.abs_diff(b) > 1 && !(a <= wall && b <= wall),
                    (a, b) => a != b,
                };
                if moved || stamp != p.stamp {
                    p.ready = ready;
                    p.stamp = stamp;
                    changed = true;
                }
            }
            match p.ready {
                Some(r) if r <= wall => {
                    if !p.told {
                        p.told = true;
                        changed = true;
                        ready_now.push(format!("{} is ready.", l.name));
                    }
                }
                _ => {
                    if p.told {
                        p.told = false;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.save();
        }
        for text in ready_now {
            self.notice(text, now);
        }
        if !self.rent_seen && state.in_world {
            if let Some(h) = &state.world_services.house {
                self.rent_seen = true;
                if h.rent_owed {
                    let left = h.maintenance_next_due - i64::try_from(wall).unwrap_or(i64::MAX);
                    self.notice(rent_notice(left), now);
                }
            }
        }
    }

    /// The panel's rows: each pin that is there now, in the order pinned, then the rent when it
    /// is owed soon.
    #[must_use]
    pub fn rows(&self, state: &GameState) -> Vec<TrackedRow> {
        let wall = state.wall;
        let left_text = |ready: u64| {
            #[allow(clippy::cast_precision_loss)]
            let left = ready.saturating_sub(wall) as f64;
            pyreal_presentation::journal::short_time(left)
        };
        let mut rows: Vec<TrackedRow> = self
            .pins
            .iter()
            .filter_map(|p| {
                let l = seen(p, state)?;
                let (right, ready) = match p.ready {
                    Some(r) if r <= wall => ("Ready".to_owned(), true),
                    Some(r) => (left_text(r), false),
                    None => (l.status, false),
                };
                Some(TrackedRow {
                    name: l.name,
                    from: l.from,
                    right,
                    ready,
                    running: p.ready.is_some() && !ready,
                    pin: Some(p.pin.clone()),
                })
            })
            .collect();
        if let Some(h) = state.world_services.house.as_ref().filter(|h| h.rent_owed) {
            let left = h.maintenance_next_due - i64::try_from(wall).unwrap_or(i64::MAX);
            if left <= RENT_SOON {
                rows.push(TrackedRow {
                    name: "House rent".to_owned(),
                    from: "House",
                    right: if left > 0 {
                        u64::try_from(h.maintenance_next_due)
                            .map_or_else(|_| String::new(), left_text)
                    } else {
                        "Overdue".to_owned()
                    },
                    ready: false,
                    running: left > 0,
                    pin: None,
                });
            }
        }
        rows
    }
}

/// `text` cut at the end with an ellipsis to fit `w`, so a name keeps its start.
fn end_ellipsis(p: &Painter<'_>, style: &TextStyle, text: &str, w: f32) -> String {
    if p.measure(style, text) <= w {
        return text.to_owned();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let cut: String = chars.iter().collect();
        let t = format!("{}\u{2026}", cut.trim_end());
        if p.measure(style, &t) <= w {
            return t;
        }
    }
    String::new()
}

/// Where the panel stands unless the HUD layout moves it: under the radar, at its right edge.
#[must_use]
pub fn panel_origin(scale: f32, screen: (f32, f32)) -> (f32, f32) {
    (screen.0 - (26.0 + WIDTH) * scale, 304.0 * scale)
}

impl Windows {
    /// The quest tracker on the HUD: a row for each pinned thing with its time left. A click opens
    /// it in the Journal, a right click unpins it. With nothing to show it is not drawn, but for
    /// a placeholder while the HUD layout is being arranged. Returns the tooltip asked for.
    pub fn quest_tracker(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        dir: Option<&Path>,
        out: &mut Outcome,
    ) -> Option<(String, Vec<String>)> {
        self.tracker.load_for(dir, &state.address, &state.name);
        self.tracker.follow(state, ctx.time);
        self.tracker.drawn.clear();
        let k = p.scale;
        let (x, y) = panel_origin(k, p.screen);
        let rows = self.tracker.rows(state);
        let heading =
            TextStyle::new(Family::Body, 12.0, ctx.colours.heading()).edge(ctx.colours.edge());
        if rows.is_empty() {
            if self.is_open(WindowId::Layout) {
                let r = Rect::new(x, y, WIDTH * k, 30.0 * k);
                kit::plain_ground(p, r);
                p.text_in(&heading, r.offset(10.0 * k, 0.0), Align::Left, TITLE);
            }
            return None;
        }
        let name_style =
            TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        #[allow(clippy::cast_precision_loss)]
        let h = (24.0 + ROW * rows.len() as f32 + 6.0) * k;
        let ground = Rect::new(x, y, WIDTH * k, h);
        kit::plain_ground(p, ground);
        if ctx.over(&ground) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        p.text_in(
            &heading,
            Rect::new(x + 10.0 * k, y + 4.0 * k, ground.w - 20.0 * k, 18.0 * k),
            Align::Left,
            TITLE,
        );
        let mut tip = None;
        for (i, row) in rows.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                x + 4.0 * k,
                y + (24.0 + ROW * i as f32) * k,
                ground.w - 8.0 * k,
                ROW * k,
            );
            let hover = ctx.over(&r);
            if hover {
                p.fill(r, 0x30C8_9646);
            }
            let right_style = if row.ready {
                TextStyle::new(Family::Body, 13.0, super::notebook::READY_COLOUR)
                    .edge(ctx.colours.edge())
            } else if row.running {
                TextStyle::new(Family::Body, 13.0, super::notebook::RUNNING_COLOUR)
                    .edge(ctx.colours.edge())
            } else {
                dim
            };
            let right_w = if row.right.is_empty() {
                0.0
            } else {
                p.measure(&right_style, &row.right).min(r.w * 0.4)
            };
            let right = Rect::new(r.right() - right_w - 6.0 * k, r.y, right_w, r.h);
            let right_text = end_ellipsis(p, &right_style, &row.right, right_w);
            p.text_in(&right_style, right, Align::Left, &right_text);
            let name_r = Rect::new(r.x + 6.0 * k, r.y, right.x - r.x - 14.0 * k, r.h);
            let name = end_ellipsis(p, &name_style, &row.name, name_r.w);
            p.text_in(&name_style, name_r, Align::Left, &name);
            self.tracker.drawn.push((r, row.pin.clone()));
            if hover {
                let mut lines = vec![row.from.to_owned()];
                if row.pin.is_some() {
                    lines.push("Click to open it in the Journal.".to_owned());
                    lines.push("Right-click to unpin it.".to_owned());
                }
                tip = Some((row.name.clone(), lines));
            }
            let Some(pin) = &row.pin else {
                continue;
            };
            if ctx.input.right_clicked(&r) {
                self.tracker.unpin(pin);
            } else if ctx.input.clicked(&r) {
                self.open_pinned(pin, state, ctx.time, out);
            }
        }
        tip
    }

    /// Open the Journal on what `pin` names: its page, the contract, or the timers.
    fn open_pinned(&mut self, pin: &Pin, state: &GameState, now: f64, out: &mut Outcome) {
        self.open(WindowId::Journal, now);
        match pin {
            Pin::Page { .. } => {
                self.show_journal_tab(JournalTab::Notebook);
                if let Some(page) = state.journal.pages.iter().find(|p| pin.is_page(p)) {
                    out.requests
                        .push(UiRequest::Journal(JournalAction::Goto(page.page_number)));
                }
            }
            Pin::Contract(id) => {
                self.show_journal_tab(JournalTab::Contracts);
                self.contract_selected = Some(*id);
            }
            Pin::Quest(_) | Pin::Heard(_) => self.show_journal_tab(JournalTab::Timers),
        }
    }

    /// The Journal's Pin button for `pin` at `r`: Pin, or Unpin while it is pinned. None, or no
    /// room for another, shows it greyed.
    pub(super) fn pin_button(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        pin: Option<Pin>,
        state: &GameState,
    ) {
        let pinned = pin.as_ref().is_some_and(|x| self.tracker.is_pinned(x));
        let label = if pinned { "Unpin" } else { "Pin" };
        let enabled = pin.is_some() && (pinned || self.tracker.has_room());
        if ctx.input.hover(&r) {
            let line = if pinned {
                "Takes it off the quest tracker on the screen."
            } else if self.tracker.has_room() {
                "Shows it in the quest tracker on the screen."
            } else {
                "The quest tracker is full. Unpin something first."
            };
            self.tip = Some((TITLE.to_owned(), vec![line.to_owned()]));
        }
        if kit::button(p, ctx, r, label, enabled) {
            if let Some(pin) = pin {
                self.tracker.toggle(pin, state);
            }
        }
    }

    /// The tracker's notices at the top of the screen, newest lowest, each fading at the end.
    pub fn tracker_notices(&self, p: &mut Painter<'_>, now: f64) {
        let k = p.scale;
        let (sw, _) = p.screen;
        let mut y = 108.0 * k;
        for (text, at) in self.tracker.notices() {
            let age = now - at;
            let a = crate::ui::narrow(((NOTICE_SECONDS - age) / 0.5).clamp(0.0, 1.0));
            let style = TextStyle::new(Family::Body, 18.0, with_alpha(0xFFFF_E080, a))
                .edge(with_alpha(0xFF40_2000, a));
            p.text_in(&style, Rect::new(0.0, y, sw, 26.0 * k), Align::Centre, text);
            y += 26.0 * k;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pins_read_back_what_they_wrote() {
        let pins = vec![
            Pinned {
                pin: Pin::Page {
                    made: 1_700_000_000,
                    name: "Drudge\tcamp".into(),
                },
                ready: Some(1_700_003_600),
                told: false,
                stamp: 4_600.5,
                fresh: true,
            },
            Pinned {
                pin: Pin::Contract(42),
                ready: None,
                told: true,
                stamp: 0.0,
                fresh: true,
            },
            Pinned {
                pin: Pin::Heard("Quest from Ayn Tayan".into()),
                ready: Some(9),
                told: false,
                stamp: 0.0,
                fresh: true,
            },
        ];
        let back = parse(&to_text(&pins));
        assert_eq!(back.len(), 3);
        assert_eq!(
            back[0].pin,
            Pin::Page {
                made: 1_700_000_000,
                name: "Drudge camp".into()
            }
        );
        assert_eq!(back[0].ready, Some(1_700_003_600));
        assert!((back[0].stamp - 4_600.5).abs() < 1e-9);
        assert!(
            !back[0].fresh,
            "a stamp read back is from an earlier session"
        );
        assert_eq!(back[1].pin, Pin::Contract(42));
        assert!(back[1].told);
        assert_eq!(back[2].pin, Pin::Heard("Quest from Ayn Tayan".into()));
        assert!(parse("nonsense\nquest\tx\t0\t0\t0\tA\n").is_empty());
    }

    #[test]
    fn a_page_is_the_same_by_its_date_or_without_one_by_its_name() {
        let a = Pin::Page {
            made: 5,
            name: "Old".into(),
        };
        let renamed = Pin::Page {
            made: 5,
            name: "New".into(),
        };
        assert!(a.same(&renamed));
        let undated = |n: &str| Pin::Page {
            made: 0,
            name: n.into(),
        };
        assert!(undated("Camp").same(&undated("camp")));
        assert!(!undated("Camp").same(&a));
        assert!(Pin::Quest("Colo".into()).same(&Pin::Quest("colo".into())));
        assert!(!Pin::Quest("Colo".into()).same(&Pin::Heard("Colo".into())));
    }

    #[test]
    fn the_rent_notice_names_the_largest_unit() {
        assert_eq!(
            rent_notice(3 * 86_400 + 5),
            "Your house's rent is due in 3 days."
        );
        assert_eq!(rent_notice(86_400), "Your house's rent is due in 1 day.");
        assert_eq!(rent_notice(7_200), "Your house's rent is due in 2 hours.");
        assert_eq!(rent_notice(30), "Your house's rent is due in 1 minute.");
        assert_eq!(rent_notice(0), "Your house's rent is overdue.");
    }
}
