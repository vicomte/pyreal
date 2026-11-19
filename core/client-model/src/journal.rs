//! A character's shared notebook. The host performs the requested file operations.

use pyreal_client_contract::journal::{
    HeardTimer, JournalAction, JournalField, JournalIdentity, JournalIo, JournalPage, JournalView,
    QuestTimerView, TIMER_TAG,
};

/// Seconds after the last change before the pages are saved.
pub const SAVE_DELAY: f64 = 2.0;

/// The most quest timers heard in chat that are kept.
const HEARD_KEPT: usize = 20;

#[derive(Debug, Clone, Default)]
pub struct JournalStore {
    state: JournalView,
    identity: Option<JournalIdentity>,
    io: Vec<JournalIo>,
    dirty: bool,
    load_pending: bool,
    /// Pages added while the file was still being read, added once it has been.
    waiting: Vec<JournalPage>,
    /// When the pages are next saved, on the game's clock, after a change.
    save_due: Option<f64>,
    /// The game's clock at the last [`Self::tick`]; a timer that runs out after it is told.
    last_tick: Option<f64>,
    /// The wall clock at the last [`Self::tick`], in Unix seconds, for a page's dates.
    wall: u64,
}

impl JournalStore {
    #[must_use]
    pub fn view(&self) -> JournalView {
        self.state.clone()
    }

    pub fn take_io(&mut self) -> Vec<JournalIo> {
        std::mem::take(&mut self.io)
    }

    /// Count successful host operations, including a save tagged to the previous character.
    pub fn record_io(
        &mut self,
        _identity: &JournalIdentity,
        _generation: u64,
        read: bool,
        success: bool,
    ) {
        if !success {
            return;
        }
        if read {
            self.state.page_loads = self.state.page_loads.wrapping_add(1);
        } else {
            self.state.page_saves = self.state.page_saves.wrapping_add(1);
        }
    }

    fn changed(&mut self) {
        self.state.revision = self.state.revision.wrapping_add(1);
    }

    /// A change to the pages, saved [`SAVE_DELAY`] after the last one.
    fn edited(&mut self, now: f64) {
        self.dirty = true;
        self.save_due = Some(now + SAVE_DELAY);
    }

    fn save(&mut self) {
        if self.load_pending {
            return;
        }
        self.save_due = None;
        if let Some(identity) = &self.identity {
            self.io.push(JournalIo::Save {
                identity: identity.clone(),
                generation: self.state.generation,
                pages: self.state.pages.clone(),
            });
        }
    }

    /// The frame's clocks: saves a change once [`SAVE_DELAY`] has passed, and returns the chat
    /// lines for the timers that ran out since the last tick (pages, the world's quests and those
    /// heard in chat).
    pub fn tick(&mut self, now: f64, wall: u64) -> Vec<String> {
        self.wall = wall;
        if self.save_due.is_some_and(|due| now >= due) {
            self.save();
        }
        let Some(before) = self.last_tick.replace(now) else {
            self.state.since = now;
            return Vec::new();
        };
        if !self.state.loaded || now <= before {
            return Vec::new();
        }
        let ran_out = |at: f64| before < at && at <= now;
        let mut lines: Vec<String> = self
            .state
            .pages
            .iter()
            .filter(|p| p.timer_running && ran_out(p.timer_stamp))
            .map(|p| pyreal_client_contract::journal::timer_ready_line(p.name()))
            .collect();
        lines.extend(
            self.state
                .quests
                .iter()
                .filter(|q| q.ready_at.is_some_and(ran_out))
                .map(|q| pyreal_client_contract::journal::quest_ready_line(&q.title)),
        );
        lines
    }

    /// The world's quest timers, `whole` or changes only, with its clock `sent` and the game's
    /// `now`.
    pub fn set_quest_timers(&mut self, m: &pyreal_protocol::extensions::QuestTimers, now: f64) {
        self.state.quests_known = true;
        if m.whole {
            self.state.quests.clear();
        }
        self.state
            .quests
            .retain(|q| !m.removed.iter().any(|r| r.eq_ignore_ascii_case(&q.name)));
        for t in &m.timers {
            let view = QuestTimerView {
                name: t.name.clone(),
                title: pyreal_client_contract::journal::quest_title(&t.name, &t.message),
                solves: t.solves,
                max_solves: t.max_solves,
                min_delta: t.min_delta,
                ready_at: match t.ready_at {
                    u32::MAX => None,
                    0 => Some(f64::NEG_INFINITY),
                    at => Some(now + (f64::from(at) - f64::from(m.now))),
                },
            };
            match self
                .state
                .quests
                .iter_mut()
                .find(|q| q.name.eq_ignore_ascii_case(&t.name))
            {
                Some(q) => *q = view,
                None => self.state.quests.push(view),
            }
        }
        self.changed();
    }

