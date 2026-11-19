//! The Journal's Notebook tab: the player's own notes, kept by the game's shared notebook and
//! saved beside the preferences for the character, never sent to the world. The pages are listed
//! on the left, searched as the player types, narrowed to a tag and sorted by their column; the
//! open page is on the right, with its title, tags, place, timer and notes. A page with a place
//! can be shown on the map or made the waypoint, and a place written in the notes is a link to
//! the map.

use pyreal_client_contract::journal::{JournalAction, JournalField, JournalPage, JournalView};
use pyreal_client_contract::UiRequest;
use pyreal_client_model::cmd::waypoint::find_places;
use pyreal_presentation::journal::{location_text, page_contains_string, parse_count, short_time};

use super::map::link_text;
use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::{edit, Outcome};

/// The text box ids the notebook's fields take the keyboard under.
mod field {
    pub const TAGS: u32 = 41;
    pub const TITLE: u32 = 42;
    pub const DAYS: u32 = 43;
    pub const HOURS: u32 = 44;
    pub const MINUTES: u32 = 45;
    pub const NOTES: u32 = 46;
    pub const SEARCH: u32 = 47;
}

/// A ready timer's colour, and a running one's.
pub const READY_COLOUR: u32 = 0xFF9F_E08A;
pub const RUNNING_COLOUR: u32 = 0xFFFF_CF6A;

/// What the page list is sorted by, and whether the other way round.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sort {
    pub by: SortBy,
    pub reverse: bool,
}

/// The page list's columns, each a sort.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Number,
    Title,
    Timer,
}

/// The pages the list shows, in its order: those holding `search` (in a title, tag or note) and
/// tagged `tag`, sorted. Running timers sort soonest first, ahead of the rest.
#[must_use]
pub fn listed<'a>(
    pages: &'a [JournalPage],
    search: &str,
    tag: Option<&str>,
    sort: Sort,
) -> Vec<&'a JournalPage> {
    use pyreal_presentation::journal::{compare, JournalSortCriteria as C};
    let search = search.trim();
    let mut shown: Vec<&JournalPage> = pages
        .iter()
        .filter(|p| page_contains_string(p, search))
        .filter(|p| tag.is_none_or(|t| p.has_tag(t)))
        .collect();
    let by = match sort.by {
        SortBy::Number => C::PageNumber,
        SortBy::Title => C::Title,
        SortBy::Timer => C::Timer,
    };
    shown.sort_by(|a, b| {
        let order = compare(a, b, by).then(a.page_number.cmp(&b.page_number));
        if sort.reverse {
            order.reverse()
        } else {
            order
        }
    });
    shown
}

/// Every tag the pages carry, once each (the first spelling met), in the order first met.
#[must_use]
pub fn all_tags(pages: &[JournalPage]) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for t in pages.iter().flat_map(JournalPage::tags) {
        if !tags.iter().any(|have| have.eq_ignore_ascii_case(t)) {
            tags.push(t.to_owned());
        }
    }
    tags
}

/// A timer's words at `now`: how long is left, `Ready`, or nothing when it is not running.
#[must_use]
pub fn timer_words(page: &JournalPage, now: f64) -> Option<(String, u32)> {
    let left = page.timer_left(now)?;
    Some(if left <= 0.0 {
        ("Ready".to_owned(), READY_COLOUR)
    } else {
        (short_time(left), RUNNING_COLOUR)
    })
}

/// The day a Unix instant falls on, as the page's dates say it: today, yesterday, or the date.
#[must_use]
pub fn day_words(at: u64, wall: u64, offset: impl Fn(i64) -> i32) -> String {
    use pyreal_client_contract::ctime::broken_down;
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let at = i64::try_from(at).unwrap_or(0);
    let wall = i64::try_from(wall).unwrap_or(0);
    let day = |t: i64| (t + i64::from(offset(t))).div_euclid(86_400);
    match day(wall) - day(at) {
        0 => "today".to_owned(),
        1 => "yesterday".to_owned(),
        _ => {
            let d = broken_down(at, offset(at));
            let month = usize::try_from(d.month - 1).map_or("", |m| MONTHS[m.min(11)]);
            if broken_down(wall, offset(wall)).year == d.year {
                format!("{} {month}", d.day)
            } else {
                format!("{} {month} {}", d.day, d.year)
            }
        }
    }
}

