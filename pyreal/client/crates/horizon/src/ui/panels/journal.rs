//! The Journal (the contracts the player holds, each with its notes, contact and places, and
//! Abandon; the notebook, in its own module; every timer in one list; and the deaths, in the
//! recap module), and the Profile tab's
//! titles and life facts, doing what the game's contracts, titles and character information
//! panels do.

use pyreal_client_contract::journal::JournalAction;
use pyreal_client_contract::UiRequest;

use super::Windows;
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// Seconds as the game counts an age: days, hours, minutes and seconds, the largest first and
/// the zero ones left out.
#[must_use]
pub fn duration_words(seconds: i64) -> String {
    let s = seconds.max(0);
    let parts = [
        (s / 86_400, "day"),
        (s / 3_600 % 24, "hour"),
        (s / 60 % 60, "minute"),
        (s % 60, "second"),
    ];
    let words: Vec<String> = parts
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, w)| format!("{n} {w}{}", if *n == 1 { "" } else { "s" }))
        .collect();
    if words.is_empty() {
        "0 seconds".to_owned()
    } else {
        words.join(", ")
    }
}

/// The Journal's tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalTab {
    Contracts,
    Notebook,
    Timers,
    Deaths,
}

/// One running or lately ready timer on the Timers tab: what it is called, when it is ready on
/// the game's clock, where it comes from, and what its button does.
#[derive(Debug, Clone, PartialEq)]
pub struct TimerRow {
    pub title: String,
    pub ready_at: f64,
    pub from: &'static str,
    pub action: TimerAction,
    /// What the quest tracker pins for it.
    pub pin: Option<super::tracker::Pin>,
}

/// A Timers row's button.
#[derive(Debug, Clone, PartialEq)]
pub enum TimerAction {
    /// Open this notebook page.
    Open(u32),
    /// Start a notebook page's timer for this quest.
    Track,
    /// Show the contracts.
    Contracts,
}

/// Every timer for the Timers tab at `now`: the notebook's running ones, the contracts waiting
/// to repeat, the world's quest timers and those heard in chat, each that is running or ran out
/// since the player arrived; running ones soonest first, then the ready ones, latest first.
#[must_use]
pub fn timer_rows(state: &GameState) -> Vec<TimerRow> {
    let j = &state.journal;
    let now = state.clock;
    let mut rows: Vec<TimerRow> = j
        .pages
        .iter()
        .filter(|p| p.timer_running && p.timer_stamp > 0.0)
        .map(|p| TimerRow {
            title: if p.name().is_empty() {
                format!("Page {}", p.page_number)
            } else {
                p.name().to_owned()
            },
            ready_at: p.timer_stamp,
            from: "Notebook page",
            action: TimerAction::Open(p.page_number),
            pin: super::tracker::Pin::page(p),
        })
        .collect();
    rows.extend(state.contracts.iter().filter_map(|c| {
        c.repeat_remaining.map(|left| TimerRow {
            title: c.name.clone(),
            ready_at: now + left,
            from: "Contract",
            action: TimerAction::Contracts,
            pin: Some(super::tracker::Pin::Contract(c.contract_id)),
        })
    }));
    let tracked = |title: &str| {
        j.pages
            .iter()
            .any(|p| p.timer_running && p.name().eq_ignore_ascii_case(title))
    };
    rows.extend(j.quests.iter().filter_map(|q| {
        let at = q.ready_at?;
        (q.min_delta > 0 && at > j.since && !tracked(&q.title)).then(|| TimerRow {
            title: q.title.clone(),
            ready_at: at,
            from: "Quest",
            action: TimerAction::Track,
            pin: Some(super::tracker::Pin::Quest(q.name.clone())),
        })
    }));
    rows.extend(
        j.heard
            .iter()
            .filter(|h| !tracked(&h.title))
            .map(|h| TimerRow {
                title: h.title.clone(),
                ready_at: h.ready_at,
                from: "Heard in chat",
                action: TimerAction::Track,
                pin: Some(super::tracker::Pin::Heard(h.title.clone())),
            }),
    );
    rows.sort_by(|a, b| {
        let (ra, rb) = (a.ready_at <= now, b.ready_at <= now);
        ra.cmp(&rb).then(if ra {
            b.ready_at.total_cmp(&a.ready_at)
        } else {
            a.ready_at.total_cmp(&b.ready_at)
        })
    });
    rows
}

