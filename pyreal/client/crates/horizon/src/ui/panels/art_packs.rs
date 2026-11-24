//! The Graphics page's art pack: which of the library's packs is drawn over the built-in pack, or
//! none, first under Surfaces & Detail and shown only while the library holds one (the Data Files
//! panel keeps the library).

use super::Windows;
use crate::art_packs::ArtPackRequest;
use crate::draw::Rect;
use crate::ui::kit::Ctx;
use crate::ui::paint::Painter;
use pyreal_client_contract::options::sheet::Impact;

/// The caption of the row choosing the pack.
pub const CHOICE_CAPTION: &str = "Selected Art Pack";

/// What the choice does, for its tooltip.
const NOTE: &str = "Chooses an installed art pack to draw over the built-in art pack. \
                    Switching packs reloads the world around you.";

/// The first choice: the built-in pack alone.
pub const NONE: &str = "Built-in";

impl Windows {
    /// The row choosing the pack, across `row` of the page's `list`.
    pub(super) fn art_pack_choice(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        row: Rect,
        list: Rect,
    ) {
        let Some(view) = self.options_page.art_packs.clone() else {
            return;
        };
        let k = p.scale;
        let at = Rect::new(row.x + 16.0 * k, row.y, row.w * 0.5, row.h);
        self.caption(
            p,
            ctx,
            at,
            list,
            CHOICE_CAPTION,
            Some(NOTE),
            view.status.as_deref(),
            Some(Impact::Pack),
        );
        let control = Rect::new(
            row.x + row.w * 0.55,
            row.y + 3.0 * k,
            row.w * 0.42,
            28.0 * k,
        );
        self.options_page
            .controls
            .push((CHOICE_CAPTION.to_owned(), control));
        let labels: Vec<String> = std::iter::once(NONE.to_owned())
            .chain(view.packs.iter().map(crate::art_packs::ArtPackRow::label))
            .collect();
        let names: Vec<&str> = labels.iter().map(String::as_str).collect();
        let shown = view
            .chosen
            .as_ref()
            .and_then(|c| view.packs.iter().position(|r| &r.path == c))
            .map_or(0, |i| i + 1);
        if let Some(i) = self.dropdown(p, ctx, CHOICE_CAPTION, control, &names, shown) {
            let path = i
                .checked_sub(1)
                .and_then(|i| view.packs.get(i))
                .map(|r| r.path.clone());
            self.options_page
                .art_pack_requests
                .push(ArtPackRequest::Choose(path));
        }
    }
}
