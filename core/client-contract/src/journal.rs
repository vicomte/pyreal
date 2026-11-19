//! Shared journal pages, editing actions, identity and host file requests.
//!
//! The model owns pages, drafts, timer stamps and save boundaries. Both interfaces project
//! [`JournalView`] and forward [`JournalAction`]; the host supplies [`JournalIdentity`] and
//! services tagged [`JournalIo`] requests. Pure captions and file parsing remain in presentation.

/// The stem of the journal file name, at both of the path builder's call sites. Retail's, and
/// also this client's.
///
/// A separate prefix is not needed to keep a retail journal from being clobbered. **That
/// separation is the directory's job**: this client keeps its files in its own settings directory
/// (`Dereth\client` under the roaming application data on Windows, ...) and retail keeps writing to
/// `Documents\Asheron's Call`, so the two never name the same file and a prefix would buy nothing
/// but an ugly name. A journal copied in by the first-run migration is read *and* written in
/// place, which is what carrying it over is for; the copy retail still owns is a different file
/// in a different directory and is not touched.
pub const STEM: &str = "Journal";

/// The three values the journal path needs and this crate cannot reach.
///
/// `directory` is the client's settings directory, `world` is the current world name, and
/// `character` is the local player's singular object name. All three are
/// process globals in the client; here the host hands them over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalIdentity {
    pub directory: std::path::PathBuf,
    pub world: String,
    pub character: String,
}

impl JournalIdentity {
    /// The file this client reads and writes — [`STEM`] in [`Self::directory`].
    #[must_use]
    pub fn client_path(&self) -> std::path::PathBuf {
        create_journal_path(&self.directory, STEM, &self.world, &self.character)
    }
}

/// The journal file path: `sprintf("%s%s-%s-%s.txt", dir, stem, world, character)`, in that
/// argument order.
///
/// The directory is joined rather than concatenated, because the client's directory lookup returns a path with
/// its trailing separator and `Path::join` is that separator spelled portably.
#[must_use]
pub fn create_journal_path(
    directory: &std::path::Path,
    stem: &str,
    world: &str,
    character: &str,
) -> std::path::PathBuf {
    directory.join(format!("{stem}-{world}-{character}.txt"))
}

// The elapsed-time formatter and its four divisors, shared with
// the retired retail UI's `panels::journal`. `pyreal_client_runtime::hud` formats the house-purchase countdown
// with it, which is a number the world half owns; the function is arithmetic and `format!`.

/// One second in the client's timer units, and the four divisors the elapsed-time formatter uses:
/// `0x278D00`, `0x15180`, `0xE10`, `0x3C`.
const MONTH: u64 = 2_592_000;
const DAY: u64 = 86_400;
const HOUR: u64 = 3_600;
const MINUTE: u64 = 60;
/// The client's elapsed-time string.
///
/// `"%dmo "`, `"%dd "`, `"%dh "` and `"%dm "` are each emitted only when their term is non-zero;
/// `"%ds "` is **unconditional**; and the whole buffer then loses its last character, which is
/// always the trailing space. So one minute exactly is `"1m 0s"` and ninety seconds is
/// `"1m 30s"`, while fifty seconds is `"50s"`.
#[must_use]
pub fn delta_time_to_string(seconds: i64) -> String {
    let t = u64::try_from(seconds).unwrap_or(0);
    let (months, rest) = (t / MONTH, t % MONTH);
    let (days, rest) = (rest / DAY, rest % DAY);
    let (hours, rest) = (rest / HOUR, rest % HOUR);
    let (minutes, secs) = (rest / MINUTE, rest % MINUTE);
    let mut s = String::new();
    if months != 0 {
        s.push_str(&format!("{months}mo "));
    }
    if days != 0 {
        s.push_str(&format!("{days}d "));
    }
    if hours != 0 {
        s.push_str(&format!("{hours}h "));
    }
    if minutes != 0 {
        s.push_str(&format!("{minutes}m "));
    }
    s.push_str(&format!("{secs}s "));
    // `while (*p) p++; p[-1] = 0;` — the trailing space of whichever run was written last.
    s.pop();
    s
}

