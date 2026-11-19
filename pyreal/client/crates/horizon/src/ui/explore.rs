//! Explore mode's overlay: where the camera is and how fast it flies, the sky's hour, the weather
//! and the clouds, and the way back to the worlds; the Map and Settings buttons in the corner, and
//! the waypoint above them while one is set. The world takes every press off them.

use pyreal_client_model::cmd::clouds::NAMED_COVERS;
use pyreal_client_model::cmd::weather::Weather;
use pyreal_client_runtime::explore::{Ask, ExploreView, SPEEDS};
use pyreal_primitives::EraId;

use crate::art::Family;
use crate::draw::Rect;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};

/// The weathers the overlay steps through, with their names.
const WEATHERS: [(Weather, &str); 5] = [
    (Weather::Auto, "Default"),
    (Weather::Clear, "Clear"),
    (Weather::Rain, "Rain"),
    (Weather::Storm, "Storm"),
    (Weather::Snow, "Snow"),
];

/// The cloud covers the overlay steps through, with their names: the day's own, then `@clouds`'
/// named covers.
fn cloud_covers() -> [(Option<f32>, &'static str); 4] {
    let [none, thin, thick] = NAMED_COVERS.map(|(_, c)| Some(c));
    [
        (None, "Default"),
        (none, "None"),
        (thin, "Thin"),
        (thick, "Thick"),
    ]
}

/// What the keys do, under the panel's controls.
const KEYS: &str = "Fly with W, A, S and D, rise with E and sink with Q. Hold a mouse button to \
                    look round. The wheel changes the speed. C copies the place and R copies the \
                    report line.";

/// The name of the files an era draws.
#[must_use]
pub fn era_name(era: EraId) -> &'static str {
    match era {
        EraId::Launch => "1999 Launch",
        EraId::Infiltration => "February 2005",
        EraId::Eor => "End of Retail",
    }
}

/// Draw the overlay over the world and say what the player asked. `back` names the way out:
/// back to the worlds with them, out of the client without.
pub fn frame(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    view: &ExploreView,
    back: &str,
    asks: &mut Vec<Ask>,
) {
    let k = p.scale;
    let (_, sh) = p.screen;
    let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
    let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
    let value = TextStyle::new(Family::Numerals, 14.0, 0xFFFF_FFFF).edge(0xFF00_0000);
    let dim = TextStyle::new(Family::Body, 13.0, 0xFFC8_D8E8).edge(0xFF00_0000);
    let note = TextStyle::new(Family::Body, 13.0, 0xFF9A_E89A).edge(0xFF00_0000);
    let w = 360.0 * k;
    let inner = w - 32.0 * k;
    let keys = p.wrap(&dim, KEYS, inner);
    let line = p.line_height(&dim);
    #[allow(clippy::cast_precision_loss)]
    let keys_h = line * keys.len() as f32;
    // A note (a copy made, or frame timings saved by their key) over the keys, as many lines as it takes.
    let noted = view
        .copied
        .as_deref()
        .map(|n| p.wrap(&note, n, inner))
        .unwrap_or_default();
    #[allow(clippy::cast_precision_loss)]
    let note_h = line * noted.len().saturating_sub(1) as f32;
    let h = 338.0 * k + keys_h + note_h;
    let r = Rect::new(24.0 * k, sh - 24.0 * k - h, w, h);
    kit::grounded_panel(p, r, 0.85, 2);
    let x = r.x + 16.0 * k;
    p.text(&head, x, r.y + 10.0 * k, "Explore");
    p.text_in(
        &dim,
        Rect::new(x, r.y + 16.0 * k, inner, 18.0 * k),
        Align::Right,
        era_name(view.era),
    );
    let mut y = r.y + 46.0 * k;
    p.text(
        &value,
        x,
        y,
        view.coordinates.as_deref().unwrap_or("Beyond the map"),
    );
    y += 22.0 * k;
    p.text(
        &dim,
        x,
        y,
        &format!("{:.0} m above the ground", view.height),
    );
    y += 28.0 * k;
    // The speed, a step at a time.
    let at = SPEEDS
        .iter()
        .position(|s| (*s - view.speed).abs() < 0.5)
        .unwrap_or(0);
    let names: Vec<String> = SPEEDS.iter().map(|s| format!("{s:.0} m/s")).collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    let control = |y: f32| Rect::new(x + 90.0 * k, y, inner - 90.0 * k, 26.0 * k);
    p.text(&label, x, y + 5.0 * k, "Speed");
    if let Some(i) = kit::stepper(p, ctx, control(y), &names, at) {
        let step = if i == (at + 1) % names.len() { 1 } else { -1 };
        asks.push(Ask::Speed(step));
    }
    y += 36.0 * k;
    // The sky's hour.
    p.text(&label, x, y + 5.0 * k, "Time");
    if let Some(t) = kit::track_slider(p, ctx, control(y), view.time) {
        asks.push(Ask::Time(t));
    }
    y += 26.0 * k;
    p.text_in(
        &dim,
        Rect::new(x + 90.0 * k, y, inner - 90.0 * k, 18.0 * k),
        Align::Left,
        &view.time_name,
    );
    y += 26.0 * k;
    // The weather.
    p.text(&label, x, y + 5.0 * k, "Weather");
    let names: Vec<&str> = WEATHERS.iter().map(|(_, n)| *n).collect();
    let at = WEATHERS
        .iter()
        .position(|(w, _)| *w == view.weather)
        .unwrap_or(0);
    if let Some(i) = kit::stepper(p, ctx, control(y), &names, at) {
        asks.push(Ask::Weather(WEATHERS[i].0));
    }
    y += 36.0 * k;
    // The clouds' cover.
    p.text(&label, x, y + 5.0 * k, "Clouds");
    let covers = cloud_covers();
    let names: Vec<&str> = covers.iter().map(|(_, n)| *n).collect();
    let at = covers
        .iter()
        .position(|(c, _)| *c == view.clouds)
        .unwrap_or(0);
    if let Some(i) = kit::stepper(p, ctx, control(y), &names, at) {
        asks.push(Ask::Clouds(covers[i].0));
    }
    y += 40.0 * k;
    for (i, l) in noted.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        p.text(&note, x, y + line * i as f32, l);
    }
    y += 24.0 * k + note_h;
    for (i, l) in keys.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        p.text(&dim, x, y + line * i as f32, l);
    }
    let b = Rect::new(x, r.bottom() - 48.0 * k, inner, 32.0 * k);
    ctx.input.nav.cancel(b);
    if kit::button(p, ctx, b, back, true) || ctx.input.take_key(pyreal_ui_kit::input::vk::ESCAPE) {
        asks.push(Ask::Leave);
    }
}

