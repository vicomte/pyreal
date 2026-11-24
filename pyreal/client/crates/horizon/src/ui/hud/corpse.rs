//! The corpse on the radar: a stone on the rim the way to the place the player died, another on
//! the place itself while it is in reach, and Set Waypoint to Corpse under the radar while no
//! waypoint is set. A right click on the rim's stone forgets the corpse.

use super::Hud;
use crate::draw::{Rect, WHITE};
use crate::ui::corpse as cp;
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::Painter;
use crate::ui::waypoint::{distance_text, Waypoints};

impl Hud {
    /// The corpse on the minimap `r` turned to `heading` with `range` metres to its rim. A click
    /// on its stone sets the waypoint to it, and a right click forgets it.
    pub(super) fn minimap_corpse(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        waypoints: &mut Waypoints,
        (r, heading, range): (Rect, f32, f32),
    ) {
        let k = p.scale;
        let waypoint = waypoints.corpse.waypoint(&state.name);
        let followed = waypoints.corpse.is_followed(waypoints.active.as_ref());
        // Under the coordinates, where Clear Waypoint stands while a waypoint is set.
        if let (Some(w), None) = (&waypoint, &waypoints.active) {
            let label = crate::ui::panels::map::SET_WAYPOINT_TO_CORPSE;
            let bw = kit::button_label_width(p, label) + 24.0 * k;
            let button = Rect::new(r.x + (r.w - bw) / 2.0, r.bottom() + 30.0 * k, bw, 24.0 * k);
            if ctx.over(&button) {
                self.tip("Your corpse", vec![crate::ui::waypoint::set_text(w)]);
            }
            if kit::button(p, ctx, button, label, true) {
                waypoints.set(w.clone());
            }
        }
        let Some((bearing, metres)) = state
            .here
            .and_then(|here| waypoints.corpse.way(here, state.in_dungeon, state.cell))
        else {
            return;
        };
        let Some(mark) = p.art.cached(cp::MARK_KEY, || Some(cp::mark_image())) else {
            return;
        };
        let centre = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let a = bearing - heading.to_radians();
        let (sin, cos) = (
            pyreal_primitives::num::math::sinf(a),
            pyreal_primitives::num::math::cosf(a),
        );
        let radius = r.w / 2.0 - 16.0 * k;
        #[allow(clippy::cast_possible_truncation)] // LINT-OK: metres within the world.
        let reach = metres as f32 / range;
        // Within reach a small stone stands on the place itself, its foot on it.
        if reach < 1.0 {
            let s = 12.0 * k;
            let (x, y) = (
                centre.0 + sin * reach * radius,
                centre.1 - cos * reach * radius,
            );
            p.sprite(
                &mark,
                Rect::new(x - s / 2.0, y - s * cp::MARK_FOOT, s, s),
                WHITE,
            );
        }
        // On the rim the way to it, unless the waypoint's arrow already points there.
        if followed && self.waypoint_style.on_radar() {
            return;
        }
        let s = 20.0 * k;
        let (x, y) = (centre.0 + sin * radius, centre.1 - cos * radius);
        let stone = Rect::new(x - s / 2.0, y - s / 2.0, s, s);
        p.sprite(&mark, stone, WHITE);
        self.corpse_stone = Some(stone);
        if !ctx.over(&stone) {
            return;
        }
        let mut lines = vec![format!("{} away.", distance_text(metres))];
        if waypoint.is_some() && !followed {
            lines.push("Click to set a waypoint to it.".to_owned());
        }
        lines.push(cp::FORGET_TIP.to_owned());
        self.tip("Your corpse", lines);
        if ctx.input.right_clicked(&stone) {
            waypoints.corpse.forget();
        } else if let (true, Some(w)) = (ctx.input.clicked(&stone) && !followed, waypoint) {
            waypoints.set(w);
        }
    }
}
