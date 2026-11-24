//! The social windows, doing what the game's fellowship, friends and allegiance panels do: the
//! fellowship's members and its management, the friends list with adding, removing and telling,
//! the blacklist (the game's squelch list) with squelching a character or an account and taking a
//! squelch off, and the allegiance with swearing, breaking and releasing (each after the game's
//! own question).
//! While a window is open the game is asked for its live updates, as its panels ask.
//! On a world that tells the client its AI Players, each AI fellow's row shows its mode, and the
//! leader sets the selected one's mode (`@aifellow`) and gives it orders (a tell); a player in no
//! fellowship does the same for the AI Players following them.

use pyreal_client_contract::view::{AllegianceAction, PlayerOption};
use pyreal_client_contract::UiRequest;
use pyreal_primitives::ObjectId;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::colours;
use crate::ui::game::{GameState, Relation};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The most a fellowship holds.
const FELLOWSHIP_FULL: usize = 9;

/// How tall a one-line list row is, in layout units: room round its name.
const ROW_H: f32 = 30.0;

/// How tall a text box is, in layout units.
const FIELD_H: f32 = 34.0;

/// How tall a button along a window's foot is, in layout units.
const BUTTON_H: f32 = 30.0;

/// How tall the AI fellow controls are, in layout units: two rows of buttons.
const AI_CONTROLS_H: f32 = 64.0;

/// One AI fellow control: its name, what it does, whether it is lit, and what it asks.
type ControlButton<'a> = (&'a str, &'a str, bool, UiRequest);

