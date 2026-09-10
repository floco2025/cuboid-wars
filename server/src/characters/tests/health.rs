use super::*;

#[test]
fn regeneration_does_not_exceed_max_health() {
    let mut health = Health(90.0);
    regenerate_health(&mut health, 100.0, 20.0);
    assert_eq!(health, Health(100.0));
}
