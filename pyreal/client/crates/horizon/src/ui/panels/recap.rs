//! The Journal's Deaths tab: the deaths kept, the newest first, and the chosen one's recap: who
//! killed the player over the game's own death line, their health over the seconds before, and
//! the hits they took, newest first; then the items the death cost and the vitae penalty. For the
//! newest death outside a dungeon, Set Waypoint to Corpse stands at its foot.

use super::{JournalTab, WindowId, Windows};
use crate::art::Family;
use crate::draw::{with_alpha, Rect, WHITE};
use crate::ui::death_recap::{before_words, Kind, Recap, HEALTH_STEP, KEEP_SECONDS};
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};

/// The colours of damage, of healing, and of evades and resists in the list, as the combat
/// numbers have them.
const DAMAGE: u32 = 0xFFFF_6A56;
const HEALING: u32 = 0xFF96_E07E;
const AVOIDED: u32 = 0xFFD8_D4CC;

/// The list's columns, as shares of its width: before the death, from, amount and type.
const COLUMNS: [f32; 4] = [0.13, 0.34, 0.13, 0.40];

/// What the tab says while the game has not yet said what the death cost.
pub const WAITING: &str = "Waiting for the game to say what you lost.";

/// The vitae penalty as its enchantment has it: `vitae` is what is left of the player's
/// strength.
#[must_use]
pub fn vitae_words(vitae: Option<f32>) -> String {
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a share in percent.
    let penalty = ((1.0 - vitae.unwrap_or(1.0)) * 100.0).round() as i32;
    if penalty > 0 {
        format!("Your vitae penalty is {penalty}%.")
    } else {
        "You have no vitae penalty.".to_owned()
    }
}

/// What the tab says before any death.
pub const NO_DEATH: &str = "You have not died since you entered the world.";

/// How long ago a death `seconds` back came, as the list of deaths says it.
#[must_use]
pub fn ago_words(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // LINT-OK: whole minutes.
    let minutes = (seconds.max(0.0) / 60.0).floor() as u64;
    let hours = minutes / 60;
    match (hours, minutes) {
        (0, 0) => "Just now".to_owned(),
        (0, 1) => "1 minute ago".to_owned(),
        (0, m) => format!("{m} minutes ago"),
        (1, _) => "1 hour ago".to_owned(),
        (h, _) => format!("{h} hours ago"),
    }
}

impl Windows {
    /// Open the Journal at its Deaths tab.
    pub fn show_deaths(&mut self, now: f64) {
        self.open(WindowId::Journal, now);
        self.show_journal_tab(JournalTab::Deaths);
    }

    /// The death recap's key: the Journal at its Deaths tab, or shut while it shows them.
    pub fn toggle_deaths(&mut self, now: f64) {
        if self.is_open(WindowId::Journal) && self.journal_page() == JournalTab::Deaths {
            self.toggle(WindowId::Journal, now);
        } else {
            self.show_deaths(now);
        }
    }

    /// The death the Deaths tab shows.
    #[must_use]
    pub fn chosen_death(&self) -> Option<&Recap> {
        self.death_recap.deaths().nth(self.death_selected)
    }

    /// A new death: it is the one chosen, and the Journal opens at it when `opens`.
    pub(crate) fn death_came(&mut self, opens: bool, now: f64) {
        self.death_selected = 0;
        self.death_recap_scroll = 0.0;
        if opens {
            self.show_deaths(now);
        }
    }

    /// The Deaths tab: the deaths kept on the left, the newest first, and the chosen one's recap
    /// on the right.
    pub(super) fn journal_deaths(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
    ) {
        let k = p.scale;
        let deaths: Vec<Recap> = self.death_recap.deaths().cloned().collect();
        if deaths.is_empty() {
            let dim =
                TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
            p.fill(body, 0x6010_0C08);
            p.text_in(&dim, body, Align::Centre, NO_DEATH);
            return;
        }
        self.death_selected = self.death_selected.min(deaths.len() - 1);
        let list = Rect::new(body.x, body.y, body.w * 0.3, body.h);
        self.death_list(p, ctx, list, &deaths);
        let pane = Rect::new(
            list.right() + 12.0 * k,
            body.y,
            body.right() - list.right() - 12.0 * k,
            body.h,
        );
        kit::panel(p, pane, 0.5);
        let recap = &deaths[self.death_selected];
        let newest = self.death_selected == 0;
        let foot_h = if self.corpse_waypoint(state, recap, newest).is_some() {
            40.0 * k
        } else {
            4.0 * k
        };
        let foot = Rect::new(pane.x, pane.bottom() - foot_h, pane.w, foot_h);
        self.death_recap_buttons(p, ctx, state, recap, newest, foot);
        let mut y = death_head(p, ctx, pane, recap);
        y = health_graph(p, ctx, pane, y, recap);
        let after = Rect::new(
            pane.x + 14.0 * k,
            foot.y - 42.0 * k,
            pane.w - 28.0 * k,
            40.0 * k,
        );
        let hits = Rect::new(
            pane.x + 10.0 * k,
            y,
            pane.w - 20.0 * k,
            after.y - y - 2.0 * k,
        );
        self.death_recap_list(p, ctx, hits, recap);
        self.death_recap_cost(p, ctx, state, recap, newest, after);
    }