/// The line under a page's title: its number, when it was written and changed, and who gave it.
#[must_use]
pub fn page_line(
    page: &JournalPage,
    count: usize,
    wall: u64,
    offset: impl Fn(i64) -> i32 + Copy,
) -> String {
    let mut line = format!("Page {} of {count}.", page.page_number);
    if page.made != 0 {
        line.push_str(&format!(" Written {}", day_words(page.made, wall, offset)));
        if page.edited != 0 {
            let changed = day_words(page.edited, wall, offset);
            if changed != day_words(page.made, wall, offset) {
                line.push_str(&format!(", changed {changed}"));
            }
        }
        line.push('.');
    }
    if !page.from.is_empty() {
        line.push_str(&format!(" From {}.", page.from));
    }
    line
}

/// A page as text, for the clipboard: its title, tags and place, then its notes.
#[must_use]
pub fn page_text(page: &JournalPage) -> String {
    let mut text = page.name().to_owned();
    let tags: Vec<&str> = page.tags().collect();
    if !tags.is_empty() {
        text.push_str(&format!("\nTags: {}", tags.join(", ")));
    }
    if page.location_set {
        text.push_str(&format!(
            "\nPlace: {}",
            location_text(true, page.ns, page.ew)
        ));
    }
    if !page.notes.trim().is_empty() {
        text.push_str("\n\n");
        text.push_str(page.notes.trim_end());
    }
    text
}

/// One field's edit, for the page the notebook is open to.
/// A place written in the notes, or the part of it on one row: where it is drawn, and the place.
#[derive(Debug, Clone)]
struct PlacePiece {
    rect: Rect,
    at: (f64, f64),
}

