//! The questions the game asks the player, whatever UI shows them: which ones open, the
//! one-at-a-time slots, what an answer does and what the server's withdrawal does.
//!
//! The game's questions -- a use that needs a yes, a server's confirmation request, an invitation,
//! a swear, a house payment, `@die`, the vendor's unfinished basket, a pop-up message, a web link
//! clicked in chat, a journey that would leave the Training Academy -- arrive as queues on [`crate::interaction::Interaction`]. [`DialogService::service`](crate::dialogs::DialogService::service) turns them into
//! [`Question`](crate::dialogs::Question)s, runs each answer's rule when the player answers, and answers **No** on the way
//! out when the server withdraws a question it asked. What a question looks like, and how a UI
//! queues several of them, is the front end's: it implements [`DialogPresenter`](crate::dialogs::DialogPresenter).
//!
//! The order of [`DialogService::service`](crate::dialogs::DialogService::service) is the client's own, step for step, and it is one
//! function so that every front end gets it: questions that arrive while no world screen is up are
//! dropped (all but the pop-up strings, which wait); a new framework drops every open question
//! without answering it; pop-ups are shown before answers are read; answers run before the
//! server's withdrawals; and new questions open after both, so a question an answer raises (the
//! second `@house abandon` question) opens in the same pass.

use pyreal_client_contract::view::AllegianceAction;
use pyreal_client_contract::UiRequest;
use pyreal_client_model::inventory::targeted_use::TargetedUsageConfirmation;
use pyreal_primitives::{LocalTime, ObjectId, ServerTime};

use crate::interaction::Interaction;

/// A question's id, unique for the life of the service.
pub type QuestionId = u64;

/// What a question says. Most carry their sentence already composed; the ones whose wording is a
/// string-table row the UI resolves carry what goes into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// The sentence itself.
    Text(String),
    /// A fellowship invitation from `name`.
    FellowshipRequest { name: String },
    /// `name` asks to swear allegiance to the player.
    AcceptSwear { name: String },
    /// Buy the house the player is at.
    HouseBuy,
    /// Buy the house the player is at, asked in the world's own words.
    HouseBuyAs(String),
    /// Pay the house's rent on the owner's behalf.
    HouseRentByProxy,
    /// An Advocate, `from`, offers a waypoint labelled `label` at `place` (as the game writes one).
    AdvocateWaypoint {
        from: String,
        label: String,
        place: String,
    },
    /// An Advocate, `from`, offers a note titled `title` for the journal.
    AdvocateNote { from: String, title: String },
    /// An Advocate, `from`, offers `count` journal pages with the way to `to`.
    AdvocatePages {
        from: String,
        count: u32,
        to: String,
    },
}

/// A question as a UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    pub id: QuestionId,
    pub prompt: Prompt,
    /// A message with one button rather than a Yes/No question.
    pub message: bool,
    /// Blocks clicks elsewhere and comes to the front (the server's confirmation requests).
    pub modal: bool,
    /// Shown at once beside any other, rather than waiting its turn behind the open one.
    pub all_at_once: bool,
    /// The object the question is about, and the one it would be used on, where it has them.
    pub subject: Option<ObjectId>,
    pub target: Option<ObjectId>,
}

/// The plugin host's question to the player, apart from the game's: it never passes through the
/// [`DialogService`], whose questions come from the game and its requests.
pub use pyreal_client_contract::host_question::{HostQuestion, HostReply};

/// What a UI's box says about a question this frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// Not answered yet, or not on screen yet.
    Waiting,
    /// Closed: `Some(yes)` answered, `None` gone without an answer.
    Closed(Option<bool>),
    /// The front end no longer has it, and has nothing left to take down.
    Gone,
}

/// What the service asks of the front end that shows its questions.
pub trait DialogPresenter {
    /// Whether the front end can show questions now (the world screen is up). Questions that
    /// arrive while it cannot are dropped, except pop-up messages, which wait.
    fn accepting(&self) -> bool;
    /// Which framework is up: a change drops every open question without an answer.
    fn generation(&self) -> u64;
    /// Show `question`. `false` when the box could not be made.
    fn open(&mut self, question: &Question) -> bool;
    /// Whether `id` has been answered. A question the front end no longer knows is
    /// `Closed(None)`.
    fn poll(&mut self, id: QuestionId) -> Answer;
    /// Take the box for `id` down, after its answer has been acted on.
    fn close(&mut self, id: QuestionId);
    /// Hand an answer to the panel that owns the question (the house and mini-game panels).
    fn deliver(&mut self, request: UiRequest);
    /// A new framework is up: forget every box shown in the old one.
    fn reset(&mut self) {}
    /// Bring the boxes up to date once the pass is done.
    fn refresh(&mut self) {}
}

