use super::*;
use crate::config::fixtures;
use serde_json::json;

#[test]
fn pickup_duration_is_explicit_and_positive_or_null() {
    let mut config = fixtures::server_config().power_ups.clone();
    for duration_secs in [None, Some(1.0)] {
        config.portal_gun = PowerUpMode::Pickup { duration_secs };
        config.validate("power_ups").expect("valid pickup rejected");
    }
    for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        config.portal_gun = PowerUpMode::Pickup {
            duration_secs: Some(invalid),
        };
        let error = config.validate("power_ups").expect_err("invalid duration accepted");
        assert!(error.to_string().contains("power_ups.portal_gun.duration_secs"));
    }
    for value in [
        json!({"mode": "pickup"}),
        json!({"mode": "always", "duration_secs": null}),
        json!({"mode": "always", "duration_secs": 30}),
        json!({"mode": "unknown"}),
        json!(null),
        json!([]),
    ] {
        assert!(
            serde_json::from_value::<PowerUpMode>(value.clone()).is_err(),
            "accepted {value}"
        );
    }
    assert_eq!(
        serde_json::from_value::<PowerUpMode>(json!({"mode": "pickup", "duration_secs": null}))
            .expect("permanent pickup rejected"),
        PowerUpMode::Pickup { duration_secs: None }
    );
    assert_eq!(
        serde_json::from_value::<PowerUpMode>(json!({"mode": "always"})).expect("always mode rejected"),
        PowerUpMode::Always {}
    );
}