    /// The deaths kept, the newest first: who killed the player, and how long ago.
    fn death_list(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, list: Rect, deaths: &[Recap]) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        p.fill(list, 0x6010_0C08);
        let rows = kit::ScrollList::begin(
            p,
            ctx,
            list,
            deaths.len(),
            42.0 * k,
            &mut self.deaths_scroll,
        );
        for (i, r) in deaths.iter().enumerate() {
            let Some(row) = rows.row(i, 40.0 * k) else {
                continue;
            };
            let on = self.death_selected == i;
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            let w = row.w - 16.0 * k;
            p.text(
                &name,
                row.x + 8.0 * k,
                row.y + 3.0 * k,
                &p.fit(&name, &r.heading(), w),
            );
            p.text(
                &dim,
                row.x + 8.0 * k,
                row.y + 21.0 * k,
                &ago_words(ctx.time - r.at),
            );
            if ctx.input.hover(&row) {
                self.tip = Some((r.heading(), vec![r.detail()]));
            }
            if ctx.over(&row) && ctx.input.clicked(&row) && !on {
                self.death_selected = i;
                self.death_recap_scroll = 0.0;
            }
        }
        rows.end(p, ctx);
    }

    /// The items the death cost, once the game has said, and for the newest death the vitae
    /// penalty now.
    fn death_recap_cost(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &Ctx<'_>,
        state: &GameState,
        recap: &Recap,
        newest: bool,
        r: Rect,
    ) {
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let rows = [
            Rect::new(r.x, r.y, r.w, r.h / 2.0),
            Rect::new(r.x, r.y + r.h / 2.0, r.w, r.h / 2.0),
        ];
        match &recap.loss {
            Some(loss) => {
                let words = loss.words();
                p.text_in(&text, rows[0], Align::Left, &p.fit(&text, &words, r.w));
                if ctx.input.hover(&rows[0]) && !loss.items.is_empty() {
                    self.tip = Some(("Items lost".to_owned(), loss.items.clone()));
                }
            }
            None if ctx.time - recap.at <= crate::ui::death_recap::LOSS_SECONDS => {
                p.text_in(&dim, rows[0], Align::Left, WAITING);
            }
            None => {}
        }
        if newest {
            p.text_in(&dim, rows[1], Align::Left, &vitae_words(state.vitae));
        }
    }

    /// The hits taken, newest first, under their column headings.
    fn death_recap_list(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, recap: &Recap) {
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let text = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let head_h = 20.0 * k;
        let cells = |r: Rect| {
            let mut x = r.x + 6.0 * k;
            let w = r.w - 12.0 * k;
            COLUMNS.map(|c| {
                let cell = Rect::new(x, r.y, w * c - 6.0 * k, r.h);
                x += w * c;
                cell
            })
        };
        let heads = cells(Rect::new(r.x, r.y, r.w, head_h));
        for (cell, (label, align)) in heads.iter().zip([
            ("Before", Align::Left),
            ("From", Align::Left),
            ("Amount", Align::Right),
            ("Type", Align::Left),
        ]) {
            p.text_in(&dim, *cell, align, label);
        }
        let area = Rect::new(r.x, r.y + head_h, r.w, r.h - head_h);
        p.fill(area, 0x6010_0C08);
        if recap.taken.is_empty() {
            p.text_in(&dim, area, Align::Centre, "No hits were recorded.");
            return;
        }
        let row_h = 22.0 * k;
        let rows = kit::ScrollList::begin(
            p,
            ctx,
            area,
            recap.taken.len(),
            row_h,
            &mut self.death_recap_scroll,
        );
        for (i, t) in recap.taken.iter().rev().enumerate() {
            let Some(row) = rows.row(i, row_h) else {
                continue;
            };
            if i % 2 == 1 {
                p.fill(row, 0x2000_0000);
            }
            let [before, from, amount, kind] = cells(row);
            let colour = match t.kind {
                Kind::Healing => HEALING,
                Kind::Evaded | Kind::Resisted => AVOIDED,
                Kind::Damage | Kind::Drain => DAMAGE,
            };
            p.text_in(&dim, before, Align::Left, &before_words(recap.at, t));
            let name = kit::middle_ellipsis(p, &text, t.source(), from.w);
            p.text_in(&text, from, Align::Left, &name);
            p.text_in(
                &text.colour(colour),
                amount,
                Align::Right,
                &t.amount_words(),
            );
            p.text_in(
                &dim,
                kind,
                Align::Left,
                &p.fit(&dim, &t.type_words(), kind.w),
            );
        }
        rows.end(p, ctx);
    }

    /// The waypoint to the corpse of `recap`, while it is the newest death, outside a dungeon,
    /// and the corpse is marked.
    fn corpse_waypoint(
        &self,
        state: &GameState,
        recap: &Recap,
        newest: bool,
    ) -> Option<crate::ui::waypoint::Waypoint> {
        if !newest || recap.in_dungeon {
            return None;
        }
        self.waypoints.corpse.waypoint(&state.name)
    }

    /// Set Waypoint to Corpse, at the right of the foot, while the newest death's corpse is
    /// marked outside a dungeon.
    fn death_recap_buttons(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        recap: &Recap,
        newest: bool,
        foot: Rect,
    ) {
        let Some(w) = self.corpse_waypoint(state, recap, newest) else {
            return;
        };
        let k = p.scale;
        let h = 26.0 * k;
        let y = foot.y + (foot.h - h) / 2.0;
        let label = super::map::SET_WAYPOINT_TO_CORPSE;
        let followed = self
            .waypoints
            .corpse
            .is_followed(self.waypoints.active.as_ref());
        let bw = kit::button_label_width(p, label) + 32.0 * k;
        let set = Rect::new(foot.right() - 12.0 * k - bw, y, bw, h);
        if ctx.input.hover(&set) {
            self.tip = Some((
                "Your corpse".to_owned(),
                vec![crate::ui::waypoint::set_text(&w)],
            ));
        }
        if kit::button(p, ctx, set, label, !followed) {
            self.waypoints.set(w);
        }
    }
}

