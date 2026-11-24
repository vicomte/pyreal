//! Item details on hover: the game's appraisal of an item in a pack, a loot window or a vendor's
//! stock, in a tooltip once the pointer has rested on it, and beside it what is worn in the same
//! place while Shift is held or Compare With Worn Items is on.
//!
//! An item is asked about only from those three places ([`crate::ui::appraisals`] keeps what was
//! asked), and only while Assess on Hover is on. A worn item's tile shows its details when they
//! are already known.

use pyreal_client_contract::UiRequest;
use pyreal_primitives::ObjectId;

use super::Windows;
use crate::art::{AcArt as _, Family};
use crate::draw::{Rect, WHITE};
use crate::ui::appraisals::{self, Compared, StatKind, Status, WornStats};
use crate::ui::game::{Examined, GameState, Item};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Painter, TextStyle};
use crate::ui::Outcome;

/// How wide each panel of the details is, in layout units.
const PANEL_W: f32 = 300.0;

/// The item the pointer rests on.
#[derive(Debug, Clone)]
pub(super) struct Hover {
    item: Item,
    /// When the pointer came to it.
    since: f64,
    /// Whether the item may be appraised for this: it is in a pack, a loot window or a vendor's
    /// stock.
    may_ask: bool,
    /// The pointer was on it this frame.
    seen: bool,
}

/// Where a panel's appraisal stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailState {
    Assessing,
    /// Not assessed, and it will not be asked about.
    NotAssessed,
    Ready,
    Failed,
    NoReply,
}

/// How the hovered item compares with what it would replace.
#[derive(Debug, Clone, PartialEq)]
pub enum Comparison {
    /// The worn items' names, and each number compared.
    Differences(Compared),
    /// Why there is no comparison, in a sentence.
    Missing(&'static str),
}

/// One panel of the details, as it was drawn this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DetailPanel {
    pub id: ObjectId,
    /// The item is the worn one, beside the hovered one.
    pub worn: bool,
    pub title: String,
    pub state: DetailState,
    /// The lines and their colours, in the examine window's colours (0 plain, 1 raised, 2
    /// lowered, 3 unknown).
    pub lines: Vec<(String, u8)>,
    /// For the hovered item beside worn ones: how it compares with each.
    pub comparisons: Vec<Comparison>,
    /// The item's tile, for its picture.
    look: Option<Item>,
}

/// The sentence a panel opens with while its details are not all there.
#[must_use]
pub const fn state_line(state: DetailState) -> Option<&'static str> {
    match state {
        DetailState::Assessing => Some("Assessing..."),
        DetailState::NotAssessed => Some("Not assessed. Examine it to see its details."),
        DetailState::Ready => None,
        DetailState::Failed => Some("You could not assess this item. Some details are unknown."),
        DetailState::NoReply => Some("This item could not be assessed. The game did not answer."),
    }
}

/// The examine window's lines for an item: its description blocks, a blank line between blocks
/// that do not follow on, then a written inscription.
fn item_lines(e: &Examined) -> Vec<(String, u8)> {
    let mut out = Vec::new();
    for (i, (run, same_line, colour)) in e.runs.iter().enumerate() {
        if i > 0 && !same_line {
            out.push((String::new(), 0));
        }
        out.extend(run.split('\n').map(|l| (l.to_owned(), *colour)));
    }
    if let Some((text, signature)) = e
        .inscription
        .as_ref()
        .filter(|(t, _)| !t.is_empty() && t != pyreal_presentation::appraisal::PLACEHOLDER)
    {
        out.push((String::new(), 0));
        out.push((text.clone(), 4));
        if !signature.is_empty() {
            out.push((signature.clone(), 5));
        }
    }
    // One blank line between blocks, and none at the top.
    out.dedup_by(|b, a| a.0.trim().is_empty() && b.0.trim().is_empty());
    while out.first().is_some_and(|(l, _)| l.trim().is_empty()) {
        out.remove(0);
    }
    out
}