/// The end of the line a world says when a quest was solved too recently: the time it gives
/// follows, then a full stop.
pub const QUEST_AGAIN: &str = "You may complete this quest again in ";

/// The seconds a "You may complete this quest again in 1d 2h 3m 4s." line gives, read from
/// anywhere in `text`; `None` for any other line.
#[must_use]
pub fn heard_quest_seconds(text: &str) -> Option<u32> {
    let at = text.find(QUEST_AGAIN)? + QUEST_AGAIN.len();
    let rest = &text[at..];
    let span = rest.split('.').next()?.trim();
    let mut total: u64 = 0;
    let mut any = false;
    for part in span.split_whitespace() {
        let unit = part.trim_start_matches(|c: char| c.is_ascii_digit());
        let n: u64 = part[..part.len() - unit.len()].parse().ok()?;
        total += n * match unit {
            "d" => 86_400,
            "h" => 3_600,
            "m" => 60,
            "s" => 1,
            _ => return None,
        };
        any = true;
    }
    any.then(|| u32::try_from(total).unwrap_or(u32::MAX))
}

/// A time left in its two largest parts: `6d 4h`, `3h 12m`, `40m 5s`, `50s`.
#[must_use]
pub fn short_time(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = seconds.max(0.0).ceil() as u64;
    let parts = [
        (s / 86_400, "d"),
        (s / 3_600 % 24, "h"),
        (s / 60 % 60, "m"),
        (s % 60, "s"),
    ];
    let first = parts.iter().position(|(n, _)| *n > 0).unwrap_or(3);
    parts[first..]
        .iter()
        .take(2)
        .filter(|(n, _)| *n > 0 || first == 3)
        .map(|(n, u)| format!("{n}{u}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a quest timer is called: the quest's message when it is a short one, else its name with
/// a capital first letter.
#[must_use]
pub fn quest_title(name: &str, message: &str) -> String {
    let message = message.trim();
    if !message.is_empty() && message.chars().count() <= 48 && !message.contains('\n') {
        return message.to_owned();
    }
    let mut chars = name.trim().chars();
    chars
        .next()
        .map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}

/// The chat line when a page's timer runs out.
#[must_use]
pub fn timer_ready_line(name: &str) -> String {
    if name.is_empty() {
        "A journal timer is ready.".to_owned()
    } else {
        format!("Your journal timer {name} is ready.")
    }
}

/// The chat line when a quest's timer runs out.
#[must_use]
pub fn quest_ready_line(title: &str) -> String {
    format!("You may complete {title} again.")
}

/// One journal page.
///
/// The two coordinates are named for what they hold rather than for retail's x/y; see
/// the location formatter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalPage {
    /// The label.
    pub label: String,
    /// The title.
    pub title: String,
    /// The notes.
    pub notes: String,
    /// The page number — **1-based**, and rewritten over the whole vector when a page is deleted
    /// after deletion.
    pub page_number: u32,
    /// The timer stamp — an absolute clock time, not a duration.
    pub timer_stamp: f64,
    /// Days / hours / minutes, as `wcstoul` read them out of the three edit boxes.
    pub days: u32,
    pub hours: u32,
    pub minutes: u32,
    /// The stored y — the **north/south** value.
    pub ns: f64,
    /// The stored x — the **east/west** value.
    pub ew: f64,
    /// Whether the timer is running.
    pub timer_running: bool,
    /// Whether a location is set.
    pub location_set: bool,
    /// When the page was written and last changed, in Unix seconds; 0 when not known.
    pub made: u64,
    pub edited: u64,
    /// Who gave the page, when someone did (an Advocate's note).
    pub from: String,
}

/// The separator between a page's tags in its label.
pub const TAG_SEPARATOR: char = ',';

impl JournalPage {
    /// The page's tags: its label split at commas, trimmed, the empty ones left out.
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.label
            .split(TAG_SEPARATOR)
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// Whether one of the page's tags is `tag`, ignoring case.
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// The name the page goes by: its title, else its label.
    #[must_use]
    pub fn name(&self) -> &str {
        let title = self.title.trim();
        if title.is_empty() {
            self.label.trim()
        } else {
            title
        }
    }

    /// Seconds until the running timer is ready at `now`: `None` when it is not running, and
    /// zero or less once it is ready.
    #[must_use]
    pub fn timer_left(&self, now: f64) -> Option<f64> {
        (self.timer_running && self.timer_stamp > 0.0).then_some(self.timer_stamp - now)
    }
}

/// The tag of a page made from a timer heard in chat or taken from the Timers tab.
pub const TIMER_TAG: &str = "Timer";
/// The tag of a page an Advocate gave.
pub const ADVOCATE_TAG: &str = "Advocate";

/// Shared notebook gestures. Drafts carry their identity generation and page.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalAction {
    Edit {
        generation: u64,
        page: u32,
        draft: JournalPage,
    },
    SetField {
        generation: u64,
        page: u32,
        field: JournalField,
    },
    Goto(u32),
    Turn(i32),
    NewPage,
    Delete(u32),
    StampLocation,
    ToggleTimer,
    Visibility(bool),
    /// A page titled `title` with its timer started for `seconds`: the page of that title tagged
    /// Timer when there is one, else a new one. An empty title is the last timer heard in chat.
    TrackTimer {
        title: String,
        seconds: u32,
    },
    /// A new page titled `title` at a place.
    NewPageAt {
        title: String,
        ns: f64,
        ew: f64,
    },
}

/// A quest's timer as the world reported it ([`JournalView::quests`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct QuestTimerView {
    /// The quest's registry name.
    pub name: String,
    /// What the timer is called: the quest's message when it has a short one, else its name.
    pub title: String,
    pub solves: u32,
    /// `-1` for no limit.
    pub max_solves: i32,
    /// Seconds between solves; 0 for a quest with no timer.
    pub min_delta: u32,
    /// When it may be solved again, on the game's clock; `None` when never.
    pub ready_at: Option<f64>,
}