impl Windows {
    /// The Journal's tabs, in order: the contracts where the world has them, the notebook, the
    /// timers and the deaths.
    fn journal_tabs(&self) -> Vec<JournalTab> {
        let mut tabs = Vec::with_capacity(4);
        if self.has_contracts() {
            tabs.push(JournalTab::Contracts);
        }
        tabs.extend([JournalTab::Notebook, JournalTab::Timers, JournalTab::Deaths]);
        tabs
    }

    /// The Journal's tab on show.
    #[must_use]
    pub fn journal_page(&self) -> JournalTab {
        let tabs = self.journal_tabs();
        tabs.get(self.journal_tab)
            .copied()
            .unwrap_or(JournalTab::Notebook)
    }

    /// Bring the Journal's `tab` up.
    pub fn show_journal_tab(&mut self, tab: JournalTab) {
        if let Some(i) = self.journal_tabs().iter().position(|t| *t == tab) {
            self.journal_tab = i;
        }
    }

    /// The Journal: the contracts, the notebook, the timers and the deaths, a tab each.
    pub(super) fn journal(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let strip = Rect::new(body.x, body.y, body.w, 26.0 * k);
        let tabs = self.journal_tabs();
        let ready = timer_rows(state)
            .iter()
            .filter(|r| r.ready_at <= state.clock)
            .count();
        let timers = if ready > 0 {
            format!("Timers ({ready})")
        } else {
            "Timers".to_owned()
        };
        let labels: Vec<&str> = tabs
            .iter()
            .map(|t| match t {
                JournalTab::Contracts => "Contracts",
                JournalTab::Notebook => "Notebook",
                JournalTab::Timers => timers.as_str(),
                JournalTab::Deaths => "Deaths",
            })
            .collect();
        self.journal_tab = self.journal_tab.min(tabs.len() - 1);
        kit::tabs(p, ctx, strip, &labels, &mut self.journal_tab);
        let body = Rect::new(body.x, body.y + 34.0 * k, body.w, body.h - 34.0 * k);
        match self.journal_page() {
            JournalTab::Contracts => self.contracts(p, ctx, state, body),
            JournalTab::Notebook => self.notebook(p, ctx, state, body, out),
            JournalTab::Timers => self.journal_timers(p, ctx, state, body, out),
            JournalTab::Deaths => self.journal_deaths(p, ctx, state, body),
        }
    }

