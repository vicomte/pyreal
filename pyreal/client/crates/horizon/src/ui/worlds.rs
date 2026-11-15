//! The screens before any world: the player's worlds, signing in to one, and the data files.
//!
//! The host hands the worlds and the data files over ([`FrontDoorView`]) and carries out what the
//! player asks ([`FrontDoorRequest`]). The world list is the first screen; a world is added,
//! changed or removed there, and signed in to on its right. The data files' panel comes up
//! instead while the client has none, and from the list's Data Files button; the Plugins screen
//! from its Plugins button.

use crate::art::Family;
use crate::draw::Rect;
use crate::front_door::{DatsFolder, FrontDoorRequest, FrontDoorView, ListedRow, WorldRow};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::pregame::message_box;
use crate::ui::Outcome;

/// The text boxes' ids, for the keyboard's focus.
const ACCOUNT: u32 = 101;
const PASSWORD: u32 = 102;
const NAME: u32 = 103;
const HOST: u32 = 104;
const PORT: u32 = 105;
const MODERN: u32 = 106;
const CLASSIC: u32 = 107;
const SOURCE: u32 = 108;
const ORIGINAL: u32 = 109;
const SEARCH: u32 = 110;

/// What the password box holds while the password kept for the account stands in it: never the
/// password itself, only as many dots.
const SAVED: &str = "\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}\u{25CF}";

/// A world being added or changed.
#[derive(Debug, Clone, Default)]
struct WorldForm {
    slug: Option<String>,
    name: String,
    host: String,
    port: String,
}

impl WorldForm {
    /// Whether it can be saved: a name, a host and a port from 1 to 65535.
    fn ready(&self) -> bool {
        !self.name.trim().is_empty()
            && !self.host.trim().is_empty()
            && self.port.trim().parse::<u16>().is_ok_and(|p| p != 0)
    }
}

/// The world list's state across frames.
#[derive(Debug, Default)]
pub struct Worlds {
    selected: Option<String>,
    /// The sign-in fields, and the world they were filled for.
    account: String,
    password: String,
    remember: bool,
    /// The kept password stands in the password box ([`SAVED`]), for this account.
    saved_for: Option<String>,
    filled_for: Option<String>,
    focus: u32,
    form: Option<WorldForm>,
    removing: Option<(String, String)>,
    /// The data files' panel, shown on request; its fields, and the host's folders they were
    /// last filled from (none to fill them again).
    dats_open: bool,
    modern: String,
    classic: String,
    original: String,
    dats_filled: Option<[Option<String>; 3]>,
    scroll: f32,
    /// The art packs' library, while the host offers it: under the data files.
    pub art_packs: Option<crate::art_packs::ArtPacksView>,
    /// What the player asked of it, for the host.
    pub art_pack_requests: Vec<crate::art_packs::ArtPackRequest>,
    /// The Plugins screen, shown on request; its source box and its lists' scrolls.
    plugins_open: bool,
    source_url: String,
    installed_scroll: f32,
    available_scroll: f32,
    /// The installed plugins and the sources, while the host offers them.
    pub plugins: Option<crate::plugin_shelf::PluginShelfView>,
    /// What the player asked of them, for the host.
    pub plugin_requests: Vec<crate::plugin_shelf::PluginShelfRequest>,
    /// The Known Worlds screen, shown on request; its search box and its list's scroll.
    directory_open: bool,
    search: String,
    directory_scroll: f32,
}

/// The art packs' section of the Data Files panel: its heading, a row a pack and the button.
const PACK_ROW: f32 = 30.0;

/// What the world list asks of the interface besides the host's requests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Asked {
    /// Open the Settings window.
    pub settings: bool,
}

