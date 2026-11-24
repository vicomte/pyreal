//! Keep and Junk marks on the character's items, and the question before a Keep item leaves.
//!
//! Marks are kept per world and character in a small text file in the client's settings folder
//! ([`file_name`]): one line per item, `junk 80000011 2426 Black Opal`, by the item's id, class
//! and name. A mark holds only while all three match, since the server gives a destroyed item's
//! id to a new one. A Keep item (or a pack holding one) asks before it is sold, salvaged, traded,
//! given or dropped ([`guarded`]); Junk items are what a vendor's Add Junk puts on the sell list,
//! less those it keeps back ([`kept_back`]).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use pyreal_client_contract::panels::salvage::SalvageAction;
use pyreal_client_contract::view::DropTarget;
use pyreal_client_contract::UiRequest;
use pyreal_primitives::ObjectId;

use super::Windows;
use crate::ui::game::{GameState, Item};
use crate::ui::kit::Ctx;
use crate::ui::paint::Painter;
use crate::ui::Outcome;

/// A mark the player put on an item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Keep,
    Junk,
}

impl Mark {
    /// The mark as the file writes it.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Junk => "junk",
        }
    }
}

/// A mark and the item it was put on. A class of `0` and no name is a line from before marks
/// named their item: the item has not been seen since.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marked {
    pub mark: Mark,
    pub wcid: u32,
    pub name: String,
}

impl Marked {
    /// The mark from an older file, which named only the id.
    #[must_use]
    pub const fn is_old(&self) -> bool {
        self.wcid == 0 && self.name.is_empty()
    }

    /// Whether this mark was put on `item`: same class and name.
    #[must_use]
    pub fn fits(&self, item: &Item) -> bool {
        self.wcid == item.wcid && self.name == item.name
    }
}

/// The question about an old Junk mark: the item, as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JunkAsked {
    pub id: ObjectId,
    pub wcid: u32,
    pub name: String,
}

/// The marks of the character in the world, and the file they are kept in.
#[derive(Debug, Default)]
pub struct Marks {
    marks: BTreeMap<ObjectId, Marked>,
    path: Option<PathBuf>,
    loaded_for: Option<String>,
    /// The marks of items not carried have been dropped since the character came in.
    pruned: bool,
    /// Old Junk marks waiting for the player to say they still hold, and those already asked.
    pub junk_asked: Vec<JunkAsked>,
    asked: BTreeSet<ObjectId>,
}

/// How long after entering the world the character's items are taken to have all come, in
/// seconds: from then a mark on an item not carried is dropped.
pub const SETTLE_SECONDS: f64 = 10.0;

/// How long Add Junk waits between asking for appraisals of Junk items, in seconds.
pub const JUNK_ASK_GAP: f64 = 0.25;

/// How many kept-back items Add Junk's tooltip names.
const TIP_NAMES: usize = 8;

/// The file, in the settings folder, a character's marks are kept in: by its world's address
/// and its name. Anything a file name cannot hold is made an underscore.
#[must_use]
pub fn file_name(address: &str, name: &str) -> String {
    let safe = |t: &str| -> String {
        t.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    format!("horizon-marks-{}-{}.txt", safe(address), safe(name))
}

/// The marks in a file's text; a line it cannot read is passed over. A line with only an id is
/// an old mark ([`Marked::is_old`]).
#[must_use]
pub fn parse(text: &str) -> BTreeMap<ObjectId, Marked> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.trim().splitn(4, ' ').filter(|w| !w.is_empty());
            let mark = match words.next()? {
                "keep" => Mark::Keep,
                "junk" => Mark::Junk,
                _ => return None,
            };
            let id = u32::from_str_radix(words.next()?, 16).ok()?;
            let (wcid, name) = match words.next() {
                Some(w) => (w.parse().ok()?, words.next()?.trim().to_owned()),
                None => (0, String::new()),
            };
            Some((ObjectId(id), Marked { mark, wcid, name }))
        })
        .collect()
}

/// The marks as the file keeps them.
#[must_use]
pub fn to_text(marks: &BTreeMap<ObjectId, Marked>) -> String {
    marks
        .iter()
        .map(|(id, m)| {
            if m.is_old() {
                format!("{} {:08x}\n", m.mark.word(), id.0)
            } else {
                format!("{} {:08x} {} {}\n", m.mark.word(), id.0, m.wcid, m.name)
            }
        })
        .collect()
}

