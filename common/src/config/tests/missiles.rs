use super::*;

const fn missiles_config() -> MissilesConfig {
    MissilesConfig {
        lock_range: 60.0,
        lock_assist_radius: 1.2,
        require_lock: true,
        max_missiles: 3,
        turn_radius: 1.7,
        lifetime_secs: 10.0,
        launch_spread_degrees: 45.0,
        weave_strength: 0.1,
        proximity_fuse_distance: 1.0,
        stall_secs: 2.0,
    }
}

#[test]
fn accepts_valid_values() {
    assert!(missiles_config().validate("missiles").is_ok());
}

#[test]
fn rejects_zero_max_missiles() {
    let config = MissilesConfig {
        max_missiles: 0,
        ..missiles_config()
    };
    let err = config
        .validate("missiles")
        .expect_err("zero max_missiles passed validation");
    assert!(err.to_string().contains("max_missiles"));
}

#[test]
fn rejects_out_of_range_flight_tuning_by_field() {
    let cases: [(&str, fn(&mut MissilesConfig)); 6] = [
        ("turn_radius", |config| config.turn_radius = 0.0),
        ("lifetime_secs", |config| config.lifetime_secs = -1.0),
        ("launch_spread_degrees", |config| config.launch_spread_degrees = 91.0),
        ("weave_strength", |config| config.weave_strength = -0.1),
        ("proximity_fuse_distance", |config| {
            config.proximity_fuse_distance = f32::NAN;
        }),
        ("stall_secs", |config| config.stall_secs = 0.0),
    ];
    for (field, break_it) in cases {
        let mut config = missiles_config();
        break_it(&mut config);
        let err = config
            .validate("missiles")
            .expect_err(&format!("{field} passed validation"))
            .to_string();
        assert!(err.contains(field), "{field} is not named in {err}");
    }
}

#[test]
fn rejects_non_positive_lock_distance() {
    let config = MissilesConfig {
        lock_range: 0.0,
        ..missiles_config()
    };
    assert!(config.validate("missiles").is_err());
}
