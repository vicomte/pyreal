//! One retained chat entry per logical window.

use pyreal_client_contract::chat::{
    entry::{EntryAction, EntryUpdate, ReplyTarget},
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
        }
        Some(EntryUpdate {
            window,
            cursor: cursor.unwrap_or_else(|| self.text.chars().count()),
            text: self.text.clone(),
            focus,
        })
    }
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
        }
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
}