impl Windows {
    /// While the social or allegiance window is open, or the HUD's party bars show a
    /// fellowship, the game keeps it up to date; when neither does, it stops.
    pub(super) fn follow_social(&mut self, state: &GameState, out: &mut Outcome) {
        let party = (self.is_open(WindowId::Social) && self.social_tab == 0)
            || (self.party_bars && !state.fellowship.is_empty());
        if party != self.fellowship_updates {
            self.fellowship_updates = party;
            out.requests
                .push(UiRequest::FellowshipUpdateRequest { on: party });
        }
        let allegiance = self.is_open(WindowId::Allegiance);
        if allegiance != self.allegiance_updates {
            self.allegiance_updates = allegiance;
            out.requests
                .push(UiRequest::AllegianceUpdateRequest { on: allegiance });
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn social(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        kit::tabs(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &["Fellowship", "Friends", "Squelch"],
            &mut self.social_tab,
        );
        if self.social_tab == 2 {
            self.blacklist(p, ctx, state, body, out);
            return;
        }
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let top = body.y + 36.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let bw = 96.0 * k;
        let selected_player = state
            .target
            .as_ref()
            .filter(|t| t.relation == Relation::Player)
            .map(|t| (t.id, t.name.clone()));
        if self.social_tab == 0 {
            let Some(f) = state.fellowship_view.as_ref() else {
                // No fellowship: make one.
                p.text(&dim, body.x + 8.0 * k, top, "You are not in a fellowship.");
                p.text(&dim, body.x + 8.0 * k, top + 30.0 * k, "Fellowship name");
                let field = Rect::new(
                    body.x + 8.0 * k,
                    top + 48.0 * k,
                    body.w - 16.0 * k - bw - 8.0 * k,
                    FIELD_H * k,
                );
                let submitted = kit::text_box(
                    p,
                    ctx,
                    field,
                    &mut self.fellowship_name,
                    &mut self.typing_field,
                    1,
                );
                let create = Rect::new(field.right() + 8.0 * k, field.y, bw, field.h);
                let can = !self.fellowship_name.trim().is_empty();
                if (kit::button(p, ctx, create, "Create", can) || submitted) && can {
                    out.requests.push(UiRequest::FellowshipCreate {
                        name: self.fellowship_name.trim().to_owned(),
                        share_xp: state.option(PlayerOption::FellowshipShareXP),
                    });
                    self.fellowship_name.clear();
                }
                self.fellowship_options(
                    p,
                    ctx,
                    state,
                    Rect::new(body.x, top + 96.0 * k, body.w, 120.0 * k),
                    out,
                );
                self.ai_followers(
                    p,
                    ctx,
                    state,
                    Rect::new(body.x, top + 166.0 * k, body.w, foot - top - 166.0 * k),
                    out,
                );
                return;
            };
            let me = state.player_id;
            let leader = me == Some(f.leader);
            let ai_fellow = |id: ObjectId| {
                state
                    .ai_fellows
                    .iter()
                    .find(|a| a.id == id && a.in_fellowship)
            };
            // The leader of a fellowship with AI Players in it has their controls.
            let ai_controls = leader && f.members.iter().any(|m| ai_fellow(m.id).is_some());
            let controls_h = if ai_controls { AI_CONTROLS_H * k } else { 0.0 };
            p.text(&name, body.x + 8.0 * k, top, &f.name);
            p.text_in(
                &dim,
                Rect::new(body.x, top, body.w - 8.0 * k, 18.0 * k),
                Align::Right,
                &format!(
                    "{} of {FELLOWSHIP_FULL}{}",
                    f.members.len(),
                    if f.open_fellow { "  ·  open" } else { "" }
                ),
            );
            // The members scroll in their own list above the options, which keeps its presses.
            let list = Rect::new(
                body.x,
                top + 26.0 * k,
                body.w,
                (foot - 78.0 * k - controls_h - top - 26.0 * k).max(0.0),
            );
            let rows = kit::ScrollList::begin(
                p,
                ctx,
                list,
                f.members.len(),
                46.0 * k,
                &mut self.fellows_scroll,
            );
            for (i, m) in f.members.iter().enumerate() {
                let Some(row) = rows.row(i, 44.0 * k) else {
                    continue;
                };
                let y = row.y;
                let on = self.fellow_selected == Some(m.id);
                if on || ctx.input.hover(&row) {
                    crate::ui::pregame::list_highlight(p, row, on);
                }
                let mark = if m.id == f.leader { "  (leader)" } else { "" };
                p.text_in(
                    &name,
                    Rect::new(row.x + 8.0 * k, y, row.w * 0.5, 24.0 * k),
                    Align::Left,
                    &format!("{}{mark}", m.name),
                );
                p.text_in(
                    &dim,
                    Rect::new(row.x + 8.0 * k, y + 24.0 * k, row.w * 0.5, 20.0 * k),
                    Align::Left,
                    &match ai_fellow(m.id) {
                        Some(a) => format!(
                            "Level {}  ·  {}% of shared XP  ·  {}",
                            m.level,
                            m.xp_percent,
                            mode_name(&a.mode)
                        ),
                        None => format!("Level {}  ·  {}% of shared XP", m.level, m.xp_percent),
                    },
                );
                let gx = row.x + row.w * 0.52;
                let gw = row.w * 0.44;
                let [health, stamina, mana] = colours::VITALS;
                for (i, (cur, max, colour)) in [
                    (m.current_health, m.max_health, health),
                    (m.current_stamina, m.max_stamina, stamina),
                    (m.current_mana, m.max_mana, mana),
                ]
                .into_iter()
                .enumerate()
                {
                    #[allow(clippy::cast_precision_loss)]
                    let fill = if max == 0 {
                        0.0
                    } else {
                        cur as f32 / max as f32
                    };
                    #[allow(clippy::cast_precision_loss)]
                    kit::gauge(
                        p,
                        Rect::new(gx, y + 6.0 * k + i as f32 * 12.0 * k, gw, 8.0 * k),
                        fill,
                        colour,
                    );
                }
                if ctx.over(&row) && ctx.input.clicked(&row) {
                    self.fellow_selected = Some(m.id);
                    out.requests.push(UiRequest::Select(m.id));
                }
            }
            rows.end(p, ctx);
            let fellow = self
                .fellow_selected
                .filter(|id| f.members.iter().any(|m| m.id == *id) && Some(*id) != me);
            if ai_controls {
                let strip = Rect::new(
                    body.x,
                    foot - 78.0 * k - controls_h + 4.0 * k,
                    body.w,
                    controls_h - 8.0 * k,
                );
                // The selected AI fellow, or the only one.
                let mut ais = f
                    .members
                    .iter()
                    .filter_map(|m| ai_fellow(m.id).map(|a| (m.id, a)));
                let only = match (ais.next(), ais.next()) {
                    (Some(one), None) => Some(one),
                    _ => None,
                };
                let chosen = fellow
                    .and_then(|id| ai_fellow(id).map(|a| (id, a)))
                    .or(only);
                self.ai_fellow_controls(p, ctx, strip, chosen, out);
            }
            let recruit_target = selected_player
                .as_ref()
                .map(|(id, _)| *id)
                .filter(|id| !f.members.iter().any(|m| m.id == *id));
            let full = f.members.len() >= FELLOWSHIP_FULL;
            let buttons: [(&str, bool, Option<UiRequest>); 6] = [
                (
                    "Recruit",
                    recruit_target.is_some() && !full && (leader || f.open_fellow),
                    recruit_target.map(|target| UiRequest::FellowshipRecruit { target }),
                ),
                (
                    "Dismiss",
                    leader && fellow.is_some(),
                    fellow.map(|target| UiRequest::FellowshipDismiss { target }),
                ),
                (
                    "Make Leader",
                    leader && fellow.is_some(),
                    fellow.map(|target| UiRequest::FellowshipAssignNewLeader { target }),
                ),
                (
                    if f.open_fellow { "Close" } else { "Open" },
                    leader,
                    Some(UiRequest::FellowshipToggleOpenness),
                ),
                (
                    "Quit",
                    true,
                    Some(UiRequest::FellowshipQuit { disband: false }),
                ),
                (
                    "Disband",
                    leader,
                    Some(UiRequest::FellowshipQuit { disband: true }),
                ),
            ];
            let bw = (body.w - 5.0 * 6.0 * k) / 6.0;
            // A world without open fellowships has no such button.
            // A 1999 server takes no new leader and no disbanding.
            let shown = buttons.into_iter().filter(|(label, ..)| {
                (state.era.is_none_or(|f| f.fellowship_open) || !matches!(*label, "Open" | "Close"))
                    && !(state.wire_1999 && matches!(*label, "Make Leader" | "Disband"))
            });
            for (i, (label, enabled, request)) in shown.enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let r = Rect::new(body.x + i as f32 * (bw + 6.0 * k), foot, bw, BUTTON_H * k);
                if kit::button(p, ctx, r, label, enabled) {
                    out.requests.extend(request);
                }
            }
            if recruit_target.is_none()
                && ctx.input.hover(&Rect::new(body.x, foot, bw, BUTTON_H * k))
            {
                self.tip = Some(("Recruit".into(), vec!["Select a player to invite.".into()]));
            }
            self.fellowship_options(
                p,
                ctx,
                state,
                Rect::new(body.x, foot - 70.0 * k, body.w, 60.0 * k),
                out,
            );
        } else {
            // Friends: online first, then by name, as the game orders them.
            let mut friends: Vec<_> = state.friend_list.iter().collect();
            friends.sort_by(|a, b| b.online.cmp(&a.online).then(a.name.cmp(&b.name)));
            let field = Rect::new(
                body.x,
                foot - (FIELD_H + 10.0) * k,
                body.w - bw - 8.0 * k,
                FIELD_H * k,
            );
            let list = Rect::new(body.x, top, body.w, field.y - 8.0 * k - top);
            let rows = kit::ScrollList::begin(
                p,
                ctx,
                list,
                friends.len(),
                (ROW_H + 2.0) * k,
                &mut self.friends_scroll,
            );
            for (i, f) in friends.iter().enumerate() {
                let Some(row) = rows.row(i, ROW_H * k) else {
                    continue;
                };
                let y = row.y;
                let on = self.friend_selected == Some(f.id);
                if on || ctx.input.hover(&row) {
                    crate::ui::pregame::list_highlight(p, row, on);
                }
                let dot = if f.online { 0xFF6E_CB5C } else { 0xFF60_6060 };
                p.fill(
                    Rect::new(
                        row.x + 8.0 * k,
                        (y + (row.h - 8.0 * k) / 2.0).round(),
                        8.0 * k,
                        8.0 * k,
                    ),
                    dot,
                );
                p.text_in(
                    &name,
                    Rect::new(row.x + 24.0 * k, y, row.w - 32.0 * k, row.h),
                    Align::Left,
                    &f.name,
                );
                if ctx.over(&row) && ctx.input.clicked(&row) {
                    self.friend_selected = Some(f.id);
                }
            }
            if friends.is_empty() {
                // A 1999 world's server keeps the list and tells it in chat.
                let empty = if state.wire_1999 {
                    "This world keeps the list. Show List asks for it."
                } else {
                    "No friends yet."
                };
                p.text(&dim, list.x + 8.0 * k, list.y, empty);
            }
            rows.end(p, ctx);
            let chosen = self
                .friend_selected
                .and_then(|id| state.friend_list.iter().find(|f| f.id == id));
            let submitted = kit::text_box(
                p,
                ctx,
                field,
                &mut self.friend_name,
                &mut self.typing_field,
                2,
            );
            let full = state.friend_list.len() >= 100;
            let can = !self.friend_name.trim().is_empty() && !full;
            if (kit::button(
                p,
                ctx,
                Rect::new(field.right() + 8.0 * k, field.y, bw, field.h),
                "Add",
                can,
            ) || submitted)
                && can
            {
                out.requests.push(UiRequest::AddFriend {
                    name: self.friend_name.trim().to_owned(),
                });
                self.friend_name.clear();
            }
            if state.wire_1999 {
                // The server answers by name, in chat: list, and remove whoever is typed.
                let typed = self.friend_name.trim().to_owned();
                let line = |text: String| UiRequest::ChatLineOnBehalf { text, window: 0 };
                if kit::button(
                    p,
                    ctx,
                    Rect::new(body.right() - 2.0 * bw - 8.0 * k, foot, bw, BUTTON_H * k),
                    "Show List",
                    true,
                ) {
                    out.requests.push(line("@friends".to_owned()));
                }
                if kit::button(
                    p,
                    ctx,
                    Rect::new(body.right() - bw, foot, bw, BUTTON_H * k),
                    "Remove",
                    !typed.is_empty(),
                ) {
                    out.requests.push(line(format!("@friends remove {typed}")));
                    self.friend_name.clear();
                }
                return;
            }
            let appear_offline = state.option(PlayerOption::AppearOffline);
            let cb = Rect::new(body.x, foot, body.w - 2.0 * bw - 16.0 * k, BUTTON_H * k);
            if let Some(on) = kit::check_row(p, ctx, cb, appear_offline, &dim, "Appear offline") {
                out.requests
                    .push(UiRequest::SetPlayerOption(PlayerOption::AppearOffline, on));
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.right() - 2.0 * bw - 8.0 * k, foot, bw, BUTTON_H * k),
                "Tell",
                chosen.is_some_and(|f| f.online),
            ) {
                if let Some(f) = chosen {
                    out.requests.push(UiRequest::ChatEntry {
                        window: pyreal_client_contract::chat::interface::window::MAIN,
                        text: String::new(),
                        action: pyreal_client_contract::chat::entry::EntryAction::StartTell {
                            name: f.name.clone(),
                        },
                    });
                }
            }
            if kit::button(
                p,
                ctx,
                Rect::new(body.right() - bw, foot, bw, BUTTON_H * k),
                "Remove",
                chosen.is_some(),
            ) {
                if let Some(f) = chosen {
                    out.requests.push(UiRequest::RemoveFriend { target: f.id });
                    self.friend_selected = None;
                }
            }
        }
    }