/// What an answer does.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Rule {
    /// Altar and volatile-rare uses: a yes performs the use.
    Usage(ObjectId),
    /// Mana stone and salvage: a yes performs the targeted use.
    TargetedUse(ObjectId, ObjectId),
    /// The vendor's unfinished-basket question: a yes closes the vendor.
    CloseVendor,
    /// `@house abandon`'s first question: a yes raises the second and sends nothing.
    HouseAbandonFirst,
    /// The second: a yes abandons the house.
    HouseAbandonSecond,
    /// The server's own confirmation request, answered either way.
    ServerConfirmation {
        confirmation_type: i32,
        context_id: u32,
    },
    /// An Advocate's offer, the server's Yes/No question in its own slot: answered either way.
    AdvocateOffer {
        confirmation_type: i32,
        context_id: u32,
    },
    /// The allegiance panel's swear, break and kick: a yes acts.
    Allegiance {
        action: AllegianceAction,
        target: ObjectId,
    },
    /// A patron asked to accept a swear: answered either way.
    AcceptSwear { context_id: u32 },
    /// A fellowship invitation: answered either way.
    FellowshipRequest { context_id: u32 },
    /// `@die`: a yes dies.
    Die,
    /// `@lifestone` where it is a death: a yes sends the lifestone event, which kills.
    LifestoneDeath,
    /// The mini-game's resign question: answered either way to the panel.
    MiniGameQuit,
    /// The house panel's buy or rent question: answered either way to the panel.
    HousePayment { rent: bool },
    /// A pop-up message: nothing to answer.
    PopUp,
    /// A web link clicked in chat: a yes opens it in the browser.
    OpenLink(String),
    /// The travel's question (a journey that would hand a token back): answered either way to
    /// the travel.
    Travel,
}

/// The questions the game has asked and the player has not answered.
#[derive(Debug, Default)]
pub struct DialogService {
    next: QuestionId,
    generation: Option<u64>,
    open: Vec<(QuestionId, Rule)>,
    /// Advocate offers waiting behind the one on screen, oldest first:
    /// `(type, context, prompt)`.
    offers: std::collections::VecDeque<(i32, u32, Prompt)>,
}

impl DialogService {
    /// Drop every open question without answering it: the front end has nowhere to show them.
    /// The mini-game's resign question is withdrawn from the game, so a new one can be asked.
    pub fn drop_all(&mut self, world: &mut pyreal_client_model::World) {
        if self.open.iter().any(|(_, r)| *r == Rule::MiniGameQuit) {
            world.minigame.cancel_resign_dialog();
        }
        self.open.clear();
        self.offers.clear();
        self.generation = None;
    }

    fn ask(
        &mut self,
        ui: &mut dyn DialogPresenter,
        rule: Rule,
        prompt: Prompt,
        shape: (bool, bool, bool),
        objects: (Option<ObjectId>, Option<ObjectId>),
    ) -> bool {
        self.next += 1;
        let q = Question {
            id: self.next,
            prompt,
            message: shape.0,
            modal: shape.1,
            all_at_once: shape.2,
            subject: objects.0,
            target: objects.1,
        };
        if ui.open(&q) {
            self.open.push((q.id, rule));
            true
        } else {
            false
        }
    }

    /// Empty the queues `service` reads once the world screen is up.
    fn drop_queued(interaction: &mut Interaction) {
        let _ = interaction.take_usage_confirmations();
        let _ = interaction.take_targeted_confirmations();
        let _ = interaction.take_vendor_close_confirmations();
        let _ = interaction.take_house_payment_confirmations();
        let _ = interaction.take_server_confirmations();
        let _ = interaction.take_confirmation_aborts();
        let _ = interaction.take_fellowship_requests();
    }

    fn has(&self, f: impl Fn(&Rule) -> bool) -> bool {
        self.open.iter().any(|(_, r)| f(r))
    }

