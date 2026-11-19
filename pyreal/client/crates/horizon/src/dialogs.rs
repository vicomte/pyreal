//! The game's questions and messages in the Horizon interface's boxes.
//!
//! What each answer does is the shared dialog service's ([`pyreal_client_runtime::dialogs`]): it
//! asks, this keeps the boxes the interface draws and reports each answer back. A fellowship
//! invitation and an offer to swear allegiance are worded from the game's panel strings; the
//! house questions use the house panel's own sentences; an Advocate's offer says who offers what;
//! every other question carries its sentence.

use std::collections::BTreeMap;

use pyreal_client_contract::UiRequest;
use pyreal_client_runtime::dialogs::{Answer, DialogPresenter, Prompt, Question, QuestionId};

/// One box on screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shown {
    pub id: QuestionId,
    pub text: String,
    /// One OK rather than Yes and No.
    pub message: bool,
    /// It takes every press until it is answered.
    pub modal: bool,
    /// An Advocate's offer, under this title, answered Accept or Decline.
    pub offer: Option<String>,
}

/// The boxes, and the answers the interface has given them.
#[derive(Debug, Default)]
pub struct HorizonDialogs {
    /// Which screen the boxes were shown on: a change takes them all down unanswered.
    pub generation: u64,
    shown: Vec<Shown>,
    answers: BTreeMap<QuestionId, bool>,
    /// Answers the service handed back for a window to act on.
    pub delivered: Vec<UiRequest>,
}

impl HorizonDialogs {
    /// The boxes on screen, oldest first.
    #[must_use]
    pub fn shown(&self) -> &[Shown] {
        &self.shown
    }

    /// The box for question `id` was answered: `yes` is Yes, or OK.
    pub fn answer(&mut self, id: QuestionId, yes: bool) {
        if self.shown.iter().any(|s| s.id == id) {
            self.answers.insert(id, yes);
        }
    }
}

/// What a question's box says. `panel` words a panel string-table token for a player's name.
#[must_use]
pub fn prompt_text(prompt: &Prompt, panel: &mut dyn FnMut(&str, &str) -> String) -> String {
    use pyreal_ui_logic::panels::{allegiance, fellowship, slumlord};
    match prompt {
        Prompt::Text(text) | Prompt::HouseBuyAs(text) => text.clone(),
        Prompt::FellowshipRequest { name } => panel(fellowship::ID_FELLOWSHIP_REQUEST, name),
        Prompt::AcceptSwear { name } => panel(allegiance::ID_ACCEPT_SWEAR_CONFIRMATION, name),
        Prompt::HouseBuy => slumlord::BUY_CONFIRMATION.to_owned(),
        Prompt::HouseRentByProxy => slumlord::RENT_BY_PROXY_CONFIRMATION.to_owned(),
        Prompt::AdvocateWaypoint { from, label, place } => {
            if label.is_empty() || label == place {
                format!("{from} offers you a waypoint at {place}.")
            } else {
                format!("{from} offers you a waypoint to {label} at {place}.")
            }
        }
        Prompt::AdvocateNote { from, title } => {
            format!("{from} offers you a note for your journal called {title}.")
        }
        Prompt::AdvocatePages { from, count, to } => {
            format!("{from} offers you {count} journal pages with the way to {to}.")
        }
    }
}

/// The title of an Advocate's offer's box; `None` for any other question.
#[must_use]
pub fn offer_title(prompt: &Prompt) -> Option<&'static str> {
    match prompt {
        Prompt::AdvocateWaypoint { .. } => Some("Waypoint Offered"),
        Prompt::AdvocateNote { .. } => Some("Journal Note Offered"),
        Prompt::AdvocatePages { .. } => Some("Journal Pages Offered"),
        _ => None,
    }
}

/// The dialog service's view of the boxes for one pass.
pub struct Presenter<'a> {
    pub dialogs: &'a mut HorizonDialogs,
    /// Whether the game screen is up.
    pub accepting: bool,
    /// Words a panel string-table token for a player's name.
    pub panel: &'a mut dyn FnMut(&str, &str) -> String,
}

impl std::fmt::Debug for Presenter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presenter")
            .field("dialogs", &self.dialogs)
            .field("accepting", &self.accepting)
            .finish_non_exhaustive()
    }
}

