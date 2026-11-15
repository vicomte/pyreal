//! The sky's sun, the weather and the shapes of the sky objects' pictures.

use glam::{Vec2, Vec3};
use pyreal_engine::atmosphere::{sky_sun, NIGHT_SUN};
use pyreal_engine::heavens::{card_of, disc_of, laying_of};
use pyreal_engine::sky::{daylight, Weather};
use pyreal_engine::SkyLight;

/// By day the sky's sun stands where the dats put it; at night (the dats' "sun" is the moon,
/// low in the east) it is below the horizon, on the same bearing.
#[test]
fn the_skys_sun_sets_as_the_daylight_goes() {
    let mut light = SkyLight {
        sun_dir: Vec3::new(0.39, 0.0, 0.92).normalize(),
        daylight: 1.0,
        ..SkyLight::default()
    };
    let day = sky_sun(&light);
    assert!((day - light.sun_dir).length() < 1e-4, "{day}");
    light.daylight = 0.0;
    let night = sky_sun(&light);
    assert!(
        night.z < 0.0 && night.x > 0.0 && night.y.abs() < 1e-4,
        "{night}"
    );
    #[allow(clippy::disallowed_methods)] // a reference value in a test
    let want = NIGHT_SUN.to_radians().sin();
    assert!((night.z - want).abs() < 1e-3);
}

/// Daylight follows the fog's brightness: the dats' night fog is night, their day fog day.
#[test]
fn daylight_follows_the_fog() {
    assert_eq!(daylight(Vec3::splat(0.01)), 0.0);
    assert_eq!(daylight(Vec3::new(0.55, 0.58, 0.72)), 1.0);
    let dusk = daylight(Vec3::new(0.10, 0.05, 0.05));
    assert!(dusk > 0.05 && dusk < 0.6, "{dusk}");
}

#[test]
fn weather_is_read_from_the_day_groups_name() {
    assert_eq!(Weather::of("Rainy Day 2"), Weather::Rainy);
    assert_eq!(Weather::of("Cloudy"), Weather::Cloudy);
    assert_eq!(Weather::of("Clear Day"), Weather::Clear);
    assert_eq!(Weather::of("Default"), Weather::Sunny);
    assert!(Weather::Rainy.haze().0 > Weather::Clear.haze().0);
}

/// A two-triangle quad is a card, its steps across the picture found from its corners; a
/// picture laid evenly on the ground's axes is a deck; neither is both.
#[test]
fn cards_and_decks_are_told_apart() {
    let corner = |x: f32, z: f32, u: f32, v: f32| (Vec3::new(x, 100.0, z), Vec2::new(u, v));
    let quad = [
        corner(-5.0, 5.0, 0.0, 0.0),
        corner(5.0, 5.0, 1.0, 0.0),
        corner(-5.0, -5.0, 0.0, 1.0),
        corner(-5.0, -5.0, 0.0, 1.0),
        corner(5.0, 5.0, 1.0, 0.0),
        corner(5.0, -5.0, 1.0, 1.0),
    ];
    let card = card_of(&quad).expect("a card");
    assert!((card.center - Vec3::new(0.0, 100.0, 0.0)).length() < 1e-4);
    assert!(
        (card.u - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3,
        "{:?}",
        card.u
    );
    assert!(
        (card.v - Vec3::new(0.0, 0.0, -10.0)).length() < 1e-3,
        "{:?}",
        card.v
    );
    assert!(laying_of(&quad).is_none(), "an upright card is no deck");

    let flat = |x: f32, y: f32| {
        (
            Vec3::new(x, y, 300.0),
            Vec2::new(0.25 + x / 4000.0, 0.5 - y / 2000.0),
        )
    };
    let deck = [
        flat(-1000.0, -1000.0),
        flat(1000.0, -1000.0),
        flat(-1000.0, 1000.0),
        flat(1000.0, 1000.0),
        flat(0.0, 0.0),
        flat(500.0, -250.0),
    ];
    let l = laying_of(&deck).expect("a deck");
    assert!((l[0] - 0.25).abs() < 1e-4 && (l[1] - 1.0 / 4000.0).abs() < 1e-7);
    assert!((l[2] - 0.5).abs() < 1e-4 && (l[3] + 1.0 / 2000.0).abs() < 1e-7);
}

/// A moon's disc is where its picture is solid: its centre and half its width each way.
#[test]
fn a_pictures_disc_is_its_solid_part() {
    let (w, h) = (8u32, 4u32);
    let mut alpha = vec![0u8; 32];
    for y in 1..3 {
        for x in 2..8 {
            alpha[y * 8 + x] = 255;
        }
    }
    let (at, half) = disc_of(w, h, &alpha).expect("a disc");
    assert!((at - Vec2::new(0.625, 0.5)).length() < 1e-6, "{at}");
    assert!((half - Vec2::new(0.375, 0.25)).length() < 1e-6, "{half}");
    assert!(
        disc_of(w, h, &[0; 32]).is_none(),
        "a clear picture paints no disc"
    );
}