    /// One pass: take what the game asked, read the answers, act on them and on the server's
    /// withdrawals, and open the new questions. Retail runs it several times a frame, at each
    /// point its own UI delivers input; every pass is complete on its own.
    #[allow(clippy::too_many_lines)]
    pub fn service(
        &mut self,
        ui: Option<&mut dyn DialogPresenter>,
        interaction: &mut Interaction,
        world: &mut pyreal_client_model::World,
        now: LocalTime,
    ) {
        let travel = interaction.take_travel_questions();
        let Some(ui) = ui else {
            // No subscriber, and no retained notice history. A journey's question nobody can
            // see is a no: the journey ends rather than waiting for ever.
            if !travel.is_empty() || self.has(|r| *r == Rule::Travel) {
                interaction.answer_travel(false);
            }
            Self::drop_queued(interaction);
            self.drop_all(world);
            return;
        };
        if self.generation != Some(ui.generation()) {
            // The flow already took the boxes down. An unanswered question has no answer; do not
            // manufacture a No or replay a stale Yes into the new framework.
            if self.has(|r| *r == Rule::MiniGameQuit) {
                world.minigame.cancel_resign_dialog();
            }
            // Nor does a journey's: it is a no, so the journey ends rather than waits for ever.
            if self.has(|r| *r == Rule::Travel) {
                interaction.answer_travel(false);
            }
            self.open.clear();
            self.offers.clear();
            ui.reset();
            self.generation = Some(ui.generation());
        }
        if !ui.accepting() {
            if !travel.is_empty() {
                interaction.answer_travel(false);
            }
            return;
        }
        // Taken only once the world screen is up, so a question that arrives earlier waits.
        let usage = interaction.take_usage_confirmations();
        let notices = interaction.take_targeted_confirmations();
        let closes = interaction.take_vendor_close_confirmations();
        let house_payments = interaction.take_house_payment_confirmations();
        let mut requests = interaction.take_server_confirmations();
        let aborts = interaction.take_confirmation_aborts();
        let mut fellowship = interaction.take_fellowship_requests();
        // **The pop-up strings**, which every log-in opens: one-button boxes, all on screen at
        // once, with nothing to answer. Taken only once the world screen is up, so one that
        // arrives earlier waits rather than being dropped.
        for text in interaction.take_pop_up_strings() {
            if self.ask(
                &mut *ui,
                Rule::PopUp,
                Prompt::Text(text),
                (true, false, true),
                (None, None),
            ) {
                interaction.stats.pop_ups_shown += 1;
            }
        }
        for (id, rule) in self.open.clone() {
            if rule != Rule::PopUp {
                continue;
            }
            match ui.poll(id) {
                Answer::Waiting => {}
                Answer::Gone => self.open.retain(|(q, _)| *q != id),
                Answer::Closed(_) => {
                    ui.close(id);
                    self.open.retain(|(q, _)| *q != id);
                    interaction.stats.pop_ups_dismissed += 1;
                }
            }
        }
        // The answers, each to its own rule.
        for (id, rule) in self.open.clone() {
            if rule == Rule::PopUp {
                continue;
            }
            let answered = match ui.poll(id) {
                Answer::Waiting => continue,
                Answer::Gone => None,
                Answer::Closed(answered) => answered,
            };
            self.act(&mut *ui, rule, answered, interaction, world, now);
            ui.close(id);
            self.open.retain(|(q, _)| *q != id);
        }
        // **The server's withdrawal.** It closes the question while the slot is still set, so
        // the close answers **No** on the way out: a server that withdraws one of its questions
        // hears a refusal. A fellowship withdrawal matches on the type alone, any context.
        for (confirmation_type, context_id) in aborts {
            let found = self.open.iter().position(|(_, r)| match *r {
                Rule::FellowshipRequest { .. } => confirmation_type == 4,
                Rule::ServerConfirmation {
                    confirmation_type: t,
                    context_id: c,
                } => confirmation_type != 4 && t == confirmation_type && c == context_id,
                Rule::AcceptSwear { context_id: c } => confirmation_type == 1 && c == context_id,
                Rule::AdvocateOffer {
                    confirmation_type: t,
                    context_id: c,
                } => t == confirmation_type && c == context_id,
                _ => false,
            });
            let Some(at) = found else {
                // A question that waited for the world screen and is withdrawn before it shows
                // is never shown, and the server hears the same No.
                let waiting = if confirmation_type == 4 {
                    fellowship.first().map(|(c, _)| *c).inspect(|_| {
                        fellowship.remove(0);
                    })
                } else if let Some(at) = self
                    .offers
                    .iter()
                    .position(|(t, c, _)| *t == confirmation_type && *c == context_id)
                {
                    self.offers.remove(at).map(|(_, c, _)| c)
                } else {
                    requests
                        .iter()
                        .position(|(t, c, _)| *t == confirmation_type && *c == context_id)
                        .map(|at| requests.remove(at).1)
                };
                match waiting {
                    Some(c) => {
                        interaction.confirm_server_confirmation(confirmation_type, c, false);
                        interaction.stats.confirmations_aborted += 1;
                    }
                    None => interaction.stats.confirmations_aborts_unmatched += 1,
                }
                continue;
            };
            let (id, rule) = self.open[at].clone();
            // The context answered with is the slot's, not the withdrawal's.
            let answered_context = match rule {
                Rule::FellowshipRequest { context_id } => context_id,
                _ => context_id,
            };
            interaction.confirm_server_confirmation(confirmation_type, answered_context, false);
            interaction.stats.confirmations_aborted += 1;
            ui.close(id);
            self.open.retain(|(q, _)| *q != id);
        }
        // **The mini-game's resign question**: all at once, its own slot on the game.
        if world.minigame.resign_prompt_pending {
            world.minigame.resign_prompt_pending = false;
            if self.ask(
                &mut *ui,
                Rule::MiniGameQuit,
                Prompt::Text(pyreal_client_model::minigame::RESIGN_PROMPT.to_owned()),
                (false, false, true),
                (None, None),
            ) {
                world.minigame.resign_dialog = u32::try_from(self.next).unwrap_or(u32::MAX);
            } else {
                world.minigame.cancel_resign_dialog();
            }
        }
        // **The house panel's two questions**, a slot each.
        for rent in house_payments {
            if self.has(|r| *r == Rule::HousePayment { rent }) {
                continue;
            }
            let prompt = if rent {
                Prompt::HouseRentByProxy
            } else if let Some(worded) = world
                .world_rules
                .text(pyreal_primitives::TextKey::HouseBuyConfirmation)
            {
                Prompt::HouseBuyAs(worded.to_owned())
            } else {
                Prompt::HouseBuy
            };
            if !self.ask(
                &mut *ui,
                Rule::HousePayment { rent },
                prompt,
                (false, false, false),
                (None, None),
            ) {
                // The panel set its slot before asking; a box that could not be made releases it.
                ui.deliver(UiRequest::HousePaymentConfirmationAnswer {
                    rent,
                    confirmed: None,
                });
            }
        }
        // **The server's confirmation requests.** The pair is stored before the one-at-a-time
        // guard, so a second question while one is up takes over the open box's pair: nothing
        // new is shown, and answering the box answers the second question. Retail's own, kept.
        for (confirmation_type, context_id, text) in requests {
            // An Advocate's offer, from a world with the extension, waits its turn in its own slot.
            if let Some(prompt) = (confirmation_type == YES_NO
                && interaction.has_extension(pyreal_protocol::extensions::ADVOCATE_OFFERS))
            .then(|| advocate_prompt(&text))
            .flatten()
            {
                self.offers
                    .push_back((confirmation_type, context_id, prompt));
                continue;
            }
            if let Some((_, slot)) = self
                .open
                .iter_mut()
                .find(|(_, r)| matches!(r, Rule::ServerConfirmation { .. }))
            {
                *slot = Rule::ServerConfirmation {
                    confirmation_type,
                    context_id,
                };
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::ServerConfirmation {
                    confirmation_type,
                    context_id,
                },
                Prompt::Text(prompt_for(confirmation_type, &text)),
                (false, true, false),
                (None, None),
            );
        }
        // **An Advocate's offer**: one at a time in its own slot, not modal, the rest waiting.
        if !self.has(|r| matches!(r, Rule::AdvocateOffer { .. })) {
            if let Some((confirmation_type, context_id, prompt)) = self.offers.pop_front() {
                self.ask(
                    &mut *ui,
                    Rule::AdvocateOffer {
                        confirmation_type,
                        context_id,
                    },
                    prompt,
                    (false, false, true),
                    (None, None),
                );
            }
        }
        // **The fellowship invitation**: its own slot, not modal, so it can sit beside the rest.
        for (context_id, name) in fellowship {
            if self.has(|r| matches!(r, Rule::FellowshipRequest { .. })) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::FellowshipRequest { context_id },
                Prompt::FellowshipRequest { name },
                (false, false, false),
                (None, None),
            );
        }
        // **The three ordinary-use confirmations**: altar, altar, volatile rare. No guard; a
        // second waits behind the first.
        for (object, kind) in usage {
            self.ask(
                &mut *ui,
                Rule::Usage(object),
                Prompt::Text(kind.prompt().to_string()),
                (false, false, false),
                (Some(object), None),
            );
        }
        for (source, target, kind) in notices {
            let (Some(src), Some(tgt)) = (world.weenie(source), world.weenie(target)) else {
                continue;
            };
            // These callers use raw names, not the object-name lookup.
            let prompt = match kind {
                TargetedUsageConfirmation::ManaStone => format!(
                    "\nAre you sure you want to attempt to destroy your {} and drain its mana into this stone?",
                    tgt.pwd.name
                ),
                TargetedUsageConfirmation::Salvage => format!(
                    "\nAre you sure you want to apply the {} to the {}? The {} may be destroyed.",
                    src.pwd.name.replace(" (100)", ""),
                    tgt.pwd.name,
                    tgt.pwd.name
                ),
            };
            self.ask(
                &mut *ui,
                Rule::TargetedUse(source, target),
                Prompt::Text(prompt),
                (false, false, false),
                (Some(source), Some(target)),
            );
        }
        // **A swear request** asks the patron to accept: its own slot.
        for (context_id, name) in interaction.take_swear_requests() {
            if self.has(|r| matches!(r, Rule::AcceptSwear { .. })) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::AcceptSwear { context_id },
                Prompt::AcceptSwear { name },
                (false, false, false),
                (None, None),
            );
        }
        // **The allegiance panel's swear, break and kick**: a slot each.
        for (action, target, prompt) in interaction.take_allegiance_confirmations() {
            if self.has(|r| matches!(r, Rule::Allegiance { action: a, .. } if *a == action)) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::Allegiance { action, target },
                Prompt::Text(prompt),
                (false, false, false),
                (None, None),
            );
        }
        // **`@die`** keeps no slot: a second `@die` asks again, behind the first.
        for prompt in interaction.take_die_confirmations() {
            self.ask(
                &mut *ui,
                Rule::Die,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        // **`@lifestone`** where it kills asks the same way, its Yes sending the lifestone event.
        for prompt in interaction.take_lifestone_death_confirmations() {
            self.ask(
                &mut *ui,
                Rule::LifestoneDeath,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        // **`@house abandon`**'s two stages keep no slot either.
        for prompt in interaction.take_house_abandon_first() {
            self.ask(
                &mut *ui,
                Rule::HouseAbandonFirst,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        for prompt in interaction.take_house_abandon_second() {
            self.ask(
                &mut *ui,
                Rule::HouseAbandonSecond,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        // **A web link clicked in chat**: a question of its own, which a yes opens. A link
        // already being asked about is not asked about again.
        for url in interaction.take_link_confirmations() {
            if self.has(|r| matches!(r, Rule::OpenLink(open) if *open == url)) {
                continue;
            }
            let prompt = pyreal_client_contract::chat::links::confirm_prompt(&url);
            self.ask(
                &mut *ui,
                Rule::OpenLink(url),
                Prompt::Text(prompt),
                (false, false, false),
                (None, None),
            );
        }
        // **A journey's question**: one at a time; a second while one is up waits on its answer.
        for question in travel {
            if self.has(|r| *r == Rule::Travel) {
                continue;
            }
            if !self.ask(
                &mut *ui,
                Rule::Travel,
                Prompt::Text(question),
                (false, false, false),
                (None, None),
            ) {
                interaction.answer_travel(false);
            }
        }
        // **The vendor's unfinished basket**: one at a time.
        for prompt in closes {
            if self.has(|r| *r == Rule::CloseVendor) {
                continue;
            }
            self.ask(
                &mut *ui,
                Rule::CloseVendor,
                Prompt::Text(prompt.to_string()),
                (false, false, false),
                (None, None),
            );
        }
        ui.refresh();
    }

    /// What an answer does, by the question's rule.
    fn act(
        &mut self,
        ui: &mut dyn DialogPresenter,
        rule: Rule,
        answered: Option<bool>,
        interaction: &mut Interaction,
        world: &mut pyreal_client_model::World,
        now: LocalTime,
    ) {
        let yes = answered == Some(true);
        match rule {
            Rule::Usage(object) if yes => {
                interaction.confirm_usage(world, object, ServerTime(now.0));
            }
            Rule::TargetedUse(source, target) if yes => {
                interaction.confirm_targeted_usage(world, source, target, ServerTime(now.0));
            }
            Rule::CloseVendor if yes => interaction.confirm_vendor_close(world),
            // No branch on the answer: a refusal is a `0x0275` with `accepted = 0`, so the server
            // learns of it at once. A box that died with its framework has no answer, and sends
            // nothing.
            Rule::ServerConfirmation {
                confirmation_type,
                context_id,
            }
            | Rule::AdvocateOffer {
                confirmation_type,
                context_id,
            } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(
                        confirmation_type,
                        context_id,
                        accepted,
                    );
                }
            }
            Rule::Allegiance { action, target } if yes => {
                interaction.confirm_allegiance(world, action, target);
            }
            Rule::Die if yes => interaction.confirm_die(world),
            Rule::LifestoneDeath if yes => interaction.confirm_lifestone_death(world),
            Rule::MiniGameQuit => match answered {
                Some(confirmed) => ui.deliver(UiRequest::MiniGameQuitAnswer(confirmed)),
                None => world.minigame.cancel_resign_dialog(),
            },
            Rule::HousePayment { rent } => ui.deliver(UiRequest::HousePaymentConfirmationAnswer {
                rent,
                confirmed: answered,
            }),
            Rule::HouseAbandonFirst if yes => interaction.confirm_house_abandon_first(),
            Rule::HouseAbandonSecond if yes => interaction.confirm_house_abandon(world),
            Rule::OpenLink(url) if yes => interaction.open_link(world, &url),
            Rule::AcceptSwear { context_id } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(1, context_id, accepted);
                }
            }
            Rule::FellowshipRequest { context_id } => {
                if let Some(accepted) = answered {
                    interaction.confirm_server_confirmation(4, context_id, accepted);
                }
            }
            // A box gone without an answer is a no: the journey ends rather than waits.
            Rule::Travel => interaction.answer_travel(yes),
            Rule::Usage(_)
            | Rule::TargetedUse(..)
            | Rule::CloseVendor
            | Rule::Allegiance { .. }
            | Rule::Die
            | Rule::LifestoneDeath
            | Rule::HouseAbandonFirst
            | Rule::HouseAbandonSecond
            | Rule::OpenLink(_)
            | Rule::PopUp => {}
        }
    }
}

/// `ConfirmationType.Yes_No`, the type a world files an Advocate's offer under.
const YES_NO: i32 = 7;

/// The prompt for an Advocate's offer, when `text` is the world's question making one.
#[must_use]
pub fn advocate_prompt(text: &str) -> Option<Prompt> {
    use pyreal_client_model::dereth_extensions::{parse_offer, Offered};
    let offer = parse_offer(text)?;
    Some(match offer.offered {
        Offered::Waypoint { label, ns, ew } => Prompt::AdvocateWaypoint {
            from: offer.from,
            label,
            place: pyreal_client_model::cmd::waypoint::place_text(ns, ew),
        },
        Offered::JournalNote { title } => Prompt::AdvocateNote {
            from: offer.from,
            title,
        },
        Offered::JournalPages { count, to } => Prompt::AdvocatePages {
            from: offer.from,
            count,
            to,
        },
    })
}

/// What the box says for a server confirmation, by its type: four of the five append the same
/// `" Continue?"` to the server's text, and the plain Yes/No request (type 7) is the server's text
/// verbatim. That is why an NPC's question reads as the NPC wrote it and a gem's warning gets a
/// question mark bolted on.
#[must_use]
pub fn prompt_for(confirmation_type: i32, text: &str) -> String {
    match confirmation_type {
        7 => text.to_string(),
        _ => format!("{text} Continue?"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use pyreal_client_contract::UiRequest;
    use pyreal_primitives::{LocalTime, ServerTime};

    use super::*;
    use crate::app::{record_shell_calls, record_shell_calls_answering, recorded_launches};

    /// A front end that shows whatever it is asked and answers what the test tells it to.
    #[derive(Debug, Default)]
    struct Boxes {
        shown: Vec<Question>,
        answers: BTreeMap<QuestionId, Option<bool>>,
        /// The game screen is not up yet.
        entering: bool,
    }

    impl DialogPresenter for Boxes {
        fn accepting(&self) -> bool {
            !self.entering
        }
        fn generation(&self) -> u64 {
            1
        }
        fn open(&mut self, question: &Question) -> bool {
            self.shown.push(question.clone());
            true
        }
        fn poll(&mut self, id: QuestionId) -> Answer {
            self.answers
                .get(&id)
                .map_or(Answer::Waiting, |a| Answer::Closed(*a))
        }
        fn close(&mut self, id: QuestionId) {
            self.shown.retain(|q| q.id != id);
            self.answers.remove(&id);
        }
        fn deliver(&mut self, _request: UiRequest) {}
    }

    /// A game where the player has clicked each of `urls` in chat, its dialog service, and the
    /// boxes after one pass of it.
    fn clicked(
        urls: &[&str],
    ) -> (
        Interaction,
        pyreal_client_model::World,
        DialogService,
        Boxes,
    ) {
        let mut inter = Interaction::new();
        let mut world = pyreal_client_model::World::new();
        inter.queue(
            vec![],
            urls.iter()
                .map(|u| UiRequest::ChatLinkClicked((*u).to_owned()))
                .collect(),
        );
        assert!(inter
            .run_ui_requests(&mut world, false, ServerTime(1.0))
            .is_empty());
        let mut service = DialogService::default();
        let mut boxes = Boxes::default();
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(1.0));
        (inter, world, service, boxes)
    }

    /// Answer the one box `answer` and run the service once more.
    fn answer(
        (inter, world, service, boxes): &mut (
            Interaction,
            pyreal_client_model::World,
            DialogService,
            Boxes,
        ),
        answer: Option<bool>,
    ) {
        let id = boxes.shown[0].id;
        boxes.answers.insert(id, answer);
        service.service(Some(boxes), inter, world, LocalTime(2.0));
    }

    /// `opcode`'s body `m` as the session hands it over.
    fn ui_event<M: pyreal_protocol::Message>(
        m: &M,
    ) -> pyreal_client_net::client_session::SessionEvent {
        let blob = pyreal_protocol::write_blob(m).expect("encodes");
        pyreal_client_net::client_session::SessionEvent::UiEvent {
            opcode: M::OPCODE,
            blob,
        }
    }

    /// A Yes/No question from the world, as the server words an Advocate's offer.
    fn yes_no(context_id: u32, text: &str) -> pyreal_client_net::client_session::SessionEvent {
        ui_event(&pyreal_protocol::comms::CharacterConfirmationRequest {
            confirmation_type: 7,
            context_id,
            text: text.into(),
        })
    }

    const WAYPOINT_OFFER: &str =
        "Advocate Tamsin offers you a waypoint: Holtburg square (42.1N 33.6E). Add it?";
    const NOTE_OFFER: &str = "Advocate Tamsin offers a note for your journal: \"Drudges\". Add it?";

    fn answers(inter: &mut Interaction) -> Vec<(i32, u32, i32)> {
        inter
            .take_pending_requests()
            .into_iter()
            .filter_map(|r| match r {
                pyreal_client_model::Request::ConfirmationResponse(c) => {
                    Some((c.confirmation_type, c.context_id, c.accepted))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn advocate_offers_queue_in_their_own_slot_and_each_answer_reaches_its_question() {
        let mut inter = Interaction::new();
        inter.set_extensions(vec![pyreal_protocol::extensions::ADVOCATE_OFFERS.to_owned()]);
        let mut world = pyreal_client_model::World::new();
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &[yes_no(1, WAYPOINT_OFFER), yes_no(2, NOTE_OFFER)],
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        let mut service = DialogService::default();
        let mut boxes = Boxes::default();
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(1.0));
        assert_eq!(boxes.shown.len(), 1, "one offer at a time, the other waits");
        let first = boxes.shown[0].clone();
        assert_eq!(
            first.prompt,
            Prompt::AdvocateWaypoint {
                from: "Advocate Tamsin".into(),
                label: "Holtburg square".into(),
                place: "42.1N 33.6E".into(),
            }
        );
        assert!(!first.modal && first.all_at_once);

        boxes.answers.insert(first.id, Some(true));
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(2.0));
        assert_eq!(answers(&mut inter), [(7, 1, 1)], "Accept says yes");
        assert_eq!(boxes.shown.len(), 1, "the next offer comes up");
        assert_eq!(
            boxes.shown[0].prompt,
            Prompt::AdvocateNote {
                from: "Advocate Tamsin".into(),
                title: "Drudges".into(),
            }
        );
        boxes.answers.insert(boxes.shown[0].id, Some(false));
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(3.0));
        assert_eq!(answers(&mut inter), [(7, 2, 0)], "Decline says no");
        assert!(boxes.shown.is_empty());
    }

    #[test]
    fn an_expired_offer_closes_and_one_still_waiting_is_never_shown() {
        let mut inter = Interaction::new();
        inter.set_extensions(vec![pyreal_protocol::extensions::ADVOCATE_OFFERS.to_owned()]);
        let mut world = pyreal_client_model::World::new();
        let mut service = DialogService::default();
        let mut boxes = Boxes::default();
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &[yes_no(1, WAYPOINT_OFFER), yes_no(2, NOTE_OFFER)],
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(1.0));
        let done = |context_id| {
            ui_event(&pyreal_protocol::comms::CharacterConfirmationDone {
                confirmation_type: 7,
                context_id,
            })
        };
        // The waiting one expires first, then the one on screen.
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &[done(2), done(1)],
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(31.0));
        assert!(boxes.shown.is_empty(), "both gone");
        assert_eq!(
            answers(&mut inter),
            [(7, 2, 0), (7, 1, 0)],
            "the world hears no for each, which releases its question"
        );
    }

    #[test]
    fn without_the_extension_an_offer_is_the_game_s_own_question() {
        let mut inter = Interaction::new();
        let mut world = pyreal_client_model::World::new();
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &[yes_no(1, WAYPOINT_OFFER)],
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        let mut service = DialogService::default();
        let mut boxes = Boxes::default();
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(1.0));
        assert_eq!(boxes.shown[0].prompt, Prompt::Text(WAYPOINT_OFFER.into()));
        assert!(boxes.shown[0].modal);
    }

    #[test]
    fn accepted_offers_set_the_waypoint_and_add_the_note_and_the_fellows_list_is_kept() {
        use pyreal_protocol::extensions::{
            AiFellow, AiFellows, JournalEntry, Waypoint, ADVOCATE_OFFERS, AI_FELLOWS,
        };
        let fellows = AiFellows {
            fellows: vec![AiFellow {
                guid: 0x5000_0002,
                name: "~Aerin".into(),
                mode: "heal".into(),
                in_fellowship: true,
            }],
        };
        let waypoint = Waypoint {
            from: 0x5000_0003,
            from_name: "Advocate Tamsin".into(),
            label: "Holtburg square".into(),
            cell: 0xA9B4_0031,
            x: 84.0,
            y: 7.5,
            z: 94.0,
        };
        let note = JournalEntry {
            from: 0x5000_0003,
            from_name: "Advocate Tamsin".into(),
            title: "Drudges".into(),
            text: "South of town.".into(),
            location: None,
        };
        let events = [ui_event(&fellows), ui_event(&waypoint), ui_event(&note)];

        // A world that never agreed to them: nothing is taken.
        let mut inter = Interaction::new();
        let mut world = pyreal_client_model::World::new();
        world.journal.set_identity(None);
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &events,
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        assert!(world.ai_fellows.is_empty());
        assert!(inter.take_waypoint_commands().is_empty());
        assert_eq!(world.journal.view().draft.title, "");

        inter.set_extensions(vec![AI_FELLOWS.to_owned(), ADVOCATE_OFFERS.to_owned()]);
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &events,
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        assert_eq!(world.ai_fellows, fellows.fellows);
        let set = inter.take_waypoint_commands();
        let [(pyreal_client_model::cmd::waypoint::WaypointCommand::Set { ns, ew, name }, _)] =
            set.as_slice()
        else {
            panic!("one waypoint set: {set:?}");
        };
        assert_eq!(name.as_deref(), Some("Holtburg square"));
        assert!(
            (ns - 42.1).abs() < 0.05 && (ew - 33.6).abs() < 0.05,
            "{ns} {ew}"
        );
        let page = world.journal.view().draft;
        assert_eq!(
            (
                page.label.as_str(),
                page.from.as_str(),
                page.title.as_str(),
                page.notes.as_str()
            ),
            ("Advocate", "Advocate Tamsin", "Drudges", "South of town.")
        );
    }

    #[test]
    fn a_question_that_arrives_before_the_game_screen_waits_for_it() {
        let mut inter = Interaction::new();
        let mut world = pyreal_client_model::World::new();
        let m = pyreal_protocol::comms::CharacterConfirmationRequest {
            confirmation_type: 5,
            context_id: 9,
            text: "Craft?".into(),
        };
        let opcode = pyreal_protocol::Opcode::CHARACTER_CONFIRMATION_REQUEST;
        let mut blob = opcode.0.to_le_bytes().to_vec();
        blob.extend(pyreal_protocol::write_body(&m).expect("encodes"));
        let event = pyreal_client_net::client_session::SessionEvent::UiEvent { opcode, blob };
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &[event],
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        let mut service = DialogService::default();
        let mut boxes = Boxes {
            entering: true,
            ..Boxes::default()
        };
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(1.0));
        assert!(
            boxes.shown.is_empty(),
            "nothing shows before the game screen"
        );
        boxes.entering = false;
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(2.0));
        assert_eq!(boxes.shown.len(), 1, "the question waited");

        // Withdrawn before the screen is up: never shown, and answered No.
        let done = pyreal_protocol::comms::CharacterConfirmationDone {
            confirmation_type: 5,
            context_id: 9,
        };
        let opcode = pyreal_protocol::Opcode::CHARACTER_CONFIRMATION_DONE;
        let mut inter = Interaction::new();
        let mut events = Vec::new();
        for (op, body) in [
            (
                pyreal_protocol::Opcode::CHARACTER_CONFIRMATION_REQUEST,
                pyreal_protocol::write_body(&m),
            ),
            (opcode, pyreal_protocol::write_body(&done)),
        ] {
            let mut blob = op.0.to_le_bytes().to_vec();
            blob.extend(body.expect("encodes"));
            events.push(pyreal_client_net::client_session::SessionEvent::UiEvent {
                opcode: op,
                blob,
            });
        }
        crate::interaction::apply_events_at_boundary(
            &mut inter,
            &events,
            &mut world,
            None,
            &mut |_, _, _| {},
        );
        let mut service = DialogService::default();
        let mut boxes = Boxes {
            entering: true,
            ..Boxes::default()
        };
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(3.0));
        boxes.entering = false;
        service.service(Some(&mut boxes), &mut inter, &mut world, LocalTime(4.0));
        assert!(boxes.shown.is_empty(), "a withdrawn question is not shown");
        assert_eq!(inter.stats.confirmations_aborted, 1, "the server hears No");
    }

    #[test]
    fn a_clicked_link_is_asked_about_and_a_yes_opens_exactly_that_address() {
        record_shell_calls();
        const URL: &str = "https://example.com/a?b=c#d";
        let mut game = clicked(&[URL]);
        let boxes = &game.3;
        assert_eq!(boxes.shown.len(), 1, "one question");
        assert_eq!(
            boxes.shown[0].prompt,
            Prompt::Text(format!("Open {URL} in your browser?"))
        );
        assert!(!boxes.shown[0].message, "a yes-or-no question");
        // Nothing opens while the question waits.
        let (inter, world, service, boxes) = &mut game;
        service.service(Some(boxes), inter, world, LocalTime(1.5));
        assert!(recorded_launches().is_empty());
        answer(&mut game, Some(true));
        assert_eq!(recorded_launches(), [URL], "the address, whole, once");
        assert!(game.3.shown.is_empty(), "the box is taken down");
        assert!(game.1.scroll.pending().is_empty(), "nothing to say");
    }

    #[test]
    fn a_no_or_a_box_gone_unanswered_opens_nothing() {
        for refusal in [Some(false), None] {
            record_shell_calls();
            let mut game = clicked(&["https://example.com/"]);
            answer(&mut game, refusal);
            assert!(recorded_launches().is_empty(), "{refusal:?}");
            assert!(game.3.shown.is_empty());
        }
    }

    #[test]
    fn only_a_whole_web_address_is_asked_about() {
        record_shell_calls();
        let (_, _, _, boxes) = clicked(&[
            "javascript:alert(1)",
            "file:///c:/windows/system32/calc.exe",
            "https://example.com/a and more",
            "www.example.com",
            "",
        ]);
        assert!(boxes.shown.is_empty(), "{:?}", boxes.shown);
        assert!(recorded_launches().is_empty());
    }

    #[test]
    fn a_link_clicked_again_while_it_is_asked_about_is_not_asked_twice() {
        let (_, _, _, boxes) = clicked(&[
            "https://example.com/a",
            "https://example.com/a",
            "https://example.com/b",
        ]);
        let asked: Vec<&Prompt> = boxes.shown.iter().map(|q| &q.prompt).collect();
        assert_eq!(
            asked,
            [
                &Prompt::Text("Open https://example.com/a in your browser?".into()),
                &Prompt::Text("Open https://example.com/b in your browser?".into()),
            ]
        );
    }

    #[test]
    fn a_browser_that_will_not_open_is_said_so_with_the_address() {
        record_shell_calls_answering(0);
        let mut game = clicked(&["https://example.com/x"]);
        answer(&mut game, Some(true));
        assert_eq!(recorded_launches(), ["https://example.com/x"]);
        let said: Vec<&str> = game
            .1
            .scroll
            .pending()
            .iter()
            .map(|l| l.body.as_str())
            .collect();
        assert_eq!(
            said,
            ["Your browser could not be opened for https://example.com/x"]
        );
    }

    #[test]
    fn a_journeys_question_is_asked_in_a_box_and_its_answer_or_its_loss_goes_back_to_the_journey() {
        const LEAVE: &str = "Leave the Training Academy? You can never come back.";
        let ask = || {
            let mut inter = Interaction::new();
            inter.ask_travel(String::from(LEAVE));
            (
                inter,
                pyreal_client_model::World::new(),
                DialogService::default(),
                Boxes::default(),
            )
        };
        // Asked in a box of its own, a yes-or-no question; nothing answered while it waits.
        let mut game = ask();
        {
            let (inter, world, service, boxes) = &mut game;
            service.service(Some(boxes), inter, world, LocalTime(1.0));
            assert_eq!(boxes.shown.len(), 1);
            assert_eq!(boxes.shown[0].prompt, Prompt::Text(LEAVE.into()));
            assert!(!boxes.shown[0].message, "a yes-or-no question");
            assert_eq!(inter.take_travel_answer(), None);
        }
        answer(&mut game, Some(true));
        assert_eq!(game.0.take_travel_answer(), Some(true));
        assert_eq!(game.0.take_travel_answer(), None, "taken once");
        // A no, and a box gone without an answer, are both a no.
        for gone in [Some(false), None] {
            let mut game = ask();
            {
                let (inter, world, service, boxes) = &mut game;
                service.service(Some(boxes), inter, world, LocalTime(1.0));
            }
            answer(&mut game, gone);
            assert_eq!(game.0.take_travel_answer(), Some(false));
        }
        // With nowhere to show it, the question is lost at once: a no.
        let (mut inter, mut world, mut service, _) = ask();
        service.service(None, &mut inter, &mut world, LocalTime(1.0));
        assert_eq!(inter.take_travel_answer(), Some(false));
    }
}
