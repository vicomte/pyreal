//! The log window: the game's chat windows as its tabs, and the input line.
//!
//! Each tab is one of the game's chat windows (the main one and the four floating ones), showing
//! what that window's filter lets through, by the shared routing rule; a tab can be popped out
//! into a window that looks as the docked one does, moved by dragging its tab, and docked again;
//! its filter is set from its menu. A tab's filter is the game's filter for its window, the same
//! one the Chat page of Settings sets; until the character has set one for a window, the tab has
//! this interface's own (see [`default_filter`]). The tabs' names and the windows' opacities are
//! this interface's settings. The input line
//! speaks to the game's talk-focus destination, chosen from the game's own list of destinations
//! the player may use now; what is typed is the game's chat entry, so its draft, recalled lines,
//! replies and tells are the game's, the same in every interface.

use pyreal_client_contract::chat::entry::{EntryAction, ReplyTarget};
use pyreal_client_contract::chat::interface::{route, window, Routed};
use pyreal_client_contract::UiRequest;
use pyreal_client_model::chat_entry::{
    bare_tell, name_slot, reply_command, seed_name, split_tell, tell_line,
};

use crate::art::Family;
use crate::draw::{Argb, Rect};
use crate::options::{ChatOpacity, TabNames};
use crate::ui::colours::{chat_colour, line_colour};
use crate::ui::game::{BlipKind, ChatLine, ChatWindow, GameState, Relation};
use crate::ui::hud::{chat_links, Link, LinkTarget};
use crate::ui::input::vk;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The side of a log window's corner it is sized by, in layout units.
const GRIP: f32 = 12.0;

/// The side of a log window's top corner it is sized by, in layout units: smaller, as the tabs
/// are held by their corners too.
const TOP_GRIP: f32 = 8.0;

/// The height of the input line's bar, in layout units.
const INPUT_H: f32 = 30.0;

/// How far the destination's chip stands in from the input bar's top line and its foot.
const CHIP_PAD: f32 = 5.0;

/// A corner of a log window, which it is made larger or smaller by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    const fn left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    const fn top(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    /// The caption of log window `slot`'s corner, for a scripted run or a test to find it.
    fn caption(self, slot: usize) -> String {
        let n = slot + 1;
        match self {
            Self::BottomRight => format!("Resize {n}"),
            Self::TopLeft => format!("Resize {n} Top Left"),
            Self::TopRight => format!("Resize {n} Top Right"),
            Self::BottomLeft => format!("Resize {n} Bottom Left"),
        }
    }
}

/// How many chat lines the log keeps.
const CHAT_HISTORY: usize = 400;

/// The longest line the input takes.
const MAX_LINE: usize = 250;

/// The game's chat windows, in the order the log window's tabs list them.
pub const WINDOWS: [u32; 5] = [
    window::MAIN,
    window::FLOATY_1,
    window::FLOATY_2,
    window::FLOATY_3,
    window::FLOATY_4,
];

/// The chat type a talk-focus destination's lines take, for its colour.
#[must_use]
pub fn focus_chat_type(focus: u32) -> u8 {
    match focus {
        2 => 3,
        3 => 19,
        4..=6 => 10,
        7 => 18,
        8 => 27,
        9 => 28,
        10 => 29,
        11 => 30,
        12 | 13 => 32,
        _ => 2,
    }
}

/// The colour a talk-focus destination's label and typed line are drawn in: its chat type's.
#[must_use]
pub fn focus_colour(focus: u32) -> Argb {
    chat_colour(focus_chat_type(focus))
}

/// The destinations the talk-focus menu lists, in the game's menu order, with Olthoi's own
/// channel for an Olthoi player.
#[must_use]
pub fn menu_focuses(is_olthoi: bool) -> Vec<u32> {
    let mut out: Vec<u32> = pyreal_client_model::chat::TALK_FOCUS_MENU_ORDER
        .iter()
        .map(|f| *f as u32)
        .collect();
    if is_olthoi {
        out.push(pyreal_client_model::chat::TalkFocus::Olthoi as u32);
    }
    out
}

/// Whether a destination is one of the game's own chat rooms.
fn is_room_focus(focus: u32) -> bool {
    use pyreal_client_model::chat::TalkFocus as F;
    [
        F::General,
        F::Trade,
        F::Lfg,
        F::Roleplay,
        F::Society,
        F::Olthoi,
    ]
    .iter()
    .any(|f| *f as u32 == focus)
}

/// Whether destination `focus` can be picked now.
#[must_use]
pub fn focus_usable(
    view: &pyreal_client_contract::chat::mainchat::ChatFocusView,
    focus: u32,
) -> bool {
    let i = focus as usize;
    view.enabled.get(i).copied().unwrap_or(false)
        && view.selectable.get(i).copied().unwrap_or(false)
}

/// The destination after `current` that can be picked now, in menu order, wrapping; `None` when
/// there is no other.
#[must_use]
pub fn next_focus(view: &pyreal_client_contract::chat::mainchat::ChatFocusView) -> Option<u32> {
    let order = menu_focuses(view.is_olthoi);
    let at = order.iter().position(|f| *f == view.focus).unwrap_or(0);
    (1..=order.len())
        .map(|step| order[(at + step) % order.len()])
        .find(|f| *f != view.focus && focus_usable(view, *f))
}

/// The talk focus a line's opening command switches to, and the rest of the line: `/cg hello`
/// is General and `hello`. The commands are the game's own channel commands
/// ([`pyreal_client_model::cmd::table`]); a command that is not a channel's, or one with nothing
/// typed after its space yet, switches nothing.
#[must_use]
pub fn focus_command(text: &str) -> Option<(u32, &str)> {
    use pyreal_client_model::chat::TalkFocus as F;
    use pyreal_client_model::cmd::table::{
        CommandHandler as H, INITIALIZE_COMMANDS, TURBINE_CHAT_COMMANDS,
    };
    let rest = text.strip_prefix('/').or_else(|| text.strip_prefix('@'))?;
    let (word, after) = rest.split_once(' ')?;
    let entry = INITIALIZE_COMMANDS
        .iter()
        .chain(TURBINE_CHAT_COMMANDS)
        .find(|e| e.name.eq_ignore_ascii_case(word))?;
    let focus = match entry.handler? {
        H::Say => F::All,
        H::General => F::General,
        H::Trade => F::Trade,
        H::Lfg => F::Lfg,
        H::Roleplay => F::Roleplay,
        H::Society => F::Society,
        H::Olthoi => F::Olthoi,
        H::Allegiance => F::Allegiance,
        H::ChannelShortcut => match word.to_ascii_lowercase().as_str() {
            "f" => F::Fellowship,
            "a" => F::Allegiance,
            "m" => F::Monarch,
            "p" => F::Patron,
            "v" => F::Vassals,
            _ => return None,
        },
        _ => return None,
    };
    Some((focus as u32, after))
}

/// The names Tab offers in a tell: the selected player, players nearby (nearest first),
/// fellowship, allegiance and friends online.
#[must_use]
pub fn tell_names(state: &GameState) -> pyreal_client_contract::chat::entry::NameSources {
    let other = |id: pyreal_primitives::ObjectId| Some(id) != state.player_id;
    let mut nearby: Vec<_> = state
        .targets
        .iter()
        .filter(|b| matches!(b.kind, BlipKind::Player | BlipKind::Fellow) && other(b.id))
        .collect();
    nearby.sort_by(|a, b| (a.dx * a.dx + a.dy * a.dy).total_cmp(&(b.dx * b.dx + b.dy * b.dy)));
    pyreal_client_contract::chat::entry::NameSources {
        me: state.in_world.then(|| state.name.clone()),
        selected: state
            .target
            .as_ref()
            .filter(|t| t.relation == Relation::Player && other(t.id))
            .map(|t| t.name.clone()),
        nearby: nearby.into_iter().map(|b| b.name.clone()).collect(),
        fellowship: state.fellowship.iter().map(|f| f.0.clone()).collect(),
        allegiance: state.allegiance.iter().map(|a| a.1.clone()).collect(),
        friends: state
            .friends
            .iter()
            .filter(|f| f.1)
            .map(|f| f.0.clone())
            .collect(),
    }
}

/// The channel a line being typed goes to, for its label: the one a command it starts with
/// names, else `focus` (the destination's label and colour).
#[must_use]
pub fn channel_label(text: &str, focus: (&str, Argb)) -> (String, Argb) {
    let lower = text.to_ascii_lowercase();
    let command = lower
        .strip_prefix('/')
        .or_else(|| lower.strip_prefix('@'))
        .map(|rest| rest.split_whitespace().next().unwrap_or(""));
    let (label, ty) = match command {
        Some("t" | "tell" | "w" | "whisper" | "send" | "r" | "reply" | "rt" | "retell") => {
            ("Tell", 3)
        }
        Some("f" | "fellowship" | "fellow" | "fellows") => ("Fellowship", 19),
        Some("a" | "allegiance") => ("Allegiance", 18),
        Some("p" | "patron" | "m" | "monarch" | "v" | "vassals" | "c" | "covassals") => {
            ("Allegiance", 10)
        }
        Some("cg" | "general") => ("General", 27),
        Some("ct" | "trade") => ("Trade", 28),
        Some("clfg" | "lfg") => ("LFG", 29),
        Some("e" | "em" | "emote" | "me") => ("Emote", 12),
        Some(_) => return ("Command".to_owned(), 0xFFCC_CCCC),
        None => return (focus.0.to_owned(), focus.1),
    };
    let colour = chat_colour(ty);
    (label.to_owned(), colour)
}

/// How the input line works: Modern holds a tell's person in a chip and keeps it after sending;
/// Classic is the original game's line, where a tell is only the text typed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChatStyle {
    #[default]
    Modern,
    Classic,
}

impl ChatStyle {
    /// Every style, in the order Settings lists them.
    pub const ALL: [Self; 2] = [Self::Modern, Self::Classic];

    /// Its name in Settings.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::Classic => "Classic",
        }
    }

    /// Its word in the settings file.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Modern => "modern",
            Self::Classic => "classic",
        }
    }

    /// The style a settings file's word names.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.word() == word.trim())
    }
}

/// The person a tell line is with, either way: `Alba tells you, "hi"` and `You tell Alba, "hi"`
/// are both Alba's. A tell to a network channel (`name@channel`) is no one's.
#[must_use]
pub fn tell_partner(line: &ChatLine) -> Option<String> {
    if !matches!(line.chat_type, 3 | 4) {
        return None;
    }
    let text = chat_links(&line.text).0;
    let mut rest = text.as_str();
    if let Some((stamp, after)) = rest.split_once(' ') {
        if !stamp.is_empty() && stamp.chars().all(|c| c.is_ascii_digit() || c == ':') {
            rest = after;
        }
    }
    let name = match rest.strip_prefix("You tell ") {
        Some(after) => after.split_once(", \"")?.0,
        None => rest.split_once(" tells you, \"")?.0,
    };
    let name = pyreal_client_model::chat::bare_name(name);
    (!name.is_empty() && !name.contains('@')).then(|| name.to_owned())
}

/// A log line's text with its tags taken out and its links, as [`chat_links`] finds them; in a
/// tell, either way, the person's name is a link too.
fn line_links(line: &ChatLine) -> (String, Vec<Link>) {
    let (text, mut links) = chat_links(&line.text);
    if line.chat_type == 4 || line.chat_type == 3 {
        let span = if let Some(at) = text.find("You tell ") {
            let from = at + "You tell ".len();
            text[from..].find(", \"").map(|len| (from, from + len))
        } else {
            tell_partner(line).and_then(|name| {
                let end = text.find(" tells you, \"")?;
                text[..end]
                    .ends_with(name.as_str())
                    .then(|| (end - name.len(), end))
            })
        };
        if let Some((from, to)) = span {
            let name = &text[from..to];
            let s = text[..from].chars().count();
            let e = s + name.chars().count();
            if !name.contains('@') && !links.iter().any(|(ls, le, _)| *ls < e && s < *le) {
                links.push((s, e, LinkTarget::Name(name.to_owned())));
                links.sort_by_key(|(s, _, _)| *s);
            }
        }
    }
    (text, links)
}

/// Whether a line typed in a tell is a command, which is sent as it is rather than told.
fn is_command(text: &str) -> bool {
    text.starts_with(['/', '@'])
}

/// What a log tab shows: one of the game's chat windows, by its filter.
#[derive(Debug, Clone)]
struct View(ChatWindow);