/// The places written in `text`, laid out as `laid` from `origin` a line `lh` apart: a piece for
/// each row a place runs over.
fn place_pieces(
    p: &Painter<'_>,
    style: &TextStyle,
    text: &str,
    laid: &[edit::Row],
    (x, top): (f32, f32),
    lh: f32,
) -> Vec<PlacePiece> {
    let chars: Vec<char> = text.chars().collect();
    let piece = |a: usize, b: usize| -> String { chars[a..b].iter().collect() };
    let mut out = Vec::new();
    for (span, at) in find_places(text) {
        for (n, row) in laid.iter().enumerate() {
            let (s, e) = (span.start.max(row.start), span.end.min(row.end));
            if s >= e {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let y = top + lh * n as f32;
            let x0 = x + p.measure(style, &piece(row.start, s));
            let rect = Rect::new(x0, y, p.measure(style, &piece(s, e)), lh);
            out.push(PlacePiece { rect, at });
        }
    }
    out
}

fn set(j: &JournalView, field: JournalField) -> UiRequest {
    UiRequest::Journal(JournalAction::SetField {
        generation: j.generation,
        page: j.current_page,
        field,
    })
}

impl Windows {
    /// The game is told when the notebook is on screen, which loads it, and when it is not.
    pub(super) fn follow_notebook(&mut self, out: &mut Outcome) {
        let shown =
            self.is_open(WindowId::Journal) && self.journal_page() == super::JournalTab::Notebook;
        if shown != self.notebook_shown {
            self.notebook_shown = shown;
            out.requests
                .push(UiRequest::Journal(JournalAction::Visibility(shown)));
        }
    }

    /// Open the Journal at page `number` of the notebook.
    pub(super) fn open_notebook_page(&mut self, number: u32, now: f64, out: &mut Outcome) {
        self.open(WindowId::Journal, now);
        self.show_journal_tab(super::JournalTab::Notebook);
        out.requests
            .push(UiRequest::Journal(JournalAction::Goto(number)));
    }

    /// The notebook: the page list on the left, the open page on the right.
    pub(super) fn notebook(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let j = &state.journal;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        if !j.loaded {
            p.text_in(&dim, body, Align::Centre, "...");
            return;
        }
        let left = Rect::new(body.x, body.y, body.w * 0.4, body.h);
        self.notebook_list(p, ctx, state, left, out);
        let x = left.right() + 14.0 * k;
        let right = Rect::new(x, body.y, body.right() - x, body.h);
        self.notebook_page(p, ctx, state, right, out);
    }

    /// The search, the tags, the column heads and the pages.
    fn notebook_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let j = &state.journal;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 11.0, ctx.colours.dim()).edge(ctx.colours.edge());
        // The search, as the player types, and a new page beside it.
        let new_w = 92.0 * k;
        let search = Rect::new(area.x, area.y, area.w - new_w - 6.0 * k, 26.0 * k);
        kit::text_box(
            p,
            ctx,
            search,
            &mut self.notebook_search,
            &mut self.typing_field,
            field::SEARCH,
        );
        if self.notebook_search.is_empty() && self.typing_field != field::SEARCH {
            p.text_in(
                &dim,
                search.offset(8.0 * k, 0.0),
                Align::Left,
                "Search titles, tags and notes",
            );
        }
        if kit::button(
            p,
            ctx,
            Rect::new(search.right() + 6.0 * k, area.y, new_w, 26.0 * k),
            "New Page",
            true,
        ) {
            out.requests
                .push(UiRequest::Journal(JournalAction::NewPage));
        }
        // The tags, each narrowing the list to its pages.
        let mut y = area.y + 32.0 * k;
        let tags = all_tags(&j.pages);
        if !tags.is_empty() {
            let mut cx = area.x;
            let chip_h = 20.0 * k;
            let all = std::iter::once(None).chain(tags.iter().map(|t| Some(t.as_str())));
            for tag in all {
                let caption = tag.unwrap_or("All");
                let w = p.measure(&dim, caption) + 16.0 * k;
                if cx + w > area.right() {
                    break;
                }
                let chip = Rect::new(cx, y, w, chip_h);
                let on = match (tag, &self.notebook_tag) {
                    (None, None) => true,
                    (Some(t), Some(have)) => t.eq_ignore_ascii_case(have),
                    _ => false,
                };
                let lit = on || ctx.input.hover(&chip);
                p.fill(chip, if on { 0xFF8A_7440 } else { 0xFF5A_5040 });
                if lit && !on {
                    p.fill(chip, 0x30FF_FFFF);
                }
                p.text_in(&dim, chip, Align::Centre, caption);
                if ctx.over(&chip) && ctx.input.clicked(&chip) {
                    self.notebook_tag = tag.map(str::to_owned);
                    self.notebook_scroll = 0.0;
                }
                cx += w + 4.0 * k;
            }
            y += chip_h + 6.0 * k;
        }
        // A tag no page carries any more is let go.
        if self
            .notebook_tag
            .as_ref()
            .is_some_and(|t| !tags.iter().any(|have| have.eq_ignore_ascii_case(t)))
        {
            self.notebook_tag = None;
        }
        // The column heads, each a sort; a second press turns it round.
        let head = Rect::new(area.x, y, area.w, 22.0 * k);
        let timer_w = 70.0 * k;
        let num_w = 30.0 * k;
        let heads = [
            (
                SortBy::Number,
                Rect::new(head.x, head.y, num_w, head.h),
                "#",
            ),
            (
                SortBy::Title,
                Rect::new(head.x + num_w, head.y, head.w - num_w - timer_w, head.h),
                "Title",
            ),
            (
                SortBy::Timer,
                Rect::new(head.right() - timer_w, head.y, timer_w, head.h),
                "Timer",
            ),
        ];
        for (by, r, caption) in heads {
            let on = self.notebook_sort.by == by;
            let style = if on { &name } else { &dim };
            let w = p.measure(style, caption);
            let tx = if by == SortBy::Timer {
                r.right() - w - 18.0 * k
            } else {
                r.x + 8.0 * k
            };
            p.text(style, tx, r.y + 3.0 * k, caption);
            if on {
                let arrow = Rect::new(tx + w + 4.0 * k, r.y + 7.0 * k, 9.0 * k, 7.0 * k);
                if self.notebook_sort.reverse {
                    kit::up_arrow(p, arrow, ctx.colours.text());
                } else {
                    kit::down_arrow(p, arrow, ctx.colours.text());
                }
            }
            if ctx.over(&r) && ctx.input.clicked(&r) {
                self.notebook_sort = if on {
                    Sort {
                        by,
                        reverse: !self.notebook_sort.reverse,
                    }
                } else {
                    Sort { by, reverse: false }
                };
            }
        }
        p.fill(
            Rect::new(head.x, head.bottom(), head.w, k.max(1.0)),
            0x80B3_9555,
        );
        // The pages.
        let list = Rect::new(
            area.x,
            head.bottom() + 2.0 * k,
            area.w,
            area.bottom() - head.bottom() - 2.0 * k,
        );
        p.fill(list, 0x6010_0C08);
        let shown = listed(
            &j.pages,
            &self.notebook_search,
            self.notebook_tag.as_deref(),
            self.notebook_sort,
        );
        if shown.is_empty() {
            p.text_in(&dim, list, Align::Centre, "No pages match.");
        }
        let row_h = 36.0 * k;
        let rows =
            kit::ScrollList::begin(p, ctx, list, shown.len(), row_h, &mut self.notebook_scroll);
        for (i, page) in shown.iter().enumerate() {
            let Some(row) = rows.row(i, row_h - 2.0 * k) else {
                continue;
            };
            let on = page.page_number == j.current_page;
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.text(
                &dim,
                row.x + 8.0 * k,
                row.y + 4.0 * k,
                &page.page_number.to_string(),
            );
            let text_w = row.w - num_w - timer_w - 4.0 * k;
            let title = if page.name().is_empty() {
                "Untitled"
            } else {
                page.name()
            };
            let title = kit::middle_ellipsis(p, &name, title, text_w);
            p.text(&name, row.x + num_w, row.y + 2.0 * k, &title);
            let tags: Vec<&str> = page.tags().collect();
            if !tags.is_empty() {
                let tags = kit::middle_ellipsis(p, &dim, &tags.join(", "), text_w);
                p.text(&dim, row.x + num_w, row.y + 19.0 * k, &tags);
            }
            if let Some((words, colour)) = timer_words(page, state.clock) {
                let style = TextStyle::new(Family::Body, 12.0, colour).edge(ctx.colours.edge());
                let w = p.measure(&style, &words);
                p.text(&style, row.right() - w - 18.0 * k, row.y + 10.0 * k, &words);
            }
            if page.location_set {
                // A small diamond: the page has a place.
                let c = (row.right() - 9.0 * k, row.y + row.h / 2.0);
                for d in 0..4 {
                    #[allow(clippy::cast_precision_loss)]
                    let half = (4 - d) as f32 * k;
                    #[allow(clippy::cast_precision_loss)]
                    let dy = d as f32 * k;
                    p.fill(Rect::new(c.0 - half, c.1 - dy, half * 2.0, k), 0xFFD9_C27A);
                    p.fill(Rect::new(c.0 - half, c.1 + dy, half * 2.0, k), 0xFFD9_C27A);
                }
            }
            if ctx.over(&row) && ctx.input.clicked(&row) {
                out.requests
                    .push(UiRequest::Journal(JournalAction::Goto(page.page_number)));
            }
        }
        rows.end(p, ctx);
    }

    /// The open page: title, the line about it, tags, place, timer and notes, and the buttons
    /// that turn and delete it.
    #[allow(clippy::too_many_lines)]
    fn notebook_page(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let j = &state.journal;
        let name = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let d = &j.draft;
        let page_shown = j.pages.iter().any(|p| p.page_number == j.current_page);
        if !page_shown {
            p.text_in(&dim, area, Align::Centre, "No page is open.");
            return;
        }
        let x = area.x;
        let w = area.w;
        let label_w = 54.0 * k;
        let fx = x + label_w;
        let fw = w - label_w;
        let mut row_y = area.y;
        // The title.
        p.text(&dim, x, row_y + 5.0 * k, "Title");
        let mut title = d.title.clone();
        kit::text_box(
            p,
            ctx,
            Rect::new(fx, row_y, fw, 26.0 * k),
            &mut title,
            &mut self.typing_field,
            field::TITLE,
        );
        if title != d.title {
            out.requests.push(set(j, JournalField::Title(title)));
        }
        row_y += 28.0 * k;
        let offset = pyreal_client_runtime::platform::clock::local_utc_offset_secs;
        p.text(
            &dim,
            fx,
            row_y,
            &page_line(d, j.pages.len(), state.wall, offset),
        );
        row_y += 22.0 * k;
        // The tags, written with commas between.
        p.text(&dim, x, row_y + 5.0 * k, "Tags");
        let mut tags = d.label.clone();
        kit::text_box(
            p,
            ctx,
            Rect::new(fx, row_y, fw, 26.0 * k),
            &mut tags,
            &mut self.typing_field,
            field::TAGS,
        );
        if tags != d.label {
            out.requests.push(set(j, JournalField::Label(tags)));
        }
        if d.label.is_empty() && self.typing_field != field::TAGS {
            p.text_in(
                &dim,
                Rect::new(fx + 8.0 * k, row_y, fw, 26.0 * k),
                Align::Left,
                "Quest, Loot, Timer",
            );
        }
        row_y += 32.0 * k;
        // The place, and under it what can be done with it.
        p.text(&dim, x, row_y + 5.0 * k, "Place");
        let place = location_text(d.location_set, d.ns, d.ew);
        self.notebook_links.clear();
        let page_name = (!d.name().is_empty()).then(|| d.name().to_owned());
        if d.location_set {
            let (r, hit) = link_text(p, ctx, &name, (fx, row_y + 5.0 * k), &place);
            self.notebook_links.push((r, (d.ns, d.ew)));
            if hit {
                self.show_place_on_map((d.ns, d.ew), page_name.clone(), ctx.time);
            }
        } else {
            p.text(&name, fx, row_y + 5.0 * k, &place);
        }
        row_y += 24.0 * k;
        let mut bx = fx;
        let mut place_button = |p: &mut Painter<'_>, ctx: &mut Ctx<'_>, caption: &str| {
            let bw = kit::button_label_width(p, caption).max(96.0 * k);
            let hit = kit::button(p, ctx, Rect::new(bx, row_y, bw, 26.0 * k), caption, true);
            bx += bw + 4.0 * k;
            hit
        };
        if place_button(p, ctx, "Record Here") {
            out.requests
                .push(UiRequest::Journal(JournalAction::StampLocation));
        }
        if d.location_set && place_button(p, ctx, "Show on Map") {
            self.show_on_map(d.ns, d.ew, ctx.time);
        }
        if d.location_set && place_button(p, ctx, "Set Waypoint") {
            let name = [&d.title, &d.label]
                .into_iter()
                .map(|t| t.trim())
                .find(|t| !t.is_empty())
                .map(str::to_owned);
            self.waypoints.set(crate::ui::waypoint::Waypoint {
                ns: d.ns,
                ew: d.ew,
                name,
            });
        }
        row_y += 34.0 * k;
        // The timer: the days, hours and minutes to set, or how long is left.
        p.text(&dim, x, row_y + 5.0 * k, "Timer");
        let start_w = 76.0 * k;
        match timer_words(d, state.clock) {
            Some((words, colour)) => {
                let style = TextStyle::new(Family::Body, 14.0, colour).edge(ctx.colours.edge());
                let words = if words == "Ready" {
                    words
                } else {
                    format!("{words} left")
                };
                p.text(&style, fx, row_y + 4.0 * k, &words);
                let ww = p.measure(&style, &words);
                let reset = Rect::new(fx + ww + 12.0 * k, row_y, start_w, 26.0 * k);
                if kit::button(p, ctx, reset, "Reset", true) {
                    out.requests
                        .push(UiRequest::Journal(JournalAction::ToggleTimer));
                }
                if d.timer_left(state.clock).is_some_and(|l| l > 0.0) {
                    let note = Rect::new(
                        reset.right() + 10.0 * k,
                        row_y,
                        area.right() - reset.right() - 10.0 * k,
                        26.0 * k,
                    );
                    let small = TextStyle::new(Family::Body, 11.0, ctx.colours.dim())
                        .edge(ctx.colours.edge());
                    p.text_in(&small, note, Align::Left, "Chat says when it is ready.");
                }
            }
            None => {
                let mut tx = fx;
                for (value, id, unit, make) in [
                    (
                        d.days,
                        field::DAYS,
                        "days",
                        JournalField::Days as fn(u32) -> JournalField,
                    ),
                    (d.hours, field::HOURS, "hours", JournalField::Hours),
                    (d.minutes, field::MINUTES, "minutes", JournalField::Minutes),
                ] {
                    let r = Rect::new(tx, row_y, 40.0 * k, 26.0 * k);
                    let shown = value.to_string();
                    let mut text = shown.clone();
                    kit::text_box(p, ctx, r, &mut text, &mut self.typing_field, id);
                    if text != shown {
                        out.requests.push(set(j, make(parse_count(&text))));
                    }
                    p.text(&dim, r.right() + 4.0 * k, row_y + 6.0 * k, unit);
                    tx = r.right() + 8.0 * k + p.measure(&dim, unit);
                }
                let start = Rect::new(tx + 4.0 * k, row_y, start_w, 26.0 * k);
                let any = d.days + d.hours + d.minutes > 0;
                if kit::button(p, ctx, start, "Start", any) {
                    out.requests
                        .push(UiRequest::Journal(JournalAction::ToggleTimer));
                }
            }
        }
        row_y += 34.0 * k;
        // The notes, written in place: a click gives them the keyboard.
        let foot = area.bottom() - 28.0 * k;
        let notes = Rect::new(x, row_y, w, foot - 8.0 * k - row_y);
        p.fill(notes, 0x9010_0C08);
        let lh = p.line_height(&name);
        let (notes_x, notes_top, notes_w) =
            (notes.x + 8.0 * k, notes.y + 6.0 * k, notes.w - 16.0 * k);
        // The places written in the notes, links while they are not being written.
        let mut linked = Vec::new();
        if self.typing_field != field::NOTES {
            let laid = edit::rows(p, &name, &d.notes, notes_w);
            linked = place_pieces(p, &name, &d.notes, &laid, (notes_x, notes_top), lh);
            let shown = |r: &Rect| r.y >= notes.y && r.bottom() <= notes.bottom();
            for piece in linked.iter().filter(|l| shown(&l.rect)) {
                self.notebook_links.push((piece.rect, piece.at));
            }
            let hit = linked
                .iter()
                .find(|l| shown(&l.rect) && ctx.over(&l.rect) && ctx.input.clicked(&l.rect));
            if let Some(piece) = hit {
                self.show_place_on_map(piece.at, page_name, ctx.time);
            }
        }
        let pressed_in = ctx.over(&notes) && ctx.input.clicked(&notes);
        if pressed_in {
            self.typing_field = field::NOTES;
        }
        let focused = self.typing_field == field::NOTES;
        let mut text = d.notes.clone();
        if focused {
            ctx.input.text_focus = true;
            edit::begin(ctx.input, edit::line_key(notes, field::NOTES), &text);
            let laid = edit::rows(p, &name, &text, notes_w);
            edit::lines_pointer(
                p,
                ctx.input,
                &name,
                &text,
                &laid,
                (notes_x, notes_top),
                lh,
                pressed_in,
            );
            edit::lines_keys(p, &name, ctx.input, &mut text, usize::MAX, notes_w);
            if ctx.input.take_key(vk::ESCAPE) {
                self.typing_field = 0;
            }
            if text != d.notes {
                out.requests.push(set(j, JournalField::Notes(text.clone())));
            }
        }
        if text.is_empty() && !focused {
            p.text(&dim, notes_x, notes_top, "Write your notes here.");
        }
        let laid = edit::rows(p, &name, &text, notes_w);
        edit::draw_lines(
            p,
            focused.then_some(&ctx.input.edit),
            &name,
            &text,
            &laid,
            (notes_x, notes_top),
            lh,
            notes,
            focused && (ctx.time * 2.0).fract() < 0.5,
        );
        // The places in them underlined, in the notes' own colour.
        p.list.push_clip(notes);
        for r in linked.iter().map(|l| l.rect) {
            let line = Rect::new(r.x, r.bottom() - 2.0 * k, r.w, k.max(1.0));
            p.fill(line, name.colour);
        }
        p.list.pop_clip();
        // Turning and deleting.
        let last = j.pages.iter().map(|p| p.page_number).max().unwrap_or(0);
        let bw = 92.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(x, foot, bw, 28.0 * k),
            "Previous",
            j.current_page > 1,
        ) {
            out.requests
                .push(UiRequest::Journal(JournalAction::Turn(-1)));
        }
        if kit::button(
            p,
            ctx,
            Rect::new(x + bw + 4.0 * k, foot, bw, 28.0 * k),
            "Next",
            j.current_page < last,
        ) {
            out.requests
                .push(UiRequest::Journal(JournalAction::Turn(1)));
        }
        if kit::button(
            p,
            ctx,
            Rect::new(x + 2.0 * (bw + 4.0 * k), foot, bw, 28.0 * k),
            "Copy",
            true,
        ) {
            ctx.input.copied = Some(page_text(d));
        }
        let del_w = 112.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(area.right() - del_w, foot, del_w, 28.0 * k),
            "Delete Page",
            true,
        ) {
            let shown = if d.name().is_empty() {
                format!("page {}", j.current_page)
            } else {
                d.name().to_owned()
            };
            self.delete_asked = Some((j.current_page, shown));
        }
    }

    /// The question before a page is deleted.
    pub(super) fn delete_question(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        out: &mut Outcome,
    ) {
        let Some((page, name)) = self.delete_asked.clone() else {
            return;
        };
        let text = format!("Delete {name} from your journal? This can't be undone.");
        if let Some(b) = crate::ui::pregame::dialog_box(
            p,
            ctx,
            "Delete Page",
            &text,
            &["Delete", "Keep"],
            true,
            0,
        ) {
            self.delete_asked = None;
            if b == 0 {
                out.requests
                    .push(UiRequest::Journal(JournalAction::Delete(page)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(n: u32, title: &str, label: &str, notes: &str) -> JournalPage {
        JournalPage {
            page_number: n,
            title: title.into(),
            label: label.into(),
            notes: notes.into(),
            ..JournalPage::default()
        }
    }

    #[test]
    fn the_list_holds_what_is_searched_and_tagged_in_its_sort() {
        let mut pages = vec![
            page(1, "Gharu'ndim relics", "Quest, Al-Arqas", "Three relics."),
            page(2, "Aerbax's Defeat", "Timer", "Every 20 hours."),
            page(3, "Mana forge keys", "Loot", "Sell at the shop."),
        ];
        pages[1].timer_running = true;
        pages[1].timer_stamp = 50.0;
        let numbers = |shown: Vec<&JournalPage>| -> Vec<u32> {
            shown.iter().map(|p| p.page_number).collect()
        };
        let sort = |by, reverse| Sort { by, reverse };
        assert_eq!(
            numbers(listed(&pages, "", None, Sort::default())),
            [1, 2, 3]
        );
        assert_eq!(numbers(listed(&pages, "RELIC", None, Sort::default())), [1]);
        assert_eq!(numbers(listed(&pages, "hours", None, Sort::default())), [2]);
        assert_eq!(
            numbers(listed(&pages, "", Some("quest"), Sort::default())),
            [1]
        );
        assert_eq!(
            numbers(listed(&pages, "", Some("al-arqas"), Sort::default())),
            [1]
        );
        assert_eq!(
            numbers(listed(&pages, "", None, sort(SortBy::Title, false))),
            [2, 1, 3]
        );
        assert_eq!(
            numbers(listed(&pages, "", None, sort(SortBy::Title, true))),
            [3, 1, 2]
        );
        assert_eq!(
            numbers(listed(&pages, "", None, sort(SortBy::Timer, false))),
            [2, 1, 3],
            "a running timer first"
        );
        assert_eq!(all_tags(&pages), ["Quest", "Al-Arqas", "Timer", "Loot"]);
    }

    #[test]
    fn a_timer_says_how_long_is_left_then_ready() {
        let mut p = page(1, "x", "", "");
        assert_eq!(timer_words(&p, 0.0), None);
        p.timer_running = true;
        p.timer_stamp = 1_000.0 + 3.0 * 3_600.0 + 12.0 * 60.0;
        assert_eq!(timer_words(&p, 1_000.0).map(|w| w.0), Some("3h 12m".into()));
        assert_eq!(timer_words(&p, 1e9).map(|w| w.0), Some("Ready".into()));
    }

    #[test]
    fn a_page_copies_as_its_title_tags_place_and_notes() {
        let mut p = page(1, "Drudges", "Quest, Holtburg", "South of town.\n");
        p.location_set = true;
        p.ns = 42.1;
        p.ew = 33.6;
        assert_eq!(
            page_text(&p),
            "Drudges\nTags: Quest, Holtburg\nPlace: 42.1N, 33.6E\n\nSouth of town."
        );
        assert_eq!(page_text(&page(2, "", "Loot", "")), "Loot\nTags: Loot");
    }

    #[test]
    fn a_page_says_when_it_was_written_and_changed_and_who_gave_it() {
        const DAY: u64 = 86_400;
        let utc = |_: i64| 0;
        let now = 1_763_337_600 + 12 * 3_600; // 17 Nov 2025, noon.
        let mut p = page(2, "x", "", "");
        assert_eq!(page_line(&p, 6, now, utc), "Page 2 of 6.");
        p.made = now - 3 * DAY;
        p.edited = now - 60;
        p.from = "Advocate Tamsin".into();
        assert_eq!(
            page_line(&p, 6, now, utc),
            "Page 2 of 6. Written 14 Nov, changed today. From Advocate Tamsin."
        );
        p.edited = p.made;
        assert_eq!(
            page_line(&p, 6, now, utc),
            "Page 2 of 6. Written 14 Nov. From Advocate Tamsin."
        );
        assert_eq!(day_words(now - DAY, now, utc), "yesterday");
        assert_eq!(day_words(now - 400 * DAY, now, utc), "13 Oct 2024");
    }
}
