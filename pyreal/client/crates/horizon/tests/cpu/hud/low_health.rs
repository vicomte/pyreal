//! Low Health Warning: under the threshold the screen's edges redden under the HUD and the health
//! bar glows; recovered health, death or the setting turned off fades them out.

use pyreal_client_contract::actions::pyreal::HIDE_INTERFACE;
use pyreal_horizon::draw::Quad;
use pyreal_horizon::options::HorizonOptions;
use pyreal_horizon::ui::game::{GameState, Vital};
use pyreal_horizon::ui::low_health::LowHealthAt;
use pyreal_horizon::ui::panels::options::tab;
use pyreal_horizon::ui::panels::WindowId;

use crate::Harness;

fn at_health(current: u32) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        health: Vital { current, max: 100 },
        ..GameState::default()
    }
}

/// `seconds` of frames at 60 a second.
fn run(h: &mut Harness, state: &GameState, seconds: f64) {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    for _ in 0..(seconds * 60.0).round() as usize {
        h.frame(state);
    }
}

/// The reddened edges as drawn this frame: the one quad stretched over the screen.
fn edges(h: &Harness) -> Option<(usize, Quad)> {
    h.list
        .quads
        .iter()
        .enumerate()
        .find(|(_, q)| q.stretched)
        .map(|(i, q)| (i, *q))
}

fn alpha(q: &Quad) -> u32 {
    q.colour >> 24
}

#[test]
fn the_edges_redden_under_the_threshold_and_fade_when_health_recovers() {
    let mut h = Harness::with_built_in_art();
    run(&mut h, &at_health(80), 0.5);
    assert!(edges(&h).is_none());
    assert!(!h.ui.hud.low_health.in_danger());
    run(&mut h, &at_health(20), 0.5);
    assert!(h.ui.hud.low_health.in_danger());
    assert!((h.ui.hud.low_health.shown() - 1.0).abs() < 1e-6);
    let (at, q) = edges(&h).expect("the edges drawn");
    // First, under the whole HUD, and over the whole screen.
    assert_eq!(at, 0, "the edges are under the HUD");
    assert!(h.list.quads.len() > 1);
    assert!(q.dst.x <= 0.0 && q.dst.y <= 0.0);
    assert!(q.dst.right() >= 1920.0 && q.dst.bottom() >= 1080.0);
    // Recovered: they fade, and are gone within a second.
    run(&mut h, &at_health(60), 0.2);
    assert!(!h.ui.hud.low_health.in_danger());
    assert!(edges(&h).is_some(), "fading, not cut");
    run(&mut h, &at_health(60), 1.0);
    assert!(edges(&h).is_none());
    assert!(h.ui.hud.low_health.shown() <= 0.0);
}

#[test]
fn the_edges_grow_and_deepen_as_health_falls() {
    let measure = |current: u32| {
        let mut h = Harness::with_built_in_art();
        run(&mut h, &at_health(current), 0.5);
        // The same place in the heartbeat for both.
        let mut lightest = u32::MAX;
        let mut reach = f32::MIN;
        for _ in 0..120 {
            h.frame(&at_health(current));
            let (_, q) = edges(&h).expect("the edges drawn");
            lightest = lightest.min(alpha(&q));
            reach = reach.max(q.dst.x);
        }
        (lightest, reach)
    };
    let (near_line, near_reach) = measure(22);
    let (near_death, far_reach) = measure(3);
    assert!(near_death > near_line, "{near_death} over {near_line}");
    assert!(
        far_reach > near_reach,
        "the red reaches further in: {far_reach} {near_reach}"
    );
}

#[test]
fn the_edges_beat() {
    let mut h = Harness::with_built_in_art();
    run(&mut h, &at_health(10), 0.5);
    let mut seen = Vec::new();
    for _ in 0..90 {
        h.frame(&at_health(10));
        seen.push(alpha(&edges(&h).expect("the edges drawn").1));
    }
    let (lo, hi) = (seen.iter().min().unwrap(), seen.iter().max().unwrap());
    assert!(hi - lo >= 10, "a beat shows: {lo}..{hi}");
}

