use super::*;

#[test]
fn durations_accept_zero_and_reject_negative_or_non_finite_values() {
    let mut config = PowerUpsConfig {
        duration_secs: PowerUpDurationSecs {
            single_shot: 0.0,
            multi_shot: 0.0,
            portal_gun: 0.0,
            speed: 0.0,
            low_gravity: 0.0,
        },
    };
    assert!(config.validate("maps.test.power_ups").is_ok());
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        config.duration_secs.portal_gun = invalid;
        let error = config
            .validate("maps.test.power_ups")
            .expect_err("invalid duration accepted");
        assert!(
            error
                .to_string()
                .contains("maps.test.power_ups.duration_secs.portal_gun")
        );
    }
}