    /// The timers: what each is, how long is left, where it is from, and a button to open its
    /// page or start a page's timer for it.
    fn journal_timers(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let rows = timer_rows(state);
        let (from_w, button_w, pin_w) = (120.0 * k, 96.0 * k, 64.0 * k);
        let cols = |r: Rect| {
            let button = Rect::new(r.right() - button_w, r.y, button_w, r.h);
            let pin = Rect::new(button.x - pin_w - 4.0 * k, r.y, pin_w, r.h);
            let from = Rect::new(pin.x - from_w - 8.0 * k, r.y, from_w, r.h);
            let left = Rect::new(from.x - 100.0 * k, r.y, 92.0 * k, r.h);
            let title = Rect::new(r.x + 8.0 * k, r.y, left.x - r.x - 16.0 * k, r.h);
            (title, left, from, pin, button)
        };
        let head = Rect::new(body.x, body.y, body.w, 22.0 * k);
        let (t, l, f, _, _) = cols(head);
        p.text_in(&dim, t, Align::Left, "Timer");
        p.text_in(&dim, l, Align::Left, "Ready in");
        p.text_in(&dim, f, Align::Left, "From");
        p.fill(
            Rect::new(head.x, head.bottom(), head.w, k.max(1.0)),
            0x80B3_9555,
        );
        let foot_h = if state.journal.quests_known {
            0.0
        } else {
            40.0 * k
        };
        let list = Rect::new(
            body.x,
            head.bottom() + 2.0 * k,
            body.w,
            body.bottom() - head.bottom() - 2.0 * k - foot_h,
        );
        p.fill(list, 0x6010_0C08);
        if rows.is_empty() {
            let lines = p.wrap(
                &dim,
                "No timers are running. Start one on a notebook page, or follow a quest timer from chat.",
                list.w - 60.0 * k,
            );
            let lh = p.line_height(&dim);
            #[allow(clippy::cast_precision_loss)]
            let mut y = list.y + list.h / 2.0 - lh * lines.len() as f32 / 2.0;
            for l in lines {
                p.text_in(&dim, Rect::new(list.x, y, list.w, lh), Align::Centre, &l);
                y += lh;
            }
        }
        let row_h = 30.0 * k;
        let scroll =
            kit::ScrollList::begin(p, ctx, list, rows.len(), row_h, &mut self.timers_scroll);
        for (i, row) in rows.iter().enumerate() {
            let Some(r) = scroll.row(i, row_h - 2.0 * k) else {
                continue;
            };
            if ctx.input.hover(&r) {
                crate::ui::pregame::list_highlight(p, r, false);
            }
            let (t, l, f, pin, b) = cols(r);
            let title = kit::middle_ellipsis(p, &name, &row.title, t.w);
            p.text_in(&name, t, Align::Left, &title);
            let left = row.ready_at - state.clock;
            let (words, colour) = if left <= 0.0 {
                ("Ready".to_owned(), super::notebook::READY_COLOUR)
            } else {
                (
                    pyreal_presentation::journal::short_time(left),
                    super::notebook::RUNNING_COLOUR,
                )
            };
            let style = TextStyle::new(Family::Body, 13.0, colour).edge(ctx.colours.edge());
            p.text_in(&style, l, Align::Left, &words);
            p.text_in(&dim, f, Align::Left, row.from);
            let button = Rect::new(b.x, b.y + 2.0 * k, b.w, b.h - 4.0 * k);
            let pin = Rect::new(pin.x, button.y, pin.w, button.h);
            self.pin_button(p, ctx, pin, row.pin.clone(), state);
            match &row.action {
                TimerAction::Open(page) => {
                    if kit::button(p, ctx, button, "Open", true) {
                        self.show_journal_tab(JournalTab::Notebook);
                        out.requests
                            .push(UiRequest::Journal(JournalAction::Goto(*page)));
                    }
                }
                TimerAction::Contracts => {
                    if kit::button(p, ctx, button, "Contracts", true) {
                        self.show_journal_tab(JournalTab::Contracts);
                    }
                }
                TimerAction::Track => {
                    let seconds = left.max(0.0).ceil();
                    if kit::button(p, ctx, button, "Track", left > 0.0) {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                        let seconds = seconds.min(f64::from(u32::MAX)) as u32;
                        self.show_journal_tab(JournalTab::Notebook);
                        out.requests
                            .push(UiRequest::Journal(JournalAction::TrackTimer {
                                title: row.title.clone(),
                                seconds,
                            }));
                    }
                }
            }
        }
        scroll.end(p, ctx);
        if foot_h > 0.0 {
            let note = Rect::new(body.x, list.bottom() + 4.0 * k, body.w, foot_h - 4.0 * k);
            for (n, l) in p
                .wrap(
                    &dim,
                    "This world doesn't send quest timers. A timer a quest giver tells you of in chat shows here, and its link in chat starts one on a page.",
                    note.w,
                )
                .into_iter()
                .take(2)
                .enumerate()
            {
                #[allow(clippy::cast_precision_loss)]
                p.text(&dim, note.x, note.y + n as f32 * 16.0 * k, &l);
            }
        }
    }