impl Worlds {
    /// Draw the screens before any world and say what the player asked.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &FrontDoorView,
        requests: &mut Vec<FrontDoorRequest>,
        out: &mut Outcome,
    ) -> Asked {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let mut asked = Asked::default();
        let logo = logo(p);
        self.fill_dats(&view.dats);
        let modal = self.form.is_some() || self.removing.is_some();
        if !view.dats.ready || self.dats_open {
            self.data_files(p, ctx, view, requests);
        } else if self.plugins_open && self.plugins.is_some() {
            self.plugin_screen(p, ctx);
        } else if self.directory_open {
            self.directory(p, ctx, view, requests);
        } else {
            self.world_list(p, ctx, view, requests, modal, logo, &mut asked, out);
        }
        if let Some(message) = &view.message {
            let style = TextStyle::new(Family::Body, 14.0, 0xFFFF_9A6A).edge(0xFF00_0000);
            let lines = p.wrap(&style, message, sw - 80.0 * k);
            for (i, line) in lines.iter().rev().enumerate() {
                let r = Rect::new(
                    40.0 * k,
                    sh - (44.0 + lines_tall(i)) * k,
                    sw - 80.0 * k,
                    24.0 * k,
                );
                p.text_in(&style, r, Align::Centre, line);
            }
        }
        self.form_box(p, ctx, requests);
        if let Some((slug, name)) = self.removing.clone() {
            let text =
                format!("Remove {name} from the list, with its accounts and saved passwords?");
            match message_box(p, ctx, "Remove World", &text, &["Remove", "Cancel"]) {
                Some(0) => {
                    requests.push(FrontDoorRequest::RemoveWorld { slug });
                    self.removing = None;
                    self.selected = None;
                }
                Some(_) => self.removing = None,
                None => {}
            }
        }
        asked
    }

    #[allow(clippy::too_many_arguments, clippy::too_many_lines)]
    fn world_list(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &FrontDoorView,
        requests: &mut Vec<FrontDoorRequest>,
        modal: bool,
        logo: Rect,
        asked: &mut Asked,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        if self
            .selected
            .as_ref()
            .is_none_or(|s| !view.worlds.iter().any(|w| &w.slug == s))
        {
            self.selected = view
                .last_world
                .clone()
                .filter(|s| view.worlds.iter().any(|w| &w.slug == s))
                .or_else(|| view.worlds.first().map(|w| w.slug.clone()));
        }
        let selected = self
            .selected
            .as_ref()
            .and_then(|s| view.worlds.iter().find(|w| &w.slug == s))
            .cloned();
        if let Some(w) = &selected {
            if self.filled_for.as_deref() != Some(w.slug.as_str()) {
                self.fill_for(w);
            }
        }
        // The list on the left with its own buttons; the sign-in pinned to the right with the
        // client's buttons in a row above it.
        let gap = 10.0 * k;
        let ListLayout { row, sign, list } = ListLayout::new(p.screen, k, logo);
        kit::grounded_panel(p, list, 0.85, 2);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        p.text(&head, list.x + 16.0 * k, list.y + 10.0 * k, "Worlds");
        // Up and down the list, at the panel's top right, for the selected world.
        let place = selected
            .as_ref()
            .and_then(|w| view.worlds.iter().position(|v| v.slug == w.slug));
        let arrow = |i: f32| {
            Rect::new(
                list.right() - (16.0 + 34.0 * (2.0 - i)) * k,
                list.y + 12.0 * k,
                28.0 * k,
                28.0 * k,
            )
        };
        for (i, up, can, tip) in [
            (0.0, true, place.is_some_and(|i| i > 0), "Move Up"),
            (
                1.0,
                false,
                place.is_some_and(|i| i + 1 < view.worlds.len()),
                "Move Down",
            ),
        ] {
            let b = arrow(i);
            let can = can && !modal;
            if move_button(p, ctx, b, up, can) {
                if let Some(w) = &selected {
                    requests.push(FrontDoorRequest::MoveWorld {
                        slug: w.slug.clone(),
                        up,
                    });
                }
            }
            if can && ctx.input.hover(&b) {
                kit::tooltip(p, ctx, tip, &[]);
            }
        }
        let area = Rect::new(
            list.x + 12.0 * k,
            list.y + 48.0 * k,
            list.w - 24.0 * k,
            list.h - 108.0 * k,
        );
        let row_h = 52.0 * k;
        let name_style = TextStyle::new(Family::Body, 17.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let info_style = TextStyle::new(Family::Numerals, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        if view.worlds.is_empty() {
            let dim = TextStyle::new(Family::Body, 14.0, 0xFFA0_A8B0).edge(0xFF00_0000);
            p.text(&dim, area.x + 4.0 * k, area.y, "No worlds yet: Add one.");
        }
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * view.worlds.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.scroll);
        p.list.push_clip(area);
        for (i, w) in view.worlds.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(
                area.x,
                area.y + row_h * i as f32 - offset,
                area.w - 10.0 * k,
                row_h - 4.0 * k,
            );
            if r.bottom() < area.y || r.y > area.bottom() {
                continue;
            }
            let on = self.selected.as_deref() == Some(w.slug.as_str());
            let over = !modal && ctx.over(&r) && ctx.input.hover(&area);
            p.fill(
                r,
                if on {
                    0xC020_4070
                } else if over {
                    0xA018_2840
                } else {
                    0x8010_1830
                },
            );
            p.text(&name_style, r.x + 12.0 * k, r.y + 5.0 * k, &w.name);
            p.text(&info_style, r.x + 12.0 * k, r.y + 28.0 * k, &world_line(w));
            if over && ctx.input.clicked(&r) {
                if on && ctx.input.double {
                    self.sign_in(w, requests);
                }
                self.selected = Some(w.slug.clone());
            }
        }
        p.list.pop_clip();
        // The list's own buttons, along its foot.
        // Known Worlds at the right end, as wide as its label; the rest from the left.
        let known_w = (kit::button_label_width(p, "Known Worlds") + 40.0 * k).max(110.0 * k);
        let bw = (110.0 * k).min((list.w - 32.0 * k - 3.0 * gap - known_w) / 3.0);
        let by = list.bottom() - 48.0 * k;
        let at = |i: f32| Rect::new(list.x + 16.0 * k + (bw + gap) * i, by, bw, 32.0 * k);
        if kit::button(p, ctx, at(0.0), "Add", !modal) {
            self.form = Some(WorldForm {
                port: "9000".into(),
                ..WorldForm::default()
            });
            self.focus = NAME;
        }
        if kit::button(p, ctx, at(1.0), "Edit", !modal && selected.is_some()) {
            if let Some(w) = &selected {
                self.form = Some(WorldForm {
                    slug: Some(w.slug.clone()),
                    name: w.name.clone(),
                    host: w.host.clone(),
                    port: w.port.to_string(),
                });
                self.focus = NAME;
            }
        }
        if kit::button_danger(p, ctx, at(2.0), "Remove", !modal && selected.is_some()) {
            if let Some(w) = &selected {
                self.removing = Some((w.slug.clone(), w.name.clone()));
            }
        }
        let known = Rect::new(list.right() - 16.0 * k - known_w, by, known_w, 32.0 * k);
        if kit::button(p, ctx, known, "Known Worlds", !modal) {
            self.directory_open = true;
            requests.push(FrontDoorRequest::ReadDirectory { again: false });
        }
        // The client's buttons above the sign-in, and Quit at the foot of the column.
        let cw = (row.w - 2.0 * gap) / 3.0;
        let col = |i: f32| Rect::new(row.x + (cw + gap) * i, row.y, cw, row.h);
        if kit::button(p, ctx, col(0.0), "Settings", !modal) {
            asked.settings = true;
        }
        if kit::button(p, ctx, col(1.0), "Data Files", !modal) {
            self.dats_open = true;
            self.dats_filled = None;
        }
        if kit::button(
            p,
            ctx,
            col(2.0),
            "Plugins",
            !modal && self.plugins.is_some(),
        ) {
            self.plugins_open = true;
        }
        let quit = Rect::new(sign.right() - cw, list.bottom() - 32.0 * k, cw, 32.0 * k);
        ctx.input.nav.cancel(quit);
        if kit::button(p, ctx, quit, "Quit", !modal) {
            out.quit = true;
        }
        // Explore, under the sign-in where the screen has room for it.
        let top = if selected.is_some() {
            sign.bottom() + gap
        } else {
            sign.y
        };
        let explore = Rect::new(sign.x, top, sign.w, 132.0 * k);
        if explore.bottom() + gap <= quit.y {
            self.explore(p, ctx, view, requests, explore, modal);
        }
        // Signing in to the selected world.
        let Some(w) = selected else {
            return;
        };
        kit::grounded_panel(p, sign, 0.85, 2);
        p.text(&head, sign.x + 16.0 * k, sign.y + 10.0 * k, &w.name);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        let x = sign.x + 16.0 * k;
        let fw = sign.w - 32.0 * k;
        p.text(
            &info_style,
            x,
            sign.y + 42.0 * k,
            &format!("{}:{}", w.host, w.port),
        );
        p.text(&label, x, sign.y + 70.0 * k, "Account");
        let account = Rect::new(x, sign.y + 90.0 * k, fw, 28.0 * k);
        let mut enter = false;
        if !modal {
            enter |= kit::text_box(p, ctx, account, &mut self.account, &mut self.focus, ACCOUNT);
        }
        p.text(&label, x, sign.y + 128.0 * k, "Password");
        let password = Rect::new(x, sign.y + 148.0 * k, fw, 28.0 * k);
        if !modal {
            let before = self.password.clone();
            enter |= kit::password_box(
                p,
                ctx,
                password,
                &mut self.password,
                &mut self.focus,
                PASSWORD,
            );
            self.typed(&before);
        }
        let remember = Rect::new(x, sign.y + 188.0 * k, 22.0 * k, 22.0 * k);
        if !modal {
            if let Some(on) = kit::checkbox(p, ctx, remember, self.remember) {
                self.set_remember(on);
            }
        }
        p.text(
            &label,
            remember.right() + 8.0 * k,
            remember.y + 2.0 * k,
            "Remember the password",
        );
        let go = Rect::new(x, sign.y + 236.0 * k, fw, 36.0 * k);
        ctx.input.nav.home(go);
        let can = !modal && !self.account.trim().is_empty();
        if (kit::button(p, ctx, go, "Log In", can) || (enter && can)) && !modal {
            self.sign_in(&w, requests);
        }
    }

    /// The Explore panel across `r`: a button for each era's files the player has.
    fn explore(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &FrontDoorView,
        requests: &mut Vec<FrontDoorRequest>,
        r: Rect,
        modal: bool,
    ) {
        let k = p.scale;
        kit::grounded_panel(p, r, 0.85, 2);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        let x = r.x + 16.0 * k;
        p.text(&head, x, r.y + 10.0 * k, "Explore");
        p.text(&dim, x, r.y + 42.0 * k, "Fly over Dereth with no server.");
        let gap = 10.0 * k;
        let span = r.w - 32.0 * k - 2.0 * gap;
        // The first button's name is the longest.
        let widths = [span * 0.4, span * 0.3, span * 0.3];
        let eras = [
            (
                "End of Retail",
                view.dats.ready,
                DatsFolder::Modern,
                "Fly over the End of Retail files.",
            ),
            (
                "2005",
                view.dats.ready && view.dats.classic.is_some(),
                DatsFolder::Classic,
                "Fly over the February 2005 files. Choose their folder in Data Files first.",
            ),
            (
                "1999",
                view.dats.ready && view.dats.original.is_some(),
                DatsFolder::Original,
                "Fly over the 1999 Retail CD files. Choose their folder in Data Files first.",
            ),
        ];
        let mut bx = x;
        for ((label, can, folder, tip), bw) in eras.into_iter().zip(widths) {
            let b = Rect::new(bx, r.bottom() - 48.0 * k, bw, 32.0 * k);
            bx += bw + gap;
            if kit::button(p, ctx, b, label, can && !modal) {
                requests.push(FrontDoorRequest::Explore { folder });
            }
            if !modal && ctx.input.hover(&b) {
                kit::tooltip(p, ctx, tip, &[]);
            }
        }
    }

    fn sign_in(&mut self, w: &WorldRow, requests: &mut Vec<FrontDoorRequest>) {
        requests.push(FrontDoorRequest::SignIn {
            slug: w.slug.clone(),
            account: self.account.trim().to_owned(),
            password: self.password_to_send(),
            remember: self.remember,
        });
        self.focus = 0;
    }

    /// The sign-in filled for world `w`: its last account, and the dots of its kept password.
    fn fill_for(&mut self, w: &WorldRow) {
        self.account = w.account.clone().unwrap_or_default();
        self.remember = w.password_saved;
        self.saved_for = w.password_saved.then(|| self.account.clone());
        self.password = if self.saved_for.is_some() {
            SAVED.to_owned()
        } else {
            String::new()
        };
        self.filled_for = Some(w.slug.clone());
    }

    /// After the boxes took this frame's keys (`before` the password box's text): typing over
    /// the dots replaces them, and another account has no kept password here.
    fn typed(&mut self, before: &str) {
        let Some(account) = self.saved_for.as_deref() else {
            return;
        };
        if self.password != before {
            // What was typed after the dots, or nothing for a deletion.
            self.password = self
                .password
                .strip_prefix(SAVED)
                .unwrap_or_default()
                .to_owned();
            self.saved_for = None;
        } else if self.account.trim() != account.trim() {
            self.password.clear();
            self.saved_for = None;
        }
    }

    /// Remember the password or not: not, and the kept one is no longer offered.
    fn set_remember(&mut self, on: bool) {
        self.remember = on;
        if !on && self.saved_for.take().is_some() {
            self.password.clear();
        }
    }

    /// The password a sign-in carries: none while the dots stand (the host reads the kept one),
    /// else what was typed.
    fn password_to_send(&mut self) -> String {
        if self.saved_for.take().is_some() {
            self.password.clear();
            return String::new();
        }
        std::mem::take(&mut self.password)
    }

    /// The world being added or changed, over everything.
    fn form_box(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        requests: &mut Vec<FrontDoorRequest>,
    ) {
        let Some(form) = self.form.as_mut() else {
            return;
        };
        let k = p.scale;
        let (sw, sh) = p.screen;
        let r = Rect::new(
            sw / 2.0 - 220.0 * k,
            sh / 2.0 - 150.0 * k,
            440.0 * k,
            300.0 * k,
        );
        p.fill(Rect::new(0.0, 0.0, sw, sh), 0x8000_0000);
        kit::panel(p, r, 0.95);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        p.text(
            &head,
            r.x + 16.0 * k,
            r.y + 10.0 * k,
            if form.slug.is_some() {
                "Edit World"
            } else {
                "Add a World"
            },
        );
        let x = r.x + 16.0 * k;
        let fw = r.w - 32.0 * k;
        let mut enter = false;
        for (i, (name, text, id)) in [
            ("Name", &mut form.name, NAME),
            ("Host", &mut form.host, HOST),
            ("Port", &mut form.port, PORT),
        ]
        .into_iter()
        .enumerate()
        {
            #[allow(clippy::cast_precision_loss)]
            let y = r.y + (50.0 + 64.0 * i as f32) * k;
            p.text(&label, x, y, name);
            enter |= kit::text_box(
                p,
                ctx,
                Rect::new(x, y + 20.0 * k, fw, 28.0 * k),
                text,
                &mut self.focus,
                id,
            );
        }
        let save = Rect::new(x, r.bottom() - 50.0 * k, 120.0 * k, 32.0 * k);
        let cancel = Rect::new(r.right() - 136.0 * k, save.y, 120.0 * k, 32.0 * k);
        let ready = form.ready();
        if kit::button(p, ctx, save, "Save", ready) || (enter && ready) {
            requests.push(FrontDoorRequest::SaveWorld {
                slug: form.slug.clone(),
                name: form.name.clone(),
                host: form.host.clone(),
                port: form.port.clone(),
            });
            self.form = None;
            self.focus = 0;
            return;
        }
        if kit::button(p, ctx, cancel, "Cancel", true)
            || ctx.input.take_key(crate::ui::input::vk::ESCAPE)
        {
            self.form = None;
            self.focus = 0;
        }
    }

    /// The list of known worlds, each added to the player's list in one press.
    fn directory(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &FrontDoorView,
        requests: &mut Vec<FrontDoorRequest>,
    ) {
        let d = &view.directory;
        let k = p.scale;
        let (sw, sh) = p.screen;
        let r = Rect::new(40.0 * k, 120.0 * k, sw - 80.0 * k, sh - 180.0 * k);
        kit::grounded_panel(p, r, 0.92, 2);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        let name_style = TextStyle::new(Family::Body, 17.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let info_style = TextStyle::new(Family::Numerals, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        p.text(&head, r.x + 16.0 * k, r.y + 10.0 * k, "Known Worlds");
        p.text(
            &dim,
            r.x + 180.0 * k,
            r.y + 16.0 * k,
            "The worlds the Dereth Network lists. Add one to play there.",
        );
        // The search, and reading the list again.
        let x = r.x + 16.0 * k;
        let row_y = r.y + 52.0 * k;
        p.text(&label, x, row_y + 6.0 * k, "Search");
        let search = Rect::new(x + 60.0 * k, row_y, (r.w * 0.4).min(360.0 * k), 28.0 * k);
        kit::text_box(p, ctx, search, &mut self.search, &mut self.focus, SEARCH);
        let refresh = Rect::new(r.right() - 136.0 * k, row_y - 2.0 * k, 120.0 * k, 32.0 * k);
        if kit::button(p, ctx, refresh, "Refresh", !d.reading) {
            requests.push(FrontDoorRequest::ReadDirectory { again: true });
        }
        // The worlds.
        let area = Rect::new(x, r.y + 96.0 * k, r.w - 32.0 * k, r.h - 160.0 * k);
        p.fill(area, 0x4010_1820);
        let shown: Vec<&ListedRow> = d
            .worlds
            .iter()
            .filter(|w| matches(w, &self.search))
            .collect();
        if shown.is_empty() {
            let text = if d.reading {
                "Reading the list..."
            } else if d.worlds.is_empty() {
                "The list could not be read. Refresh to try again."
            } else {
                "No known world matches the search."
            };
            p.text(&dim, area.x + 8.0 * k, area.y + 8.0 * k, text);
        }
        let row_h = 52.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * shown.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.directory_scroll);
        let mut hovered = None;
        p.list.push_clip(area);
        for (i, w) in shown.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let rr = Rect::new(
                area.x,
                area.y + row_h * i as f32 - offset,
                area.w - 12.0 * k,
                row_h - 4.0 * k,
            );
            if rr.bottom() < area.y || rr.y > area.bottom() {
                continue;
            }
            p.fill(rr, 0x8010_1830);
            p.text(&name_style, rr.x + 12.0 * k, rr.y + 5.0 * k, &w.name);
            p.text(
                &info_style,
                rr.x + 12.0 * k,
                rr.y + 28.0 * k,
                &listed_line(w),
            );
            let add = Rect::new(rr.right() - 110.0 * k, rr.y + 8.0 * k, 100.0 * k, 32.0 * k);
            let label = if w.added { "Added" } else { "Add" };
            let inside = ctx.input.hover(&area);
            if kit::button(p, ctx, add, label, !w.added) && inside {
                requests.push(FrontDoorRequest::AddListedWorld {
                    slug: w.slug.clone(),
                });
            }
            if inside && ctx.input.hover(&rr) && !ctx.input.hover(&add) {
                hovered = Some(i);
            }
        }
        p.list.pop_clip();
        if let Some(w) = hovered.and_then(|i| shown.get(i)) {
            if let Some(desc) = &w.description {
                kit::tooltip(p, ctx, &w.name, std::slice::from_ref(desc));
            }
        }
        // Where the list came from, and closing.
        if let Some(note) = &d.note {
            p.text(&dim, x, r.bottom() - 42.0 * k, note);
        }
        let close = Rect::new(
            r.right() - 136.0 * k,
            r.bottom() - 50.0 * k,
            120.0 * k,
            32.0 * k,
        );
        ctx.input.nav.cancel(close);
        if kit::button(p, ctx, close, "Done", true)
            || (self.focus != SEARCH && ctx.input.take_key(crate::ui::input::vk::ESCAPE))
        {
            self.directory_open = false;
            self.focus = 0;
        }
    }

    /// The data files: the End of Retail folder, the two older ones (optional), and what is wrong.
    fn data_files(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &FrontDoorView,
        requests: &mut Vec<FrontDoorRequest>,
    ) {
        let k = p.scale;
        let (sw, sh) = p.screen;
        let packs = self.art_packs.clone();
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 14.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        let warn = TextStyle::new(Family::Body, 14.0, 0xFFFF_C46A).edge(0xFF00_0000);
        let tag = TextStyle::new(Family::Body, 13.0, 0xFFA0_A8B0).edge(0xFF00_0000);
        // The text is wrapped first, so the panel is sized to hold it.
        let fw = (640.0 - 32.0) * k;
        let intro = p.wrap(
            &dim,
            "Pyreal draws the world from the game's own data files. Choose the End of Retail \
             folder, which holds client_portal.dat. A world of 2005 also needs the February 2005 \
             folder, and a world of 1999 needs the 1999 Retail CD folder.",
            fw,
        );
        let problem = view
            .dats
            .problem
            .as_ref()
            .filter(|_| !view.dats.ready)
            .map(|t| p.wrap(&warn, t, fw))
            .unwrap_or_default();
        let rows_top = 46.0 + lines_tall(intro.len()) + 22.0;
        let packs_top = rows_top + 212.0 + lines_tall(problem.len().max(1)) + 14.0;
        let extra = packs
            .as_ref()
            .map_or(0.0, |v| art_pack_library_tall(p, v, fw));
        let h = packs_top + 76.0 + extra;
        let r = Rect::new(
            sw / 2.0 - 320.0 * k,
            sh / 2.0 - h / 2.0 * k,
            640.0 * k,
            h * k,
        );
        kit::grounded_panel(p, r, 0.92, 2);
        p.text(&head, r.x + 16.0 * k, r.y + 10.0 * k, "Data Files");
        let x = r.x + 16.0 * k;
        for (i, line) in intro.iter().enumerate() {
            p.text(&dim, x, r.y + (46.0 + lines_tall(i)) * k, line);
        }
        let browse_w = if view.can_browse { 100.0 * k } else { 0.0 };
        let field_w = fw - browse_w - if view.can_browse { 10.0 * k } else { 0.0 };
        let rows = [
            (
                "End of Retail",
                "Required",
                r.y + rows_top * k,
                MODERN,
                DatsFolder::Modern,
            ),
            (
                "February 2005",
                "Optional",
                r.y + (rows_top + 70.0) * k,
                CLASSIC,
                DatsFolder::Classic,
            ),
            (
                "1999 Retail CD",
                "Optional",
                r.y + (rows_top + 140.0) * k,
                ORIGINAL,
                DatsFolder::Original,
            ),
        ];
        let mut enter = false;
        for (name, need, y, id, folder) in rows {
            p.text(&label, x, y, name);
            let field = Rect::new(x, y + 20.0 * k, field_w, 28.0 * k);
            p.text_in(&tag, Rect::new(x, y, field_w, 18.0 * k), Align::Right, need);
            let text = match folder {
                DatsFolder::Modern => &mut self.modern,
                DatsFolder::Classic => &mut self.classic,
                DatsFolder::Original => &mut self.original,
            };
            enter |=
                kit::text_box_holding(p, ctx, field, text, &mut self.focus, id, Align::Left, 1024);
            if view.can_browse {
                let b = Rect::new(field.right() + 10.0 * k, field.y, browse_w, 28.0 * k);
                if kit::button(p, ctx, b, "Browse...", true) {
                    requests.push(FrontDoorRequest::BrowseDats { folder });
                }
            }
        }
        for (i, line) in problem.iter().enumerate() {
            p.text(&warn, x, r.y + (rows_top + 212.0 + lines_tall(i)) * k, line);
        }
        if let Some(packs) = &packs {
            let at = Rect::new(x, r.y + packs_top * k, fw, extra * k);
            self.art_pack_library(p, ctx, packs, at);
        }
        // One button: the folders taken if they changed, and the screen closed.
        let done = Rect::new(
            r.right() - 156.0 * k,
            r.bottom() - 50.0 * k,
            140.0 * k,
            32.0 * k,
        );
        if view.dats.ready {
            ctx.input.nav.cancel(done);
        }
        if kit::button(p, ctx, done, "Done", !self.modern.trim().is_empty()) || enter {
            let changed = self.modern != view.dats.modern.clone().unwrap_or_default()
                || self.classic != view.dats.classic.clone().unwrap_or_default()
                || self.original != view.dats.original.clone().unwrap_or_default();
            if changed || !view.dats.ready {
                requests.push(FrontDoorRequest::SetDats {
                    modern: self.modern.clone(),
                    classic: self.classic.clone(),
                    original: self.original.clone(),
                });
            }
            self.dats_open = false;
            self.dats_filled = None;
            self.focus = 0;
        }
    }
}

