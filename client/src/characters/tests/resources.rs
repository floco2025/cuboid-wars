use super::*;

#[test]
fn health_ratio_clamps_to_unit_range() {
    assert_eq!(health_ratio(Health(-10.0), 100.0), 0.0);
    assert_eq!(health_ratio(Health(50.0), 100.0), 0.5);
    assert_eq!(health_ratio(Health(150.0), 100.0), 1.0);
}