impl Marks {
    /// Read the marks kept for character `name` on the world at `address` in `dir`, once per
    /// character. With no folder, marks last only while the client runs.
    pub fn load_for(&mut self, dir: Option<&Path>, address: &str, name: &str) {
        let key = format!("{address}|{name}");
        if name.is_empty() || self.loaded_for.as_deref() == Some(key.as_str()) {
            return;
        }
        self.path = dir.map(|d| d.join(file_name(address, name)));
        self.marks = self
            .path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| parse(&t))
            .unwrap_or_default();
        self.loaded_for = Some(key);
        self.pruned = false;
        self.junk_asked.clear();
        self.asked.clear();
    }

    /// The character left the world: the marks are checked against its items again next time.
    pub fn left_world(&mut self) {
        self.pruned = false;
    }

    /// The mark on `item`, if it has one: a mark put on this id, class and name. An old Keep mark
    /// holds on the id alone, and an old Junk mark not at all until the player says it does.
    #[must_use]
    pub fn get(&self, item: &Item) -> Option<Mark> {
        let m = self.marks.get(&item.id)?;
        if m.fits(item) || (m.is_old() && m.mark == Mark::Keep) {
            Some(m.mark)
        } else {
            None
        }
    }

    /// Every mark, by id, as kept.
    #[must_use]
    pub const fn all(&self) -> &BTreeMap<ObjectId, Marked> {
        &self.marks
    }

    /// Mark `item` (or take its mark off, with `None`), and keep it.
    pub fn set(&mut self, item: &Item, mark: Option<Mark>) {
        let new = mark.map(|mark| Marked {
            mark,
            wcid: item.wcid,
            name: item.name.clone(),
        });
        let before = match new.clone() {
            Some(m) => self.marks.insert(item.id, m),
            None => self.marks.remove(&item.id),
        };
        self.junk_asked.retain(|q| q.id != item.id);
        if before != new {
            self.save();
        }
    }

    /// Hold the marks to the items `carried`, worn ones among them. A mark whose id has come to
    /// a different item is dropped. An old Keep mark takes the item's class and name; an old Junk
    /// mark is asked about ([`Self::junk_asked`]). Once `settled` (the character's items have all
    /// come), the marks of items not carried are dropped, once per entry into the world.
    pub fn follow<'a>(&mut self, carried: impl IntoIterator<Item = &'a Item>, settled: bool) {
        let mut changed = false;
        let mut seen = BTreeSet::new();
        for item in carried {
            seen.insert(item.id);
            // Its description has not come yet.
            if item.wcid == 0 || item.name.is_empty() {
                continue;
            }
            let Some(m) = self.marks.get_mut(&item.id) else {
                continue;
            };
            if m.is_old() {
                if m.mark == Mark::Keep {
                    m.wcid = item.wcid;
                    m.name.clone_from(&item.name);
                    changed = true;
                } else if self.asked.insert(item.id) {
                    self.junk_asked.push(JunkAsked {
                        id: item.id,
                        wcid: item.wcid,
                        name: item.name.clone(),
                    });
                }
            } else if !m.fits(item) {
                self.marks.remove(&item.id);
                changed = true;
            }
        }
        if settled && !self.pruned {
            self.pruned = true;
            let before = self.marks.len();
            self.marks.retain(|id, _| seen.contains(id));
            changed |= self.marks.len() != before;
            self.junk_asked.retain(|q| seen.contains(&q.id));
        }
        if changed {
            self.save();
        }
    }

    /// The player's answer about the first old Junk mark waiting: yes keeps it on the item as it
    /// is now, no takes it off.
    pub fn answer_junk(&mut self, yes: bool) {
        if self.junk_asked.is_empty() {
            return;
        }
        let q = self.junk_asked.remove(0);
        if yes {
            self.marks.insert(
                q.id,
                Marked {
                    mark: Mark::Junk,
                    wcid: q.wcid,
                    name: q.name,
                },
            );
        } else {
            self.marks.remove(&q.id);
        }
        self.save();
    }

    fn save(&self) {
        let Some(path) = &self.path else {
            return;
        };
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(path, to_text(&self.marks)));
        if let Err(e) = written {
            tracing::warn!("the item marks were not kept: {e}");
        }
    }
}