impl View {
    fn shows(&self, line: &ChatLine) -> bool {
        shows_in(&self.0, line)
    }
}

/// How many of a tab's lines arrived since it was last shown, and whether a tell is among them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct Unread {
    count: usize,
    tell: bool,
}

/// An unread count as a tab shows it.
fn count_word(n: usize) -> String {
    if n > 99 {
        "99+".to_owned()
    } else {
        n.to_string()
    }
}

/// Whether `line` shows in `window`.
#[must_use]
pub fn shows_in(window: &ChatWindow, line: &ChatLine) -> bool {
    route(
        window.id,
        window.filter,
        u32::from(line.chat_type),
        line.window,
    ) == Routed::Accepted
}

/// The mask of the filter group the pages caption `caption`.
fn group(caption: &str) -> u64 {
    pyreal_client_contract::options::sheet::FILTER_GROUPS
        .iter()
        .find(|(c, _)| *c == caption)
        .map_or(0, |(_, mask)| *mask)
}

/// The filter this interface gives a chat window the character keeps no filter for: the main
/// window shows talk on every channel (and the game's own messages) but not combat or magic,
/// the second shows combat and magic, the third allegiance, the fourth fellowship, and the fifth
/// the global channels and society.
#[must_use]
pub fn default_filter(id: u32) -> u64 {
    use pyreal_client_contract::options::sheet::MAIN_WINDOW_DEFAULT_FILTER;
    let society = group("Society");
    match id {
        window::MAIN => {
            (MAIN_WINDOW_DEFAULT_FILTER & !(group("Combat") | group("Magic"))) | society
        }
        window::FLOATY_1 => group("Combat") | group("Magic"),
        window::FLOATY_2 => group("Allegiance"),
        window::FLOATY_3 => group("Fellowship"),
        window::FLOATY_4 => {
            group("General") | group("Trade") | group("LFG") | group("Roleplay") | society
        }
        _ => 0,
    }
}

/// Chat window `id`'s filter: the one the game keeps for the character, else this interface's
/// [`default_filter`]. A kept filter that is the game's own starting filter for that window is
/// one the game wrote back for itself, not one the player chose, and counts as none. The log
/// window's tabs and the Chat page of Settings both show and edit this one filter.
#[must_use]
pub fn window_filter(state: &GameState, id: u32) -> u64 {
    state
        .chat_filters
        .iter()
        .find(|(w, f)| {
            *w == id && *f != pyreal_client_contract::chat::interface::default_filter(id)
        })
        .map_or_else(|| default_filter(id), |(_, f)| *f)
}

/// The menu open over the log window.
#[derive(Debug, Clone, PartialEq)]
enum Menu {
    /// The talk-focus destinations, and in Modern the recent tells.
    Focus,
    /// What can be done with a name clicked in the lines, opened at the pointer.
    /// `at` is where the window the name is in drew it: the docked one before the HUD layout
    /// placed it.
    Name {
        name: String,
        at: (f32, f32),
        docked: bool,
    },
    /// A tab's menu, opened over `at` (its tab bar, or its popped-out window's header): pop it
    /// out or dock it, and its filter. A docked tab's bar is where the docked window drew it,
    /// before the HUD layout placed the window.
    Tab { slot: usize, at: Rect, docked: bool },
}

/// A stretch of the log selected with the pointer, from where the press was to where the pointer
/// went: each end a line, counted from the first line the log ever kept, and a character in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogSelection {
    /// The window it is in, as [`windows_of`] lists them.
    slot: usize,
    anchor: (usize, usize),
    end: (usize, usize),
    /// The press is still being dragged.
    dragging: bool,
}

impl LogSelection {
    /// Its two ends, the earlier first.
    fn ends(&self) -> ((usize, usize), (usize, usize)) {
        if self.anchor <= self.end {
            (self.anchor, self.end)
        } else {
            (self.end, self.anchor)
        }
    }
}

/// One wrapped row of a log window: its colour, its text and links, the line it is from and
/// where in that line it starts.
type Row = (Argb, String, Vec<Link>, usize, usize);

/// The log window's state across frames.
#[derive(Debug, Default)]
pub struct ChatLog {
    lines: Vec<ChatLine>,
    /// The tab shown in the docked window.
    tab: usize,
    /// Each tab's scroll, in lines from the newest: the game's five windows.
    scroll: Vec<usize>,
    /// The newest line each tab has shown, counted from the first line the log kept.
    seen: Vec<usize>,
    /// How the input line works, from this interface's settings.
    pub style: ChatStyle,
    /// In Modern, the person the input line tells, kept after sending until another channel is
    /// chosen.
    tell: Option<String>,
    /// Which tabs show their unread count, from this interface's settings.
    pub unread_shown: [bool; 5],
    /// A recalled line is on its way from the game, which takes its tell or leaves it.
    recalling: bool,
    /// A name filled by Tab is on its way from the game.
    completing: bool,
    /// The tell line Tab last filled, before it became the chip: Tab on the chip with nothing
    /// typed goes on through the same names.
    cycle_from: Option<String>,
    /// The windows popped out of the dock, each a window of its own.
    floating: [kit::WindowState; 5],
    /// The input line, while it is open, and its text as last told to the game.
    typing: Option<String>,
    told: String,
    /// The player's name, for telling their own lines from other people's.
    me: String,
    /// The line was opened by the `/` key, whose own typed `/` is still to come and is dropped.
    slash_typed: bool,
    menu: Option<Menu>,
    /// The menu was opened by a press not yet let go.
    menu_fresh: bool,
    /// Where the docked window is on screen, after the HUD layout has placed it.
    pub dock: Rect,
    /// Where the input line's text was drawn.
    text_area: Rect,
    /// How many lines the log has let go of since it began.
    dropped: usize,
    /// The text selected in a window's lines.
    selection: Option<LogSelection>,
    /// The tabs' names, from this interface's settings.
    pub tab_names: TabNames,
    /// How opaque the windows' grounds are, from this interface's settings.
    pub opacity: ChatOpacity,
    /// Where each tab popped out was last put, in layout units, from this interface's settings;
    /// the main tab's is how far the docked window stands from its own place.
    pub popped_saved: [Option<(f32, f32)>; 5],
    /// Where each popped-out tab stands while it is out, in layout units, once it is moved; the
    /// main tab's is how far the docked window stands from its own place.
    placed: [Option<(f32, f32)>; 5],
    /// Where each popped-out tab's window was drawn last.
    popped_rects: [Option<Rect>; 5],
    /// Where the open menu was drawn last.
    menu_rect: Option<Rect>,
    /// HUD Layout's Reset put the windows back where they start: the places and sizes kept in
    /// the settings are not shown until the layout is saved or put back.
    forgotten: bool,
    /// The places and sizes as they stood when the HUD Layout window opened, to put back.
    layout_from: Option<Places>,
    /// HUD Layout moved, sized or reset a window since it was last saved.
    places_unsaved: bool,
    /// HUD Layout's Save: the places and sizes are to be kept in the settings.
    write_places: bool,
    /// How large the player made each log window, in layout units, from this interface's
    /// settings: the docked window's (by the main tab), then each tab's popped out.
    pub sizes_saved: [Option<(f32, f32)>; 5],
    /// How large each log window is made while this session resizes it, in layout units.
    sizes: [Option<(f32, f32)>; 5],
    /// The log window being resized, the corner it is held by, and how far the pointer is from
    /// that corner.
    resizing: Option<(usize, Corner, (f32, f32))>,
    /// The HUD Layout window is open: the popped-out tabs are moved and sized as its elements.
    pub editing: bool,
    /// Where the docked window would stand with no HUD layout: what the popped-out tabs' column
    /// stands over.
    home: Rect,
    /// Where the docked window's lines are, as it drew them before the HUD layout placed it.
    dock_drawn: Rect,
    /// Where the tabs, the open menu's controls and the web links were drawn this frame, by their
    /// captions (a link's is its address): what a scripted run or a test clicks.
    controls: Vec<(String, Rect)>,
}

/// Where each popped-out tab stands and how large each log window is, as this session moved them.
type Places = ([Option<(f32, f32)>; 5], [Option<(f32, f32)>; 5]);

