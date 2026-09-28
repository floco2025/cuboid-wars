use super::*;

#[test]
fn air_rates_allow_zero_and_reject_negative_or_nonfinite_values() {
    let valid = PlayerMovementConfig {
        move_speed: 9.0,
        move_speed_power_up: 1.5,
        jump_speed: 12.0,
        ground_acceleration: 20.0,
        ground_deceleration: 30.0,
        ground_lateral_deceleration: 40.0,
        air_acceleration: 0.0,
        air_deceleration: 0.0,
        air_lateral_deceleration: 0.0,
    };
    valid
        .validate("movement.player")
        .expect("ballistic air movement is valid");
    for field in ["air_acceleration", "air_deceleration", "air_lateral_deceleration"] {
        for value in [-1.0, f32::NAN, f32::INFINITY] {
            let mut config = valid;
            match field {
                "air_acceleration" => config.air_acceleration = value,
                "air_deceleration" => config.air_deceleration = value,
                _ => config.air_lateral_deceleration = value,
            }
            let error = config
                .validate("movement.player")
                .expect_err("invalid air rate accepted");
            assert!(error.to_string().contains(field), "{error}");
        }
    }
}

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