/// Why Add Junk leaves `item` off the sell list, or `None` to list it. Attuned, inscribed,
/// imbued, tinkered and can't-be-sold are known only from an appraisal, so an item not yet
/// appraised is kept back until it is.
#[must_use]
pub fn kept_back(item: &Item) -> Option<&'static str> {
    let s = &item.safety;
    if s.worn || item.worn != 0 {
        return Some("worn or wielded");
    }
    if s.rare {
        return Some("rare");
    }
    if s.retained {
        return Some("retained");
    }
    if s.value.unwrap_or(0) == 0 {
        return Some("no value");
    }
    match s.vendor_refusal {
        Some(0) | None => {}
        Some(1) => return Some("this vendor does not buy it"),
        Some(2) => return Some("no value"),
        Some(3) => return Some("too cheap for this vendor"),
        Some(_) => return Some("too valuable for this vendor"),
    }
    if !s.appraised {
        return Some("not appraised yet");
    }
    [
        (s.attuned, "attuned"),
        (s.inscribed, "inscribed"),
        (s.imbued, "imbued"),
        (s.tinkered, "tinkered"),
        (s.unsellable, "cannot be sold"),
    ]
    .into_iter()
    .find_map(|(held, why)| held.then_some(why))
}

/// The lines of Add Junk's tooltip: what it does, and each item it keeps back with why.
#[must_use]
pub fn junk_tip(kept: &[(ObjectId, String, &'static str)]) -> Vec<String> {
    let mut lines = vec!["Put every item marked Junk on the sell list.".to_owned()];
    lines.extend(
        kept.iter()
            .take(TIP_NAMES)
            .map(|(_, name, why)| format!("{name}. Kept back: {why}.")),
    );
    if kept.len() > TIP_NAMES {
        lines.push(format!("{} more kept back.", kept.len() - TIP_NAMES));
    }
    lines
}

/// What Add Junk does: the items it lists, and those it keeps back with why.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JunkPlan {
    pub listed: Vec<UiRequest>,
    pub kept: Vec<(ObjectId, String, &'static str)>,
}

/// What `request` does with an item that a Keep mark asks about first, and the item: selling,
/// salvaging, trading, giving or dropping it. A drag onto the world gives it to whoever is under
/// the pointer, or drops it.
#[must_use]
pub fn guarded(request: &UiRequest) -> Option<(ObjectId, &'static str)> {
    match request {
        UiRequest::VendorAddToSell { item } | UiRequest::VendorSplitToSell { item, .. } => {
            Some((*item, "sell"))
        }
        UiRequest::SalvageList(SalvageAction::Add(item)) => Some((*item, "salvage")),
        UiRequest::TradeAddItem { item, .. } | UiRequest::TradeSplitItem { item, .. } => {
            Some((*item, "trade"))
        }
        UiRequest::GiveTo { item, .. } => Some((*item, "give")),
        UiRequest::PutInWorld(item) => Some((*item, "drop")),
        UiRequest::DragDrop {
            item,
            target: DropTarget::World,
        } => Some((*item, "give or drop")),
        _ => None,
    }
}

/// Every item the character has: in the pack, the side packs and what they hold, and worn.
pub fn carried(state: &GameState) -> impl Iterator<Item = &Item> {
    state
        .pack
        .iter()
        .chain(state.side_packs.iter().map(|(p, _)| p))
        .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
        .chain(state.equipped.iter())
}

/// The name of the Keep item `item` is, or that it holds when it is a side pack.
#[must_use]
pub fn keep_in(state: &GameState, marks: &Marks, item: ObjectId) -> Option<String> {
    if let Some(it) = carried(state).find(|i| i.id == item) {
        if marks.get(it) == Some(Mark::Keep) {
            return Some(it.name.clone());
        }
    }
    state
        .side_packs
        .iter()
        .find(|(pack, _)| pack.id == item)
        .and_then(|(_, items)| {
            items
                .iter()
                .find(|i| marks.get(i) == Some(Mark::Keep))
                .map(|i| i.name.clone())
        })
}

/// The question before a Keep item goes: the held request and the question's words.
#[derive(Debug, Clone)]
pub struct KeepAsked {
    pub request: UiRequest,
    pub text: String,
}

impl Windows {
    /// Hold back this frame's requests that would sell, salvage, trade, give or drop a Keep item,
    /// each to be asked about in turn, and let through those the player said yes to.
    pub fn hold_keep(&mut self, state: &GameState, requests: &mut Vec<UiRequest>) {
        let mut kept = Vec::with_capacity(requests.len());
        for r in requests.drain(..) {
            let held = guarded(&r).and_then(|(item, verb)| {
                keep_in(state, &self.marks, item).map(|name| {
                    let own = carried(state)
                        .find(|i| i.id == item)
                        .and_then(|i| self.marks.get(i));
                    let text = if own == Some(Mark::Keep) {
                        format!("{name} is marked Keep. Do you still want to {verb} it?")
                    } else {
                        format!("This pack holds {name}, which is marked Keep. Do you still want to {verb} it?")
                    };
                    (r.clone(), text)
                })
            });
            match held {
                Some((request, text)) => self.keep_asked.push(KeepAsked { request, text }),
                None => kept.push(r),
            }
        }
        kept.append(&mut self.keep_released);
        *requests = kept;
    }

    /// The question before a Keep item goes, over everything else; Yes lets its request through.
    /// With none waiting, the question about an old Junk mark.
    pub fn keep_question(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        let Some(asked) = self.keep_asked.first() else {
            self.junk_question(p, ctx);
            return;
        };
        let title = if self.keep_asked.len() > 1 {
            format!("Keep ({} more waiting)", self.keep_asked.len() - 1)
        } else {
            "Keep".to_owned()
        };
        if let Some(b) =
            crate::ui::pregame::dialog_box(p, ctx, &title, &asked.text, &["Yes", "No"], true, 0)
        {
            let asked = self.keep_asked.remove(0);
            if b == 0 {
                self.keep_released.push(asked.request);
            }
        }
    }

    /// Whether an old Junk mark still holds, asked the first time its item is seen.
    fn junk_question(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        let Some(q) = self.marks.junk_asked.first() else {
            return;
        };
        let text = format!(
            "{} was marked Junk by an older version of Pyreal. Keep it marked Junk?",
            q.name
        );
        if let Some(b) =
            crate::ui::pregame::dialog_box(p, ctx, "Junk", &text, &["Yes", "No"], true, 0)
        {
            self.marks.answer_junk(b == 0);
        }
    }

    /// What Add Junk does: every Junk item carried in the pack and side packs, and not yet on
    /// the sell list, goes on it unless [`kept_back`] says why not.
    #[must_use]
    pub fn junk_plan(&self, state: &GameState) -> JunkPlan {
        let listed = state
            .shop
            .as_ref()
            .map(|s| s.sell_list.iter().map(|r| r.item).collect::<Vec<_>>())
            .unwrap_or_default();
        let mut plan = JunkPlan::default();
        for i in state
            .pack
            .iter()
            .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
            .filter(|i| self.marks.get(i) == Some(Mark::Junk) && !listed.contains(&i.id))
        {
            match kept_back(i) {
                Some(why) => plan.kept.push((i.id, i.name.clone(), why)),
                None => plan.listed.push(UiRequest::VendorAddToSell { item: i.id }),
            }
        }
        plan
    }

    /// Ask the game to appraise the next Junk item not yet appraised, one every [`JUNK_ASK_GAP`],
    /// so Add Junk can check what only an appraisal says.
    pub fn ask_junk_appraisals(&mut self, state: &GameState, now: f64, out: &mut Outcome) {
        if now - self.junk_asked_at < JUNK_ASK_GAP {
            return;
        }
        let marks = &self.marks;
        let book = &mut self.appraisals;
        let next = state
            .pack
            .iter()
            .chain(state.side_packs.iter().flat_map(|(_, items)| items.iter()))
            .filter(|i| !i.safety.appraised && marks.get(i) == Some(Mark::Junk))
            .find(|i| book.want(i, None, now));
        if let Some(i) = next {
            self.junk_asked_at = now;
            out.requests.push(UiRequest::Appraise(i.id));
        }
    }

    /// The requests that put every Junk item carried, and not yet on the sell list, on it, less
    /// those kept back.
    #[must_use]
    pub fn add_junk(&self, state: &GameState) -> Vec<UiRequest> {
        self.junk_plan(state).listed
    }
}