impl ChatLog {
    /// Take this frame's lines, the game's `@clear`, and what the game did to the entry.
    pub fn take(&mut self, state: &GameState, keep: impl Fn(&ChatLine) -> bool) {
        if state.chat_cleared {
            self.dropped += self.lines.len();
            self.lines.clear();
            self.scroll.iter_mut().for_each(|s| *s = 0);
            self.selection = None;
        }
        self.lines
            .extend(state.new_chat.iter().filter(|l| keep(l)).cloned());
        self.fit_slots();
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
            self.dropped += extra;
        }
        self.me.clone_from(&state.name);
        for update in &state.chat_entry {
            if update.window != window::MAIN {
                continue;
            }
            if update.focus || self.typing.is_some() {
                let mut text = update.text.clone();
                // In Modern a tell line opens as its person's chip and the message; a recalled
                // line that is not a tell goes to the channel.
                if self.style == ChatStyle::Modern {
                    if let Some((name, body)) = split_tell(&text) {
                        self.tell = Some(name);
                        text = body;
                        self.cycle_from = self.completing.then(|| update.text.clone());
                    } else if self.recalling && !text.is_empty() {
                        self.tell = None;
                    }
                }
                self.recalling = false;
                self.completing = false;
                self.typing = Some(text);
                self.told.clone_from(&update.text);
            }
        }
    }

    /// Gives every tab a scroll and a mark of the lines it has shown, a new tab having seen them
    /// all.
    fn fit_slots(&mut self) {
        let n = WINDOWS.len();
        let end = self.dropped + self.lines.len();
        self.scroll.resize(n, 0);
        self.seen.resize(n, end);
    }

    /// The line as the game keeps it: in a tell, the `@tell` line with the person in front.
    fn entry_text(&self, text: &str) -> String {
        match &self.tell {
            Some(name) if !is_command(text) => tell_line(name, text),
            _ => text.to_owned(),
        }
    }

    /// The log holds `lines` and nothing else.
    pub fn replace(&mut self, lines: Vec<ChatLine>) {
        self.dropped += self.lines.len();
        self.selection = None;
        self.lines = lines;
        if self.lines.len() > CHAT_HISTORY {
            let extra = self.lines.len() - CHAT_HISTORY;
            self.lines.drain(..extra);
        }
        self.scroll.iter_mut().for_each(|s| *s = 0);
    }

    /// Open the input line with `text`.
    pub fn open(&mut self, text: String) {
        self.told.clone_from(&text);
        let mut text = text;
        if self.style == ChatStyle::Modern {
            if let Some((name, body)) = split_tell(&text) {
                self.tell = Some(name);
                text = body;
            }
        }
        self.typing = Some(text);
    }

    /// Whether the input line is open.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.typing.is_some()
    }

    /// Where the control captioned `label` was drawn this frame: a docked tab by its name, a
    /// check box or the button of the open tab menu, or a web link in the lines by its address.
    #[must_use]
    pub fn control(&self, label: &str) -> Option<Rect> {
        self.controls
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, r)| *r)
    }

    /// Where popped-out tab `slot`'s window is: its tab bar's top-left, once drawn.
    #[must_use]
    pub fn popped_at(&self, slot: usize) -> Option<(f32, f32)> {
        self.floating
            .get(slot)
            .filter(|w| w.open)
            .and_then(|w| w.pos)
    }

    /// Put every popped-out tab back in its own place in the column over the docked window, the
    /// docked window back in its own place, and every log window back to its first size, until
    /// HUD Layout is saved or put back.
    pub fn reset_places(&mut self) {
        self.placed = [None; 5];
        self.sizes = [None; 5];
        self.forgotten = true;
        self.places_unsaved = true;
    }

    /// The HUD Layout window opened: what it changes can be put back to this.
    pub fn begin_layout(&mut self) {
        self.layout_from = Some((self.placed, self.sizes));
        self.places_unsaved = false;
    }

    /// Whether HUD Layout moved, sized or reset a window since it was last saved.
    #[must_use]
    pub fn places_unsaved(&self) -> bool {
        self.places_unsaved
    }

    /// Keep the places and sizes as they stand: the next frame asks for them to be kept.
    pub fn save_places(&mut self) {
        self.write_places = true;
        self.places_unsaved = false;
        self.layout_from = Some((self.placed, self.sizes));
    }

    /// Put back the places and sizes as they stood when HUD Layout opened or last saved.
    pub fn revert_places(&mut self) {
        if let Some((placed, sizes)) = self.layout_from.take() {
            self.placed = placed;
            self.sizes = sizes;
        }
        self.forgotten = false;
        self.places_unsaved = false;
    }

    /// Where popped-out tab `slot` was last put, as the settings keep it, unless HUD Layout's
    /// Reset is waiting to be saved.
    fn kept_place(&self, slot: usize) -> Option<(f32, f32)> {
        self.popped_saved[slot].filter(|_| !self.forgotten)
    }

    /// How large log window `slot` was last made, as the settings keep it, unless HUD Layout's
    /// Reset is waiting to be saved.
    fn kept_size(&self, slot: usize) -> Option<(f32, f32)> {
        self.sizes_saved[slot].filter(|_| !self.forgotten)
    }

    /// How large log window `slot` is now, in screen pixels at scale `k`: the docked window's
    /// (by the main tab) or a tab's popped out, its tab included.
    #[must_use]
    pub fn window_size(&self, slot: usize, k: f32) -> (f32, f32) {
        self.sizes[slot]
            .or(self.kept_size(slot))
            .map_or_else(|| self.default_size(slot, k), |(w, h)| (w * k, h * k))
    }

    /// How large log window `slot` starts: the docked window as it stands, a popped-out tab as
    /// the docked window's lines and a tab over them.
    fn default_size(&self, slot: usize, k: f32) -> (f32, f32) {
        let h = if slot == 0 { 230.0 } else { 24.0 + 180.0 };
        (self.home.w, h * k)
    }

    /// The smallest a log window may be made, in screen pixels at scale `k`.
    fn smallest(slot: usize, k: f32) -> (f32, f32) {
        (200.0 * k, if slot == 0 { 120.0 } else { 100.0 } * k)
    }

    /// The open menu, as it was last drawn: what is under it does not have the pointer.
    #[must_use]
    pub fn menu_rect(&self) -> Option<Rect> {
        self.menu.as_ref().and(self.menu_rect)
    }

    /// The popped-out tabs' windows as they were last drawn: what is under them does not
    /// have the pointer.
    #[must_use]
    pub fn popped_rects(&self) -> Vec<Rect> {
        (0..5)
            .filter(|i| self.floating[*i].open)
            .filter_map(|i| self.popped_rects[i])
            .collect()
    }

    /// Where tab `slot` stands popped out until it is put elsewhere: in a column over the docked
    /// window at its left edge, the second tab at the top and the fifth just above the docked
    /// window, each a window's height and a small gap apart; in screen pixels at scale `k`.
    #[must_use]
    pub fn popped_default(&self, slot: usize, k: f32) -> (f32, f32) {
        let d = self.home;
        let whole = self.default_size(1, k).1;
        #[allow(clippy::cast_precision_loss)]
        let above = (5 - slot.clamp(1, 4)) as f32;
        (d.x, d.y - above * (whole + 8.0 * k))
    }

    /// The tab the docked window shows, as the log window's tabs number them.
    #[must_use]
    pub fn shown_tab(&self) -> usize {
        self.tab
    }

    /// Open `menu`, from a press not yet let go.
    fn open_menu(&mut self, menu: Menu) {
        self.menu = Some(menu);
        self.menu_fresh = true;
    }

    /// Whether a tab's menu is open, titled Filters.
    #[must_use]
    pub fn filters_open(&self) -> bool {
        matches!(self.menu, Some(Menu::Tab { .. }))
    }

    /// Whether text is selected in the lines, for Control-C to copy.
    #[must_use]
    pub fn has_selection(&self) -> bool {
        self.selection.is_some_and(|s| s.anchor != s.end)
    }

    /// Whether Escape has something to close here.
    #[must_use]
    pub fn wants_escape(&self) -> bool {
        self.typing.is_some() || self.menu.is_some()
    }

    /// The lines kept, oldest first.
    #[must_use]
    pub fn lines(&self) -> &[ChatLine] {
        &self.lines
    }

    /// Which windows are popped out.
    #[must_use]
    pub fn floating(&self) -> [bool; 5] {
        self.floating.map(|w| w.open)
    }

    /// Pop tab `slot` out of the dock (`open`) or dock it again, as its menu's button does. The
    /// main tab stays docked.
    pub fn pop_out(&mut self, slot: usize, open: bool, time: f64) {
        let Some(float) = self.floating.get_mut(slot).filter(|_| slot > 0) else {
            return;
        };
        if float.open != open {
            float.open = open;
            float.opened_at = time;
            self.placed[slot] = None;
        }
    }

    /// One of the game's chat actions, its key pressed: replies, a tell to the selection, a
    /// command begun, the entry opened, and the recalled lines.
    pub fn action(&mut self, action: u32, state: &GameState, out: &mut Outcome) {
        use pyreal_client_contract::actions::{chat_entry as a, ActionId};
        let id = ActionId(action);
        let text = self.typing.clone().unwrap_or_default();
        let entry = |action| UiRequest::ChatEntry {
            window: window::MAIN,
            text: text.clone(),
            action,
        };
        if let Some(target) = ReplyTarget::from_action(id) {
            out.requests.push(entry(EntryAction::Reply {
                target,
                prefix: "@tell ".into(),
            }));
        } else if id == a::TELL_TO_SELECTED {
            if let Some(t) = state
                .target
                .as_ref()
                .filter(|t| Some(t.id) != state.player_id && !t.name.is_empty())
            {
                out.requests.push(entry(EntryAction::StartTell {
                    name: t.name.clone(),
                }));
            }
        } else if id == a::START_COMMAND {
            self.open("/".into());
            // The `/` key also types its own "/", this frame or the next: the line already has it.
            self.slash_typed = true;
            out.requests.push(UiRequest::ChatEntry {
                window: window::MAIN,
                text: "/".into(),
                action: EntryAction::Draft,
            });
        } else if id == a::BEGIN_CHAT_MODE || id == a::TOGGLE_CHAT_ENTRY {
            if self.typing.is_none() {
                self.open(state.chat_draft.clone());
            }
        } else if Some(id) == named("RecallLastMessage") {
            out.requests.push(entry(EntryAction::RecallLast));
        } else if Some(id) == named("PreviousMessage") {
            out.requests.push(entry(EntryAction::Previous));
        } else if Some(id) == named("NextMessage") {
            out.requests.push(entry(EntryAction::Next));
        }
    }

    /// The docked log window, in the HUD layout's default place.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        self.controls.clear();
        if std::mem::take(&mut self.write_places) {
            let value =
                |v: Option<(f32, f32)>| v.map_or_else(String::new, |(x, y)| format!("{x},{y}"));
            for slot in 0..5 {
                let at = self.placed[slot].or(self.kept_place(slot));
                out.settings
                    .push((crate::options::popped_key(slot), value(at)));
            }
            for slot in 0..5 {
                let size = self.sizes[slot].or(self.kept_size(slot));
                out.settings
                    .push((crate::options::size_key(slot), value(size)));
            }
            self.forgotten = false;
        }
        let home_x = 20.0 * k;
        // As wide as it may be without reaching what is centred along the bottom, at its widest
        // (the spell bar's tab keys, or the cross hotbars), and no narrower than it may be made.
        let foot_left = sw / 2.0 - crate::ui::hud::FOOT_REACH * k;
        let home_h = 230.0 * k;
        self.home = Rect::new(
            home_x,
            sh - home_h - 24.0 * k,
            (480.0 * k)
                .min(foot_left - home_x - 12.0 * k)
                .max(Self::smallest(0, k).0),
            home_h,
        );
        // Where it starts, unless a top or left corner moved it, held on the screen.
        let least = Self::smallest(0, k);
        let (dx, dy) = self.placed[0]
            .or(self.kept_place(0))
            .map_or((0.0, 0.0), |(dx, dy)| (dx * k, dy * k));
        let x = (self.home.x + dx).clamp(0.0, (sw - least.0).max(0.0));
        let y = (self.home.y + dy).clamp(0.0, (sh - least.1).max(0.0));
        let (w, h) = self.window_size(0, k);
        let (w, h) = (w.min(sw - x).max(least.0), h.min(sh - y).max(least.1));
        let r = Rect::new(x, y + 24.0 * k, w, h - 24.0 * k);
        // The docked log is a panel of its own for the pad's focus, which the Back button
        // brings to it.
        ctx.input.nav.layer(
            Rect::new(x, y, w, h),
            "Chat",
            crate::ui::nav::PanelKind::Chat,
        );
        let showing = Rect::new(x, y, w, h);
        let windows = windows_of(state);
        if self.style == ChatStyle::Classic {
            self.tell = None;
        }
        self.fit_slots();
        let tab_bar = Rect::new(x, y, w, 24.0 * k);
        let (docked, titles, marks, counted) = self.tab_row(&windows);
        self.resize_grip(p, ctx, 0, showing, out);
        // Active while the pointer is over it or its line is being typed.
        let active = self.typing.is_some() || ctx.input.hover(&showing);
        p.fill(r, ground(self.opacity.get(None, active)));
        let labels: Vec<&str> = counted.iter().map(String::as_str).collect();
        let mut shown = docked.iter().position(|i| *i == self.tab).unwrap_or(0);
        if !labels.is_empty() {
            for (n, tr) in kit::tab_rects_for(p, tab_bar, &labels, shown)
                .into_iter()
                .enumerate()
            {
                self.controls.push((titles[n].clone(), tr));
                // A right click shows the tab as well as opening its menu.
                if ctx.input.right_clicked(&tr) {
                    self.open_menu(Menu::Tab {
                        slot: docked[n],
                        at: tab_bar,
                        docked: true,
                    });
                    if shown != n {
                        shown = n;
                        self.scroll[docked[n]] = 0;
                    }
                }
            }
            if kit::tabs(p, ctx, tab_bar, &labels, &mut shown) {
                self.scroll[docked[shown]] = 0;
            }
            self.tab = docked[shown];
            self.tab_marks(p, tab_bar, &labels, &marks, &docked);
        }
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        // The lines end above the corner the window is sized by.
        let text_area = Rect::new(
            r.x + 8.0 * k,
            r.y + 4.0 * k,
            r.w - 16.0 * k,
            r.h - (INPUT_H + 14.0 + GRIP) * k + 4.0 * k,
        );
        if !docked.is_empty() {
            let tab = self.tab;
            if let Some(view) = self.view(&windows, tab) {
                self.draw_lines(p, ctx, &view, tab, text_area, &body, out);
            }
        }
        // What the docked tab and the popped-out windows show now is seen.
        let end = self.dropped + self.lines.len();
        let tab = self.tab;
        for (slot, seen) in self.seen.iter_mut().enumerate() {
            if slot == tab || self.floating[slot].open {
                *seen = end;
            }
        }
        self.input_line(p, ctx, state, r, &body, out);
        // The pad's focus rests on the input line, where CONFIRM opens it.
        {
            let bar = (INPUT_H + 2.0) * k;
            let line = Rect::new(x, y + h - bar, w, bar);
            let input = &mut *ctx.input;
            input
                .nav
                .note(line, crate::ui::nav::Kind::Text, &input.occluders);
            input.nav.home(line);
        }
        if ctx.over_quiet(&showing) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        self.grip_mark(p, ctx, 0, showing);
        ctx.input.nav.end_layer();
        self.dock = r;
        self.dock_drawn = r;
    }

    /// The docked tabs (the windows not popped out), keeping the tab shown
    /// among them: each tab's slot, name, unread lines and caption.
    fn tab_row(
        &mut self,
        windows: &[ChatWindow],
    ) -> (Vec<usize>, Vec<String>, Vec<Unread>, Vec<String>) {
        let docked: Vec<usize> = (0..windows.len())
            .filter(|i| !self.floating[*i].open)
            .collect();
        if !docked.contains(&self.tab) {
            self.tab = docked.first().copied().unwrap_or(0);
        }
        let titles: Vec<String> = docked.iter().map(|i| self.tab_title(*i)).collect();
        let marks: Vec<Unread> = docked
            .iter()
            .map(|i| {
                if *i == self.tab {
                    return Unread::default();
                }
                let mut unread = self
                    .view(windows, *i)
                    .map(|v| self.unread(&v, *i))
                    .unwrap_or_default();
                if !self.unread_shown.get(*i).copied().unwrap_or(true) {
                    unread.count = 0;
                }
                unread
            })
            .collect();
        // A tab with lines it has not shown carries their count after its name.
        let counted: Vec<String> = titles
            .iter()
            .zip(&marks)
            .map(|(t, m)| {
                if m.count > 0 {
                    format!("{t}  {}", count_word(m.count))
                } else {
                    t.clone()
                }
            })
            .collect();
        (docked, titles, marks, counted)
    }

    /// A tab's name: the game window's tab name.
    fn tab_title(&self, slot: usize) -> String {
        self.tab_names.name(slot).to_owned()
    }

    /// What tab `slot` shows: a game window.
    fn view(&self, windows: &[ChatWindow], slot: usize) -> Option<View> {
        windows.get(slot).cloned().map(View)
    }

    /// The lines tab `slot` shows that came since it was last shown, but the player's own.
    fn unread(&self, view: &View, slot: usize) -> Unread {
        let from = self.seen.get(slot).copied().unwrap_or(0);
        let mut unread = Unread::default();
        for (i, line) in self.lines.iter().enumerate() {
            if self.dropped + i < from
                || crate::ui::colours::is_own_line(line.chat_type, &line.text, &self.me)
                || !view.shows(line)
            {
                continue;
            }
            unread.count += 1;
            unread.tell |= line.chat_type == 3;
        }
        unread
    }

    /// Over the docked tabs as `tabs` drew them: each unread count in a badge, a line in tell
    /// colour under a tab with an unread tell.
    fn tab_marks(
        &mut self,
        p: &mut Painter<'_>,
        bar: Rect,
        labels: &[&str],
        marks: &[Unread],
        docked: &[usize],
    ) {
        let k = p.scale;
        let shown = docked.iter().position(|i| *i == self.tab).unwrap_or(0);
        let pal = *p.art.palette();
        let style = TextStyle::new(Family::Body, 12.0, pal.label).edge(pal.edge);
        let rects = kit::tab_rects_for(p, bar, labels, shown);
        for (n, ((tr, label), mark)) in rects.iter().zip(labels).zip(marks).enumerate() {
            // The count in a round badge over where the tab's name left room for it, in smaller
            // figures.
            if mark.count > 0 && p.fit(&style, label, tr.w - 16.0 * k) == *label {
                let count = count_word(mark.count);
                let small = TextStyle::new(Family::Body, 9.5, 0xFFFF_FFFF).edge(pal.edge);
                let (tw, cw) = (p.measure(&style, label), p.measure(&style, &count));
                let mid = tr.x + (tr.w - tw) / 2.0 + tw - cw / 2.0;
                let d = (15.0 * k).round();
                let w = d.max(p.measure(&small, &count) + 8.0 * k).round();
                let badge = Rect::new(
                    (mid - w / 2.0).round(),
                    (tr.y + (tr.h - d) / 2.0).round(),
                    w,
                    d,
                );
                kit::pill(p, badge, if mark.tell { 0xFF8A_3F8A } else { 0xFF5A_5040 });
                p.text_in(&small, badge, Align::Centre, &count);
                let title = self.tab_title(docked[n]);
                self.controls.push((format!("Unread {title}"), badge));
            }
            if mark.tell {
                p.fill(
                    Rect::new(
                        tr.x + 4.0 * k,
                        tr.bottom() - 2.0 * k,
                        tr.w - 8.0 * k,
                        2.0 * k,
                    ),
                    chat_colour(3),
                );
            }
        }
    }

    /// A log window's corners it is made larger or smaller by: a press on one takes hold, a drag
    /// moves that corner while the opposite one stays put, held between the window's smallest and
    /// the screen, and letting go keeps the size and, from a top or left corner, the place.
    /// `shown` is the window, its tab included.
    fn resize_grip(
        &mut self,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        slot: usize,
        shown: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let grips = corners(shown, k);
        let (mx, my) = ctx.input.mouse;
        if self.resizing.is_none() && ctx.input.pressed[0] {
            if let Some((c, _)) = grips.iter().find(|(_, r)| ctx.input.hover(r)) {
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
                self.menu = None;
                let cx = if c.left() { shown.x } else { shown.right() };
                let cy = if c.top() { shown.y } else { shown.bottom() };
                self.resizing = Some((slot, *c, (cx - mx, cy - my)));
            }
        }
        if let Some((s, c, (ox, oy))) = self.resizing.filter(|(s, _, _)| *s == slot) {
            let least = Self::smallest(s, k);
            let (sw, sh) = p.screen;
            // Where the held corner is to be; the opposite edges stay where they are.
            let (cx, cy) = (mx + ox, my + oy);
            let (right, bottom) = (shown.right(), shown.bottom());
            let (x, w) = if c.left() {
                let w = (right - cx).clamp(least.0, right.max(least.0));
                (right - w, w)
            } else {
                let w = (cx - shown.x).clamp(least.0, (sw - shown.x).max(least.0));
                (shown.x, w)
            };
            let (y, h) = if c.top() {
                let h = (bottom - cy).clamp(least.1, bottom.max(least.1));
                (bottom - h, h)
            } else {
                let h = (cy - shown.y).clamp(least.1, (sh - shown.y).max(least.1));
                (shown.y, h)
            };
            self.sizes[s] = Some((w / k, h / k));
            // The docked window's place is kept as how far it stands from its own.
            let place = if s == 0 {
                ((x - self.home.x) / k, (y - self.home.y) / k)
            } else {
                (x / k, y / k)
            };
            let moves = c.left() || c.top();
            if moves {
                self.placed[s] = Some(place);
            }
            ctx.input.captured = true;
            if !ctx.input.down[0] {
                self.resizing = None;
                out.settings
                    .push((crate::options::size_key(s), format!("{},{}", w / k, h / k)));
                if moves {
                    out.settings.push((
                        crate::options::popped_key(s),
                        format!("{},{}", place.0, place.1),
                    ));
                }
            }
        }
        for (c, r) in &grips {
            self.controls.push((c.caption(slot), *r));
        }
    }

    /// The marks of a log window's corners it is sized by: each a fine bronze angle, lit under
    /// the pointer or while it is held. The foot's corners always show theirs; the top's, over
    /// the tabs, show only lit. `shown` is the window as it shows.
    fn grip_mark(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, slot: usize, shown: Rect) {
        let k = p.scale;
        let t = 2.0 * k;
        for (c, grip) in &corners(shown, k) {
            let lit = ctx.input.hover(grip)
                || self
                    .resizing
                    .is_some_and(|(s, held, _)| s == slot && held == *c);
            if c.top() && !lit {
                continue;
            }
            let colour = if lit { 0xFFE8_D29A } else { 0xA0C8_B27A };
            let (hx, vx) = if c.left() {
                (grip.x + k, grip.x + k)
            } else {
                (grip.x + 3.0 * k, grip.right() - t - k)
            };
            let (hy, vy) = if c.top() {
                (grip.y + k, grip.y + k)
            } else {
                (grip.bottom() - t - k, grip.y + 3.0 * k)
            };
            p.fill(Rect::new(hx, hy, grip.w - 4.0 * k, t), colour);
            p.fill(Rect::new(vx, vy, t, grip.h - 4.0 * k), colour);
        }
    }

    /// The popped-out windows and the menus, over the HUD, with [`Self::dock`] placed.
    pub fn overlays(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) {
        let windows = windows_of(state);
        let body = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let r = self.dock;
        // The open menu is over the popped-out windows: they do not have the pointer under it.
        let occluders = ctx.input.occluders.len();
        if let Some(r) = self.menu_rect() {
            ctx.input.occluders.push(r);
        }
        for (i, window) in windows.iter().enumerate() {
            if self.floating[i].open {
                self.popped(p, ctx, window, i, &body, out);
            }
        }
        ctx.input.occluders.truncate(occluders);
        self.menus(p, ctx, state, &windows, r, out);
        // Control-C (or Control-Insert) copies what is selected in the lines, when no text being
        // edited took the key for its own selection.
        if let Some(sel) = self.selection.filter(|s| s.anchor != s.end) {
            if ctx.input.ctrl && (ctx.input.take_key(0x43) || ctx.input.take_key(0x2D)) {
                if let Some(view) = self.view(&windows, sel.slot) {
                    ctx.input.copied = Some(self.selected_text(&view, sel));
                }
            }
        }
    }

    /// Tab `slot` popped out of the dock: a window as the docked one looks without its input
    /// line, its one tab over its lines, moved by dragging the tab.
    fn popped(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        window: &ChatWindow,
        slot: usize,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let tab_h = 24.0 * k;
        let least = Self::smallest(slot, k);
        let (w, whole_h) = self.window_size(slot, k);
        let (w, whole_h) = (w.min(sw).max(least.0), whole_h.min(sh).max(least.1));
        let h = whole_h - tab_h;
        // Where it was moved to while out, else where it was last put, else its own place in
        // the column over the docked window; kept in layout units, so a new scale keeps it.
        let (mut x, mut y) = self.placed[slot]
            .or(self.kept_place(slot))
            .map_or_else(|| self.popped_default(slot, k), |(x, y)| (x * k, y * k));
        // Held on the screen.
        let hold = |x: f32, y: f32| {
            (
                x.clamp(0.0, (sw - w).max(0.0)),
                y.clamp(0.0, (sh - tab_h - h).max(0.0)),
            )
        };
        (x, y) = hold(x, y);
        let name = self.tab_names.name(slot).to_owned();
        let (mx, my) = ctx.input.mouse;
        let editing = self.editing;
        // Its corners take a press before its tab does.
        if !editing {
            self.resize_grip(p, ctx, slot, Rect::new(x, y, w, whole_h), out);
        }
        let float = &mut self.floating[slot];
        let tab = |p: &Painter<'_>, x: f32, y: f32| {
            kit::tab_rects_for(p, Rect::new(x, y, w, tab_h), &[name.as_str()], 0)[0]
        };
        // Moved by its tab; while the HUD Layout window is open, by anywhere on it, as the HUD's
        // elements are.
        let handle = if editing {
            Rect::new(x, y, w, whole_h)
        } else {
            tab(p, x, y)
        };
        if ctx.input.pressed[0] && ctx.input.hover(&handle) {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            float.grab = Some((mx - x, my - y));
            // Taking hold of a window closes the Filters menu.
            self.menu = None;
        }
        let float = &mut self.floating[slot];
        let mut let_go = false;
        if let Some((gx, gy)) = float.grab {
            if ctx.input.down[0] {
                (x, y) = (mx - gx, my - gy);
                ctx.input.captured = true;
            } else {
                float.grab = None;
                let_go = true;
            }
        }
        (x, y) = hold(x, y);
        float.pos = Some((x, y));
        if float.grab.is_some() || let_go {
            self.placed[slot] = Some((x / k, y / k));
        }
        // Where it was let go is kept for the next time it pops out; while the HUD Layout window
        // is open, once the layout is saved.
        if let_go && editing {
            self.places_unsaved = true;
        } else if let_go {
            out.settings.push((
                crate::options::popped_key(slot),
                format!("{},{}", x / k, y / k),
            ));
        }
        let whole = Rect::new(x, y, w, whole_h);
        // While the HUD Layout window is open the wheel over it makes it larger or smaller, a
        // tenth of its first size a notch, kept once the layout is saved.
        if editing && ctx.input.wheel != 0.0 && ctx.input.hover(&whole) {
            let first = self.default_size(slot, k);
            let step = if ctx.input.wheel > 0.0 { 0.1 } else { -0.1 };
            let size = (
                (w + first.0 * step).clamp(least.0, sw.max(least.0)),
                (whole_h + first.1 * step).clamp(least.1, sh.max(least.1)),
            );
            self.sizes[slot] = Some((size.0 / k, size.1 / k));
            self.places_unsaved = true;
            ctx.input.wheel = 0.0;
        }
        let dragging = self.floating[slot].grab.is_some();
        self.popped_rects[slot] = Some(whole);
        let tab_bar = Rect::new(x, y, w, tab_h);
        let tr = tab(p, x, y);
        let lines = Rect::new(x, y + tab_h, w, h);
        let active = dragging || ctx.input.hover(&whole);
        p.fill(lines, ground(self.opacity.get(Some(slot), active)));
        self.controls.push((name.clone(), tr));
        // While the HUD Layout window is open, what is in it has no pointer.
        let pointer = ctx.input.mouse;
        if editing {
            ctx.input.mouse = (-10_000.0, -10_000.0);
        }
        if ctx.input.right_clicked(&tr) {
            self.open_menu(Menu::Tab {
                slot,
                at: tab_bar,
                docked: false,
            });
        }
        kit::tabs(p, ctx, tab_bar, &[name.as_str()], &mut 0);
        // The lines end above the corner the window is sized by.
        let area = Rect::new(
            x + 8.0 * k,
            lines.y + 4.0 * k,
            w - 16.0 * k,
            h - (8.0 + GRIP) * k,
        );
        self.draw_lines(p, ctx, &View(window.clone()), slot, area, body, out);
        if editing {
            ctx.input.mouse = pointer;
            outline(p, whole, &name);
        }
        if !editing {
            self.grip_mark(p, ctx, slot, whole);
        }
        if ctx.over_quiet(&whole) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
    }

    /// The text of `sel` among the lines `window` shows, its lines on lines of their own.
    fn selected_text(&self, view: &View, sel: LogSelection) -> String {
        let (a, b) = sel.ends();
        let mut out: Vec<String> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let at = self.dropped + i;
            if at < a.0 || at > b.0 || !view.shows(line) {
                continue;
            }
            let chars: Vec<char> = chat_links(&line.text).0.chars().collect();
            let from = if at == a.0 { a.1.min(chars.len()) } else { 0 };
            let to = if at == b.0 {
                b.1.min(chars.len())
            } else {
                chars.len()
            };
            out.push(chars[from..to.max(from)].iter().collect());
        }
        out.join("\n")
    }

    /// The lines `window` shows, wrapped into `area`, newest at the bottom; the wheel scrolls
    /// them; a name in a line is a link that begins a tell to it, and a web address one that asks
    /// to be opened in the browser.
    #[allow(clippy::too_many_arguments)]
    fn draw_lines(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &View,
        slot: usize,
        area: Rect,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let lh = p.line_height(body);
        // The own scrollbar sits inside the window, at its right, and the lines wrap short of it.
        let k = p.scale;
        let area = match kit::scrollbar_width(p) {
            Some(w) => Rect::new(area.x, area.y, area.w - (w + 4.0) * k, area.h),
            None => area,
        };
        let mut lines: Vec<Row> = Vec::new();
        for (i, line) in self.lines.iter().enumerate().filter(|(_, l)| view.shows(l)) {
            let at = self.dropped + i;
            let (text, links) = line_links(line);
            let colour = line_colour(line.chat_type, &text, &self.me);
            // Wrapped a paragraph at a time, so a blank line keeps its place.
            let mut paragraph_at = 0;
            for paragraph in text.split('\n') {
                let chars: Vec<char> = paragraph.chars().collect();
                let mut from = 0;
                for wrapped in p.wrap(body, paragraph, area.w) {
                    // Where this piece starts in the paragraph: the wrap drops only the spaces
                    // between pieces.
                    let piece: Vec<char> = wrapped.chars().collect();
                    while from < chars.len() && !chars[from..].starts_with(&piece) {
                        from += 1;
                    }
                    let start = paragraph_at + from;
                    let len = piece.len();
                    let here: Vec<Link> = links
                        .iter()
                        .filter(|(s, e, _)| *s < start + len && *e > start)
                        .map(|(s, e, n)| {
                            (s.saturating_sub(start), (*e - start).min(len), n.clone())
                        })
                        .collect();
                    lines.push((colour, wrapped, here, at, start));
                    from += len;
                }
                paragraph_at += chars.len() + 1;
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let visible = (area.h / lh).floor().max(1.0) as usize;
        self.fit_slots();
        let scroll = &mut self.scroll[slot];
        if ctx.over_quiet(&area) && ctx.input.wheel != 0.0 {
            let max = lines.len().saturating_sub(visible);
            if ctx.input.wheel > 0.0 {
                *scroll = (*scroll + 3).min(max);
            } else {
                *scroll = scroll.saturating_sub(3);
            }
            ctx.input.wheel = 0.0;
        }
        // The scrollbar: where the lines shown are among all of them; a press or a drag on it
        // moves there.
        let max = lines.len().saturating_sub(visible);
        // The log is one stop for the pad, scrolled while it is entered.
        {
            let input = &mut *ctx.input;
            input
                .nav
                .list_whole(area, *scroll < max, *scroll > 0, &input.occluders);
        }
        let own = max > 0 && kit::scrollbar_width(p).is_some();
        if own {
            let k = p.scale;
            let w = kit::scrollbar_width(p).unwrap_or(16.0) * k;
            let bar = Rect::new(area.right() + 4.0 * k, area.y, w, area.h);
            #[allow(clippy::cast_precision_loss)]
            let (shown, at) = (
                visible as f32 / lines.len() as f32,
                (max - (*scroll).min(max)) as f32 / max as f32,
            );
            match kit::own_scrollbar(p, ctx, bar, shown, at).flatten() {
                // Up is back through the log: the scroll counts lines back from the newest.
                Some(kit::ScrollAct::Step(n)) => {
                    *scroll = if n < 0 {
                        (*scroll + 3).min(max)
                    } else {
                        scroll.saturating_sub(3)
                    };
                }
                Some(kit::ScrollAct::To(t)) => {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let from_top = (t * max as f32).round() as usize;
                    *scroll = max - from_top.min(max);
                }
                None => {}
            }
        } else if max > 0 {
            let k = p.scale;
            let track = Rect::new(area.right() + 2.0 * k, area.y, 4.0 * k, area.h);
            let grab = Rect::new(track.x - 4.0 * k, track.y, track.w + 8.0 * k, track.h);
            if ctx.over_quiet(&grab) && ctx.input.down[0] {
                ctx.input.pressed[0] = false;
                ctx.input.captured = true;
                let t = ((ctx.input.mouse.1 - track.y) / track.h).clamp(0.0, 1.0);
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let from_top = (t * max as f32).round() as usize;
                *scroll = max - from_top.min(max);
            }
            p.fill(track, 0x6000_0000);
            #[allow(clippy::cast_precision_loss)]
            let (thumb_h, at) = (
                (track.h * visible as f32 / lines.len() as f32).max(12.0 * k),
                (max - (*scroll).min(max)) as f32 / max as f32,
            );
            p.fill(
                Rect::new(
                    track.x,
                    track.y + (track.h - thumb_h) * at,
                    track.w,
                    thumb_h,
                ),
                0xC0C8_B27A,
            );
        }
        let end = lines.len().saturating_sub(*scroll);
        let start = end.saturating_sub(visible);
        p.list.push_clip(area);
        #[allow(clippy::cast_precision_loss)]
        let mut ly = area.bottom() - lh * (end - start) as f32;
        let selected = self
            .selection
            .filter(|s| s.slot == slot && s.anchor != s.end)
            .map(|s| s.ends());
        for (colour, text, links, at, from) in &lines[start..end] {
            // The selected part of the row, lit over its text.
            let lit = selected.and_then(|(a, b)| {
                let len = text.chars().count();
                let (s, e) = (a.max((*at, *from)), b.min((*at, from + len)));
                (s < e).then(|| {
                    let chars: Vec<char> = text.chars().collect();
                    let upto = |c: usize| -> String { chars[..c - from].iter().collect() };
                    let x0 = area.x + p.measure(body, &upto(s.1));
                    let x1 = area.x + p.measure(body, &upto(e.1));
                    Rect::new(x0, ly, x1 - x0, lh)
                })
            });
            if links.is_empty() {
                p.text(&body.colour(*colour), area.x, ly, text);
            } else {
                // The line in pieces: plain, a link, plain; a link is lit and can be clicked. A
                // name begins a tell; a web address, underlined, asks to be opened.
                let chars: Vec<char> = text.chars().collect();
                let mut x = area.x;
                let mut at = 0;
                for (s, e, target) in links {
                    let before: String = chars[at.min(chars.len())..(*s).min(chars.len())]
                        .iter()
                        .collect();
                    x += p.text(&body.colour(*colour), x, ly, &before);
                    let shown: String = chars[(*s).min(chars.len())..(*e).min(chars.len())]
                        .iter()
                        .collect();
                    let w = p.measure(body, &shown);
                    let r = Rect::new(x, ly, w, lh);
                    let over = ctx.over(&r);
                    let lit = if over { 0xFFFF_F0B0 } else { 0xFFB0_D8FF };
                    p.text(&body.colour(lit), x, ly, &shown);
                    if let LinkTarget::Web(url) = target {
                        p.fill(Rect::new(x, ly + lh - 2.0 * k, w, k.max(1.0)), lit);
                        self.controls.push((url.clone(), r));
                    }
                    if let LinkTarget::QuestTimer(_) = target {
                        p.fill(Rect::new(x, ly + lh - 2.0 * k, w, k.max(1.0)), lit);
                        self.controls
                            .push((crate::ui::hud::TRACK_LINK.to_owned(), r));
                    }
                    // A right click on a name opens what can be done with it.
                    if let LinkTarget::Name(name) = target {
                        self.controls.push((format!("Name: {name}"), r));
                        if over && ctx.input.right_clicked(&r) {
                            let docked = !self.floating[slot].open;
                            self.open_menu(Menu::Name {
                                name: name.clone(),
                                at: ctx.input.mouse,
                                docked,
                            });
                        }
                    }
                    if over && ctx.input.clicked(&r) {
                        out.requests.push(match target {
                            LinkTarget::Name(name) => UiRequest::ChatEntry {
                                window: window::MAIN,
                                text: self.typing.clone().unwrap_or_default(),
                                action: EntryAction::StartTell { name: name.clone() },
                            },
                            LinkTarget::Web(url) => UiRequest::ChatLinkClicked(url.clone()),
                            LinkTarget::QuestTimer(seconds) => UiRequest::Journal(
                                pyreal_client_contract::journal::JournalAction::TrackTimer {
                                    title: String::new(),
                                    seconds: *seconds,
                                },
                            ),
                        });
                    }
                    x += w;
                    at = *e;
                }
                let after: String = chars[at.min(chars.len())..].iter().collect();
                p.text(&body.colour(*colour), x, ly, &after);
            }
            if let Some(lit) = lit {
                p.fill(lit, crate::ui::edit::SELECTION);
            }
            ly += lh;
        }
        p.list.pop_clip();
        self.select(p, ctx, slot, &lines, start..end, area, body);
    }

    /// The pointer selecting in a window's lines: a press on them (not on a name) starts a
    /// selection, a drag carries it, the lines scrolling when the pointer goes above or below
    /// them, and a press let go where it began selects nothing.
    #[allow(clippy::too_many_arguments)]
    fn select(
        &mut self,
        p: &Painter<'_>,
        ctx: &mut Ctx<'_>,
        slot: usize,
        lines: &[Row],
        shown: std::ops::Range<usize>,
        area: Rect,
        body: &TextStyle,
    ) {
        if shown.is_empty() {
            return;
        }
        let lh = p.line_height(body);
        #[allow(clippy::cast_precision_loss)]
        let top = area.bottom() - lh * shown.len() as f32;
        let (mx, my) = ctx.input.mouse;
        let hit = |mx: f32, my: f32| -> (usize, usize) {
            if my < top {
                return (lines[shown.start].3, lines[shown.start].4);
            }
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let row = (shown.start + ((my - top) / lh) as usize).min(shown.end - 1);
            let (_, text, _, at, from) = &lines[row];
            let chars: Vec<char> = text.chars().collect();
            let col = if my > area.bottom() {
                chars.len()
            } else {
                crate::ui::edit::char_at(p, body, &chars, mx - area.x)
            };
            (*at, from + col)
        };
        if ctx.input.pressed[0] && ctx.over_quiet(&area) {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
            let here = hit(mx, my);
            self.selection = Some(LogSelection {
                slot,
                anchor: here,
                end: here,
                dragging: true,
            });
            return;
        }
        let Some(sel) = self
            .selection
            .as_mut()
            .filter(|s| s.slot == slot && s.dragging)
        else {
            return;
        };
        if ctx.input.down[0] {
            sel.end = hit(mx, my);
            let max = lines.len().saturating_sub(shown.len());
            let scroll = &mut self.scroll[slot];
            if my < area.y {
                *scroll = (*scroll + 1).min(max);
            } else if my > area.bottom() {
                *scroll = scroll.saturating_sub(1);
            }
        } else {
            sel.dragging = false;
            if sel.anchor == sel.end {
                self.selection = None;
            }
        }
    }

    /// The input line: Enter opens it with the game's draft, Enter sends to the destination,
    /// Escape closes it and the game keeps the draft, Up and Down walk the lines sent, Tab steps
    /// to the next destination, and a click on the destination's label lists them. In Modern a
    /// tell's person stands in a chip before the message and stays after sending; Tab steps
    /// through recent tell partners, and Backspace on an empty message turns the chip back into
    /// the `/t` line.
    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn input_line(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        body: &TextStyle,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let input = Rect::new(
            r.x + 4.0 * k,
            r.bottom() - (INPUT_H + 4.0) * k,
            r.w - 8.0 * k,
            INPUT_H * k,
        );
        // Under the bar's top line, where the chip stands with as much room above as below and
        // the label and the typed text share one line, their letters centred.
        let inner = Rect::new(input.x, input.y + k, input.w, input.h - k);
        let chip_box = Rect::new(
            inner.x,
            inner.y + CHIP_PAD * k,
            inner.w,
            inner.h - 2.0 * CHIP_PAD * k,
        );
        let modern = self.style == ChatStyle::Modern;
        let Some(mut text) = self.typing.take() else {
            self.closed_line(p, ctx, state, input, chip_box, body);
            return;
        };
        ctx.input.text_focus = true;
        if self.slash_typed {
            if let Some(i) = ctx.input.chars.iter().position(|c| *c == '/') {
                ctx.input.chars.remove(i);
                self.slash_typed = false;
            } else if !ctx.input.chars.is_empty() {
                self.slash_typed = false;
            }
        }
        let line_key = crate::ui::edit::line_key(input, 0x0C4A_7000);
        crate::ui::edit::begin(ctx.input, line_key, &text);
        let pressed_in = ctx.input.pressed[0] && ctx.input.hover(&self.text_area);
        if pressed_in {
            ctx.input.pressed[0] = false;
            ctx.input.captured = true;
        }
        let double = pressed_in && ctx.input.double;
        // The pointer on the text as it was drawn last frame, after the label.
        crate::ui::edit::pointer(
            p,
            ctx.input,
            body,
            &text,
            self.text_area,
            pressed_in,
            double,
        );
        // Backspace on an empty tell turns its chip back into the line that made it.
        if modern && text.is_empty() && ctx.input.chars.is_empty() {
            if let Some(name) = self.tell.clone() {
                if ctx.input.take_key(vk::BACK) {
                    self.tell = None;
                    text = format!("/t {name}");
                    // The caret goes to the end of it.
                    crate::ui::edit::begin(ctx.input, line_key, &text);
                }
            }
        }
        if !ctx.input.chars.is_empty() {
            self.recalling = false;
            self.completing = false;
        }
        crate::ui::edit::keys(ctx.input, &mut text, MAX_LINE);
        if !text.is_empty() {
            self.cycle_from = None;
        }
        let entry = |text: &str, action| UiRequest::ChatEntry {
            window: window::MAIN,
            text: text.to_owned(),
            action,
        };
        // A channel's command at the start of the line switches the talk focus to it and is
        // taken off the line: the focus stays until another is chosen, as the line is retyped.
        if let Some((focus, rest)) = focus_command(&text) {
            if focus_usable(&state.chat_focus, focus) {
                out.requests.push(UiRequest::SetTalkFocus { focus });
                text = rest.to_owned();
                self.tell = None;
            }
        }
        // In Modern a tell's name ended by its comma, or a reply command's space, becomes the
        // chip, and the line keeps only the message.
        if modern {
            if let Some((name, message)) = split_tell(&text) {
                self.tell = Some(name);
                text = message;
            } else if let Some((seed, rest)) = reply_command(&text) {
                if let Some(name) = seed_name(seed, &state.reply_targets) {
                    self.tell = Some(name.to_owned());
                    text = rest.to_owned();
                }
            }
        }
        if ctx.input.take_key(vk::TAB) {
            let back = ctx.input.shift;
            let names = || EntryAction::Complete {
                names: tell_names(state),
                back,
            };
            // In a tell's name Tab fills and cycles names, and goes on cycling them once the name
            // became the chip; on the chip it steps through recent tell partners; elsewhere it
            // changes the channel.
            if let (Some(line), true) = (self.cycle_from.clone(), text.is_empty()) {
                self.completing = true;
                out.requests.push(entry(&line, names()));
            } else if name_slot(&text).is_some() {
                self.completing = true;
                out.requests.push(entry(&text, names()));
            } else if let (true, Some(name)) = (modern, self.tell.as_deref()) {
                if let Some(next) = next_partner(&state.reply_targets.recent, name, back) {
                    self.tell = Some(next);
                }
            } else if let Some(focus) = next_focus(&state.chat_focus) {
                out.requests.push(UiRequest::SetTalkFocus { focus });
            }
        }
        if ctx.input.take_key(vk::UP) {
            self.recalling = true;
            out.requests
                .push(entry(&self.entry_text(&text), EntryAction::Previous));
        }
        if ctx.input.take_key(vk::DOWN) {
            self.recalling = true;
            out.requests
                .push(entry(&self.entry_text(&text), EntryAction::Next));
        }
        let send = ctx.input.take_key(vk::ENTER);
        // Escape, or a press anywhere off the log window, lets the keyboard go back to the game;
        // the line is kept for the next opening, and in Modern so is the tell.
        let (mx, my) = ctx.input.mouse;
        let pressed_away = (ctx.input.pressed[0] || ctx.input.pressed[1]) && !r.contains(mx, my);
        // An open menu takes Escape first.
        let cancel = (self.menu.is_none() && ctx.input.take_key(vk::ESCAPE)) || pressed_away;
        p.fill(input, 0xE020_1C14);
        p.fill(Rect::new(input.x, input.y, input.w, 1.0 * k), 0xFFC8_B27A);
        let (label_rect, colour) = self.destination(p, ctx, state, &text, input, chip_box, 1.0);
        let tx = label_rect.right() + 4.0 * k;
        self.text_area = Rect::new(
            tx,
            input.y,
            (input.right() - 6.0 * k - tx).max(0.0),
            input.h,
        );
        crate::ui::edit::begin(ctx.input, line_key, &text);
        let blink = (ctx.time * 2.0).fract() < 0.5;
        // The typed text takes the colour of the channel it goes to, as the label does.
        let typed = body.colour(colour);
        let top = p.text_top(&typed, chip_box.y, chip_box.h);
        crate::ui::edit::draw(
            p,
            &mut ctx.input.edit,
            &typed,
            &text,
            self.text_area,
            top,
            top,
            blink,
        );
        if send {
            // A channel's command sent alone (`/cg`) chooses that channel and sends nothing.
            let alone = focus_command(&format!("{} ", text.trim()))
                .filter(|(_, rest)| rest.is_empty())
                .map(|(focus, _)| focus);
            if let Some(focus) = alone {
                if focus_usable(&state.chat_focus, focus) {
                    out.requests.push(UiRequest::SetTalkFocus { focus });
                    self.tell = None;
                }
                // Nothing was said, so the game keeps no draft of the command.
                out.requests.push(entry("", EntryAction::Draft));
            } else if let Some(name) = bare_tell(&text).filter(|_| modern) {
                // A tell sent with only its name turns the line to that person and keeps it open.
                self.tell = Some(name.to_owned());
                let told = self.entry_text("");
                out.requests.push(entry(&told, EntryAction::Draft));
                self.told = told;
                self.typing = Some(String::new());
                return;
            } else if !text.trim().is_empty() {
                out.requests.push(UiRequest::ChatLine {
                    text: self.entry_text(&text),
                    window: window::MAIN,
                });
            }
            self.told.clear();
            self.scroll[0] = 0;
            return;
        }
        let told = self.entry_text(&text);
        if told != self.told {
            out.requests.push(entry(&told, EntryAction::Draft));
            self.told = told;
        }
        if cancel {
            // The game keeps the line, and the next opening begins with it.
            return;
        }
        self.typing = Some(text);
    }

    /// Where the line goes, at the left of the bar `input`: the tell's person, or the channel,
    /// in a chip on `row` in Modern; a command being typed is named plainly. A click on it opens
    /// the list of destinations. Drawn at `alpha`; where it was drawn, and its colour.
    #[allow(clippy::too_many_arguments)]
    fn destination(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        text: &str,
        input: Rect,
        row: Rect,
        alpha: f32,
    ) -> (Rect, Argb) {
        let k = p.scale;
        let modern = self.style == ChatStyle::Modern;
        let focus_label = destination_label(state, state.chat_focus.focus, modern);
        let (label, colour) = match &self.tell {
            Some(name) if modern && !is_command(text) => (format!("Tell: {name}"), chat_colour(3)),
            _ => channel_label(text, (&focus_label, focus_colour(state.chat_focus.focus))),
        };
        let shade = crate::draw::with_alpha(colour, alpha);
        let channel = TextStyle::new(Family::Body, 12.0, shade).edge(0xFF00_0000);
        let label_rect = if modern && !is_command(text) {
            draw_chip(p, &channel, row, &label, shade)
        } else {
            let top = p.text_top(&channel, row.y, row.h);
            let cw = p.text(&channel, input.x + 6.0 * k, top, &label);
            Rect::new(input.x, input.y, cw + 12.0 * k, input.h)
        };
        self.controls
            .push(("Chat Destination".to_owned(), label_rect));
        if ctx.over(&label_rect) && ctx.input.clicked(&label_rect) {
            self.open_menu(Menu::Focus);
        }
        (label_rect, colour)
    }

    /// The input line while it is closed: its bar, dimmed, with where the line will go and the
    /// draft the game keeps. A click on the bar opens the line, and on the destination its list.
    fn closed_line(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        input: Rect,
        row: Rect,
        body: &TextStyle,
    ) {
        let k = p.scale;
        p.fill(input, 0xE020_1C14);
        p.fill(Rect::new(input.x, input.y, input.w, k), 0x80C8_B27A);
        // In Modern a tell's draft shows as its message, its person in the chip.
        let draft = match split_tell(&state.chat_draft) {
            Some((_, message)) if self.style == ChatStyle::Modern => message,
            _ => state.chat_draft.clone(),
        };
        let (label_rect, colour) = self.destination(p, ctx, state, &draft, input, row, 0.6);
        let tx = label_rect.right() + 4.0 * k;
        let area = Rect::new(
            tx,
            input.y,
            (input.right() - 6.0 * k - tx).max(0.0),
            input.h,
        );
        let typed = body.colour(crate::draw::with_alpha(colour, 0.6));
        let shown = p.fit(&typed, &draft, area.w);
        let top = p.text_top(&typed, row.y, row.h);
        p.text(&typed, area.x.round(), top, &shown);
        self.controls.push(("Chat Line".to_owned(), input));
        if ctx.over(&input) && ctx.input.clicked(&input) {
            self.open(state.chat_draft.clone());
        }
    }

    /// A tab's menu, titled Filters, with a button beside the title that pops the tab out or
    /// docks it (not the main tab's, which stays docked), over its window's filter groups in columns of check boxes: two, or three when
    /// two would not fit above `at`. It opens above `at`, its left edge on `at`'s, or below it
    /// where there is no room above. The menu's rect, and whether the button was pressed.
    #[allow(clippy::too_many_arguments)]
    fn filter_menu(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        w: &ChatWindow,
        slot: usize,
        at: Rect,
        era: Option<pyreal_primitives::EraFeatures>,
        out: &mut Outcome,
    ) -> (Rect, bool) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let text = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let heading =
            TextStyle::new(Family::Heading, 18.4, ctx.colours.heading()).edge(ctx.colours.edge());
        let groups = filter_groups(w.id, era.as_ref());
        let (left, top, right, bottom) = kit::panel_inset(p);
        let row = 24.0 * k;
        let title_h = kit::TITLE_BAR * k;
        let button_w = 110.0 * k;
        let gap = 8.0 * k;
        // A column as wide as its longest caption beside its box, and a gap.
        let column = groups
            .iter()
            .map(|(caption, _)| p.measure(&text, caption))
            .fold(0.0_f32, f32::max)
            + 28.0 * k
            + 16.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let size = |columns: usize| {
            let rows = groups.len().div_ceil(columns);
            (
                left + right + 2.0 * gap + column * columns as f32,
                top + title_h + row * rows as f32 + gap + bottom,
            )
        };
        let columns = if size(2).1 + 2.0 * k > at.y { 3 } else { 2 };
        let rows = groups.len().div_ceil(columns);
        let size = size(columns);
        let x = at.x.clamp(0.0, (sw - size.0).max(0.0));
        let above = at.y - size.1 - 2.0 * k;
        let y = if above >= 0.0 {
            above
        } else {
            (at.bottom() + 2.0 * k).min(sh - size.1).max(0.0)
        };
        let area = Rect::new(x, y, size.0, size.1);
        kit::panel(p, area, 0.95);
        let inner = Rect::new(
            area.x + left + gap,
            area.y + top,
            area.w - left - right - 2.0 * gap,
            area.h - top - bottom,
        );
        p.text_in(
            &heading,
            Rect::new(inner.x, inner.y, inner.w - button_w, title_h),
            Align::Left,
            "Filters",
        );
        // Down the first column, then the next.
        for (n, (caption, mask)) in groups.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let rr = Rect::new(
                inner.x + column * (n / rows) as f32,
                inner.y + title_h + row * (n % rows) as f32,
                column,
                row,
            );
            let on = w.filter & mask == *mask;
            self.controls
                .push(((*caption).to_owned(), Rect::new(rr.x, rr.y, 20.0 * k, rr.h)));
            if let Some(now) = kit::check_row(p, ctx, rr, on, &text, caption) {
                let filter = if now {
                    w.filter | mask
                } else {
                    w.filter & !mask
                };
                out.requests.push(UiRequest::SetChatWindowFilter {
                    window: w.id,
                    mask: filter,
                });
            }
        }
        // The main tab stays docked.
        if slot == 0 {
            return (area, false);
        }
        let float = &mut self.floating[slot];
        let label = if float.open { "Dock" } else { "Undock" };
        let button = Rect::new(
            inner.right() - button_w,
            inner.y + 2.0 * k,
            button_w,
            title_h - 4.0 * k,
        );
        self.controls.push((label.to_owned(), button));
        let pressed = kit::button(p, ctx, button, label, true);
        if pressed {
            float.open = !float.open;
            float.opened_at = ctx.time;
            self.placed[slot] = None;
        }
        (area, pressed)
    }

    /// The menus over the log window: the destinations, or a tab's.
    fn menus(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        windows: &[ChatWindow],
        r: Rect,
        out: &mut Outcome,
    ) {
        let Some(menu) = self.menu.clone() else {
            return;
        };
        let k = p.scale;
        let row = 22.0 * k;
        let text = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let dim = text.colour(0xFF80_8080);
        let mut chosen = false;
        let area = match menu {
            Menu::Focus => {
                let modern = self.style == ChatStyle::Modern;
                let rooms = state.era.is_none_or(|e| e.global_chat_rooms);
                let mut focuses = menu_focuses(state.chat_focus.is_olthoi);
                // A world without the game's own rooms lists only one the server named.
                focuses.retain(|f| {
                    rooms
                        || !is_room_focus(*f)
                        || state.chat_focus.enabled.get(*f as usize).copied() == Some(true)
                });
                let mut items: Vec<Item> = focuses.into_iter().map(Item::Focus).collect();
                // In Modern the recent tells follow the channels, then a row that starts a tell.
                if modern {
                    let recent: Vec<String> = state
                        .reply_targets
                        .recent
                        .iter()
                        .map(|n| pyreal_client_model::chat::bare_name(n).to_owned())
                        .filter(|n| !n.is_empty())
                        .take(8)
                        .collect();
                    if !recent.is_empty() {
                        items.push(Item::Heading("Recent Tells"));
                        items.extend(recent.into_iter().map(Item::Person));
                    }
                    items.push(Item::Someone);
                }
                #[allow(clippy::cast_precision_loss)]
                let area = Rect::new(
                    r.x + 4.0 * k,
                    r.bottom() - 30.0 * k - row * items.len() as f32 - 20.0 * k,
                    200.0 * k,
                    row * items.len() as f32 + 20.0 * k,
                );
                kit::panel(p, area, 0.95);
                for (n, item) in items.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let rr = Rect::new(area.x, area.y + 10.0 * k + row * n as f32, area.w, row);
                    let (label, usable, colour, current) = match item {
                        Item::Focus(focus) => {
                            let label = if modern {
                                destination_label(state, *focus, true)
                            } else {
                                let mut label = focus_label(state, *focus);
                                if *focus == 2 {
                                    if let Some(t) = &state.chat_focus.target {
                                        label = format!("{label}: {}", t.name);
                                    }
                                }
                                label
                            };
                            let current = self.tell.is_none() && *focus == state.chat_focus.focus;
                            (
                                label,
                                focus_usable(&state.chat_focus, *focus),
                                focus_colour(*focus),
                                current,
                            )
                        }
                        Item::Heading(h) => ((*h).to_owned(), false, 0xFF80_8080, false),
                        Item::Person(name) => {
                            let current = self
                                .tell
                                .as_deref()
                                .is_some_and(|t| t.eq_ignore_ascii_case(name));
                            (name.clone(), true, chat_colour(3), current)
                        }
                        Item::Someone => ("Tell Someone...".to_owned(), true, 0xFFCC_CCCC, false),
                    };
                    if current {
                        p.fill(rr, crate::draw::with_alpha(colour, 0.14));
                    }
                    if usable && ctx.over(&rr) {
                        p.fill(rr, 0x40FF_FFFF);
                    }
                    let style = if usable || matches!(item, Item::Heading(_)) {
                        text.colour(colour)
                    } else {
                        dim
                    };
                    p.text(&style, rr.x + 10.0 * k, rr.y + 3.0 * k, &label);
                    self.controls.push((label.clone(), rr));
                    if usable && ctx.input.clicked(&rr) {
                        match item {
                            Item::Focus(focus) => {
                                out.requests.push(UiRequest::SetTalkFocus { focus: *focus });
                                self.tell = None;
                            }
                            Item::Person(name) => self.tell = Some(name.clone()),
                            Item::Someone => {
                                self.tell = None;
                                self.typing = Some("/t ".to_owned());
                            }
                            Item::Heading(_) => {}
                        }
                        chosen = true;
                    }
                }
                area
            }
            Menu::Name { name, at, docked } => {
                // Where the name was, as the HUD layout placed the docked window.
                let (x, y) = if docked {
                    let (d, s) = (self.dock, self.dock_drawn);
                    let f = if s.w > 0.0 { d.w / s.w } else { 1.0 };
                    (d.x + (at.0 - s.x) * f, d.y + (at.1 - s.y) * f)
                } else {
                    at
                };
                let in_view = state
                    .targets
                    .iter()
                    .find(|b| {
                        pyreal_client_model::chat::bare_name(&b.name).eq_ignore_ascii_case(&name)
                    })
                    .map(|b| b.id);
                let rows: [(&str, bool); 5] = [
                    ("Tell", true),
                    ("Select", in_view.is_some()),
                    ("Add Friend", true),
                    ("Squelch", true),
                    ("Copy Name", true),
                ];
                let (sw, sh) = p.screen;
                let (w, h) = (140.0 * k, row * 5.0 + 20.0 * k);
                let area = Rect::new(
                    x.min(sw - w).max(0.0),
                    (y - h).clamp(0.0, (sh - h).max(0.0)),
                    w,
                    h,
                );
                kit::panel(p, area, 0.95);
                for (n, (label, usable)) in rows.iter().enumerate() {
                    #[allow(clippy::cast_precision_loss)]
                    let rr = Rect::new(area.x, area.y + 10.0 * k + row * n as f32, area.w, row);
                    if *usable && ctx.over(&rr) {
                        p.fill(rr, 0x40FF_FFFF);
                    }
                    let colour = if n == 0 { chat_colour(3) } else { 0xFFFF_FFFF };
                    let style = if *usable { text.colour(colour) } else { dim };
                    p.text(&style, rr.x + 10.0 * k, rr.y + 3.0 * k, label);
                    self.controls.push(((*label).to_owned(), rr));
                    if !(*usable && ctx.input.clicked(&rr)) {
                        continue;
                    }
                    chosen = true;
                    match n {
                        0 => out.requests.push(UiRequest::ChatEntry {
                            window: window::MAIN,
                            text: self.typing.clone().unwrap_or_default(),
                            action: EntryAction::StartTell { name: name.clone() },
                        }),
                        1 => {
                            if let Some(id) = in_view {
                                out.requests.push(UiRequest::Select(id));
                            }
                        }
                        2 => out
                            .requests
                            .push(UiRequest::AddFriend { name: name.clone() }),
                        3 => out.requests.push(UiRequest::ChatLine {
                            text: format!("@squelch {name}"),
                            window: window::MAIN,
                        }),
                        _ => ctx.input.copied = Some(name.clone()),
                    }
                }
                area
            }
            Menu::Tab { slot, at, docked } => {
                let Some(w) = windows.get(slot) else {
                    self.menu = None;
                    return;
                };
                // A docked tab's bar, where the HUD layout placed the docked window.
                let at = if docked {
                    let (d, s) = (self.dock, self.dock_drawn);
                    let f = if s.w > 0.0 { d.w / s.w } else { 1.0 };
                    Rect::new(
                        d.x + (at.x - s.x) * f,
                        d.y + (at.y - s.y) * f,
                        at.w * f,
                        at.h * f,
                    )
                } else {
                    at
                };
                let (area, pressed) = self.filter_menu(p, ctx, w, slot, at, state.era, out);
                chosen = pressed;
                area
            }
        };
        self.menu_rect = Some(area);
        if ctx.over(&area) && (ctx.input.pressed[0] || ctx.input.pressed[1]) {
            ctx.input.captured = true;
        }
        // Any other use of the pointer or the keys closes it: a press or a release off it (a
        // press something under it already took still lets go), the wheel off it, a key, a
        // character typed or one of the game's actions. The release of the press that opened it
        // does not.
        let escape = ctx.input.take_key(vk::ESCAPE);
        let input = &ctx.input;
        let off = !area.contains(input.mouse.0, input.mouse.1);
        let released = input.released.iter().any(|r| *r);
        let fresh = self.menu_fresh;
        self.menu_fresh = fresh && !released;
        let elsewhere = off
            && ((input.pressed.iter().any(|b| *b) && !fresh)
                || (released && !fresh)
                || input.wheel != 0.0);
        let keyed = !input.keys.is_empty() || !input.chars.is_empty() || !input.actions.is_empty();
        if chosen || elsewhere || escape || keyed {
            self.menu = None;
        }
    }
}