    /// The contracts: the list on the left, the chosen one's notes, contact and places on the
    /// right, and Abandon.
    fn contracts(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, state: &GameState, body: Rect) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let list = Rect::new(body.x, body.y, body.w * 0.42, body.h - 36.0 * k);
        p.fill(list, 0x6010_0C08);
        if state.contracts.is_empty() {
            p.text_in(&dim, list, Align::Centre, "No contracts held.");
        }
        let rows = kit::ScrollList::begin(
            p,
            ctx,
            list,
            state.contracts.len(),
            42.0 * k,
            &mut self.contracts_scroll,
        );
        for (i, c) in state.contracts.iter().enumerate() {
            let Some(row) = rows.row(i, 40.0 * k) else {
                continue;
            };
            let y = row.y;
            let on = self.contract_selected == Some(c.contract_id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.text(&name, row.x + 8.0 * k, y + 3.0 * k, &c.name);
            p.text(&dim, row.x + 8.0 * k, y + 21.0 * k, &c.status);
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.contract_selected = Some(c.contract_id);
            }
        }
        rows.end(p, ctx);
        let pane = Rect::new(
            list.right() + 12.0 * k,
            body.y,
            body.right() - list.right() - 12.0 * k,
            list.h,
        );
        kit::panel(p, pane, 0.5);
        let Some(c) = self
            .contract_selected
            .and_then(|id| state.contracts.iter().find(|c| c.contract_id == id))
        else {
            p.text_in(&dim, pane, Align::Centre, "Choose a contract.");
            return;
        };
        let head = TextStyle::new(Family::Body, 15.0, 0xFFFF_FFFF).edge(ctx.colours.edge());
        let x = pane.x + 12.0 * k;
        let w = pane.w - 24.0 * k;
        let mut y = pane.y + 10.0 * k;
        for l in p.wrap(&head, &c.name, w) {
            p.text(&head, x, y, &l);
            y += 20.0 * k;
        }
        let mut facts = vec![c.status.clone()];
        if !c.timed.is_empty() {
            facts.push(c.timed.clone());
        }
        if !c.contact.is_empty() {
            facts.push(format!("Contact: {}", c.contact));
            facts.push(format!(
                "Contact's place: {}",
                c.contact_location
                    .clone()
                    .unwrap_or_else(|| "Indoors".to_owned())
            ));
        }
        if let Some(area) = &c.area_location {
            facts.push(format!("Where: {area}"));
        }
        for f in facts.iter().filter(|f| !f.is_empty()) {
            for l in p.wrap(&dim, f, w) {
                p.text(&dim, x, y, &l);
                y += 16.0 * k;
            }
        }
        y += 8.0 * k;
        let lh = p.line_height(&name);
        for l in p.wrap(&name, &c.description, w) {
            if y + lh > pane.bottom() - 6.0 * k {
                break;
            }
            p.text(&name, x, y, &l);
            y += lh;
        }
        let pin = Rect::new(
            body.right() - 224.0 * k,
            body.bottom() - 30.0 * k,
            96.0 * k,
            28.0 * k,
        );
        self.pin_button(
            p,
            ctx,
            pin,
            Some(super::tracker::Pin::Contract(c.contract_id)),
            state,
        );
        if kit::button(
            p,
            ctx,
            Rect::new(
                body.right() - 120.0 * k,
                body.bottom() - 30.0 * k,
                120.0 * k,
                28.0 * k,
            ),
            "Abandon",
            true,
        ) {
            self.abandon_asked = Some((c.contract_id, c.name.clone()));
        }
    }

    /// Whether the world has the Journal window's Contracts tab.
    fn has_contracts(&self) -> bool {
        !self.refuses(pyreal_client_contract::era::EraSurface::Contracts)
    }

    /// The question before a contract is abandoned.
    pub(super) fn abandon_question(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        out: &mut Outcome,
    ) {
        let Some((id, name)) = self.abandon_asked.clone() else {
            return;
        };
        let text = format!("Abandon {name}?");
        if let Some(b) = crate::ui::pregame::dialog_box(
            p,
            ctx,
            "Abandon Contract",
            &text,
            &["Yes", "No"],
            true,
            0,
        ) {
            self.abandon_asked = None;
            if b == 0 {
                out.requests
                    .push(UiRequest::AbandonContract { contract_id: id });
                self.contract_selected = None;
            }
        }
    }

    /// The Profile tab's lower half: the titles to choose from, and the character's life.
    pub(super) fn profile_extra(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let section =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let label = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let value = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut y = area.y;
        let row = |p: &mut Painter<'_>, y: f32, l: &str, v: &str| {
            super::stats::label_and_value(
                p,
                Rect::new(area.x, y - 2.0 * k, area.w, 18.0 * k),
                &label,
                area.x + 20.0 * k,
                l,
                &value,
                area.right() - 20.0 * k,
                v,
            );
        };
        if let Some(info) = &state.character_info {
            p.text(&section, area.x + 14.0 * k, y, "LIFE");
            y += 24.0 * k;
            if let Some(age) = info.age {
                row(p, y, "Age", &duration_words(i64::from(age)));
                y += 18.0 * k;
            }
            row(p, y, "Deaths", &info.num_deaths.to_string());
            y += 18.0 * k;
            row(
                p,
                y,
                "Burden",
                &format!("{} / {}", info.encumbrance, info.capacity),
            );
            y += 18.0 * k;
            if self.world.features.is_none_or(|f| f.augmentations) {
                row(p, y, "Augmentations", &info.augmentations.to_string());
                y += 18.0 * k;
            }
            let innate = info
                .innate
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(" / ");
            // The short label when the long one would run into the six numbers.
            let room = area.w - 40.0 * k - p.measure(&value, &innate) - 12.0 * k;
            let name = if p.measure(&label, "Innate attributes") <= room {
                "Innate attributes"
            } else {
                "Innate"
            };
            row(p, y, name, &innate);
            y += 26.0 * k;
        }
        // The titles: the one shown, and a list to choose another from.
        let titles = &state.titles;
        if titles.titles.is_empty() || self.refuses(pyreal_client_contract::era::EraSurface::Titles)
        {
            return;
        }
        p.text(&section, area.x + 14.0 * k, y, "TITLES");
        // A little room between the heading and the first row, so a highlight on it does not crowd
        // the heading.
        y += 28.0 * k;
        let list = Rect::new(
            area.x + 10.0 * k,
            y,
            area.w - 20.0 * k,
            area.bottom() - y - 4.0 * k,
        );
        let row_h = 22.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * titles.titles.len() as f32;
        let offset = kit::scroll(p, ctx, list, content, &mut self.titles_scroll);
        // For the pad the titles are the page's one list of rows.
        {
            let input = &mut *ctx.input;
            input.nav.list_of_rows(list, &input.occluders);
        }
        p.list.push_clip(list);
        for (i, (id, name)) in titles.titles.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                list.x,
                list.y + i as f32 * row_h - offset,
                list.w - 8.0 * k,
                row_h,
            );
            if r.bottom() < list.y || r.y > list.bottom() {
                continue;
            }
            let on = *id == titles.display;
            if on || ctx.input.hover(&r) {
                crate::ui::pregame::list_highlight(p, r, on);
            }
            p.text_in(&value, r.offset(8.0 * k, 0.0), Align::Left, name);
            // The title shown is a stop too, though choosing it again changes nothing.
            if ctx.over(&r) && !on && ctx.input.clicked(&r) {
                out.requests
                    .push(UiRequest::SetDisplayCharacterTitle { title_id: *id });
            }
        }
        p.list.pop_clip();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_age_is_worded_largest_first_without_its_zero_parts() {
        assert_eq!(
            super::duration_words(90_061),
            "1 day, 1 hour, 1 minute, 1 second"
        );
        assert_eq!(super::duration_words(7_200), "2 hours");
        assert_eq!(super::duration_words(0), "0 seconds");
    }
}