/// The heading: the corpse's stone, who killed the player and the game's line, over a rule.
/// Where what follows begins.
fn death_head(p: &mut Painter<'_>, ctx: &Ctx<'_>, body: Rect, recap: &Recap) -> f32 {
    let k = p.scale;
    let head = Rect::new(
        body.x + 12.0 * k,
        body.y + 8.0 * k,
        body.w - 24.0 * k,
        44.0 * k,
    );
    let mut tx = head.x;
    let mark = p.art.cached(crate::ui::corpse::MARK_KEY, || {
        Some(crate::ui::corpse::mark_image())
    });
    if let Some(mark) = mark {
        p.sprite(&mark, Rect::new(head.x, head.y, head.h, head.h), WHITE);
        tx += head.h + 10.0 * k;
    }
    let w = head.right() - tx;
    let big = TextStyle::new(Family::Body, 16.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
    let sub = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
    p.text(
        &big,
        tx,
        head.y + 2.0 * k,
        &p.fit(&big, &recap.heading(), w),
    );
    p.text(
        &sub,
        tx,
        head.y + 24.0 * k,
        &p.fit(&sub, &recap.detail(), w),
    );
    let rule = head.bottom() + 8.0 * k;
    p.fill(
        Rect::new(body.x + 8.0 * k, rule, body.w - 16.0 * k, 1.0 * k),
        0x60C8_B27A,
    );
    rule + 8.0 * k
}

/// The health over the seconds before the death, as bars from the oldest at the left to the death
/// at the right, with the damage taken in those seconds. Where what follows begins.
fn health_graph(p: &mut Painter<'_>, ctx: &Ctx<'_>, body: Rect, y: f32, recap: &Recap) -> f32 {
    let k = p.scale;
    let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
    let x = body.x + 14.0 * k;
    let w = body.w - 28.0 * k;
    let label = Rect::new(x, y, w, 18.0 * k);
    p.text_in(&dim, label, Align::Left, "Health");
    let total = recap.damage_total();
    if total > 0 {
        let words = format!("{total} damage taken");
        p.text_in(&dim.colour(DAMAGE), label, Align::Right, &words);
    }
    let graph = Rect::new(x, label.bottom() + 2.0 * k, w, 40.0 * k);
    p.fill(graph, 0x6010_0C08);
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: sixty samples.
    let slots = (KEEP_SECONDS / HEALTH_STEP) as f32 + 1.0;
    let step = graph.w / slots;
    let n = recap.health.len();
    for (i, h) in recap.health.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)] // LINT-OK: sixty samples.
        let from_right = (n - 1 - i) as f32;
        let bx = graph.right() - (from_right + 1.0) * step;
        let bh = (graph.h - 2.0 * k) * h.clamp(0.0, 1.0);
        let bar = Rect::new(bx, graph.bottom() - 1.0 * k - bh, step.max(1.0), bh);
        p.fill(bar, with_alpha(crate::ui::colours::HEALTH, 0.9));
    }
    let marks = Rect::new(x, graph.bottom() + 2.0 * k, w, 16.0 * k);
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: fifteen seconds.
    let reach = format!("{}s before", KEEP_SECONDS as u32);
    p.text_in(&dim, marks, Align::Left, &reach);
    p.text_in(&dim, marks, Align::Right, "Death");
    marks.bottom() + 6.0 * k
}