/// Where each corner of a log window showing as `shown` is held to size it.
fn corners(shown: Rect, k: f32) -> [(Corner, Rect); 4] {
    Corner::ALL.map(|c| {
        let g = if c.top() { TOP_GRIP } else { GRIP } * k;
        let x = if c.left() { shown.x } else { shown.right() - g };
        let y = if c.top() { shown.y } else { shown.bottom() - g };
        (c, Rect::new(x, y, g, g))
    })
}

/// A popped-out log window's outline while the HUD Layout window is open, as the HUD's elements
/// are outlined there, with its tab's name.
fn outline(p: &mut Painter<'_>, r: Rect, name: &str) {
    let k = p.scale;
    p.fill(r, 0x20FF_FFFF);
    for side in [
        Rect::new(r.x, r.y, r.w, 1.5),
        Rect::new(r.x, r.bottom() - 1.5, r.w, 1.5),
        Rect::new(r.x, r.y, 1.5, r.h),
        Rect::new(r.right() - 1.5, r.y, 1.5, r.h),
    ] {
        p.fill(side, 0xC0FF_FFFF);
    }
    let label = TextStyle::new(Family::Body, 12.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    p.text(
        &label,
        r.x + 4.0 * k,
        r.y + 2.0 * k,
        &format!("Log Window: {name}"),
    );
}

/// A log window's ground at `opacity`.
fn ground(opacity: f32) -> Argb {
    crate::draw::with_alpha(0xFF10_1010, opacity)
}

/// The game's chat windows as this frame's state has them, the main one at least.
/// Each with its filter as [`window_filter`] has it.
fn windows_of(state: &GameState) -> Vec<ChatWindow> {
    let mut windows = if state.chat_windows.is_empty() {
        vec![ChatWindow {
            id: window::MAIN,
            filter: 0,
            title: None,
        }]
    } else {
        state.chat_windows.clone()
    };
    for w in &mut windows {
        w.filter = window_filter(state, w.id);
    }
    windows
}

/// A row of the destinations' list.
enum Item {
    Focus(u32),
    Heading(&'static str),
    Person(String),
    Someone,
}

/// The chip the Modern input line names its destination in, at the left of `row` and as tall as
/// it, with the small arrow that says it opens a list. Where it was drawn.
fn draw_chip(p: &mut Painter<'_>, style: &TextStyle, row: Rect, label: &str, colour: Argb) -> Rect {
    let k = p.scale;
    let lw = p.measure(style, label);
    let chip = Rect::new(row.x + 3.0 * k, row.y, lw + 26.0 * k, row.h);
    p.fill(chip, crate::draw::with_alpha(colour, 0.16));
    p.outline(chip, k.max(1.0), crate::draw::with_alpha(colour, 0.55));
    p.text_in(
        style,
        Rect::new(chip.x + 7.0 * k, chip.y, lw, chip.h),
        Align::Left,
        label,
    );
    let (ax, ay) = (chip.x + 11.0 * k + lw, chip.y + chip.h / 2.0 - 1.5 * k);
    for row in 0..3u8 {
        let row = f32::from(row);
        p.fill(
            Rect::new(ax + row * k, ay + row * k, (5.0 - 2.0 * row) * k, k),
            colour,
        );
    }
    chip
}

/// The name a destination goes by on the input line and in its list. In Modern the game's tell
/// to the selected object reads "Selected", with its name, so it is not taken for a tell by name.
fn destination_label(state: &GameState, focus: u32, modern: bool) -> String {
    if modern && focus == 2 {
        return state.chat_focus.target.as_ref().map_or_else(
            || "Selected".to_owned(),
            |t| format!("Selected: {}", t.name),
        );
    }
    line_label(state, focus)
}

/// The tell partner after `current` among `recent` (newest first), or before it when `back`,
/// wrapping; the newest when `current` is not among them.
#[must_use]
pub fn next_partner(recent: &[String], current: &str, back: bool) -> Option<String> {
    let names: Vec<&str> = recent
        .iter()
        .map(|n| pyreal_client_model::chat::bare_name(n))
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        return None;
    }
    let n = names.len();
    let at = names.iter().position(|x| x.eq_ignore_ascii_case(current));
    let next = match at {
        Some(i) if back => (i + n - 1) % n,
        Some(i) => (i + 1) % n,
        None => 0,
    };
    Some(names[next].to_owned())
}

/// A destination's label: the game's own word for it, else a plain one.
/// The destination's name on the input line, in full (General, where the game's menu says Gen),
/// so the channel the player is talking in reads plainly while they type.
fn line_label(state: &GameState, focus: u32) -> String {
    FOCUS_WORDS
        .get(focus as usize)
        .filter(|w| !w.is_empty())
        .map_or_else(|| focus_label(state, focus), |w| (*w).to_owned())
}

fn focus_label(state: &GameState, focus: u32) -> String {
    state
        .chat_focus_labels
        .get(focus as usize)
        .filter(|l| !l.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            FOCUS_WORDS
                .get(focus as usize)
                .copied()
                .unwrap_or("Chat")
                .to_owned()
        })
}