impl Worlds {
    /// Fill the folder fields from the host's folders when they changed, or when asked to. A
    /// folder picked with Browse reaches the view a frame after the request.
    fn fill_dats(&mut self, dats: &crate::front_door::DatsView) {
        let now = [
            dats.modern.clone(),
            dats.classic.clone(),
            dats.original.clone(),
        ];
        if self.dats_filled.as_ref() == Some(&now) {
            return;
        }
        let text = |d: &Option<String>| d.clone().unwrap_or_default();
        self.modern = text(&now[0]);
        self.classic = text(&now[1]);
        self.original = text(&now[2]);
        self.dats_filled = Some(now);
    }

    /// The art packs' library across `r`: each pack, its size and a button taking it out, and the
    /// button adding one.
    fn art_pack_library(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        view: &crate::art_packs::ArtPacksView,
        r: Rect,
    ) {
        use crate::art_packs::ArtPackRequest;
        let k = p.scale;
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        p.text(&label, r.x, r.y, "Art Packs (optional)");
        let first = if view.packs.is_empty() {
            "The world is drawn with the built-in art pack."
        } else {
            "Drawn over the built-in art pack. Choose one in Settings, Display."
        };
        p.text(&dim, r.x, r.y + 20.0 * k, first);
        let mut y = r.y + 44.0 * k;
        for pack in &view.packs {
            let row = Rect::new(r.x, y, r.w, (PACK_ROW - 4.0) * k);
            p.fill(row, 0x4010_1820);
            let drawn = view.chosen.as_deref() == Some(pack.path.as_str());
            let text = match (&pack.problem, drawn) {
                (Some(_), _) => format!("{}   (will not open)", pack.label()),
                (None, true) => format!("{}   (drawn)", pack.label()),
                (None, false) => pack.label(),
            };
            let text = p.fit(&label, &text, row.w - 112.0 * k);
            p.text_in(&label, row.inset(8.0 * k), Align::Left, &text);
            let remove = Rect::new(
                row.right() - 96.0 * k,
                row.y + 1.0 * k,
                92.0 * k,
                row.h - 2.0 * k,
            );
            if kit::button(p, ctx, remove, "Remove", true) {
                self.art_pack_requests
                    .push(ArtPackRequest::Remove(pack.path.clone()));
            }
            y += PACK_ROW * k;
        }
        let add = Rect::new(r.x, y + 4.0 * k, 140.0 * k, 28.0 * k);
        if kit::button(p, ctx, add, "Add Pack...", view.can_browse) {
            self.art_pack_requests.push(ArtPackRequest::AddPack);
        }
        let top = p.text_top(&dim, add.y, add.h);
        for (i, line) in art_pack_status(p, view, r.w).iter().enumerate() {
            p.text(&dim, add.right() + 12.0 * k, top + lines_tall(i) * k, line);
        }
    }
}

