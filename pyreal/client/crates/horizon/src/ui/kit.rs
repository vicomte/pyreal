//! The interface's widgets: the kit's ([`pyreal_ui_kit::widgets`]), drawn from the interface's
//! own art, with what the interface adds to their context: the item or spell the player has in
//! hand, and where it can be dropped.

use crate::draw::Rect;
use crate::ui::paint::Painter;

pub use pyreal_ui_kit::widgets::*;

/// The per-frame context every widget is drawn with: the kit's, and the drag and drop and the
/// game's notices the interface keeps beside it.
#[derive(Debug)]
pub struct Ctx<'a> {
    /// The kit's share: the time, the input, the colours and whether the pointer is over a
    /// widget.
    pub kit: pyreal_ui_kit::widgets::Ctx<'a>,
    /// What the player has in hand.
    pub drag: &'a mut Option<crate::ui::Drag>,
    /// Where a drag can land this frame, in drawing order: what a drop there asks for, or
    /// `None` for a window that takes the drop and does nothing with it. The last one drawn
    /// under the pointer wins.
    pub drops: Vec<(Rect, Option<Drop>)>,
    /// The spell keys the game raised since the last frame.
    pub magic: Vec<pyreal_client_contract::view::MagicNotice>,
    /// Whether the player is off the ground, which refuses a log-off.
    pub airborne: bool,
    /// Whose buttons the pad has, which its marks are drawn as.
    pub pad_family: pyreal_input::pad::PadFamily,
}

impl<'a> std::ops::Deref for Ctx<'a> {
    type Target = pyreal_ui_kit::widgets::Ctx<'a>;
    fn deref(&self) -> &Self::Target {
        &self.kit
    }
}

impl std::ops::DerefMut for Ctx<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.kit
    }
}

/// What a drop asks for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Drop {
    /// The game's own drop: a slot, a container, the character, the shortcut bar, the world.
    Target(pyreal_client_contract::view::DropTarget),
    /// Onto the vendor's sell list.
    Sell,
    /// Onto the secure trade's offer, at this position.
    Trade(u32),
    /// Onto a spell bar's slot: spells only.
    SpellSlot { tab: usize, index: usize },
    /// Onto the salvage window.
    Salvage,
    /// Onto a window that takes what is dropped on it itself.
    Window,
    /// Onto a slot of the pad's cross hotbars, which binds it there.
    CrossSlot(usize),
}

impl From<pyreal_client_contract::view::DropTarget> for Drop {
    fn from(t: pyreal_client_contract::view::DropTarget) -> Self {
        Self::Target(t)
    }
}

impl Drop {
    /// The request a drop of `item` here raises, if any. `part` is the splitter's `(split, max)`
    /// while it holds back part of `item`'s stack.
    #[must_use]
    pub fn request(
        self,
        item: pyreal_primitives::ObjectId,
        part: Option<(u32, u32)>,
    ) -> Option<pyreal_client_contract::UiRequest> {
        use pyreal_client_contract::UiRequest;
        match (self, part) {
            (Self::Sell, Some((split, max))) => {
                Some(UiRequest::VendorSplitToSell { item, split, max })
            }
            (Self::Trade(_), Some((split, max))) => {
                Some(UiRequest::TradeSplitItem { item, split, max })
            }
            (Self::Target(target), _) => Some(UiRequest::DragDrop { item, target }),
            (Self::Sell, None) => Some(UiRequest::VendorAddToSell { item }),
            (Self::Trade(position), None) => Some(UiRequest::TradeAddItem { item, position }),
            (Self::Salvage, _) => Some(UiRequest::SalvageList(
                pyreal_client_contract::panels::salvage::SalvageAction::Add(item),
            )),
            // A spell bar takes spells; an item dropped there goes nowhere.
            (Self::SpellSlot { .. } | Self::Window | Self::CrossSlot(_), _) => None,
        }
    }
}

/// Draw a window's frame: the kit's ([`pyreal_ui_kit::widgets::window`]); a drop on the window
/// lands in it, not on what is under it.
pub fn window(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &mut WindowState,
    title: &str,
    size: (f32, f32),
    default_pos: (f32, f32),
) -> WindowOut {
    let out = pyreal_ui_kit::widgets::window(p, &mut ctx.kit, state, title, size, default_pos);
    ctx.drops.push((out.rect, None));
    out
}

/// A list of rows `step` apart that scrolls inside `area` and is clipped to it. While it is
/// drawn, the bands a row can overhang above and below `area` take no press, so what stands
/// round the list keeps its own. [`Self::end`] closes it.
#[derive(Debug, Clone, Copy)]
pub struct ScrollList {
    pub area: Rect,
    step: f32,
    /// How far the rows are scrolled.
    pub offset: f32,
    /// How wide a row is: the list's width, less the bar while it scrolls.
    pub width: f32,
    occluded: usize,
}

impl ScrollList {
    /// Begin a list of `rows` rows in `area`, its scroll kept in `offset`.
    pub fn begin(
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        area: Rect,
        rows: usize,
        step: f32,
        offset: &mut f32,
    ) -> Self {
        let k = p.scale;
        #[allow(clippy::cast_precision_loss)]
        let content = step * rows as f32;
        let offset = scroll(p, ctx, area, content, offset);
        let bar = if content > area.h {
            scrollbar_width(p).unwrap_or(4.0) * k + 4.0 * k
        } else {
            0.0
        };
        p.list.push_clip(area);
        let occluded = ctx.input.occluders.len();
        ctx.input.occluders.extend([
            Rect::new(area.x, area.y - step, area.w, step),
            Rect::new(area.x, area.bottom(), area.w, step),
        ]);
        Self {
            area,
            step,
            offset,
            width: (area.w - bar).max(0.0),
            occluded,
        }
    }

    /// The top of row `i`, scrolled.
    #[must_use]
    pub fn top(&self, i: usize) -> f32 {
        #[allow(clippy::cast_precision_loss)]
        let y = self.area.y + self.step * i as f32 - self.offset;
        y
    }

    /// Row `i`, `h` tall across the list, or `None` while it is wholly out of sight.
    #[must_use]
    pub fn row(&self, i: usize, h: f32) -> Option<Rect> {
        let r = Rect::new(self.area.x, self.top(i), self.width, h);
        (r.bottom() >= self.area.y && r.y <= self.area.bottom()).then_some(r)
    }

    /// Close the list: the bands round it take presses again, and the clip is let go.
    pub fn end(self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>) {
        ctx.input.occluders.truncate(self.occluded);
        p.list.pop_clip();
    }
}
