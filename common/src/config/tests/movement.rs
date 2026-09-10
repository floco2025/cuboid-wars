use super::*;

#[test]
fn movable_actor_speeds_must_be_positive() {
    for (roam_speed, active_speed, valid) in [
        (0.0, 0.0, false),
        (2.0, 4.0, true),
        (0.0, 4.0, false),
        (2.0, 0.0, false),
        (-1.0, 0.0, false),
        (f32::NAN, 0.0, false),
    ] {
        let movement = ActorMovementConfig {
            roam_speed,
            active_speed,
        };
        assert_eq!(movement.validate("movement").is_ok(), valid);
    }
}