/// The height of `n` lines of the front door's 14-point text, unscaled.
fn lines_tall(n: usize) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let n = n as f32;
    18.0 * n
}

/// The art packs' status wrapped beside the Add Pack button, in a library `w` wide.
fn art_pack_status(p: &Painter<'_>, view: &crate::art_packs::ArtPacksView, w: f32) -> Vec<String> {
    let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
    let room = w - (140.0 + 12.0) * p.scale;
    view.status
        .as_ref()
        .map(|s| p.wrap(&dim, s, room))
        .unwrap_or_default()
}

/// The art packs' library's height in a library `w` wide, unscaled: its rows, and the status's
/// lines past the first.
fn art_pack_library_tall(p: &Painter<'_>, view: &crate::art_packs::ArtPacksView, w: f32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let rows = PACK_ROW * view.packs.len() as f32;
    let status = art_pack_status(p, view, w).len().saturating_sub(1);
    96.0 + rows + lines_tall(status)
}

impl Worlds {
    /// The Plugins screen: the installed plugins on the left, the sources and what they list on
    /// the right.
    #[allow(clippy::too_many_lines)]
    fn plugin_screen(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        use crate::plugin_shelf::PluginShelfRequest as Ask;
        let Some(view) = self.plugins.clone() else {
            return;
        };
        let k = p.scale;
        let (sw, sh) = p.screen;
        let r = Rect::new(40.0 * k, 120.0 * k, sw - 80.0 * k, sh - 180.0 * k);
        kit::grounded_panel(p, r, 0.92, 2);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
        let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
        let name_style = TextStyle::new(Family::Body, 16.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let warn = TextStyle::new(Family::Body, 13.0, 0xFFFF_C46A).edge(0xFF00_0000);
        p.text(&head, r.x + 16.0 * k, r.y + 10.0 * k, "Plugins");
        p.text(
            &dim,
            r.x + 120.0 * k,
            r.y + 16.0 * k,
            "Experimental. An installed plugin loads while it is ticked; nothing a source lists \
             is reviewed by Pyreal.",
        );
        let gap = 24.0 * k;
        let left = Rect::new(
            r.x + 16.0 * k,
            r.y + 52.0 * k,
            (r.w - 32.0 * k - gap) * 0.55,
            r.h - 120.0 * k,
        );
        let right = Rect::new(
            left.right() + gap,
            left.y,
            r.w - 32.0 * k - gap - left.w,
            left.h,
        );

        // Installed, with each one's tick, update and removal.
        p.text(&label, left.x, left.y, "Installed");
        let area = Rect::new(left.x, left.y + 22.0 * k, left.w, left.h - 64.0 * k);
        p.fill(area, 0x4010_1820);
        let row_h = 46.0 * k;
        if view.installed.is_empty() {
            p.text(
                &dim,
                area.x + 8.0 * k,
                area.y + 8.0 * k,
                "None yet: install one from a file or a source.",
            );
        }
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * view.installed.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.installed_scroll);
        p.list.push_clip(area);
        for (i, row) in view.installed.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = area.y + row_h * i as f32 - offset;
            let rr = Rect::new(area.x, y, area.w - 12.0 * k, row_h - 4.0 * k);
            if rr.bottom() < area.y || rr.y > area.bottom() {
                continue;
            }
            p.fill(rr, 0x8010_1830);
            let title = if row.version.is_empty() {
                row.name.clone()
            } else {
                format!("{}  {}", row.name, row.version)
            };
            p.text(&name_style, rr.x + 40.0 * k, rr.y + 4.0 * k, &title);
            let second = row.problem.as_ref().unwrap_or(&row.from);
            let buttons = if row.update.is_some() { 236.0 } else { 104.0 };
            let line = Rect::new(
                rr.x + 40.0 * k,
                rr.y + 24.0 * k,
                rr.w - (40.0 + buttons) * k,
                18.0 * k,
            );
            p.list.push_clip(line);
            p.text(
                if row.problem.is_some() { &warn } else { &dim },
                line.x,
                line.y,
                second,
            );
            p.list.pop_clip();
            let tick = Rect::new(rr.x + 10.0 * k, rr.y + 11.0 * k, 20.0 * k, 20.0 * k);
            match row.on {
                Some(on) => {
                    if let Some(on) = kit::checkbox(p, ctx, tick, on) {
                        self.plugin_requests.push(Ask::SetOn {
                            id: row.id.clone(),
                            on,
                        });
                    }
                }
                None => kit::checkbox_greyed(p, tick, false),
            }
            let remove = Rect::new(rr.right() - 92.0 * k, rr.y + 7.0 * k, 86.0 * k, 28.0 * k);
            if kit::button(p, ctx, remove, "Remove", true) {
                self.plugin_requests.push(match &row.folder {
                    Some(folder) => Ask::RemoveFolder {
                        folder: folder.clone(),
                    },
                    None => Ask::Remove { id: row.id.clone() },
                });
            }
            if let Some(to) = &row.update {
                let up = Rect::new(remove.x - 132.0 * k, remove.y, 126.0 * k, remove.h);
                if kit::button(p, ctx, up, &format!("Update to {to}"), !view.busy) {
                    self.plugin_requests
                        .push(Ask::Update { id: row.id.clone() });
                }
            }
        }
        p.list.pop_clip();
        let by = area.bottom() + 10.0 * k;
        let file = Rect::new(left.x, by, 170.0 * k, 30.0 * k);
        if kit::button(p, ctx, file, "Install from File...", view.can_browse) {
            self.plugin_requests.push(Ask::InstallFile);
        }
        let folder = Rect::new(file.right() + 10.0 * k, by, 150.0 * k, 30.0 * k);
        if kit::button(p, ctx, folder, "Add Folder...", view.can_browse) {
            self.plugin_requests.push(Ask::AddFolder);
        }