    /// A chat line from the world: a quest timer when it is one, called `title` (who the player
    /// was dealing with), kept for the Timers tab.
    pub fn hear(&mut self, text: &str, now: f64, title: Option<&str>) {
        let Some(seconds) = pyreal_client_contract::journal::heard_quest_seconds(text) else {
            return;
        };
        let title = title
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map_or_else(|| "Quest timer".to_owned(), |t| format!("Quest from {t}"));
        self.state.heard.retain(|h| h.title != title);
        self.state.heard.push(HeardTimer {
            title,
            ready_at: now + f64::from(seconds),
            seconds,
        });
        let over = self.state.heard.len().saturating_sub(HEARD_KEPT);
        self.state.heard.drain(..over);
        self.changed();
    }

    fn blank_page(number: u32) -> JournalPage {
        JournalPage {
            page_number: number,
            ..Default::default()
        }
    }

    /// A new page numbered `number`, written now.
    fn new_page(&self, number: u32) -> JournalPage {
        JournalPage {
            made: self.wall,
            edited: self.wall,
            ..Self::blank_page(number)
        }
    }

    /// Add `page` after the last, written now, and open it.
    fn push_page(&mut self, mut page: JournalPage, now: f64) {
        self.capture();
        let untouched = self.state.pages.len() == 1 && self.state.pages[0] == Self::blank_page(1);
        if untouched {
            self.state.pages.clear();
        }
        let Some(number) = u32::try_from(self.state.pages.len())
            .ok()
            .and_then(|n| n.checked_add(1))
        else {
            return;
        };
        page.page_number = number;
        if page.made == 0 {
            page.made = self.wall;
        }
        page.edited = self.wall;
        self.state.pages.push(page);
        self.edited(now);
        self.goto(number);
    }

    fn initialize(&mut self) {
        if self.state.pages.is_empty() {
            self.state.pages.push(Self::blank_page(1));
        }
        self.state.loaded = true;
        self.state.current_page = 1;
        self.state.draft = self.state.pages[0].clone();
        self.changed();
    }

    /// A late identity can name an untouched session; changing an established identity clears it.
    pub fn set_identity(&mut self, identity: Option<JournalIdentity>) {
        if !self.state.loaded {
            self.initialize();
        }
        if identity == self.identity {
            return;
        }
        if self.identity.is_some() {
            self.waiting.clear();
            self.apply(JournalAction::Visibility(false), 0.0, None);
            let generation = self.state.generation.wrapping_add(1);
            let revision = self.state.revision.wrapping_add(1);
            self.state = JournalView {
                generation,
                revision,
                page_loads: self.state.page_loads,
                page_saves: self.state.page_saves,
                ..Default::default()
            };
            self.dirty = false;
            self.save_due = None;
            self.last_tick = None;
            self.initialize();
        } else {
            self.state.generation = self.state.generation.wrapping_add(1);
        }
        self.identity = identity;
        self.load_pending = false;
        if let Some(identity) = &self.identity {
            if !self.dirty {
                self.io.push(JournalIo::Load {
                    identity: identity.clone(),
                    generation: self.state.generation,
                    revision: self.state.revision,
                });
                self.load_pending = true;
            }
        }
    }

    /// Returns whether a matching malformed file should produce the load complaint.
    pub fn complete_load(
        &mut self,
        identity: JournalIdentity,
        generation: u64,
        revision: u64,
        pages: Result<Vec<JournalPage>, ()>,
    ) -> bool {
        if self.identity.as_ref() != Some(&identity)
            || self.state.generation != generation
            || !self.load_pending
        {
            return false;
        }
        self.load_pending = false;
        if self.state.revision != revision || self.dirty {
            return false;
        }
        let complaint = pages.is_err();
        self.state.pages = pages.unwrap_or_default();
        self.initialize();
        for page in std::mem::take(&mut self.waiting) {
            self.add_page(page);
        }
        complaint
    }

    /// Add `page` after the last and save. An untouched first page is filled rather than left
    /// blank; a page that comes while the file is being read waits for it.
    pub fn add_page(&mut self, page: JournalPage) {
        if !self.state.loaded || self.load_pending {
            self.waiting.push(page);
            return;
        }
        self.push_page(page, 0.0);
        self.save();
    }

