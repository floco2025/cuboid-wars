use super::*;

#[test]
fn terminal_velocity_warning_only_flags_unreachable_lethal_speeds() {
    let path = "maps/example/settings.json: player_fall";
    for (lethal_speed, should_warn) in [
        (CHARACTER_TERMINAL_VELOCITY - 1.0, false),
        (CHARACTER_TERMINAL_VELOCITY, false),
        (CHARACTER_TERMINAL_VELOCITY + 1.0, true),
    ] {
        let fall = FallDamageConfig {
            safe_distance: 0.0,
            lethal_distance: lethal_speed * lethal_speed / 2.0,
        };
        let warning = fall.terminal_velocity_warning(path, 1.0);
        assert_eq!(warning.is_some(), should_warn);
        if let Some(warning) = warning {
            assert!(warning.contains("maps/example/settings.json: player_fall.lethal_distance"));
            assert!(warning.contains(&format!("{lethal_speed:.2} m/s")));
            assert!(warning.contains(&format!("terminal velocity ({CHARACTER_TERMINAL_VELOCITY} m/s)")));
        }
        fall.validate(path, 1.0)
            .expect("unreachable lethal falls should warn, not reject the map");
    }
}

#[test]
fn terminal_velocity_warning_depends_on_normal_gravity() {
    let fall = FallDamageConfig {
        safe_distance: 0.0,
        lethal_distance: CHARACTER_TERMINAL_VELOCITY * CHARACTER_TERMINAL_VELOCITY / 2.0,
    };
    assert!(fall.terminal_velocity_warning("player_fall", 0.5).is_none());
    assert!(fall.terminal_velocity_warning("player_fall", 2.0).is_some());
}
