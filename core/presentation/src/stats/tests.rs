use super::*;
#[test]
fn fractional_vitae_uses_variant_rounding_without_narrowing() {
    for (m, classic, modern) in [
        (1.0, 0, 0),
        (0.999, 1, 0),
        (0.986, 1, 1),
        (0.984, 2, 2),
        (0.995, 1, 0),
        (0.985, 1, 1),
    ] {
        let d = VitaeDisplay {
            multiplier: m,
            cp_pool: 12,
            threshold: 10,
        };
        assert_eq!(
            vitae_content(d, DisplayVariant::Classic),
            VitaeContent {
                penalty: classic,
                experience: -2
            }
        );
        assert_eq!(vitae_content(d, DisplayVariant::Modern).penalty, modern);
    }
    for (m, expected) in [
        (f32::from_bits(0.985f32.to_bits() - 1), 2),
        (f32::from_bits(0.985f32.to_bits() + 1), 1),
    ] {
        let d = VitaeDisplay {
            multiplier: m,
            cp_pool: 0,
            threshold: 1,
        };
        for v in [DisplayVariant::Classic, DisplayVariant::Modern] {
            assert_eq!(vitae_content(d, v).penalty, expected);
        }
    }
    assert!(VitaeContent {
        penalty: 1,
        experience: 1
    }
    .classic_text()
    .ends_with("1 more experience point.\n"));
    assert!(VitaeContent {
        penalty: 1,
        experience: -2
    }
    .classic_text()
    .ends_with("-2 more experience points.\n"));
}
#[test]
fn the_classic_meter_ratio_is_clamped_to_one() {
    assert_eq!(classic_meter_ratio(2, 1), 1.0);
}