/// A panel for `item` from what is known of its appraisal. `asking`: it is asked about when
/// nothing is known.
fn panel(item: &Item, status: Status<'_>, worn: bool, asking: bool) -> DetailPanel {
    let (state, appraised) = match status {
        Status::Ready(a) => (DetailState::Ready, Some(a)),
        Status::Failed(a) => (DetailState::Failed, Some(a)),
        Status::NoReply => (DetailState::NoReply, None),
        Status::Unknown if !asking => (DetailState::NotAssessed, None),
        Status::Unknown | Status::Waiting => (DetailState::Assessing, None),
    };
    let mut lines: Vec<(String, u8)> = state_line(state)
        .map(|s| {
            (
                s.to_owned(),
                if matches!(state, DetailState::Assessing | DetailState::NotAssessed) {
                    0
                } else {
                    2
                },
            )
        })
        .into_iter()
        .collect();
    if let Some(a) = appraised {
        lines.extend(item_lines(&a.detail));
    }
    DetailPanel {
        id: item.id,
        worn,
        title: appraised.map_or_else(|| item.name.clone(), |a| a.detail.title.clone()),
        state,
        lines,
        comparisons: Vec::new(),
        look: Some(item.clone()),
    }
}

/// How the hovered item (its places and status) compares with the worn ones it would replace.
/// `asking`: an item nothing is known of is asked about.
#[must_use]
pub fn comparison(
    places: u32,
    hovered: Status<'_>,
    worn: &[(&Item, Status<'_>)],
    asking: bool,
) -> Comparison {
    let failed = |s: Status<'_>| matches!(s, Status::Failed(_) | Status::NoReply);
    if failed(hovered) {
        return Comparison::Missing("No comparison. This item could not be assessed.");
    }
    if worn.iter().any(|(_, s)| failed(*s)) {
        return Comparison::Missing("No comparison. The worn item could not be assessed.");
    }
    if !asking && worn.iter().any(|(_, s)| *s == Status::Unknown) {
        return Comparison::Missing("No comparison. The worn item is not assessed.");
    }
    let Status::Ready(a) = hovered else {
        return Comparison::Missing("Comparing...");
    };
    let mut pieces = Vec::new();
    for (w, s) in worn {
        let Status::Ready(b) = s else {
            return Comparison::Missing("Comparing...");
        };
        pieces.push(WornStats {
            name: &w.name,
            places: w.worn,
            stats: &b.stats,
        });
    }
    Comparison::Differences(appraisals::compare_worn(places, &a.stats, &pieces))
}

/// The sentences under a comparison about the spells on what is worn: its numbers are compared
/// without them, and the first number they change, with them.
#[must_use]
pub fn spell_notes(c: &Compared) -> Vec<String> {
    let whose = if c.totals { "their" } else { "its" };
    let mut out = Vec::new();
    let stripped = c.rows.iter().find(|d| {
        d.worn_with_spells
            .zip(d.worn)
            .is_some_and(|(s, v)| (s - v).abs() > 0.5)
    });
    if let Some(d) = stripped {
        let with = d.worn_with_spells.unwrap_or_default();
        let total = if c.totals { " in total" } else { "" };
        out.push(format!(
            "The worn numbers leave out spells. With {whose} spells the worn {} is {}{total}.",
            d.kind.name(),
            d.kind.show(with)
        ));
    }
    let kept = c.rows.iter().find(|d| {
        d.worn_with_spells
            .zip(d.worn)
            .is_some_and(|(s, v)| (s - v).abs() <= 0.5)
    });
    if let Some(d) = kept {
        out.push(format!("The worn {} includes spells.", d.kind.name()));
    }
    out
}

impl Windows {
    /// The pointer is on `item` this frame. `may_ask`: it is in a pack, a loot window or a
    /// vendor's stock, so it may be appraised.
    pub(super) fn hover_item(&mut self, ctx: &Ctx<'_>, item: &Item, may_ask: bool) {
        if ctx.drag.as_ref().is_some_and(|d| d.active) {
            return;
        }
        match &mut self.item_hover {
            Some(h) if h.item.id == item.id => {
                h.item = item.clone();
                h.may_ask = may_ask;
                h.seen = true;
            }
            _ => {
                self.item_hover = Some(Hover {
                    item: item.clone(),
                    since: ctx.time,
                    may_ask,
                    seen: true,
                });
            }
        }
    }

    /// A new frame: the item hovered last frame is forgotten unless the pointer is still on it.
    pub(super) fn begin_item_hover(&mut self) {
        if let Some(h) = &mut self.item_hover {
            h.seen = false;
        }
        self.item_details.clear();
    }

    /// The panels of item details drawn this frame: the hovered item's, then what is worn in its
    /// place.
    #[must_use]
    pub fn item_details(&self) -> &[DetailPanel] {
        &self.item_details
    }

    /// The details of the item the pointer rests on, asked for and drawn once it has rested
    /// [`appraisals::HOVER_DELAY`]. Whether they were drawn, in place of the plain tooltip.
    pub(super) fn draw_item_details(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) -> bool {
        // A written inscription is asked about again a moment later, hovered or not.
        for r in &out.requests {
            if let UiRequest::SetInscription { object, .. } = r {
                if self.assess_on_hover {
                    self.appraisals.inscribed(*object, ctx.time);
                }
            }
        }
        for id in self.appraisals.due(ctx.time) {
            out.requests.push(UiRequest::Appraise(id));
        }
        let Some(h) = self.item_hover.clone().filter(|h| h.seen) else {
            self.item_hover = None;
            return false;
        };
        let now = ctx.time;
        let compare = ctx.input.shift || self.compare_worn;
        let worn: Vec<Item> = if compare {
            appraisals::worn_in_place(&h.item, &state.equipped)
                .into_iter()
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        out.watch_appraisals.push(h.item.id);
        out.watch_appraisals.extend(worn.iter().map(|w| w.id));
        if now - h.since < appraisals::HOVER_DELAY {
            return false;
        }
        let asking = h.may_ask && self.assess_on_hover;
        if asking {
            // A worn item's spells run out, so its answer is asked for again now and then.
            let hovered = std::iter::once((&h.item, f64::INFINITY));
            let worn_ones = worn.iter().map(|w| (w, appraisals::WORN_ANSWER_LASTS));
            for (it, lasts) in hovered.chain(worn_ones) {
                let known = appraisals::known(state, it.id);
                if self.appraisals.want_within(it, known, now, lasts) {
                    out.requests.push(UiRequest::Appraise(it.id));
                }
            }
        }
        let status = self.appraisals.lookup(state, h.item.id, now);
        // An item not asked about shows what is known, and its name alone while nothing is.
        if !asking && status == Status::Unknown {
            return false;
        }
        let mut first = panel(&h.item, status, false, asking);
        let mut panels = Vec::new();
        let worn_status: Vec<(&Item, Status<'_>)> = worn
            .iter()
            .map(|w| (w, self.appraisals.lookup(state, w.id, now)))
            .collect();
        let places = h.item.equip_locations;
        if places & appraisals::BodyPart::PLACES != 0 && !worn_status.is_empty() {
            // Armour is compared with everything it would displace at once.
            first
                .comparisons
                .push(comparison(places, status, &worn_status, asking));
        } else {
            for w in &worn_status {
                first
                    .comparisons
                    .push(comparison(places, status, std::slice::from_ref(w), asking));
            }
        }
        panels.extend(
            worn_status
                .iter()
                .map(|(w, ws)| panel(w, *ws, true, asking)),
        );
        panels.insert(0, first);
        draw_panels(p, ctx, &panels);
        self.item_details = panels;
        true
    }
}

/// One drawn row of a panel.
enum Row {
    Text(String, u32),
    /// A comparison row: the name, the item's number, and how it differs, in its colour.
    Stat(String, String, String, u32),
}

/// The colours of the details: the examine window's for its runs, then the inscription's and
/// the signature's.
fn colour(ctx: &Ctx<'_>, c: u8, plain: u32) -> u32 {
    match c {
        1 => BETTER,
        2 => WORSE,
        3 => 0xFFF0_D060,
        4 => 0xFFE8_D8B0,
        5 => ctx.colours.dim(),
        _ => plain,
    }
}

/// A better number, and a worse one.
const BETTER: u32 = 0xFF7C_E07C;
const WORSE: u32 = 0xFFF0_6A5A;

/// The rows of `panel`, wrapped to `w`.
fn rows(p: &Painter<'_>, ctx: &Ctx<'_>, panel: &DetailPanel, body: &TextStyle, w: f32) -> Vec<Row> {
    let plain = p.art.palette().tooltip_text;
    let mut out = Vec::new();
    for (line, c) in &panel.lines {
        if line.is_empty() {
            out.push(Row::Text(String::new(), plain));
            continue;
        }
        for l in p.wrap(body, line, w) {
            out.push(Row::Text(l, colour(ctx, *c, plain)));
        }
    }
    for c in &panel.comparisons {
        out.push(Row::Text(String::new(), plain));
        match c {
            Comparison::Missing(why) => {
                for l in p.wrap(body, why, w) {
                    out.push(Row::Text(l, ctx.colours.dim()));
                }
            }
            Comparison::Differences(c) => {
                for l in p.wrap(body, &format!("Compared with {}", c.with), w) {
                    out.push(Row::Text(l, p.art.palette().label));
                }
                if c.rows.is_empty() && c.parts.is_empty() {
                    out.push(Row::Text(
                        "No numbers to compare.".into(),
                        ctx.colours.dim(),
                    ));
                }
                if !c.parts.is_empty() {
                    out.push(Row::Text(
                        "Armor Level by body part".into(),
                        ctx.colours.dim(),
                    ));
                }
                for d in &c.parts {
                    let (by, colour) = match d.better {
                        Some(better) => (
                            StatKind::ArmorLevel.show_difference(d.item - d.worn),
                            if better { BETTER } else { WORSE },
                        ),
                        None => ("same".into(), ctx.colours.dim()),
                    };
                    let value = StatKind::ArmorLevel.show(d.item);
                    out.push(Row::Stat(d.part.name().into(), value, by, colour));
                }
                if c.totals && !c.rows.is_empty() {
                    out.push(Row::Text("Totals".into(), ctx.colours.dim()));
                }
                for d in &c.rows {
                    let value = d.item.map(|v| d.kind.show(v)).unwrap_or_default();
                    let (by, colour) = match (d.by(), d.better) {
                        (Some(by), Some(better)) => (
                            d.kind.show_difference(by),
                            if better { BETTER } else { WORSE },
                        ),
                        (Some(_), None) => ("same".into(), ctx.colours.dim()),
                        _ => (String::new(), plain),
                    };
                    out.push(Row::Stat(d.kind.name().into(), value, by, colour));
                }
                for note in spell_notes(c) {
                    for l in p.wrap(body, &note, w) {
                        out.push(Row::Text(l, ctx.colours.dim()));
                    }
                }
            }
        }
    }
    out
}

/// The panels side by side beside the pointer, each as tall as the screen allows.
fn draw_panels(p: &mut Painter<'_>, ctx: &Ctx<'_>, panels: &[DetailPanel]) {
    let k = p.scale;
    let (sw, sh) = p.screen;
    let pal = *p.art.palette();
    let head = TextStyle::new(Family::Body, 14.0, pal.label).edge(pal.edge);
    let body = TextStyle::new(Family::Body, 12.0, pal.tooltip_text).edge(pal.edge);
    let small = TextStyle::new(Family::Body, 11.0, ctx.colours.dim()).edge(pal.edge);
    #[allow(clippy::cast_precision_loss)]
    let n = panels.len().max(1) as f32;
    let w = (PANEL_W * k)
        .min((sw - 16.0 * k) / n - 6.0 * k)
        .max(120.0 * k);
    let inner = w - 28.0 * k;
    let lh = p.line_height(&body);
    let hh = p.line_height(&head);
    let icon = 32.0 * k;
    let laid: Vec<(Vec<String>, Vec<Row>)> = panels
        .iter()
        .map(|panel| {
            let title: Vec<String> = p
                .wrap(&head, &panel.title, inner - icon - 8.0 * k)
                .into_iter()
                .take(2)
                .collect();
            (title, rows(p, ctx, panel, &body, inner))
        })
        .collect();
    let top = |panel: &DetailPanel, title: &[String]| {
        let marker = if panel.worn { lh } else { 0.0 };
        #[allow(clippy::cast_precision_loss)]
        let t = hh * title.len().max(1) as f32;
        marker + t.max(icon) + 10.0 * k
    };
    let room = sh - 16.0 * k;
    let heights: Vec<f32> = panels
        .iter()
        .zip(&laid)
        .map(|(panel, (title, rows))| {
            #[allow(clippy::cast_precision_loss)]
            let h = top(panel, title) + lh * rows.len() as f32 + 20.0 * k;
            h.min(room)
        })
        .collect();
    let total_w = w * n + 6.0 * k * (n - 1.0);
    let tallest = heights.iter().copied().fold(0.0, f32::max);
    let (mx, my) = ctx.input.mouse;
    let mut x = mx + 18.0 * k;
    if x + total_w > sw {
        x = mx - total_w - 8.0 * k;
    }
    let mut y = my + 18.0 * k;
    if y + tallest > sh {
        y = my - tallest - 8.0 * k;
    }
    let x = x.clamp(0.0, (sw - total_w).max(0.0));
    let y = y.clamp(0.0, (sh - tallest).max(0.0));
    for (i, ((panel, (title, rows)), h)) in panels.iter().zip(&laid).zip(&heights).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let px = x + i as f32 * (w + 6.0 * k);
        let r = Rect::new(px, y, w, *h);
        kit::tooltip_plate(p, r);
        let left = px + 14.0 * k;
        let mut ly = y + 9.0 * k;
        if panel.worn {
            p.text(&small, left, ly, "Worn");
            ly += lh;
        }
        let icon_r = Rect::new(left, ly, icon, icon);
        if let Some(s) = panel.look.as_ref().and_then(|l| p.art.ac_item(l)) {
            p.sprite(&s, icon_r, WHITE);
        }
        let mut ty = ly;
        for l in title {
            p.text(&head, icon_r.right() + 8.0 * k, ty, l);
            ty += hh;
        }
        ly += top(panel, title) - if panel.worn { lh } else { 0.0 };
        p.fill(Rect::new(left, ly - 5.0 * k, inner, 1.0 * k), 0x60C8_B27A);
        let bottom = r.bottom() - 10.0 * k;
        // Rows past the screen's room give way to a line saying where the rest is.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let fits = rows
            .len()
            .min(((bottom - ly) / lh).floor().max(0.0) as usize);
        let cut = fits < rows.len();
        let shown = if cut { fits.saturating_sub(1) } else { fits };
        for row in &rows[..shown] {
            match row {
                Row::Text(t, c) => {
                    p.text(&body.colour(*c), left, ly, t);
                }
                Row::Stat(name, value, by, c) => {
                    p.text(&body.colour(ctx.colours.dim()), left, ly, name);
                    let bw = p.measure(&body, by);
                    p.text(&body.colour(*c), left + inner - bw, ly, by);
                    let vw = p.measure(&body, value);
                    p.text(&body, left + inner - 52.0 * k - vw, ly, value);
                }
            }
            ly += lh;
        }
        if cut {
            p.text(&small, left, ly, "Examine the item to read the rest.");
        }
    }
}
