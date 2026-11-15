//! What the objects standing on the land clear of the cover.

use glam::{Vec2, Vec3};
use pyreal_engine::cover::{clearing_radius, clearing_words};

/// A cylinder at the foot clears its reach; one up in the air, a missing shape or one too small
/// or too large clears nothing.
#[test]
fn an_objects_clearing_is_its_cylinders_reach_at_its_foot() {
    let foot = clearing_radius([(Vec3::new(0.3, 0.4, 0.0), 0.5)], None, 1.0);
    assert!((foot.unwrap() - 1.0).abs() < 1e-6);
    assert_eq!(
        clearing_radius([(Vec3::new(0.0, 0.0, 2.0), 0.5)], None, 1.0),
        None
    );
    assert_eq!(
        clearing_radius([(Vec3::new(0.0, 0.0, 2.0), 0.5)], Some(1.2), 2.0),
        Some(2.4)
    );
    assert_eq!(clearing_radius([], None, 1.0), None);
    assert_eq!(clearing_radius([], Some(0.1), 1.0), None);
    assert_eq!(clearing_radius([], Some(9.0), 1.0), None);
}

/// A clearing is listed in each land cell it reaches, and nowhere else.
#[test]
fn clearings_are_listed_by_the_land_cells_they_reach() {
    let words = clearing_words(Vec2::ZERO, (3, 3), &[Vec3::new(23.0, 12.0, 1.5)]);
    assert_eq!(&words[..2], &[3, 3]);
    let (lists, discs) = (words[2] as usize, words[3] as usize);
    let count = |x: usize, y: usize| words[4 + 2 * (y * 3 + x) + 1];
    assert_eq!((count(0, 0), count(1, 0), count(2, 0)), (1, 1, 0));
    assert_eq!(count(0, 1) + count(1, 1) + count(2, 1), 0);
    assert_eq!(words[lists], 0);
    assert_eq!(
        &words[discs..],
        &[23.0f32.to_bits(), 12.0f32.to_bits(), 1.5f32.to_bits()]
    );
}