    /// The AI Players following a player in no fellowship, each with its mode, and the controls
    /// for the selected one or the only one: they take a mode and orders from whoever they follow.
    fn ai_followers(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        r: Rect,
        out: &mut Outcome,
    ) {
        let followers: Vec<_> = state
            .ai_fellows
            .iter()
            .filter(|a| !a.in_fellowship)
            .collect();
        if followers.is_empty() {
            return;
        }
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        p.text(&dim, r.x + 8.0 * k, r.y, "AI Players following you");
        let controls_h = AI_CONTROLS_H * k;
        let list = Rect::new(
            r.x,
            r.y + 20.0 * k,
            r.w,
            (r.h - 20.0 * k - controls_h).max(0.0),
        );
        let rows = kit::ScrollList::begin(
            p,
            ctx,
            list,
            followers.len(),
            (ROW_H + 2.0) * k,
            &mut self.fellows_scroll,
        );
        for (i, a) in followers.iter().enumerate() {
            let Some(row) = rows.row(i, ROW_H * k) else {
                continue;
            };
            let on = self.fellow_selected == Some(a.id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.text_in(
                &name,
                Rect::new(row.x + 8.0 * k, row.y, row.w * 0.6, row.h),
                Align::Left,
                &a.name,
            );
            p.text_in(
                &dim,
                Rect::new(row.x, row.y, row.w - 8.0 * k, row.h),
                Align::Right,
                &mode_name(&a.mode),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                self.fellow_selected = Some(a.id);
            }
        }
        rows.end(p, ctx);
        let chosen = self
            .fellow_selected
            .and_then(|id| followers.iter().find(|a| a.id == id))
            .or_else(|| (followers.len() == 1).then(|| &followers[0]))
            .map(|a| (a.id, *a));
        let strip = Rect::new(r.x, list.bottom() + 4.0 * k, r.w, controls_h - 8.0 * k);
        self.ai_fellow_controls(p, ctx, strip, chosen, out);
    }

    /// An AI fellow's mode and orders, for the fellowship's leader; a hint while there are several
    /// and none is selected.
    fn ai_fellow_controls(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        chosen: Option<(ObjectId, &pyreal_client_contract::view::AiFellowView)>,
        out: &mut Outcome,
    ) {
        use pyreal_client_model::dereth_extensions::{FellowMode, FellowOrder};
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let Some((id, fellow)) = chosen else {
            p.text_in(
                &dim,
                Rect::new(r.x + 8.0 * k, r.y, r.w - 16.0 * k, r.h),
                Align::Left,
                "Select an AI Player to set its mode or give it an order.",
            );
            return;
        };
        let label_w = 52.0 * k;
        let row_h = (r.h - 4.0 * k) / 2.0;
        let current = FellowMode::from_word(&fellow.mode);
        let rows: [(&str, Vec<ControlButton<'_>>); 2] = [
            (
                "Mode",
                FellowMode::ALL
                    .iter()
                    .map(|m| {
                        (
                            m.name(),
                            m.says(),
                            current == Some(*m),
                            UiRequest::ChatLineOnBehalf {
                                text: m.command(id),
                                window: 0,
                            },
                        )
                    })
                    .collect(),
            ),
            (
                "Order",
                FellowOrder::ALL
                    .iter()
                    .map(|o| {
                        (
                            o.name(),
                            o.says(),
                            false,
                            UiRequest::Tell {
                                target: id,
                                text: o.word().to_owned(),
                            },
                        )
                    })
                    .collect(),
            ),
        ];
        for (n, (label, buttons)) in rows.into_iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = r.y + n as f32 * (row_h + 4.0 * k);
            p.text_in(
                &dim,
                Rect::new(r.x + 8.0 * k, y, label_w, row_h),
                Align::Left,
                label,
            );
            #[allow(clippy::cast_precision_loss)]
            let count = buttons.len() as f32;
            let bw = (r.w - label_w - 8.0 * k - (count - 1.0) * 4.0 * k) / count;
            for (i, (name, says, lit, request)) in buttons.into_iter().enumerate() {
                #[allow(clippy::cast_precision_loss)]
                let b = Rect::new(
                    r.x + label_w + 8.0 * k + i as f32 * (bw + 4.0 * k),
                    y,
                    bw,
                    row_h,
                );
                if kit::button_lit(p, ctx, b, name, true, lit) && !lit {
                    out.requests.push(request);
                }
                if ctx.input.hover(&b) {
                    self.tip = Some((name.to_owned(), vec![says.to_owned()]));
                }
            }
        }
    }

    /// The blacklist: the game's squelch list, one alphabetical list of names, each a character's
    /// or an account's; a typed name squelched as a character or as an account, and the chosen
    /// row's squelch taken off, as the game's squelch panel does.
    fn blacklist(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let top = body.y + 36.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let mut rows: Vec<&pyreal_client_contract::view::SquelchEntry> =
            state.squelches.iter().collect();
        rows.sort_by(|a, b| a.name.cmp(&b.name));
        if self.squelch_selected.is_some_and(|i| i >= rows.len()) {
            self.squelch_selected = None;
        }
        let field = Rect::new(body.x, foot - (FIELD_H + 10.0) * k, body.w, FIELD_H * k);
        let row_h = ROW_H * k;
        if rows.is_empty() {
            p.text(&dim, body.x + 8.0 * k, top, "No one is squelched.");
        }
        let list = kit::ScrollList::begin(
            p,
            ctx,
            Rect::new(body.x, top, body.w, field.y - 8.0 * k - top),
            rows.len(),
            row_h + 2.0 * k,
            &mut self.squelch_scroll,
        );
        for (i, e) in rows.iter().enumerate() {
            let Some(r) = list.row(i, row_h) else {
                continue;
            };
            let on = self.squelch_selected == Some(i);
            if on || ctx.input.hover(&r) {
                crate::ui::pregame::list_highlight(p, r, on);
            }
            p.text_in(
                &name,
                Rect::new(r.x + 8.0 * k, r.y, r.w * 0.6, r.h),
                Align::Left,
                &e.name,
            );
            p.text_in(
                &dim,
                Rect::new(r.x, r.y, r.w - 8.0 * k, r.h),
                Align::Right,
                if e.account { "Account" } else { "Character" },
            );
            if ctx.over(&r) && ctx.input.clicked(&r) {
                self.squelch_selected = Some(i);
            }
        }
        list.end(p, ctx);
        kit::text_box(
            p,
            ctx,
            field,
            &mut self.squelch_name,
            &mut self.typing_field,
            31,
        );
        let typed = self.squelch_name.trim().to_owned();
        // Each button as wide as its name needs, the two squelches alike.
        let label = TextStyle::new(Family::Body, 14.0, 0xFFFF_FFFF);
        let bw = ["Squelch Character", "Squelch Account"]
            .iter()
            .map(|l| p.measure(&label, l) + 36.0 * k)
            .fold(130.0 * k, f32::max);
        if kit::button(
            p,
            ctx,
            Rect::new(body.x, foot, bw, BUTTON_H * k),
            "Squelch Character",
            !typed.is_empty(),
        ) {
            out.requests.push(UiRequest::ModifyCharacterSquelch {
                object: ObjectId(0),
                add: true,
                account: typed.clone(),
                message_type: pyreal_client_model::chat::text_type::ALL_CHANNELS,
            });
            self.squelch_name.clear();
        }
        // A 1999 squelch covers one character.
        if !state.wire_1999
            && kit::button(
                p,
                ctx,
                Rect::new(body.x + bw + 8.0 * k, foot, bw, BUTTON_H * k),
                "Squelch Account",
                !typed.is_empty(),
            )
        {
            out.requests.push(UiRequest::ModifyAccountSquelch {
                add: true,
                name: typed,
            });
            self.squelch_name.clear();
        }
        let chosen = self.squelch_selected.and_then(|i| rows.get(i));
        let remove_w = 110.0 * k;
        if kit::button(
            p,
            ctx,
            Rect::new(body.right() - remove_w, foot, remove_w, BUTTON_H * k),
            "Remove",
            chosen.is_some(),
        ) {
            if let Some(e) = chosen {
                out.requests.push(squelch_removal(e));
                self.squelch_selected = None;
            }
        }
    }

    /// The four fellowship switches, the player's own options.
    fn fellowship_options(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        area: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let options = [
            (PlayerOption::FellowshipShareXP, "Share experience"),
            (PlayerOption::FellowshipShareLoot, "Share loot"),
            (
                PlayerOption::FellowshipAutoAcceptRequests,
                "Accept All Invitations",
            ),
            (PlayerOption::IgnoreFellowshipRequests, "Ignore invitations"),
        ];
        // A box for what the world lacks is left out, as on the options page.
        let shown = options.into_iter().filter(|(o, _)| {
            pyreal_client_contract::options::sheet::row_of_option(*o)
                .is_none_or(|row| row.needs.met(state.era.as_ref()))
                && (!state.wire_1999 || super::options::option_on_1999_wire(*o))
        });
        for (i, (o, label)) in shown.enumerate() {
            let (col, row) = (i % 2, i / 2);
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x + col as f32 * area.w / 2.0,
                area.y + row as f32 * 28.0 * k,
                area.w / 2.0 - 8.0 * k,
                24.0 * k,
            );
            if let Some(on) = kit::check_row(p, ctx, r, state.option(o), &dim, label) {
                out.requests.push(UiRequest::SetPlayerOption(o, on));
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(super) fn allegiance(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        let a = &state.roster;
        let head =
            TextStyle::new(Family::Heading, 24.0, ctx.colours.heading()).edge(ctx.colours.edge());
        let name = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let dim = TextStyle::new(Family::Body, 12.0, ctx.colours.dim()).edge(ctx.colours.edge());
        let mut y = body.y + 4.0 * k;
        let title = if a.allegiance_name.is_empty() {
            "Allegiance"
        } else {
            a.allegiance_name.as_str()
        };
        p.text(&head, body.x + 8.0 * k, y, title);
        y += 32.0 * k;
        if let Some(me) = &a.subject {
            p.text(
                &dim,
                body.x + 8.0 * k,
                y,
                &format!(
                    "{}  ·  {} followers  ·  {} XP passed up",
                    rank_text(me.rank, a.player_rank_quality),
                    a.total_vassals,
                    crate::ui::hud::grouped(u64::from(a.own_cp_tithed))
                ),
            );
            y += 22.0 * k;
        }
        let online = |on: bool| if on { 0xFF6E_CB5C } else { 0xFF60_6060 };
        let person = |p: &mut Painter<'_>,
                      ctx: &mut Ctx<'_>,
                      y: f32,
                      label: &str,
                      e: &pyreal_client_contract::view::AllegianceEntry,
                      sel: &mut Option<ObjectId>| {
            let row = Rect::new(body.x, y, body.w, 26.0 * k);
            let on = *sel == Some(e.id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.fill(
                Rect::new(row.x + 8.0 * k, y + 9.0 * k, 8.0 * k, 8.0 * k),
                online(e.logged_in),
            );
            p.text_in(
                &dim,
                Rect::new(row.x + 24.0 * k, y, 80.0 * k, row.h),
                Align::Left,
                label,
            );
            p.text_in(
                &name,
                Rect::new(row.x + 110.0 * k, y, row.w - 120.0 * k, row.h),
                Align::Left,
                &e.full_name,
            );
            p.text_in(
                &dim,
                row.offset(-8.0 * k, 0.0),
                Align::Right,
                &format!("rank {}", e.rank),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                *sel = Some(e.id);
            }
        };
        let mut selected = self.allegiance_selected;
        if let Some(m) = &a.monarch {
            person(p, ctx, y, "Monarch", m, &mut selected);
            y += 28.0 * k;
        }
        if let Some(pa) = a
            .patron
            .as_ref()
            .filter(|pa| a.monarch.as_ref().is_none_or(|m| m.id != pa.id))
        {
            person(p, ctx, y, "Patron", pa, &mut selected);
            y += 28.0 * k;
        }
        y += 8.0 * k;
        p.text(
            &dim,
            body.x + 8.0 * k,
            y,
            &format!("Vassals ({})", a.vassals.len()),
        );
        y += 20.0 * k;
        let foot = body.bottom() - 30.0 * k;
        let list = Rect::new(body.x, y, body.w, foot - y - 8.0 * k);
        let rows = kit::ScrollList::begin(
            p,
            ctx,
            list,
            a.vassals.len(),
            28.0 * k,
            &mut self.vassals_scroll,
        );
        for (i, v) in a.vassals.iter().enumerate() {
            let Some(row) = rows.row(i, 26.0 * k) else {
                continue;
            };
            let y = row.y;
            let on = selected == Some(v.id);
            if on || ctx.input.hover(&row) {
                crate::ui::pregame::list_highlight(p, row, on);
            }
            p.fill(
                Rect::new(row.x + 8.0 * k, y + 9.0 * k, 8.0 * k, 8.0 * k),
                online(v.logged_in),
            );
            p.text_in(
                &name,
                Rect::new(row.x + 24.0 * k, y, row.w - 32.0 * k, row.h),
                Align::Left,
                &v.full_name,
            );
            p.text_in(
                &dim,
                row.offset(-8.0 * k, 0.0),
                Align::Right,
                &format!(
                    "{} XP passed up",
                    crate::ui::hud::grouped(u64::from(v.cp_cached))
                ),
            );
            if ctx.over(&row) && ctx.input.clicked(&row) {
                selected = Some(v.id);
            }
        }
        if a.vassals.is_empty() {
            p.text(&dim, list.x + 8.0 * k, list.y, "None.");
        }
        rows.end(p, ctx);
        self.allegiance_selected = selected;
        // Swear to the selected player, break from the patron, release the selected vassal:
        // each after the game's own question.
        let swear_to = state
            .target
            .as_ref()
            .filter(|t| state.swear_target == Some(t.id))
            .map(|t| (t.id, t.name.clone()));
        let vassal = selected.and_then(|id| a.vassals.iter().find(|v| v.id == id));
        let bw = (body.w - 12.0 * k) / 3.0;
        let swear = kit::button(
            p,
            ctx,
            Rect::new(body.x, foot, bw, 28.0 * k),
            "Swear",
            swear_to.is_some(),
        );
        let brk = kit::button(
            p,
            ctx,
            Rect::new(body.x + bw + 6.0 * k, foot, bw, 28.0 * k),
            "Break",
            a.patron.is_some(),
        );
        let release = kit::button(
            p,
            ctx,
            Rect::new(body.x + 2.0 * (bw + 6.0 * k), foot, bw, 28.0 * k),
            "Release",
            vassal.is_some(),
        );
        use pyreal_ui_logic::panels::allegiance as retail;
        if swear {
            if let Some((id, n)) = &swear_to {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Swear,
                    *id,
                    retail::ID_SWEAR_CONFIRMATION,
                    n,
                );
            }
        }
        if brk {
            if let Some(pa) = &a.patron {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Break,
                    pa.id,
                    retail::ID_BREAK_CONFIRMATION,
                    &pa.full_name,
                );
            }
        }
        if release {
            if let Some(v) = vassal {
                self.ask_allegiance(
                    out,
                    AllegianceAction::Kick,
                    v.id,
                    retail::ID_KICK_CONFIRMATION,
                    &v.full_name,
                );
            }
        }
        if ctx.input.hover(&Rect::new(body.x, foot, bw, 28.0 * k)) && swear_to.is_none() {
            self.tip = Some(("Swear".into(), vec![swear_refusal(state).into()]));
        }
    }

    /// Ask the game's question before an allegiance action: its words filled with the other
    /// player's name, found by the shell in the game's string table.
    fn ask_allegiance(
        &mut self,
        out: &mut Outcome,
        action: AllegianceAction,
        target: ObjectId,
        token: &'static str,
        who: &str,
    ) {
        out.allegiance_question = Some((action, target, token, who.to_owned()));
    }
}

/// What taking a squelch off sends: an account's by its account name, a character's by its
/// name, for every kind of message, as the game's squelch panel sends it.
#[must_use]
pub fn squelch_removal(entry: &pyreal_client_contract::view::SquelchEntry) -> UiRequest {
    if entry.account {
        UiRequest::ModifyAccountSquelch {
            add: false,
            name: entry.name.clone(),
        }
    } else {
        UiRequest::ModifyCharacterSquelch {
            object: ObjectId(0),
            add: false,
            account: entry.name.clone(),
            message_type: pyreal_client_model::chat::text_type::ALL_CHANNELS,
        }
    }
}

/// Why Swear is off, in the order the shared rule refuses: a patron already, too little
/// experience, or no player to swear to selected.
/// An AI fellow's mode as the window names it: the world's word, named where it is one this
/// client knows.
fn mode_name(word: &str) -> String {
    pyreal_client_model::dereth_extensions::FellowMode::from_word(word)
        .map_or_else(|| word.to_owned(), |m| m.name().to_owned())
}

fn swear_refusal(state: &GameState) -> &'static str {
    if state.roster.patron.is_some() {
        "You already have a patron."
    } else if state.short_of_oath_experience {
        "You don't have enough available xp to swear Allegiance."
    } else {
        "Select the player to swear allegiance to."
    }
}

/// The header's rank: the tree's, or the rank quality where a spell raises it, with the
/// difference. A quality of -1 or none (0) is the tree's.
fn rank_text(rank: u16, quality: i32) -> String {
    let tree = i32::from(rank);
    if quality == tree || quality == -1 || quality == 0 {
        format!("Rank {rank}")
    } else {
        format!("Rank {quality} ({:+})", quality - tree)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swear_says_a_patron_already_held_is_why_it_is_off() {
        let mut state = GameState::default();
        assert_eq!(
            swear_refusal(&state),
            "Select the player to swear allegiance to."
        );
        state.short_of_oath_experience = true;
        assert!(swear_refusal(&state).contains("enough available xp"));
        state.roster.patron = Some(pyreal_client_contract::view::AllegianceEntry::default());
        assert_eq!(swear_refusal(&state), "You already have a patron.");
    }

    #[test]
    fn the_rank_shows_a_raised_rank_with_the_difference() {
        assert_eq!(rank_text(5, 5), "Rank 5");
        assert_eq!(rank_text(5, -1), "Rank 5");
        assert_eq!(rank_text(5, 0), "Rank 5");
        assert_eq!(rank_text(5, 6), "Rank 6 (+1)");
    }

    #[test]
    fn an_account_squelch_comes_off_by_account_and_a_character_s_by_name_for_every_message() {
        let account = pyreal_client_contract::view::SquelchEntry {
            name: "someone".into(),
            account: true,
        };
        assert_eq!(
            squelch_removal(&account),
            UiRequest::ModifyAccountSquelch {
                add: false,
                name: "someone".into()
            }
        );
        let character = pyreal_client_contract::view::SquelchEntry {
            name: "Borin".into(),
            account: false,
        };
        assert_eq!(
            squelch_removal(&character),
            UiRequest::ModifyCharacterSquelch {
                object: ObjectId(0),
                add: false,
                account: "Borin".into(),
                message_type: 1,
            }
        );
    }
}