#[test]
fn the_threshold_follows_the_setting_and_holds_at_the_line() {
    let mut h = Harness::with_built_in_art();
    assert_eq!(h.ui.options.low_health, LowHealthAt::Quarter);
    h.frame(&at_health(25));
    assert!(
        !h.ui.hud.low_health.in_danger(),
        "a quarter is not under it"
    );
    h.frame(&at_health(24));
    assert!(h.ui.hud.low_health.in_danger());
    // A regeneration tick at the line does not flicker it.
    h.frame(&at_health(26));
    assert!(h.ui.hud.low_health.in_danger());
    h.frame(&at_health(27));
    assert!(!h.ui.hud.low_health.in_danger());
    h.ui.options.low_health = LowHealthAt::Fifth;
    h.frame(&at_health(24));
    assert!(!h.ui.hud.low_health.in_danger());
    h.frame(&at_health(19));
    assert!(h.ui.hud.low_health.in_danger());
    h.ui.options.low_health = LowHealthAt::Third;
    h.frame(&at_health(100));
    h.frame(&at_health(33));
    assert!(h.ui.hud.low_health.in_danger());
}

#[test]
fn the_setting_turned_off_shows_nothing() {
    let mut h = Harness::with_built_in_art();
    h.ui.options.low_health = LowHealthAt::Off;
    run(&mut h, &at_health(5), 0.5);
    assert!(!h.ui.hud.low_health.in_danger());
    assert!(edges(&h).is_none());
    // Turned off while showing: it fades out.
    h.ui.options.low_health = LowHealthAt::Quarter;
    run(&mut h, &at_health(5), 0.5);
    assert!(edges(&h).is_some());
    h.ui.options.low_health = LowHealthAt::Off;
    run(&mut h, &at_health(5), 1.0);
    assert!(edges(&h).is_none());
}

#[test]
fn the_setting_is_kept_in_the_settings_file() {
    for at in LowHealthAt::ALL {
        let o = HorizonOptions {
            low_health: at,
            ..HorizonOptions::default()
        };
        assert_eq!(HorizonOptions::parse(&o.to_text()).low_health, at);
    }
    assert_eq!(
        HorizonOptions::parse("low-health=sometimes").low_health,
        LowHealthAt::Quarter,
        "an unknown word keeps the default"
    );
}

#[test]
fn the_display_page_chooses_the_threshold() {
    let mut h = Harness::with_built_in_art();
    let state = at_health(80);
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    h.ui.windows.options_page.tab = tab::INTERFACE;
    h.frame(&state);
    let caption = "Low Health Warning";
    let mut found = None;
    for _ in 0..2000 {
        let page = &h.ui.windows.options_page;
        if let (Some(list), Some(r)) = (page.control("list"), page.control(caption)) {
            if r.y >= list.y && r.bottom() <= list.bottom() {
                found = Some(r);
                break;
            }
        }
        h.ui.windows.options_page.scroll[tab::INTERFACE] += 6.0;
        h.frame(&state);
    }
    let r = found.expect("the Low Health Warning row");
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    h.frame(&state);
    h.release();
    h.frame(&state);
    let menu = h.ui.windows.options_page.menu.as_ref().expect("its list");
    assert_eq!(
        menu.names,
        ["Off", "Under a Fifth", "Under a Quarter", "Under a Third"]
    );
    assert_eq!(menu.at, 2);
    h.ui.windows.options_page.choose_in_open_list(0);
    let out = h.frame(&state);
    assert!(
        out.settings
            .iter()
            .any(|(k, v)| k == "low-health" && v == "off"),
        "{:?}",
        out.settings
    );
}

#[test]
fn death_clears_it() {
    let mut h = Harness::with_built_in_art();
    run(&mut h, &at_health(10), 0.5);
    assert!(edges(&h).is_some());
    let dead = at_health(0);
    h.frame(&dead);
    assert!(!h.ui.hud.low_health.in_danger());
    run(&mut h, &dead, 1.0);
    assert!(edges(&h).is_none());
    assert!(h.ui.hud.low_health.shown() <= 0.0);
}

