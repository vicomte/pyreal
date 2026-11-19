//! Vapour is drawn among the translucent: each body's run of balls with its distance, far to near.

use glam::{Affine3A, Vec3};
use pyreal_engine::particles::{Drawn, Look, Particles};
use pyreal_engine::puffs::STEAM;

fn ball(group: u32, distance: f32) -> Drawn {
    Drawn {
        sprite: 0x0100_0FC3,
        transform: Affine3A::from_translation(Vec3::new(0.0, distance, 0.0)),
        alpha: 0.5,
        cell: 0,
        distance,
        light: false,
        look: Look::Puff(STEAM),
        age: 0.5,
        seed: 0,
        group,
    }
}

#[test]
fn each_body_of_vapour_is_a_run_at_its_balls_mean_distance_far_to_near() {
    // A near body and a far one, the near one's balls spread round 10 m.
    let mut p = Particles {
        external: vec![ball(1, 8.0), ball(2, 30.0), ball(1, 12.0), ball(2, 34.0)],
        ..Particles::default()
    };
    p.collect(Vec3::ZERO, &[], |_| true);
    let (first, count) = p.puffs;
    assert_eq!(count, 4);
    let runs = p.puff_runs.clone();
    assert_eq!(runs.len(), 2, "{runs:?}");
    assert!(
        (runs[0].2 - 32.0).abs() < 1e-4 && (runs[1].2 - 10.0).abs() < 1e-4,
        "{runs:?}"
    );
    // The runs are the vapour's instances, each body's two.
    let mut covered: Vec<u32> = runs.iter().flat_map(|r| r.0..r.0 + r.1).collect();
    covered.sort_unstable();
    assert_eq!(covered, (first..first + count).collect::<Vec<_>>());
}