/// The destinations' plain names, by focus.
pub const FOCUS_WORDS: [&str; 14] = [
    "",
    "Chat",
    "Tell",
    "Fellowship",
    "Patron",
    "Monarch",
    "Vassals",
    "Allegiance",
    "General",
    "Trade",
    "LFG",
    "Roleplay",
    "Society",
    "Olthoi",
];

/// The filter groups the options offer for window `id`, as the shared options sheet lists them
/// for this interface: caption and mask.
#[must_use]
pub fn filter_groups(
    id: u32,
    features: Option<&pyreal_primitives::EraFeatures>,
) -> Vec<(&'static str, u64)> {
    use pyreal_client_contract::options::sheet::{rows_for, PageId, Value};
    rows_for(PageId::Chat)
        .filter(|r| r.needs.met(features))
        .filter_map(|r| match r.value {
            Value::Filter { window, mask } if window == id => Some((r.caption, mask)),
            _ => None,
        })
        .collect()
}

/// The action an action name stands for.
fn named(name: &str) -> Option<pyreal_client_contract::actions::ActionId> {
    pyreal_client_contract::actions::names::action_for_enum_name(name)
}

/// Whether `action` is one of the chat entry's, which the log window answers.
#[must_use]
pub fn is_chat_action(action: u32) -> bool {
    use pyreal_client_contract::actions::{chat_entry as a, ActionId};
    let id = ActionId(action);
    [
        a::REPLY,
        a::MONARCH_REPLY,
        a::PATRON_REPLY,
        a::TELL_TO_SELECTED,
        a::START_COMMAND,
        a::BEGIN_CHAT_MODE,
        a::TOGGLE_CHAT_ENTRY,
    ]
    .contains(&id)
        || ["RecallLastMessage", "PreviousMessage", "NextMessage"]
            .iter()
            .any(|n| named(n) == Some(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pyreal_client_contract::chat::mainchat::ChatFocusView;

    fn view(focus: u32, usable: &[u32]) -> ChatFocusView {
        let mut v = ChatFocusView {
            focus,
            ..ChatFocusView::default()
        };
        for f in usable {
            v.enabled[*f as usize] = true;
            v.selectable[*f as usize] = true;
        }
        v
    }

    #[test]
    fn tab_steps_to_the_next_destination_the_game_lets_the_player_use_in_its_menu_order() {
        // The menu's order begins Monarch, Selected, Patron, All, Vassals, Fellowship.
        let v = view(1, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(3));
        let v = view(3, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(8));
        let v = view(8, &[1, 3, 8]);
        assert_eq!(next_focus(&v), Some(1), "it wraps round");
        assert_eq!(next_focus(&view(1, &[1])), None);
    }

    #[test]
    fn a_destination_switched_off_or_not_selectable_cannot_be_picked() {
        let mut v = view(1, &[1, 4]);
        assert!(focus_usable(&v, 4));
        v.selectable[4] = false;
        assert!(!focus_usable(&v, 4));
        assert!(!focus_usable(&v, 13));
        assert!(!menu_focuses(false).contains(&13));
        assert!(menu_focuses(true).contains(&13));
    }

    #[test]
    fn the_input_line_names_the_saved_channel_in_full_where_the_game_s_menu_shortens_it() {
        let mut labels = vec![String::new(); 14];
        labels[8] = "Gen".into();
        let state = GameState {
            chat_focus_labels: labels,
            ..GameState::default()
        };
        assert_eq!(line_label(&state, 8), "General");
        assert_eq!(
            focus_label(&state, 8),
            "Gen",
            "the menu keeps the game's word"
        );
    }

    #[test]
    fn a_typed_command_names_its_channel_and_plain_text_takes_the_destination() {
        let focus = ("Monarch", 0xFFAB_DBE5);
        assert_eq!(channel_label("/t Borin, hi", focus).0, "Tell");
        assert_eq!(channel_label("@f ready", focus).0, "Fellowship");
        assert_eq!(channel_label("/cg anyone?", focus).0, "General");
        assert_eq!(
            channel_label("hello", focus),
            ("Monarch".to_owned(), 0xFFAB_DBE5)
        );
    }

    #[test]
    fn a_window_shows_lines_addressed_to_it_and_the_broadcasts_its_filter_lets_through() {
        let w = ChatWindow {
            id: window::FLOATY_1,
            filter: 1 << 2,
            title: None,
        };
        let line = |chat_type, window| ChatLine {
            chat_type,
            text: String::new(),
            window,
        };
        assert!(shows_in(&w, &line(2, 0)));
        assert!(!shows_in(&w, &line(3, 0)));
        assert!(shows_in(&w, &line(3, window::FLOATY_1)));
        assert!(!shows_in(&w, &line(2, window::MAIN)));
    }

    #[test]
    fn the_main_window_offers_twelve_filter_groups_and_a_floating_one_thirteen() {
        assert_eq!(filter_groups(window::MAIN, None).len(), 12);
        assert_eq!(filter_groups(window::FLOATY_2, None).len(), 13);
        assert_eq!(filter_groups(window::FLOATY_2, None)[0].0, "Gameplay");
    }

    #[test]
    fn each_window_s_own_default_shows_its_channels_until_the_character_keeps_a_filter() {
        let shows = |id: u32, caption: &str| default_filter(id) & group(caption) == group(caption);
        for c in [
            "Area Speech",
            "Tells",
            "Allegiance",
            "Fellowship",
            "General",
            "Trade",
            "LFG",
            "Roleplay",
            "Society",
        ] {
            assert!(shows(window::MAIN, c), "main shows {c}");
        }
        assert!(!shows(window::MAIN, "Combat") && !shows(window::MAIN, "Magic"));
        assert!(!shows(window::MAIN, "Errors"));
        assert!(
            shows(window::MAIN, "Gameplay"),
            "the main window keeps the game's own messages"
        );
        assert_eq!(
            default_filter(window::FLOATY_1),
            group("Combat") | group("Magic")
        );
        assert_eq!(default_filter(window::FLOATY_2), group("Allegiance"));
        assert_eq!(default_filter(window::FLOATY_3), group("Fellowship"));
        assert!(shows(window::FLOATY_4, "Society") && shows(window::FLOATY_4, "LFG"));
        let mut state = GameState::default();
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            default_filter(window::FLOATY_1)
        );
        state.chat_filters = vec![(
            window::FLOATY_1,
            pyreal_client_contract::chat::interface::default_filter(window::FLOATY_1),
        )];
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            default_filter(window::FLOATY_1),
            "the game's own starting filter kept for the window is not the player's choice"
        );
        state.chat_filters = vec![(window::FLOATY_1, 0)];
        assert_eq!(
            window_filter(&state, window::FLOATY_1),
            0,
            "a filter the character keeps is the player's, even one showing nothing"
        );
    }

    #[test]
    fn a_channel_command_at_the_start_of_the_line_names_its_talk_focus_and_leaves_the_rest() {
        use pyreal_client_model::chat::TalkFocus as F;
        assert_eq!(
            focus_command("/cg hello"),
            Some((F::General as u32, "hello"))
        );
        assert_eq!(focus_command("@CT "), Some((F::Trade as u32, "")));
        assert_eq!(
            focus_command("/f we go"),
            Some((F::Fellowship as u32, "we go"))
        );
        assert_eq!(focus_command("/clfg x"), Some((F::Lfg as u32, "x")));
        assert_eq!(focus_command("/cg"), None, "no space typed yet");
        assert_eq!(
            focus_command("/tell Bob, hi"),
            None,
            "a tell is not a channel"
        );
        assert_eq!(focus_command("hello"), None);
    }

    fn line(chat_type: u8, text: &str) -> ChatLine {
        ChatLine {
            chat_type,
            text: text.into(),
            window: 0,
        }
    }

    #[test]
    fn a_tell_line_is_with_the_person_either_way() {
        for (ty, text, who) in [
            (3, "Alba tells you, \"hi\"", Some("Alba")),
            (
                3,
                "<Tell:IIDString:1342177281:+Sir Fenn>+Sir Fenn<\\Tell> tells you, \"x\"",
                Some("Sir Fenn"),
            ),
            (4, "You tell Alba, \"see you\"", Some("Alba")),
            (4, "12:30 You tell Bex, \"a, b\"", Some("Bex")),
            (4, "You tell group@channel, \"bro\"", None),
            (2, "Alba says, \"hi\"", None),
            (3, "You hear nothing.", None),
        ] {
            assert_eq!(tell_partner(&line(ty, text)).as_deref(), who, "{text}");
        }
    }

    #[test]
    fn tab_on_the_chip_steps_through_recent_partners_wrapping() {
        let recent: Vec<String> = ["Alba", "+Bex", "Cato"].map(String::from).to_vec();
        assert_eq!(next_partner(&recent, "alba", false).as_deref(), Some("Bex"));
        assert_eq!(
            next_partner(&recent, "Cato", false).as_deref(),
            Some("Alba")
        );
        assert_eq!(next_partner(&recent, "Alba", true).as_deref(), Some("Cato"));
        assert_eq!(
            next_partner(&recent, "Dain", false).as_deref(),
            Some("Alba")
        );
        assert_eq!(next_partner(&[], "Alba", false), None);
    }

    #[test]
    fn the_name_in_a_tell_the_player_sent_is_a_link() {
        let (text, links) = line_links(&line(4, "You tell Sir Fenn, \"hi\""));
        assert_eq!(text, "You tell Sir Fenn, \"hi\"");
        assert_eq!(links, [(9, 17, LinkTarget::Name("Sir Fenn".into()))]);
        assert!(line_links(&line(4, "You tell a@b, \"x\"")).1.is_empty());
        assert!(line_links(&line(2, "You tell no one, \"x\"")).1.is_empty());
    }

    #[test]
    fn the_chat_input_styles_read_back_from_their_words() {
        for s in ChatStyle::ALL {
            assert_eq!(ChatStyle::from_word(s.word()), Some(s));
        }
        assert_eq!(ChatStyle::default(), ChatStyle::Modern);
    }
}