impl DialogPresenter for Presenter<'_> {
    fn accepting(&self) -> bool {
        self.accepting
    }
    fn generation(&self) -> u64 {
        self.dialogs.generation
    }
    fn open(&mut self, question: &Question) -> bool {
        let text = prompt_text(&question.prompt, self.panel);
        self.dialogs.shown.push(Shown {
            id: question.id,
            text,
            message: question.message,
            modal: question.modal,
            offer: offer_title(&question.prompt).map(str::to_owned),
        });
        true
    }
    fn poll(&mut self, id: QuestionId) -> Answer {
        if let Some(yes) = self.dialogs.answers.remove(&id) {
            return Answer::Closed(Some(yes));
        }
        if self.dialogs.shown.iter().any(|s| s.id == id) {
            Answer::Waiting
        } else {
            Answer::Closed(None)
        }
    }
    fn close(&mut self, id: QuestionId) {
        self.dialogs.shown.retain(|s| s.id != id);
        self.dialogs.answers.remove(&id);
    }
    fn deliver(&mut self, request: UiRequest) {
        self.dialogs.delivered.push(request);
    }
    fn reset(&mut self) {
        self.dialogs.shown.clear();
        self.dialogs.answers.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(id: QuestionId, message: bool) -> Question {
        Question {
            id,
            prompt: Prompt::Text(format!("question {id}")),
            message,
            modal: false,
            all_at_once: false,
            subject: None,
            target: None,
        }
    }

    #[test]
    fn an_answer_reaches_the_question_its_box_was_shown_for_once() {
        let mut dialogs = HorizonDialogs::default();
        let mut panel = |token: &str, name: &str| format!("{token} {name}");
        let mut p = Presenter {
            dialogs: &mut dialogs,
            accepting: true,
            panel: &mut panel,
        };
        assert!(p.open(&question(7, false)));
        assert_eq!(p.poll(7), Answer::Waiting);
        p.dialogs.answer(7, true);
        p.dialogs.answer(8, true);
        assert_eq!(p.poll(7), Answer::Closed(Some(true)));
        p.close(7);
        assert_eq!(p.poll(7), Answer::Closed(None));
        assert!(p.dialogs.shown().is_empty());
        assert_eq!(p.poll(8), Answer::Closed(None));
    }

    #[test]
    fn an_invitation_is_worded_from_the_panel_strings_with_the_inviter_s_name() {
        let mut panel = |token: &str, name: &str| format!("[{token}] {name}");
        let text = prompt_text(
            &Prompt::FellowshipRequest {
                name: "Aurelia".into(),
            },
            &mut panel,
        );
        assert!(text.ends_with("] Aurelia"), "{text}");
        assert_eq!(
            prompt_text(&Prompt::Text("Sure?".into()), &mut panel),
            "Sure?"
        );
    }

    #[test]
    fn an_advocate_s_offer_says_who_offers_what_under_its_own_title() {
        let mut panel = |_: &str, n: &str| n.to_owned();
        let waypoint = Prompt::AdvocateWaypoint {
            from: "Advocate Tamsin".into(),
            label: "Holtburg square".into(),
            place: "42.1N 33.6E".into(),
        };
        assert_eq!(
            prompt_text(&waypoint, &mut panel),
            "Advocate Tamsin offers you a waypoint to Holtburg square at 42.1N 33.6E."
        );
        let note = Prompt::AdvocateNote {
            from: "Advocate Tamsin".into(),
            title: "Drudges".into(),
        };
        assert_eq!(
            prompt_text(&note, &mut panel),
            "Advocate Tamsin offers you a note for your journal called Drudges."
        );
        let mut dialogs = HorizonDialogs::default();
        let mut p = Presenter {
            dialogs: &mut dialogs,
            accepting: true,
            panel: &mut panel,
        };
        p.open(&Question {
            prompt: note,
            ..question(3, false)
        });
        p.open(&question(4, false));
        let titles: Vec<_> = dialogs.shown().iter().map(|s| s.offer.as_deref()).collect();
        assert_eq!(titles, [Some("Journal Note Offered"), None]);
    }

    #[test]
    fn a_new_screen_takes_every_box_down_unanswered() {
        let mut dialogs = HorizonDialogs::default();
        let mut panel = |_: &str, n: &str| n.to_owned();
        let mut p = Presenter {
            dialogs: &mut dialogs,
            accepting: true,
            panel: &mut panel,
        };
        p.open(&question(1, true));
        p.open(&question(2, false));
        p.dialogs.answer(1, true);
        p.reset();
        assert_eq!(p.poll(1), Answer::Closed(None));
        assert!(dialogs.shown().is_empty());
    }
}