        // The sources, a box adding one, and what they list.
        p.text(&label, right.x, right.y, "Sources");
        let add_w = 70.0 * k;
        let field = Rect::new(
            right.x,
            right.y + 22.0 * k,
            right.w - add_w - 8.0 * k,
            28.0 * k,
        );
        let enter = kit::text_box(p, ctx, field, &mut self.source_url, &mut self.focus, SOURCE);
        if self.source_url.is_empty() && self.focus != SOURCE {
            p.text_in(
                &dim,
                field.inset(6.0 * k),
                Align::Left,
                "https://.../plugins.json",
            );
        }
        let add = Rect::new(field.right() + 8.0 * k, field.y, add_w, field.h);
        let can_add = !view.busy && !self.source_url.trim().is_empty();
        if (kit::button(p, ctx, add, "Add", can_add) || enter) && can_add {
            self.plugin_requests.push(Ask::AddSource {
                url: self.source_url.trim().to_owned(),
            });
            self.source_url.clear();
            self.focus = 0;
        }
        let mut y = field.bottom() + 8.0 * k;
        if view.sources.is_empty() {
            p.text(
                &dim,
                right.x,
                y + 4.0 * k,
                "No sources: add one by its address.",
            );
            y += 26.0 * k;
        }
        for s in &view.sources {
            let rr = Rect::new(right.x, y, right.w, 40.0 * k);
            p.fill(rr, 0x4010_1820);
            p.text(&label, rr.x + 8.0 * k, rr.y + 3.0 * k, &s.name);
            let (style, said) = match &s.error {
                Some(e) => (&warn, e.clone()),
                None => (&dim, format!("{} plugin(s)   {}", s.plugins, s.url)),
            };
            let line = Rect::new(rr.x + 8.0 * k, rr.y + 20.0 * k, rr.w - 104.0 * k, 18.0 * k);
            p.list.push_clip(line);
            p.text(style, line.x, line.y, &said);
            p.list.pop_clip();
            let remove = Rect::new(rr.right() - 90.0 * k, rr.y + 6.0 * k, 84.0 * k, 28.0 * k);
            if kit::button(p, ctx, remove, "Remove", !view.busy) {
                self.plugin_requests
                    .push(Ask::RemoveSource { url: s.url.clone() });
            }
            y += 44.0 * k;
        }
        let refresh = Rect::new(right.x, y + 2.0 * k, 110.0 * k, 28.0 * k);
        if kit::button(
            p,
            ctx,
            refresh,
            "Refresh",
            !view.busy && !view.sources.is_empty(),
        ) {
            self.plugin_requests.push(Ask::Refresh);
        }
        y = refresh.bottom() + 14.0 * k;
        p.text(&label, right.x, y, "Available");
        let area = Rect::new(
            right.x,
            y + 22.0 * k,
            right.w,
            (right.bottom() - y - 22.0 * k).max(row_h),
        );
        p.fill(area, 0x4010_1820);
        if view.available.is_empty() {
            p.text(
                &dim,
                area.x + 8.0 * k,
                area.y + 8.0 * k,
                "Nothing more to install from the sources.",
            );
        }
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * view.available.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.available_scroll);
        p.list.push_clip(area);
        for (i, a) in view.available.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let y = area.y + row_h * i as f32 - offset;
            let rr = Rect::new(area.x, y, area.w - 12.0 * k, row_h - 4.0 * k);
            if rr.bottom() < area.y || rr.y > area.bottom() {
                continue;
            }
            p.fill(rr, 0x8010_1830);
            p.text(
                &name_style,
                rr.x + 8.0 * k,
                rr.y + 4.0 * k,
                &format!("{}  {}", a.name, a.version),
            );
            let second = a.problem.clone().unwrap_or_else(|| {
                if a.description.is_empty() {
                    a.source_name.clone()
                } else {
                    format!("{}: {}", a.source_name, a.description)
                }
            });
            let line = Rect::new(rr.x + 8.0 * k, rr.y + 24.0 * k, rr.w - 110.0 * k, 18.0 * k);
            p.list.push_clip(line);
            p.text(
                if a.problem.is_some() { &warn } else { &dim },
                line.x,
                line.y,
                &second,
            );
            p.list.pop_clip();
            let install = Rect::new(rr.right() - 92.0 * k, rr.y + 7.0 * k, 86.0 * k, 28.0 * k);
            let can = !view.busy && a.problem.is_none();
            if kit::button(p, ctx, install, "Install", can) {
                self.plugin_requests.push(Ask::Install {
                    source: a.source.clone(),
                    id: a.id.clone(),
                });
            }
        }
        p.list.pop_clip();

        if let Some(status) = &view.status {
            let at = Rect::new(
                r.x + 16.0 * k,
                r.bottom() - 46.0 * k,
                r.w - 200.0 * k,
                28.0 * k,
            );
            p.text_in(&dim, at, Align::Left, status);
        }
        let done = Rect::new(
            r.right() - 156.0 * k,
            r.bottom() - 50.0 * k,
            140.0 * k,
            32.0 * k,
        );
        ctx.input.nav.cancel(done);
        if kit::button(p, ctx, done, "Done", true)
            || (self.focus != SOURCE && ctx.input.take_key(crate::ui::input::vk::ESCAPE))
        {
            self.plugins_open = false;
            self.focus = 0;
        }
    }
}

