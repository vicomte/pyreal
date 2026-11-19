use super::*;

/// Oracle: the HUD's vital-stat table uses current id 2 and maximum id 1 for health.
#[test]
fn the_vital_stat_numbers_are_the_documented_attribute_2nd_types() {
    assert_eq!(Vital::Health.stats(), (2, 1));
    assert_eq!(Vital::Stamina.stats(), (4, 3));
    assert_eq!(Vital::Mana.stats(), (6, 5));
}
