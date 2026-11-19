//! One retained chat entry per logical window.

use crate::chat::bare_name;
use pyreal_client_contract::chat::{
    entry::{EntryAction, EntryUpdate, NameSources, ReplyTarget},
    window::ReplyTargets,
};

/// How many submitted lines an entry remembers.
const HISTORY: usize = 100;

/// Text and command history. Focus, glyphs and selection belong to the active widget.
#[derive(Debug, Clone, Default)]
pub struct ChatEntry {
    pub text: String,
    history: Vec<String>,
    position: Option<usize>,
    cycle: Option<Cycle>,
}

/// A Tab cycle: the line before the name, the names matched, and the line last shown.
#[derive(Debug, Clone)]
struct Cycle {
    head: String,
    names: Vec<String>,
    at: usize,
    shown: String,
}

impl ChatEntry {
    #[must_use]
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Record exactly one submitted line, preserving the original wide text.
    pub fn submit(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.history.push(text.to_owned());
        if self.history.len() > HISTORY {
            self.history.drain(..self.history.len() - HISTORY);
        }
        self.position = None;
        self.cycle = None;
        self.text.clear();
    }

    /// Apply one entry operation.
    pub fn apply(
        &mut self,
        window: u32,
        text: String,
        action: EntryAction,
        targets: &ReplyTargets,
    ) -> Option<EntryUpdate> {
        self.text = text;
        let mut focus = false;
        let mut cursor = None;
        if !matches!(action, EntryAction::Draft | EntryAction::Complete { .. }) {
            self.cycle = None;
        }
        match action {
            EntryAction::Draft => return None,
            EntryAction::Previous => {
                let next = match self.position {
                    Some(0) if !self.history.is_empty() => return None,
                    Some(p) if p < self.history.len() => p - 1,
                    _ if self.history.is_empty() => return None,
                    _ => self.history.len() - 1,
                };
                self.position = Some(next);
                self.text.clone_from(&self.history[next]);
            }
            EntryAction::Next => match self.position.filter(|p| *p + 1 < self.history.len()) {
                Some(p) => {
                    self.position = Some(p + 1);
                    self.text.clone_from(&self.history[p + 1]);
                }
                None => {
                    self.position = None;
                    self.text.clear();
                }
            },
            EntryAction::RecallLast => {
                let last = self.history.last()?;
                self.text.clone_from(last);
            }
            EntryAction::Reply { target, prefix } => {
                let name = reply_name(targets, target)?;
                self.text = format!("{prefix}{name}, ");
                focus = true;
            }
            EntryAction::StartTell { name } => {
                self.text = format!("@tell {name}, ");
                focus = true;
            }
            EntryAction::ExpandAlias => {
                let (value, at) = expand_alias(&self.text, targets)?;
                self.text = value;
                cursor = Some(at);
            }
            EntryAction::Complete { names, back } => self.complete(&names, back, targets)?,
        }
        Some(EntryUpdate {
            window,
            cursor: cursor.unwrap_or_else(|| self.text.chars().count()),
            text: self.text.clone(),
            focus,
        })
    }
}

impl ChatEntry {
    /// Fills a tell's name from `names`, or steps the running cycle when the line is unchanged.
    fn complete(&mut self, names: &NameSources, back: bool, targets: &ReplyTargets) -> Option<()> {
        let cycle = match self.cycle.take().filter(|c| c.shown == self.text) {
            Some(mut c) => {
                let n = c.names.len();
                c.at = if back {
                    (c.at + n - 1) % n
                } else {
                    (c.at + 1) % n
                };
                c
            }
            None => {
                let slot = name_slot(&self.text)?;
                let names = completions(&slot, names, targets);
                if names.is_empty() {
                    return None;
                }
                let at = if back { names.len() - 1 } else { 0 };
                Cycle {
                    head: slot.head,
                    names,
                    at,
                    shown: String::new(),
                }
            }
        };
        self.text = format!("{}{}, ", cycle.head, cycle.names[cycle.at]);
        self.cycle = Some(Cycle {
            shown: self.text.clone(),
            ..cycle
        });
        Some(())
    }
}