/// A small button with an arrow up (`up`) or down; `true` when clicked.
fn move_button(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, up: bool, enabled: bool) -> bool {
    let clicked = kit::button(p, ctx, r, "", enabled);
    let pal = *p.art.palette();
    let colour = if enabled {
        pal.label
    } else {
        pal.label_disabled
    };
    let k = p.scale;
    let a = Rect::new(
        r.x + r.w / 2.0 - 6.0 * k,
        r.y + r.h / 2.0 - 4.0 * k,
        12.0 * k,
        8.0 * k,
    );
    if up {
        kit::up_arrow(p, a, colour);
    } else {
        kit::down_arrow(p, a, colour);
    }
    clicked
}

/// A known world's second line: where it is, its server software, rules and how settled it is.
fn listed_line(w: &ListedRow) -> String {
    let mut line = format!("{}:{}   {}", w.host, w.port, w.emulator);
    for part in [&w.ruleset, &w.maturity].into_iter().flatten() {
        line.push_str("   ");
        line.push_str(part);
    }
    line
}

/// Whether a known world matches the search: its name or host holds every word of it.
fn matches(w: &ListedRow, search: &str) -> bool {
    let hay = format!("{} {}", w.name, w.host).to_lowercase();
    search
        .to_lowercase()
        .split_whitespace()
        .all(|word| hay.contains(word))
}