#[test]
fn the_health_bar_glows_while_in_danger() {
    let mut h = Harness::with_built_in_art();
    run(&mut h, &at_health(80), 0.5);
    let calm = h.list.quads.len();
    run(&mut h, &at_health(10), 0.5);
    // The edges' quad and the glow's three rings of four.
    assert_eq!(h.list.quads.len(), calm + 1 + 12);
}

#[test]
fn hidden_the_interface_keeps_the_edges_and_hides_the_glow() {
    let mut h = Harness::with_built_in_art();
    let state = at_health(10);
    run(&mut h, &state, 0.5);
    h.input.actions.push(HIDE_INTERFACE.0);
    h.frame(&state);
    assert!(h.ui.hidden.is_some());
    // Past the notice: only the edges are drawn.
    run(&mut h, &state, 5.0);
    assert_eq!(h.list.quads.len(), 1);
    assert!(edges(&h).is_some());
}

/// Not a claim: the HUD at a few healths, the warning at rest between beats and at a beat's top,
/// into transparent PNGs `<PYREAL_LOW_HEALTH_SHOTS>/hud-<health>-<rest|beat>.png` to lay over a
/// world picture (engine-view `--ui`). `PYREAL_LOW_HEALTH_SIZE` (`1728x1084`) sets the window and
/// `PYREAL_LOW_HEALTH_AT` the healths (`100,20,5`).
#[test]
#[ignore = "an instrument: set PYREAL_LOW_HEALTH_SHOTS to a folder"]
fn draw_the_low_health_warning_for_a_person_to_look_at() {
    let Some(dir) = std::env::var_os("PYREAL_LOW_HEALTH_SHOTS") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("the pictures' folder");
    let size = std::env::var("PYREAL_LOW_HEALTH_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?))
        })
        .unwrap_or((1728.0, 1084.0));
    let healths: Vec<u32> = std::env::var("PYREAL_LOW_HEALTH_AT")
        .unwrap_or_else(|_| "100,20,5".into())
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();
    for health in healths {
        let mut h = Harness::with_built_in_art();
        h.screen = size;
        let state = GameState {
            stamina: Vital {
                current: 180,
                max: 200,
            },
            mana: Vital {
                current: 140,
                max: 160,
            },
            ..at_health(health)
        };
        run(&mut h, &state, 1.0);
        // Between beats, then at the top of the next.
        let mut best = (0.0f32, 1.0f32);
        for _ in 0..120 {
            h.frame(&state);
            let pulse = h.ui.hud.low_health.pulse();
            if pulse <= best.1 {
                best.1 = pulse;
                if pulse <= 0.0 {
                    break;
                }
            }
        }
        write_rgba(&dir.join(format!("hud-{health}-rest.png")), &h);
        for _ in 0..120 {
            h.frame(&state);
            let pulse = h.ui.hud.low_health.pulse();
            if pulse + 1e-3 < best.0 {
                break;
            }
            best.0 = pulse;
        }
        write_rgba(&dir.join(format!("hud-{health}-beat.png")), &h);
    }
}