/// The reply command a name slot starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotSeed {
    LastTeller,
    LastTellee,
    Monarch,
    Patron,
}

/// Where Tab puts a name: the line before it, the letters typed so far, and a reply's own name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameSlot {
    pub head: String,
    pub typed: String,
    pub seed: Option<SlotSeed>,
}

/// The name slot of a tell (`/t`, `/tell`, `/w`, `/whisper`) whose message is still empty, or of a
/// bare reply command (`/r`, `/rp`, `/reply`, `/rt`, `/retell`, `/mr`, `/pr`), which becomes `/t`.
#[must_use]
pub fn name_slot(text: &str) -> Option<NameSlot> {
    let line = text.trim_start();
    let sigil = line.chars().next().filter(|c| matches!(c, '@' | '/'))?;
    let rest = &line[1..];
    let (verb, after) = rest
        .find(char::is_whitespace)
        .map_or((rest, ""), |i| rest.split_at(i));
    let seed = match verb.to_ascii_lowercase().as_str() {
        "t" | "tell" | "w" | "whisper" => None,
        "r" | "rp" | "reply" => Some(SlotSeed::LastTeller),
        "rt" | "retell" => Some(SlotSeed::LastTellee),
        "mr" => Some(SlotSeed::Monarch),
        "pr" => Some(SlotSeed::Patron),
        _ => return None,
    };
    if seed.is_some() {
        return after.trim().is_empty().then(|| NameSlot {
            head: format!("{sigil}t "),
            typed: String::new(),
            seed,
        });
    }
    let name = match after.split_once(',') {
        Some((_, message)) if !message.trim().is_empty() => return None,
        Some((name, _)) => name,
        None => after,
    };
    Some(NameSlot {
        head: format!("{sigil}{verb} "),
        typed: name.trim().to_owned(),
        seed: None,
    })
}

/// The names Tab offers for `slot`, in order. A reply offers its own name, then recent partners;
/// a tell offers the selection, nearby, recent, fellowship, allegiance and friends. Names that
/// start with the typed letters come before names with a later word that does.
#[must_use]
pub fn completions(slot: &NameSlot, names: &NameSources, targets: &ReplyTargets) -> Vec<String> {
    let mut order: Vec<&str> = Vec::new();
    match slot.seed {
        Some(seed) => {
            let first = match seed {
                SlotSeed::LastTeller => &targets.last_teller,
                SlotSeed::LastTellee => &targets.last_tellee,
                SlotSeed::Monarch => &targets.monarch,
                SlotSeed::Patron => &targets.patron,
            };
            order.extend(first.as_deref());
            order.extend(targets.recent.iter().map(String::as_str));
        }
        None => {
            order.extend(names.selected.as_deref());
            order.extend(names.nearby.iter().map(String::as_str));
            order.extend(targets.recent.iter().map(String::as_str));
            for list in [&names.fellowship, &names.allegiance, &names.friends] {
                order.extend(list.iter().map(String::as_str));
            }
        }
    }
    let key = |n: &str| bare_name(n).to_lowercase();
    let me = names.me.as_deref().map(key);
    let typed = key(&slot.typed);
    let mut seen = Vec::new();
    let mut whole = Vec::new();
    let mut word = Vec::new();
    for name in order {
        let k = key(name);
        if k.is_empty() || Some(&k) == me.as_ref() || seen.contains(&k) {
            continue;
        }
        seen.push(k.clone());
        if k.starts_with(&typed) {
            whole.push(bare_name(name).to_owned());
        } else if k.split(' ').skip(1).any(|w| w.starts_with(&typed)) {
            word.push(bare_name(name).to_owned());
        }
    }
    whole.extend(word);
    whole
}

/// `%t` and `<t>` (either case) become `target`, the selection's bare name; without one the line is kept.
#[must_use]
pub fn substitute_target(line: &str, target: Option<&str>) -> String {
    let Some(name) = target.map(bare_name).filter(|n| !n.is_empty()) else {
        return line.to_owned();
    };
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find(['%', '<']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let starts = |t: &str| {
            tail.get(..t.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(t))
        };
        let token = if starts("%t") {
            2
        } else if starts("<t>") {
            3
        } else {
            0
        };
        if token == 0 {
            out.push_str(&tail[..1]);
            rest = &tail[1..];
        } else {
            out.push_str(name);
            rest = &tail[token..];
        }
    }
    out.push_str(rest);
    out
}