/// The PYREAL logo at the top left; where it was drawn.
fn logo(p: &mut Painter<'_>) -> Rect {
    let k = p.scale;
    let title = TextStyle::new(Family::Heading, 54.0, 0xFFFF_FFFF)
        .edge(0xFF15_3A70)
        .tracking(4.0);
    let title_w = p.text(&title, 40.0 * k, 24.0 * k, "PYREAL");
    let sub = TextStyle::new(Family::Numerals, 12.0, 0xFFEE_E1C5)
        .edge(0xFF00_0000)
        .tracking(4.0);
    let sub_w = p.text(&sub, 44.0 * k, 88.0 * k, "ASHERON'S CALL");
    Rect::new(
        40.0 * k,
        24.0 * k,
        title_w.max(4.0 * k + sub_w),
        64.0 * k + p.line_height(&sub),
    )
}

/// The world list's places: the client's button row, the sign-in under it and the list.
struct ListLayout {
    row: Rect,
    sign: Rect,
    list: Rect,
}

impl ListLayout {
    /// The places on a `screen` at `k`, the row and both panels lowered below the `logo` where
    /// the row would meet it.
    fn new(screen: (f32, f32), k: f32, logo: Rect) -> Self {
        let (sw, sh) = screen;
        let gap = 10.0 * k;
        let sign_x = sw - 420.0 * k;
        let row_h = 32.0 * k;
        let row_y = if sign_x < logo.right() + 24.0 * k {
            logo.bottom() + gap
        } else {
            140.0 * k - row_h - gap
        };
        let top = row_y + row_h + gap;
        let sign = Rect::new(sign_x, top, 380.0 * k, 300.0 * k);
        let list_w = (sw * 0.45).max(360.0 * k).min(sign.x - 70.0 * k);
        let bottom = sh - 60.0 * k;
        Self {
            row: Rect::new(sign_x, row_y, sign.w, row_h),
            sign,
            list: Rect::new(40.0 * k, top, list_w, bottom - top),
        }
    }
}

/// A world's second line: where it is, and the account last played there.
fn world_line(w: &WorldRow) -> String {
    match &w.account {
        Some(a) => format!("{}:{}   {a}", w.host, w.port),
        None => format!("{}:{}", w.host, w.port),
    }
}

#[cfg(test)]
mod data_files_tests {
    use super::*;
    use crate::art_packs::{ArtPackRow, ArtPacksView};
    use crate::front_door::DatsView;

    const WARN: u32 = 0xFFFF_C46A;
    const DIM: u32 = 0xFFC8_D8E8;

    /// The glyphs of the Data Files panel drawn on a `screen` at `scale`, as (colour, place).
    fn drawn(scale: f32, screen: (f32, f32), status: &str) -> Vec<(u32, Rect)> {
        let art = crate::art::Art::new(std::sync::Arc::new(crate::pieces::Pieces::built_in()));
        let colours = crate::ui::colours::Colours::default();
        let mut list = crate::draw::DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale,
            screen,
            fade: 1.0,
        };
        let mut input = crate::ui::input::InputFrame::default();
        let mut drag = None;
        let mut ctx = Ctx {
            kit: pyreal_ui_kit::widgets::Ctx {
                time: 0.0,
                dt: 1.0 / 60.0,
                input: &mut input,
                colours: &colours,
                hot: false,
            },
            drag: &mut drag,
            drops: Vec::new(),
            magic: Vec::new(),
            airborne: false,
        };
        let view = FrontDoorView {
            dats: DatsView {
                problem: Some("Choose the End of Retail folder to play.".into()),
                ..DatsView::default()
            },
            can_browse: true,
            ..FrontDoorView::default()
        };
        let mut w = Worlds {
            art_packs: Some(ArtPacksView {
                packs: vec![ArtPackRow {
                    path: "/packs/a".into(),
                    name: "A pack with a name long enough to reach the Remove button and past it"
                        .into(),
                    bytes: 1 << 30,
                    problem: Some("no manifest".into()),
                }],
                status: Some(status.into()),
                can_browse: true,
                ..ArtPacksView::default()
            }),
            ..Worlds::default()
        };
        w.data_files(&mut p, &mut ctx, &view, &mut Vec::new());
        list.quads.iter().map(|q| (q.colour, q.dst)).collect()
    }

    /// The panel's warning and status lines wrap inside it, at any scale, however long, and the
    /// art packs' lines sit below the warning's.
    #[test]
    fn the_data_files_text_wraps_inside_the_panel() {
        let status = format!(
            "Would not open: the file at {} is not an art pack",
            "/Users/someone/Library/Application_Support/Pyreal/packs/remastered_textures_v2.zip"
        );
        for (scale, screen) in [
            (1.0, (1280.0, 800.0)),
            (0.73, (1280.0, 800.0)),
            (1.5, (1920.0, 1080.0)),
        ] {
            let quads = drawn(scale, screen, &status);
            let right = screen.0 / 2.0 + (320.0 - 16.0) * scale + 1.0;
            let warn: Vec<Rect> = quads.iter().filter(|q| q.0 == WARN).map(|q| q.1).collect();
            let dim: Vec<Rect> = quads.iter().filter(|q| q.0 == DIM).map(|q| q.1).collect();
            assert!(!warn.is_empty() && !dim.is_empty(), "{scale}: drawn");
            for g in warn.iter().chain(&dim) {
                assert!(g.right() <= right, "{scale}: a glyph at {g:?} past {right}");
            }
            let top = warn.iter().map(|g| g.y).fold(f32::MAX, f32::min);
            let bottom = warn.iter().map(|g| g.bottom()).fold(0.0, f32::max);
            for g in &dim {
                assert!(
                    g.bottom() <= top || g.y >= bottom,
                    "{scale}: a line at {g:?} overlaps the warning, {top} to {bottom}"
                );
            }
        }
    }
}