/// What the corner's buttons asked: the map, Settings.
#[derive(Debug, Default, Clone, Copy)]
pub struct Corner {
    pub map: bool,
    pub settings: bool,
}

/// The Map and Settings buttons in the lower right, as the game's menu bar draws them, lit while
/// their windows are open.
pub fn corner(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, open: Corner) -> Corner {
    let mut asked = Corner::default();
    let (_, places) = crate::ui::hud::menu_bar(p, 2);
    let buttons = [
        ("menu.map", "Map", open.map, &mut asked.map),
        (
            "menu.settings",
            "Settings",
            open.settings,
            &mut asked.settings,
        ),
    ];
    for ((icon, name, lit, asked), r) in buttons.into_iter().zip(places) {
        let over = ctx.over(&r);
        crate::ui::hud::menu_icon(p, r, icon, over || lit);
        if over {
            kit::tooltip(p, ctx, name, &[]);
        }
        if ctx.input.clicked(&r) {
            *asked = true;
        }
    }
    asked
}

/// The waypoint `w` above the corner's buttons: its name, how far it is and which way from the
/// camera at `here`, and a button that clears it. Whether that button was pressed.
pub fn waypoint(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    w: &crate::ui::waypoint::Waypoint,
    here: Option<(f64, f64)>,
) -> bool {
    use crate::ui::waypoint::{compass_word, distance_text};
    let k = p.scale;
    let (sw, sh) = p.screen;
    let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(0xFF00_0000);
    let lines: Vec<String> = std::iter::once(format!("Waypoint: {}", w.label()))
        .chain(here.map(|here| {
            let (bearing, metres) = w.way_from(here);
            format!("{} to the {}", distance_text(metres), compass_word(bearing))
        }))
        .collect();
    let widest = lines
        .iter()
        .map(|l| p.measure(&label, l))
        .fold(216.0 * k, f32::max);
    let (bw, bh) = (widest + 24.0 * k, 92.0 * k);
    let r = Rect::new(sw - 14.0 * k - bw, sh - 70.0 * k - bh, bw, bh);
    kit::grounded_panel(p, r, 0.85, 2);
    let inner = r.inset(12.0 * k);
    for (i, line) in lines.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let y = inner.y + 20.0 * k * i as f32;
        p.text_in(
            &label,
            Rect::new(inner.x, y, inner.w, 18.0 * k),
            Align::Left,
            line,
        );
    }
    let b = Rect::new(inner.x, inner.bottom() - 26.0 * k, inner.w, 26.0 * k);
    kit::button(p, ctx, b, "Clear Waypoint", true)
}
