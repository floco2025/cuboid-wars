use super::*;

#[test]
fn capsule_dimensions_reject_invalid_values_and_allow_a_sphere() {
    for (diameter, height) in [
        (0.0, 1.0),
        (-0.1, 1.0),
        (f32::NAN, 1.0),
        (1.0, f32::INFINITY),
        (1.0, 0.9),
    ] {
        assert!(
            MovementColliderConfig { diameter, height }
                .validate("player.movement_collider")
                .is_err()
        );
    }
    assert!(
        MovementColliderConfig {
            diameter: 1.0,
            height: 1.0
        }
        .validate("player.movement_collider")
        .is_ok()
    );
}