#[cfg(test)]
mod saved_password_tests {
    use super::*;

    fn kept() -> WorldRow {
        WorldRow {
            slug: "home".into(),
            name: "Home".into(),
            host: "127.0.0.1".into(),
            port: 9000,
            account: Some("lark".into()),
            password_saved: true,
        }
    }

    /// A folder picked with Browse shows in its field, though the view that first follows the
    /// request still holds the folders from before.
    #[test]
    fn a_picked_folder_shows_in_its_field_the_first_time() {
        use crate::front_door::DatsView;
        let before = DatsView {
            modern: Some("/eor".into()),
            ..DatsView::default()
        };
        let mut w = Worlds::default();
        w.fill_dats(&before);
        w.original = "/typed".into();
        w.fill_dats(&before);
        assert_eq!(
            w.original, "/typed",
            "typing is kept while the host's folders stand"
        );
        for picked in [
            DatsView {
                original: Some("/1999".into()),
                ..before.clone()
            },
            DatsView {
                classic: Some("/2005".into()),
                ..before.clone()
            },
            DatsView {
                modern: Some("/later".into()),
                ..before.clone()
            },
        ] {
            let mut w = Worlds::default();
            w.fill_dats(&before);
            w.fill_dats(&before);
            w.fill_dats(&picked);
            assert_eq!(
                [w.modern, w.classic, w.original],
                [
                    picked.modern.clone().unwrap_or_default(),
                    picked.classic.clone().unwrap_or_default(),
                    picked.original.clone().unwrap_or_default()
                ]
            );
        }
    }

    #[test]
    fn a_kept_password_shows_as_dots_and_signs_in_with_none_typed() {
        let mut w = Worlds::default();
        w.fill_for(&kept());
        assert_eq!(w.password, SAVED);
        assert!(w.remember);
        let mut asked = Vec::new();
        w.sign_in(&kept(), &mut asked);
        let [FrontDoorRequest::SignIn {
            password, remember, ..
        }] = asked.as_slice()
        else {
            panic!("{asked:?}");
        };
        assert!(
            password.is_empty() && *remember,
            "the host reads the kept one"
        );
    }

    #[test]
    fn typing_replaces_the_dots() {
        let mut w = Worlds::default();
        w.fill_for(&kept());
        w.password.push('x');
        w.typed(SAVED);
        assert_eq!(w.password, "x");
        w.password.push('y');
        w.typed("x");
        assert_eq!(w.password_to_send(), "xy");
        // A deletion clears them.
        w.fill_for(&WorldRow {
            slug: "other".into(),
            ..kept()
        });
        w.password.pop();
        w.typed(SAVED);
        assert_eq!(w.password, "");
    }

    #[test]
    fn unticking_remember_or_another_account_clears_the_dots() {
        let mut w = Worlds::default();
        w.fill_for(&kept());
        w.set_remember(false);
        assert_eq!((w.password.as_str(), w.remember), ("", false));
        let mut w = Worlds::default();
        w.fill_for(&kept());
        w.account = "wren".into();
        w.typed(SAVED);
        assert_eq!(w.password, "");
        let none = WorldRow {
            password_saved: false,
            ..kept()
        };
        w.fill_for(&none);
        assert_eq!((w.password.as_str(), w.remember), ("", false));
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;

    fn ready(name: &str, host: &str, port: &str) -> bool {
        WorldForm {
            slug: None,
            name: name.into(),
            host: host.into(),
            port: port.into(),
        }
        .ready()
    }

    /// Save waits for a name, a host and a port from 1 to 65535.
    #[test]
    fn save_waits_for_a_name_a_host_and_a_port() {
        assert!(ready("Home", "localhost", "9000"));
        assert!(ready("Home", "localhost", " 65535 "));
        assert!(!ready("", "localhost", "9000"));
        assert!(!ready("  ", "localhost", "9000"));
        assert!(!ready("Home", "", "9000"));
        assert!(!ready("Home", "localhost", ""));
        assert!(!ready("Home", "localhost", "0"));
        assert!(!ready("Home", "localhost", "65536"));
        assert!(!ready("Home", "localhost", "90a"));
    }

    /// A known world matches a search when its name or host holds every word of it.
    #[test]
    fn the_search_matches_every_word_in_the_name_or_host() {
        let w = ListedRow {
            name: "Frostfell PvE".into(),
            host: "play.frostfell.example".into(),
            ..ListedRow::default()
        };
        assert!(matches(&w, ""));
        assert!(matches(&w, "frost"));
        assert!(matches(&w, "PVE frostfell"));
        assert!(matches(&w, "play.frost"));
        assert!(!matches(&w, "frost pk"));
    }
}

#[cfg(test)]
mod list_layout_tests {
    use super::*;

    /// Whether `a` and `b` share any area.
    fn meet(a: Rect, b: Rect) -> bool {
        a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
    }

    /// The client's buttons sit in a row above the sign-in, clear of the logo and the panels, on
    /// small and large windows at every scale they offer, Mac Retina sizes included.
    #[test]
    fn the_client_buttons_sit_above_the_sign_in_clear_of_the_logo() {
        let art = crate::art::Art::new(std::sync::Arc::new(crate::pieces::Pieces::built_in()));
        let mut lowered = false;
        for screen in [
            (640.0, 480.0),
            (800.0, 600.0),
            (600.0, 1080.0),
            (700.0, 1080.0),
            (1024.0, 768.0),
            (1280.0, 720.0),
            (1280.0, 800.0),
            (1920.0, 1080.0),
            (2560.0, 1600.0),
            (2880.0, 1800.0),
            (3024.0, 1964.0),
            (3840.0, 2160.0),
        ] {
            for step in crate::scale::STEPS {
                if !crate::scale::offered(step, screen) {
                    continue;
                }
                let k = (screen.1 / 1080.0).clamp(0.5, 1.0) * step;
                let mut list = crate::draw::DrawList::default();
                let mut p = Painter {
                    list: &mut list,
                    art: &art,
                    scale: k,
                    screen,
                    fade: 1.0,
                };
                let logo = logo(&mut p);
                let at = ListLayout::new(screen, k, logo);
                let case = format!("{screen:?} at {step}");
                assert!(!meet(at.row, logo), "{case}: the row meets the logo");
                assert!(
                    at.row.bottom() < at.sign.y,
                    "{case}: the row meets the sign-in"
                );
                assert!(
                    at.row.bottom() < at.list.y,
                    "{case}: the row meets the list"
                );
                assert!(!meet(at.list, logo) && !meet(at.sign, logo), "{case}");
                assert!(
                    at.row.y >= 0.0 && at.row.right() <= screen.0,
                    "{case}: off screen"
                );
                let cw = (at.row.w - 20.0 * k) / 3.0;
                assert!(
                    kit::button_label_width(&p, "Data Files") < cw,
                    "{case}: the labels fit"
                );
                lowered |= at.row.y > logo.bottom();
            }
        }
        assert!(lowered, "a narrow window lowers the row below the logo");
    }
}