/// A tell line's person and message: `@tell Alba, hi` is `("Alba", "hi")`. Any tell verb and
/// either sigil; the name ends at the first comma and one space after it is dropped.
#[must_use]
pub fn split_tell(text: &str) -> Option<(String, String)> {
    let line = text.trim_start();
    let rest = line.strip_prefix(['@', '/'])?;
    let (verb, after) = rest.split_once(char::is_whitespace)?;
    if !matches!(
        verb.to_ascii_lowercase().as_str(),
        "t" | "tell" | "w" | "whisper" | "send"
    ) {
        return None;
    }
    let (name, message) = after.split_once(',')?;
    let name = bare_name(name);
    if name.is_empty() {
        return None;
    }
    let message = message.strip_prefix(' ').unwrap_or(message);
    Some((name.to_owned(), message.to_owned()))
}

/// The line that tells `name` `message`, as the game's `@tell` takes it.
#[must_use]
pub fn tell_line(name: &str, message: &str) -> String {
    format!("@tell {name}, {message}")
}

/// A reply command and the rest of the line, once a space follows it: `/r hi` is the last
/// teller and `hi`.
#[must_use]
pub fn reply_command(text: &str) -> Option<(SlotSeed, &str)> {
    let rest = text.trim_start().strip_prefix(['@', '/'])?;
    let (verb, after) = rest.split_once(' ')?;
    let seed = match verb.to_ascii_lowercase().as_str() {
        "r" | "rp" | "reply" => SlotSeed::LastTeller,
        "rt" | "retell" => SlotSeed::LastTellee,
        "mr" => SlotSeed::Monarch,
        "pr" => SlotSeed::Patron,
        _ => return None,
    };
    Some((seed, after))
}

/// The name a reply command's seed stands for, when there is one.
#[must_use]
pub fn seed_name(seed: SlotSeed, targets: &ReplyTargets) -> Option<&str> {
    match seed {
        SlotSeed::LastTeller => targets.last_teller.as_deref(),
        SlotSeed::LastTellee => targets.last_tellee.as_deref(),
        SlotSeed::Monarch => targets.monarch.as_deref(),
        SlotSeed::Patron => targets.patron.as_deref(),
    }
    .map(bare_name)
    .filter(|s| !s.is_empty())
}

/// A tell's name typed with nothing after it, as `/t Alba` sent alone: the name.
#[must_use]
pub fn bare_tell(text: &str) -> Option<&str> {
    let rest = text.trim().strip_prefix(['@', '/'])?;
    let (verb, name) = rest.split_once(char::is_whitespace)?;
    if !matches!(
        verb.to_ascii_lowercase().as_str(),
        "t" | "tell" | "w" | "whisper" | "send"
    ) {
        return None;
    }
    let name = bare_name(name.trim_end_matches(','));
    (!name.is_empty() && !name.contains(',')).then_some(name)
}

fn reply_name(targets: &ReplyTargets, target: ReplyTarget) -> Option<&str> {
    match target {
        ReplyTarget::LastTeller => targets.last_teller.as_deref(),
        ReplyTarget::Monarch => targets.monarch.as_deref(),
        ReplyTarget::Patron => targets.patron.as_deref(),
    }
    .filter(|s| !s.is_empty())
}