    fn capture(&mut self) {
        if let Some(page) = self
            .state
            .current_page
            .checked_sub(1)
            .and_then(|n| self.state.pages.get_mut(n as usize))
        {
            *page = self.state.draft.clone();
        }
    }

    fn goto(&mut self, number: u32) {
        let Some(page) = number
            .checked_sub(1)
            .and_then(|n| self.state.pages.get(n as usize))
        else {
            return;
        };
        self.state.current_page = number;
        self.state.draft = page.clone();
        self.changed();
    }

    /// A change is saved [`SAVE_DELAY`] after the last one ([`Self::tick`]) and whenever the
    /// notebook is shown or hidden.
    pub fn apply(&mut self, action: JournalAction, now: f64, coords: Option<(f32, f32)>) {
        if !self.state.loaded {
            return;
        }
        match action {
            JournalAction::Edit {
                generation,
                page,
                mut draft,
            } => {
                if generation != self.state.generation || page != self.state.current_page {
                    return;
                }
                draft.page_number = page;
                draft.timer_running = self.state.draft.timer_running;
                draft.timer_stamp = self.state.draft.timer_stamp;
                draft.location_set = self.state.draft.location_set;
                draft.ns = self.state.draft.ns;
                draft.ew = self.state.draft.ew;
                draft.made = self.state.draft.made;
                draft.edited = self.state.draft.edited;
                draft.from = self.state.draft.from.clone();
                if draft == self.state.draft {
                    return;
                }
                draft.edited = self.wall;
                self.state.draft = draft;
                self.edited(now);
                self.capture();
                self.changed();
            }
            JournalAction::SetField {
                generation,
                page,
                field,
            } => {
                if generation != self.state.generation || page != self.state.current_page {
                    return;
                }
                match field {
                    JournalField::Label(s) => self.state.draft.label = s,
                    JournalField::Title(s) => self.state.draft.title = s,
                    JournalField::Notes(s) => self.state.draft.notes = s,
                    JournalField::Days(n) => self.state.draft.days = n,
                    JournalField::Hours(n) => self.state.draft.hours = n,
                    JournalField::Minutes(n) => self.state.draft.minutes = n,
                }
                self.state.draft.edited = self.wall;
                self.edited(now);
                self.capture();
                self.changed();
            }
            JournalAction::Turn(delta) => {
                self.capture();
                self.goto(self.state.current_page.saturating_add_signed(delta));
            }
            JournalAction::Goto(page) => {
                self.capture();
                self.goto(page);
            }
            JournalAction::NewPage => {
                self.capture();
                let Some(number) = u32::try_from(self.state.pages.len())
                    .ok()
                    .and_then(|n| n.checked_add(1))
                else {
                    return;
                };
                let page = self.new_page(number);
                self.state.pages.push(page);
                self.edited(now);
                self.goto(number);
            }
            JournalAction::Delete(page) => {
                if page == 0 || page as usize > self.state.pages.len() {
                    return;
                }
                self.capture();
                if self.state.pages.len() == 1 {
                    self.state.pages[0] = Self::blank_page(1);
                } else {
                    self.state.pages.remove(page as usize - 1);
                    for (i, p) in self.state.pages.iter_mut().enumerate() {
                        p.page_number = u32::try_from(i + 1).unwrap_or(u32::MAX);
                    }
                }
                self.edited(now);
                self.goto(page.min(u32::try_from(self.state.pages.len()).unwrap_or(1)));
            }
            JournalAction::StampLocation => {
                let (ns, ew) = coords.unwrap_or_default();
                self.state.draft.ns = f64::from(ns);
                self.state.draft.ew = f64::from(ew);
                self.state.draft.location_set = coords.is_some();
                self.state.draft.edited = self.wall;
                self.edited(now);
                self.capture();
                self.changed();
            }
            JournalAction::ToggleTimer => {
                let p = &mut self.state.draft;
                p.timer_running = !p.timer_running;
                p.timer_stamp = if p.timer_running {
                    now + f64::from(p.days) * 86_400.0
                        + f64::from(p.hours) * 3_600.0
                        + f64::from(p.minutes) * 60.0
                } else {
                    0.0
                };
                self.edited(now);
                self.capture();
                self.changed();
            }
            JournalAction::Visibility(visible) => {
                self.capture();
                if visible {
                    self.state.pages.sort_by_key(|p| p.page_number);
                }
                self.save();
            }
            JournalAction::TrackTimer { title, seconds } => {
                // No title: the last timer heard in chat.
                let title = if title.trim().is_empty() {
                    self.state
                        .heard
                        .last()
                        .map_or_else(|| "Quest timer".to_owned(), |h| h.title.clone())
                } else {
                    title
                };
                self.capture();
                let found = self.state.pages.iter().position(|p| {
                    p.has_tag(TIMER_TAG) && p.name().eq_ignore_ascii_case(title.trim())
                });
                let number = match found {
                    Some(i) => u32::try_from(i + 1).unwrap_or(u32::MAX),
                    None => {
                        let page = JournalPage {
                            label: TIMER_TAG.to_owned(),
                            title: title.trim().to_owned(),
                            ..JournalPage::default()
                        };
                        self.push_page(page, now);
                        self.state.current_page
                    }
                };
                self.goto(number);
                let minutes = seconds.div_ceil(60);
                let p = &mut self.state.draft;
                p.days = minutes / 1_440;
                p.hours = minutes / 60 % 24;
                p.minutes = minutes % 60;
                p.timer_running = true;
                p.timer_stamp = now + f64::from(seconds);
                p.edited = self.wall;
                self.edited(now);
                self.capture();
                self.changed();
            }
            JournalAction::NewPageAt { title, ns, ew } => {
                let page = JournalPage {
                    title: title.trim().to_owned(),
                    ns,
                    ew,
                    location_set: true,
                    ..JournalPage::default()
                };
                self.push_page(page, now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity(character: &str) -> JournalIdentity {
        JournalIdentity {
            directory: "settings".into(),
            world: "World".into(),
            character: character.into(),
        }
    }

    fn edit(store: &mut JournalStore, title: &str) {
        let view = store.view();
        let mut draft = view.draft;
        draft.title = title.into();
        draft.minutes = 2;
        store.apply(
            JournalAction::Edit {
                generation: view.generation,
                page: view.current_page,
                draft,
            },
            0.0,
            None,
        );
    }

    #[test]
    fn identity_reads_and_edits_keep_their_pages_and_save_boundaries() {
        let mut store = JournalStore::default();
        store.set_identity(None);
        edit(&mut store, "Anonymous draft");
        store.apply(JournalAction::Visibility(false), 0.0, None);
        assert!(store.take_io().is_empty());
        store.set_identity(Some(identity("First")));
        assert!(
            store.take_io().is_empty(),
            "a late identity does not load over an edited anonymous draft"
        );
        store.apply(JournalAction::Visibility(false), 0.0, None);
        let saves = store.take_io();
        assert!(
            matches!(saves.as_slice(), [JournalIo::Save { identity: id, pages, .. }] if id.character == "First" && pages[0].title == "Anonymous draft")
        );

        store.set_identity(Some(identity("Second")));
        let io = store.take_io();
        assert!(
            matches!(io.first(), Some(JournalIo::Save { identity: id, pages, .. }) if id.character == "First" && pages[0].title == "Anonymous draft")
        );
        let Some(JournalIo::Load {
            identity: id,
            generation,
            revision,
        }) = io.last().cloned()
        else {
            panic!("second character load")
        };
        edit(&mut store, "New draft before read");
        assert!(!store.complete_load(
            id.clone(),
            generation,
            revision,
            Ok(vec![JournalPage {
                title: "Old file".into(),
                ..Default::default()
            }])
        ));
        assert_eq!(store.view().draft.title, "New draft before read");
        assert!(
            store.take_io().is_empty(),
            "edits never request disk writes"
        );
        store.apply(JournalAction::ToggleTimer, 100.0, None);
        store.apply(JournalAction::StampLocation, 100.0, Some((12.5, -33.25)));
        let view = store.view();
        assert_eq!(view.draft.timer_stamp, 220.0);
        assert!(view.draft.timer_running && view.draft.location_set);
        let generation = view.generation;
        store.apply(
            JournalAction::SetField {
                generation,
                page: 1,
                field: JournalField::Label("Quest".into()),
            },
            100.0,
            None,
        );
        store.apply(
            JournalAction::SetField {
                generation,
                page: 1,
                field: JournalField::Notes("Two queued field edits".into()),
            },
            100.0,
            None,
        );
        assert_eq!(store.view().draft.label, "Quest");
        assert_eq!(store.view().draft.notes, "Two queued field edits");
        let mut stale_controls = store.view().draft;
        stale_controls.timer_running = false;
        stale_controls.timer_stamp = 0.0;
        stale_controls.location_set = false;
        stale_controls.ns = 0.0;
        stale_controls.ew = 0.0;
        store.apply(
            JournalAction::Edit {
                generation,
                page: 1,
                draft: stale_controls,
            },
            100.0,
            None,
        );
        assert_eq!(store.view().draft.timer_stamp, 220.0);
        assert_eq!(
            (store.view().draft.ns, store.view().draft.ew),
            (12.5, -33.25)
        );
        assert!(store.view().draft.timer_running && store.view().draft.location_set);
        store.apply(JournalAction::NewPage, 100.0, None);
        edit(&mut store, "Second page");
        store.apply(JournalAction::Goto(1), 100.0, None);
        assert_eq!(store.view().draft.title, "New draft before read");
        assert_eq!(store.view().draft.timer_stamp, 220.0);
        assert!(
            store.take_io().is_empty(),
            "page and timer actions capture shared content, not files"
        );
        store.apply(JournalAction::Visibility(false), 100.0, None);
        assert_eq!(store.take_io().len(), 1);
        let unchanged = store.view();
        store.set_identity(Some(identity("Second")));
        assert_eq!(
            store.view(),
            unchanged,
            "an interface change keeps page, draft and timer"
        );
        assert!(store.take_io().is_empty());
        store.apply(JournalAction::Delete(1), 100.0, None);
        assert_eq!(store.view().draft.title, "Second page");
        assert_eq!(store.view().pages[0].page_number, 1);
        store.set_identity(Some(identity("Third")));
        assert_eq!(store.view().draft.title, "");
        store.record_io(&id, generation, false, true);
        store.record_io(&id, generation, true, false);
        assert_eq!((store.view().page_loads, store.view().page_saves), (0, 1));
        assert!(
            !store.complete_load(id, generation, revision, Err(())),
            "a stale malformed load raises no complaint"
        );
        let stale = unchanged;
        store.apply(
            JournalAction::Edit {
                generation: stale.generation,
                page: 1,
                draft: stale.draft,
            },
            0.0,
            None,
        );
        assert_eq!(store.view().draft.title, "");
    }

    #[test]
    fn an_added_page_waits_for_the_file_then_follows_its_pages_and_is_saved() {
        let mut store = JournalStore::default();
        store.set_identity(Some(identity("Reader")));
        let Some(JournalIo::Load {
            identity: id,
            generation,
            revision,
        }) = store.take_io().pop()
        else {
            panic!("a load")
        };
        let note = |title: &str| JournalPage {
            label: "Advocate Tamsin".into(),
            title: title.into(),
            notes: "South of town.".into(),
            ..Default::default()
        };
        store.add_page(note("Drudges"));
        assert!(
            store.take_io().is_empty(),
            "nothing saved before the file is read"
        );
        let old = JournalPage {
            title: "Old file".into(),
            page_number: 1,
            ..Default::default()
        };
        assert!(!store.complete_load(id, generation, revision, Ok(vec![old])));
        let view = store.view();
        assert_eq!(view.pages.len(), 2);
        assert_eq!(view.pages[0].title, "Old file");
        assert_eq!(
            (view.current_page, view.draft.title.as_str()),
            (2, "Drudges")
        );
        assert!(matches!(
            store.take_io().as_slice(),
            [JournalIo::Save { pages, .. }] if pages.len() == 2 && pages[1].notes == "South of town."
        ));
    }

    #[test]
    fn an_added_page_fills_an_untouched_journal_s_first_page() {
        let mut store = JournalStore::default();
        store.set_identity(None);
        store.add_page(JournalPage {
            title: "Drudges".into(),
            ..Default::default()
        });
        let view = store.view();
        assert_eq!(view.pages.len(), 1);
        assert_eq!(
            (view.pages[0].page_number, view.pages[0].title.as_str()),
            (1, "Drudges")
        );
    }

    #[test]
    fn a_change_is_saved_once_it_has_rested_and_a_timer_that_runs_out_is_told() {
        let mut store = JournalStore::default();
        store.set_identity(Some(identity("Saver")));
        let Some(JournalIo::Load {
            identity: id,
            generation,
            revision,
        }) = store.take_io().pop()
        else {
            panic!("a load")
        };
        store.complete_load(id, generation, revision, Ok(Vec::new()));
        assert!(
            store.tick(100.0, 1_700_000_000).is_empty(),
            "the first tick sets the clock"
        );
        edit(&mut store, "Aerbax");
        store.apply(JournalAction::ToggleTimer, 100.0, None);
        assert_eq!(store.view().draft.timer_stamp, 220.0, "two minutes");
        assert!(store.tick(101.0, 1_700_000_001).is_empty());
        assert!(
            store.take_io().is_empty(),
            "not yet: the change has not rested"
        );
        store.tick(100.0 + SAVE_DELAY, 1_700_000_002);
        assert!(
            matches!(store.take_io().as_slice(), [JournalIo::Save { pages, .. }] if pages[0].title == "Aerbax"),
            "saved once it rested"
        );
        assert!(store.tick(219.0, 1_700_000_100).is_empty());
        assert_eq!(
            store.tick(221.0, 1_700_000_102),
            ["Your journal timer Aerbax is ready."]
        );
        assert!(store.tick(300.0, 1_700_000_200).is_empty(), "told once");
    }

    #[test]
    fn a_tracked_timer_starts_on_its_page_and_a_new_page_is_dated() {
        let mut store = JournalStore::default();
        store.set_identity(None);
        store.tick(0.0, 1_700_000_000);
        store.apply(JournalAction::NewPage, 0.0, None);
        let made = store.view().draft;
        assert_eq!((made.made, made.edited), (1_700_000_000, 1_700_000_000));
        store.hear(
            "You may complete this quest again in 1h 30m.",
            10.0,
            Some("Ayn Tayan"),
        );
        store.hear("You have solved this quest too recently!", 10.0, None);
        let heard = store.view().heard;
        assert_eq!(heard.len(), 1);
        assert_eq!(heard[0].title, "Quest from Ayn Tayan");
        assert_eq!(heard[0].ready_at, 10.0 + 5_400.0);
        // An empty title follows the last timer heard.
        store.apply(
            JournalAction::TrackTimer {
                title: String::new(),
                seconds: 5_400,
            },
            20.0,
            None,
        );
        let view = store.view();
        let page = &view.draft;
        assert_eq!(page.title, "Quest from Ayn Tayan");
        assert!(page.has_tag("timer"));
        assert!(page.timer_running && page.timer_stamp == 5_420.0);
        assert_eq!((page.days, page.hours, page.minutes), (0, 1, 30));
        let pages = view.pages.len();
        // Again: the same page, its timer started over.
        store.apply(
            JournalAction::TrackTimer {
                title: "quest from ayn tayan".into(),
                seconds: 60,
            },
            30.0,
            None,
        );
        assert_eq!(store.view().pages.len(), pages);
        assert_eq!(store.view().draft.timer_stamp, 90.0);
        store.apply(
            JournalAction::NewPageAt {
                title: "The well".into(),
                ns: 42.1,
                ew: 33.6,
            },
            30.0,
            None,
        );
        let at = store.view().draft;
        assert!(at.location_set && at.title == "The well" && at.ns == 42.1);
    }

    #[test]
    fn the_world_s_quest_timers_are_put_on_the_game_s_clock() {
        use pyreal_protocol::extensions::{QuestTimer, QuestTimers};
        let mut store = JournalStore::default();
        store.set_identity(None);
        store.tick(1_000.0, 0);
        let timer = |name: &str, ready_at| QuestTimer {
            name: name.into(),
            message: String::new(),
            min_delta: 72_000,
            ready_at,
            ..QuestTimer::default()
        };
        store.set_quest_timers(
            &QuestTimers {
                now: 1_700_000_000,
                whole: true,
                timers: vec![
                    timer("colosseum", 1_700_000_060),
                    timer("drudges", 0),
                    timer("done", u32::MAX),
                ],
                removed: Vec::new(),
            },
            1_000.0,
        );
        let view = store.view();
        assert!(view.quests_known);
        assert_eq!(view.quests[0].title, "Colosseum");
        assert_eq!(view.quests[0].ready_at, Some(1_060.0));
        assert_eq!(view.quests[1].ready_at, Some(f64::NEG_INFINITY));
        assert_eq!(view.quests[2].ready_at, None);
        assert_eq!(
            store.tick(1_061.0, 0),
            ["You may complete Colosseum again."]
        );
        store.set_quest_timers(
            &QuestTimers {
                now: 1_700_000_100,
                whole: false,
                timers: Vec::new(),
                removed: vec!["DRUDGES".into()],
            },
            1_100.0,
        );
        assert_eq!(store.view().quests.len(), 2);
    }
}