/// A quest timer heard in a chat line ("You may complete this quest again in ...").
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeardTimer {
    /// What the timer is called: who the player was dealing with, when known.
    pub title: String,
    /// When it may be solved again, on the game's clock.
    pub ready_at: f64,
    /// The seconds the line gave.
    pub seconds: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalView {
    pub pages: Vec<JournalPage>,
    pub draft: JournalPage,
    pub current_page: u32,
    pub loaded: bool,
    pub revision: u64,
    pub generation: u64,
    pub page_loads: u32,
    pub page_saves: u32,
    /// The character's quest timers from a world with the quest-timers extension; empty
    /// otherwise.
    pub quests: Vec<QuestTimerView>,
    /// Whether the world sends quest timers.
    pub quests_known: bool,
    /// Quest timers heard in chat this session, newest last.
    pub heard: Vec<HeardTimer>,
    /// The game's clock when the session's first tick came: a timer that was ready before it was
    /// ready before the player arrived.
    pub since: f64,
}

/// File work is performed by the host, outside the shared notebook.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalIo {
    Load {
        identity: JournalIdentity,
        generation: u64,
        revision: u64,
    },
    Save {
        identity: JournalIdentity,
        generation: u64,
        pages: Vec<JournalPage>,
    },
}

/// One edited control; unrelated draft fields are left intact.
#[derive(Debug, Clone, PartialEq)]
pub enum JournalField {
    Label(String),
    Title(String),
    Notes(String),
    Days(u32),
    Hours(u32),
    Minutes(u32),
}