/// Replace only the leading command alias; its typed space and following text survive.
#[must_use]
fn expand_alias(text: &str, targets: &ReplyTargets) -> Option<(String, usize)> {
    let trimmed = text.trim_start();
    let mut chars = trimmed.chars();
    if !matches!(chars.next()?, '@' | '/') {
        return None;
    }
    let body: Vec<char> = chars.collect();
    for (alias, target) in [
        ("r ", ReplyTarget::LastTeller),
        ("rp ", ReplyTarget::LastTeller),
        ("reply ", ReplyTarget::LastTeller),
        ("mr ", ReplyTarget::Monarch),
        ("pr ", ReplyTarget::Patron),
    ] {
        let count = alias.chars().count();
        if body.len() < count
            || !body[..count]
                .iter()
                .collect::<String>()
                .eq_ignore_ascii_case(alias)
        {
            continue;
        }
        let name = reply_name(targets, target)?;
        let replacement = format!("@tell {name},");
        let tail: String = body[count - 1..].iter().collect();
        return Some((
            format!("{replacement}{tail}"),
            replacement.chars().count() + 1,
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    fn targets() -> ReplyTargets {
        ReplyTargets {
            last_teller: Some("Alba".into()),
            monarch: Some("Bex".into()),
            patron: Some("Cato".into()),
            last_tellee: Some("Dain".into()),
            recent: vec!["Alba".into(), "Esk".into(), "Sir Fenn".into()],
        }
    }
    fn sources() -> NameSources {
        NameSources {
            me: Some("Me".into()),
            selected: Some("Gale".into()),
            nearby: vec!["Hob".into(), "Me".into(), "gale".into()],
            fellowship: vec!["Me".into(), "Ivo".into()],
            allegiance: vec!["Bex".into(), "Cato".into()],
            friends: vec!["Juno".into()],
        }
    }
    fn line(text: &str) -> ChatEntry {
        ChatEntry {
            text: text.into(),
            ..ChatEntry::default()
        }
    }
    fn tab(e: &mut ChatEntry, back: bool) -> Option<String> {
        let names = sources();
        edit(e, EntryAction::Complete { names, back }).map(|u| u.text)
    }
    fn edit(entry: &mut ChatEntry, action: EntryAction) -> Option<EntryUpdate> {
        entry.apply(8, entry.text.clone(), action, &targets())
    }
    #[test]
    fn history_has_dead_ends_clears_forward_drafts_and_ignores_empty_submissions() {
        let mut e = ChatEntry::default();
        e.submit("");
        assert!(e.history().is_empty());
        assert!(edit(&mut e, EntryAction::Previous).is_none());
        for line in ["one", "two", "three"] {
            e.submit(line);
        }
        e.text = "draft".into();
        assert_eq!(edit(&mut e, EntryAction::Next).unwrap().text, "");
        for expected in ["three", "two", "one"] {
            assert_eq!(edit(&mut e, EntryAction::Previous).unwrap().text, expected);
        }
        assert!(edit(&mut e, EntryAction::Previous).is_none());
        for expected in ["two", "three", ""] {
            assert_eq!(edit(&mut e, EntryAction::Next).unwrap().text, expected);
        }
        assert_eq!(edit(&mut e, EntryAction::RecallLast).unwrap().text, "three");
    }
    #[test]
    fn history_is_bounded_on_submission_and_a_draft_keeps_it() {
        let mut e = ChatEntry::default();
        for n in 0..120 {
            e.submit(&n.to_string());
        }
        assert_eq!(e.history().len(), 100);
        assert_eq!(e.history()[0], "20");
        e.apply(8, "draft".into(), EntryAction::Draft, &targets());
        assert_eq!(e.history().len(), 100);
        assert_eq!(e.text, "draft");
    }
    #[test]
    fn canonical_reply_actions_address_distinct_sender_slots_and_keep_the_prefix() {
        for (raw, name) in [
            (0x10000020, "Bex"),
            (0x10000021, "Cato"),
            (0x10000022, "Alba"),
        ] {
            let target =
                ReplyTarget::from_action(pyreal_client_contract::actions::ActionId(raw)).unwrap();
            let mut e = ChatEntry::default();
            let update = e.apply(
                8,
                "draft".into(),
                EntryAction::Reply {
                    target,
                    prefix: "localized ".into(),
                },
                &targets(),
            );
            assert_eq!(update.unwrap().text, format!("localized {name}, "));
        }
    }
    #[test]
    fn missing_reply_preserves_the_draft() {
        for target in [
            ReplyTarget::LastTeller,
            ReplyTarget::Monarch,
            ReplyTarget::Patron,
        ] {
            let mut e = ChatEntry::default();
            let update = e.apply(
                8,
                "draft".into(),
                EntryAction::Reply {
                    target,
                    prefix: "@tell ".into(),
                },
                &ReplyTargets::default(),
            );
            assert!(update.is_none());
            assert_eq!(e.text, "draft");
        }
    }
    #[test]
    fn aliases_require_a_sigil_and_complete_leading_alias_and_preserve_tail_and_cursor() {
        for (alias, name) in [
            ("r", "Alba"),
            ("rp", "Alba"),
            ("reply", "Alba"),
            ("mr", "Bex"),
            ("pr", "Cato"),
        ] {
            for sigil in ['@', '/'] {
                let text = format!("  {sigil}{} hello", alias.to_uppercase());
                let (value, cursor) = expand_alias(&text, &targets()).unwrap();
                assert_eq!(value, format!("@tell {name}, hello"));
                assert_eq!(cursor, format!("@tell {name}, ").chars().count());
            }
        }
        for text in ["r ", "@r", "@replying ", "x @r ", "", "@"] {
            assert!(expand_alias(text, &targets()).is_none(), "{text}");
        }
        assert!(expand_alias("@r ", &ReplyTargets::default()).is_none());
    }

    #[test]
    fn start_tell_replaces_the_draft_without_submitting_or_recording_it() {
        let mut e = ChatEntry::default();
        let update = e.apply(
            8,
            "old".into(),
            EntryAction::StartTell {
                name: "+A Name".into(),
            },
            &targets(),
        );
        let update = update.unwrap();
        assert_eq!(update.text, "@tell +A Name, ");
        assert!(update.focus);
        assert_eq!(update.cursor, update.text.chars().count());
        assert!(e.history().is_empty());
    }

    #[test]
    fn a_started_tell_with_a_message_typed_after_it_is_a_tell_to_that_player() {
        use crate::chat::{ChatState, TellOutcome};
        for (name, wire) in [("+Sir Alba", "Sir Alba"), ("Lark", "Lark")] {
            let mut e = ChatEntry::default();
            let update = e
                .apply(
                    8,
                    String::new(),
                    EntryAction::StartTell { name: name.into() },
                    &targets(),
                )
                .unwrap();
            assert_eq!(update.text, format!("@tell {name}, "));
            let typed = format!("{}well met", update.text);
            let args = typed.strip_prefix("@tell ").unwrap();
            assert_eq!(
                ChatState::new().do_tell(args),
                TellOutcome::ByName {
                    message: "well met".into(),
                    target_name: wire.into(),
                },
                "{name}"
            );
        }
    }

    #[test]
    fn name_slot_finds_a_tells_name_and_bare_replies() {
        for (text, head, typed) in [
            ("/t", "/t ", ""),
            ("/T al", "/T ", "al"),
            ("  @tell Sir Al", "@tell ", "Sir Al"),
            ("/w Alba, ", "/w ", "Alba"),
            ("/whisper ", "/whisper ", ""),
        ] {
            let slot = name_slot(text).unwrap();
            assert_eq!(
                (slot.head.as_str(), slot.typed.as_str()),
                (head, typed),
                "{text}"
            );
            assert_eq!(slot.seed, None);
        }
        for (text, seed) in [
            ("/r", SlotSeed::LastTeller),
            ("@reply ", SlotSeed::LastTeller),
            ("/rp", SlotSeed::LastTeller),
            ("/rt", SlotSeed::LastTellee),
            ("/retell", SlotSeed::LastTellee),
            ("/mr", SlotSeed::Monarch),
            ("/pr", SlotSeed::Patron),
        ] {
            let slot = name_slot(text).unwrap();
            assert_eq!(slot.seed, Some(seed), "{text}");
            assert_eq!(slot.head, format!("{}t ", &text[..1]));
        }
        for text in [
            "",
            "hello",
            "/tells",
            "/t Alba, hi",
            "/r hi",
            "/f",
            "t Alba",
        ] {
            assert!(name_slot(text).is_none(), "{text}");
        }
    }

    #[test]
    fn tab_offers_selection_nearby_recent_fellowship_allegiance_friends_in_order() {
        let slot = name_slot("/t").unwrap();
        assert_eq!(
            completions(&slot, &sources(), &targets()),
            ["Gale", "Hob", "Alba", "Esk", "Sir Fenn", "Ivo", "Bex", "Cato", "Juno"]
        );
    }

    #[test]
    fn typed_letters_match_name_starts_before_later_words() {
        let mut t = targets();
        t.recent.push("Fenwick".into());
        let slot = name_slot("/t fen").unwrap();
        assert_eq!(completions(&slot, &sources(), &t), ["Fenwick", "Sir Fenn"]);
        let slot = name_slot("/t +ALB").unwrap();
        assert_eq!(completions(&slot, &sources(), &t), ["Alba"]);
    }

    #[test]
    fn admin_and_ai_player_marks_are_matched_offered_and_counted_without_the_mark() {
        let names = NameSources {
            me: Some("+Me".into()),
            selected: Some("~Bob".into()),
            nearby: vec!["Bob".into(), "+Ada".into(), "~Me".into(), "~+".into()],
            ..NameSources::default()
        };
        let mut t = targets();
        t.recent = vec!["~ada".into()];
        let all = completions(&name_slot("/t").unwrap(), &names, &t);
        assert_eq!(all, ["Bob", "Ada"]);
        for typed in ["/t b", "/t ~b", "/t +B"] {
            assert_eq!(
                completions(&name_slot(typed).unwrap(), &names, &t),
                ["Bob"],
                "{typed}"
            );
        }
        let mut t = targets();
        t.last_teller = Some("~Cy".into());
        let mut e = line("/r");
        let update = e.apply(
            8,
            "/r".into(),
            EntryAction::Complete { names, back: false },
            &t,
        );
        assert_eq!(update.unwrap().text, "/t Cy, ");
        assert_eq!(substitute_target("hi %t", Some("~Bob")), "hi Bob");
        assert_eq!(substitute_target("hi %t", Some("+")), "hi %t");
    }

    #[test]
    fn repeated_tab_cycles_and_wraps_and_shift_tab_goes_back() {
        let mut e = line("/t");
        assert_eq!(tab(&mut e, false).unwrap(), "/t Gale, ");
        assert_eq!(tab(&mut e, false).unwrap(), "/t Hob, ");
        assert_eq!(tab(&mut e, true).unwrap(), "/t Gale, ");
        assert_eq!(tab(&mut e, true).unwrap(), "/t Juno, ");
        assert_eq!(tab(&mut e, false).unwrap(), "/t Gale, ");
        // The letters typed stay the filter while cycling.
        let mut e = line("/tell a");
        assert_eq!(tab(&mut e, false).unwrap(), "/tell Alba, ");
        assert_eq!(tab(&mut e, false).unwrap(), "/tell Alba, ");
        // A draft of the shown line keeps the cycle; an edit starts a new one.
        let mut e = line("/t");
        tab(&mut e, false);
        let shown = e.text.clone();
        e.apply(8, shown, EntryAction::Draft, &targets());
        assert_eq!(tab(&mut e, false).unwrap(), "/t Hob, ");
        e.text = "/t Hob".into();
        assert_eq!(tab(&mut e, false).unwrap(), "/t Hob, ");
        assert!(tab(
            &mut ChatEntry {
                text: "/t zz".into(),
                ..ChatEntry::default()
            },
            false
        )
        .is_none());
    }

    #[test]
    fn a_reply_tab_fills_its_own_name_then_walks_recent_partners() {
        let mut e = line("/r");
        for expected in ["/t Alba, ", "/t Esk, ", "/t Sir Fenn, ", "/t Alba, "] {
            assert_eq!(tab(&mut e, false).unwrap(), expected);
        }
        for (text, expected) in [
            ("@rt", "@t Dain, "),
            ("/mr", "/t Bex, "),
            ("/pr", "/t Cato, "),
        ] {
            let mut e = ChatEntry {
                text: text.into(),
                ..ChatEntry::default()
            };
            assert_eq!(tab(&mut e, false).unwrap(), expected);
        }
        let mut e = ChatEntry {
            text: "/r".into(),
            ..ChatEntry::default()
        };
        let none = e.apply(
            8,
            "/r".into(),
            EntryAction::Complete {
                names: sources(),
                back: false,
            },
            &ReplyTargets::default(),
        );
        assert!(none.is_none());
        assert_eq!(e.text, "/r");
    }

    #[test]
    fn a_completed_tell_with_a_message_is_a_tell_to_that_player() {
        use crate::chat::{ChatState, TellOutcome};
        let mut e = ChatEntry {
            text: "/t Sir".into(),
            ..ChatEntry::default()
        };
        let typed = format!("{}well met", tab(&mut e, false).unwrap());
        let args = typed.strip_prefix("/t ").unwrap();
        assert_eq!(
            ChatState::new().do_tell(args),
            TellOutcome::ByName {
                message: "well met".into(),
                target_name: "Sir Fenn".into(),
            }
        );
    }

    #[test]
    fn percent_t_and_angle_t_become_the_selection() {
        for (line, out) in [
            ("/f heal %t please", "/f heal Gale please"),
            ("/t %T, hi", "/t Gale, hi"),
            ("<t> and <T>", "Gale and Gale"),
            ("100% sure, <tag> %", "100% sure, <tag> %"),
            ("%é<", "%é<"),
        ] {
            assert_eq!(substitute_target(line, Some("Gale")), out, "{line}");
        }
        assert_eq!(substitute_target("heal %t", None), "heal %t");
        assert_eq!(substitute_target("heal %t", Some("")), "heal %t");
    }

    #[test]
    fn a_tell_line_splits_into_its_person_and_message_and_back() {
        assert_eq!(
            split_tell("@tell Alba, hi there"),
            Some(("Alba".into(), "hi there".into()))
        );
        assert_eq!(
            split_tell("/t Sir Fenn,"),
            Some(("Sir Fenn".into(), String::new()))
        );
        assert_eq!(
            split_tell("/W +Alba,  two spaces"),
            Some(("Alba".into(), " two spaces".into()))
        );
        assert_eq!(split_tell("/t Alba"), None, "no comma yet");
        assert_eq!(split_tell("/t , hi"), None, "no name");
        assert_eq!(split_tell("/f Alba, hi"), None, "not a tell");
        assert_eq!(split_tell("hello, world"), None);
        let line = tell_line("Alba", "see you");
        assert_eq!(split_tell(&line), Some(("Alba".into(), "see you".into())));
        assert_eq!(
            split_tell(&tell_line("Alba", "")),
            Some(("Alba".into(), String::new()))
        );
    }

    #[test]
    fn a_reply_command_with_its_space_names_who_it_answers() {
        let t = targets();
        let (seed, rest) = reply_command("/r ").unwrap();
        assert_eq!((seed_name(seed, &t), rest), (Some("Alba"), ""));
        let (seed, rest) = reply_command("@rt thanks").unwrap();
        assert_eq!((seed_name(seed, &t), rest), (Some("Dain"), "thanks"));
        assert_eq!(seed_name(reply_command("/mr ").unwrap().0, &t), Some("Bex"));
        assert_eq!(
            seed_name(reply_command("/PR ").unwrap().0, &t),
            Some("Cato")
        );
        assert_eq!(reply_command("/r"), None, "no space yet");
        assert_eq!(reply_command("/rest "), None);
        let empty = ReplyTargets::default();
        assert_eq!(seed_name(SlotSeed::LastTeller, &empty), None);
    }

    #[test]
    fn a_tell_sent_with_only_its_name_names_the_person() {
        assert_eq!(bare_tell("/t Alba"), Some("Alba"));
        assert_eq!(bare_tell("@tell Sir Fenn, "), Some("Sir Fenn"));
        assert_eq!(bare_tell("/t Alba, hi"), None);
        assert_eq!(bare_tell("/t "), None);
        assert_eq!(bare_tell("/f Alba"), None);
    }
}