/// The frame's quads over nothing, straight alpha: a stretched picture sampled between its
/// texels as the device does, every other texel for texel.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn write_rgba(path: &std::path::Path, h: &Harness) {
    let art = &h.ui.art;
    let (sw, sh) = (h.screen.0 as usize, h.screen.1 as usize);
    // Premultiplied colour and alpha.
    let mut px = vec![[0.0f32; 4]; sw * sh];
    for q in &h.list.quads {
        let image = q.tex.and_then(|t| art.image(t));
        if q.tex.is_some() && image.is_none() {
            continue;
        }
        let mut r = q.dst;
        if let Some(c) = q.clip {
            match r.intersect(&c) {
                Some(v) => r = v,
                None => continue,
            }
        }
        let ca = [
            ((q.colour >> 16) & 0xFF) as f32 / 255.0,
            ((q.colour >> 8) & 0xFF) as f32 / 255.0,
            (q.colour & 0xFF) as f32 / 255.0,
            ((q.colour >> 24) & 0xFF) as f32 / 255.0,
        ];
        let texel = |img: &pyreal_horizon::art::Image, x: usize, y: usize| {
            let x = x.min(img.width as usize - 1);
            let y = y.min(img.height as usize - 1);
            let b = &img.bgra[(y * img.width as usize + x) * 4..][..4];
            [b[2], b[1], b[0], b[3]].map(|c| f32::from(c) / 255.0)
        };
        for y in (r.y.max(0.0) as usize)..(r.bottom().min(sh as f32).max(0.0) as usize) {
            for x in (r.x.max(0.0) as usize)..(r.right().min(sw as f32).max(0.0) as usize) {
                let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                let t = match &image {
                    None => [1.0; 4],
                    Some(img) => {
                        let u = q.src.x + (fx - q.dst.x) / q.dst.w * q.src.w;
                        let v = q.src.y + (fy - q.dst.y) / q.dst.h * q.src.h;
                        if q.stretched {
                            let (u, v) = ((u - 0.5).max(0.0), (v - 0.5).max(0.0));
                            let (x0, y0) = (u as usize, v as usize);
                            let (a, b) = (u.fract(), v.fract());
                            let mix = |p: [f32; 4], q: [f32; 4], t: f32| {
                                [0, 1, 2, 3].map(|i| p[i] * (1.0 - t) + q[i] * t)
                            };
                            mix(
                                mix(texel(img, x0, y0), texel(img, x0 + 1, y0), a),
                                mix(texel(img, x0, y0 + 1), texel(img, x0 + 1, y0 + 1), a),
                                b,
                            )
                        } else {
                            texel(img, u.max(0.0) as usize, v.max(0.0) as usize)
                        }
                    }
                };
                let a = t[3] * ca[3];
                let p = &mut px[y * sw + x];
                for k in 0..3 {
                    p[k] = p[k] * (1.0 - a) + t[k] * ca[k] * a;
                }
                p[3] = p[3] * (1.0 - a) + a;
            }
        }
    }
    let rgba: Vec<u8> = px
        .iter()
        .flat_map(|p| {
            let a = p[3].max(1e-6);
            [p[0] / a, p[1] / a, p[2] / a, p[3]]
                .map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
        })
        .collect();
    let file = std::fs::File::create(path).expect("the picture's file");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), sw as u32, sh as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .expect("the picture written");
}

/// Not a claim: the interface's frame on the CPU at full health, under the threshold with the
/// warning off, and with it showing, median microseconds over `PYREAL_LOW_HEALTH_FRAMES` (2000).
#[test]
#[ignore = "an instrument: times the interface's frame"]
fn time_the_interface_frame_with_and_without_the_warning() {
    let frames: usize = std::env::var("PYREAL_LOW_HEALTH_FRAMES")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2000);
    let cases = [
        ("full health", 100, LowHealthAt::Quarter),
        ("low health, warning off", 10, LowHealthAt::Off),
        ("low health, warning on", 10, LowHealthAt::Quarter),
    ];
    let mut harnesses: Vec<Harness> = cases
        .iter()
        .map(|(_, health, at)| {
            let mut h = Harness::with_built_in_art();
            h.ui.options.low_health = *at;
            run(&mut h, &at_health(*health), 1.0);
            h
        })
        .collect();
    let mut times = vec![Vec::new(); cases.len()];
    for _ in 0..frames {
        for (i, (_, health, _)) in cases.iter().enumerate() {
            let state = at_health(*health);
            let start = std::time::Instant::now();
            harnesses[i].frame(&state);
            times[i].push(start.elapsed().as_secs_f64() * 1e6);
        }
    }
    for ((name, _, _), t) in cases.iter().zip(&mut times) {
        t.sort_by(f64::total_cmp);
        println!("{name}: {:.1} us", t[t.len() / 2]);
    }
}
