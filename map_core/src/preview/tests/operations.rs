use super::*;

fn settings() -> Value {
    json!({
        "network": {"server_hz": 30, "update_hz": 30, "snapshot_hz": 4},
        "movement": {
            "gravity": 25.0,
            "low_gravity": 5.0,
            "player": {
                "move_speed": 9.0,
                "move_speed_power_up": 1.5,
                "move_speed_ladder": 0.267,
                "jump_speed": 12.0,
                "ground_acceleration": 20.0,
                "ground_deceleration": 30.0,
                "ground_lateral_deceleration": 40.0,
                "air_acceleration": 5.0,
                "air_deceleration": 5.0,
                "air_lateral_deceleration": 5.0,
            },
        },
        "player_fall": {"safe_distance": 8.0, "lethal_distance": 15.0},
        "combat": {"health": {"player": {"max": 500.0}}},
        "player": {"movement_collider": {"diameter": 0.6, "height": 1.8}, "eye_height": 1.6},
        "weapons": {"portals": {"size": {"width": 1.4, "height": 2.6}, "funnel": {"capture_margin": 0.6, "capture_growth": 1.0}}},
    })
}

#[test]
fn preview_physics_extracts_and_validates_the_settings_it_needs() {
    let physics = dispatch("preview_physics", &json!([settings()])).expect("shipped-shaped settings are valid");
    assert_eq!(physics["server_hz"], 30);
    assert_eq!(physics["player"]["move_speed"], 9.0);
    assert_eq!(physics["portal_size"]["height"], 2.6);
    assert_eq!(physics["funnel"]["capture_margin"], 0.6);
    assert_eq!(physics["funnel"]["capture_growth"], 1.0);
    assert_eq!(physics["coyote_secs"], PLAYER_COYOTE_SECS);

    let mut broken = settings();
    broken["movement"]["player"]["move_speed"] = json!(-1.0);
    let error = dispatch("preview_physics", &json!([broken])).expect_err("negative speed is invalid");
    assert!(format!("{error:#}").contains("movement.player.move_speed"), "{error:#}");

    let mut missing = settings();
    missing["player_fall"].take();
    let error = dispatch("preview_physics", &json!([missing])).expect_err("a missing block is invalid");
    assert!(format!("{error:#}").contains("player_fall"), "{error:#}");
}

#[test]
fn jump_preview_round_trips_through_json() {
    let physics = dispatch("preview_physics", &json!([settings()])).expect("valid settings");
    let request = json!({
        "takeoff": {"point": [0.0, 8.0, 0.0], "direction": [0.0, 1.0], "jumping": true, "margin": 0.1},
        "heights": [0.0, 8.0],
        "air_control": true,
        "shooter": [0.0, 0.0],
        "portals": null,
    });
    let reply = dispatch("jump_preview", &json!([physics, request])).expect("the request is valid");
    let scenarios = reply["scenarios"].as_array().expect("a scenario list");
    assert_eq!(scenarios.len(), 4);
    let normal = &scenarios[0];
    assert!(normal["path"].as_array().is_some_and(|path| path.len() > 30));
    assert_eq!(normal["end"], "below");
    assert!(normal["hop"].is_null() && normal["entry"].is_null());
    assert_eq!(normal["crossings"][0]["phase"], "before_entry");
    assert!(
        normal["capture"][0]["pieces"][0]["polygon"]
            .as_array()
            .is_some_and(|polygon| polygon.len() >= 3)
    );
    assert!(normal["range"][0]["polygon"].is_array());
    assert!(normal["capture_steered"][0]["pieces"].is_array());

    let mut through = request.clone();
    through["portals"] = json!({
        "entry": {"center": [0.0, 0.0, 50.0], "normal": [0.0, 1.0, 0.0], "yaw": 0.0},
        "exit": null,
    });
    let reply = dispatch("jump_preview", &json!([physics, through])).expect("the request is valid");
    assert_eq!(reply["scenarios"][0]["entry"], "missed");
}

#[test]
fn unknown_operations_and_unknown_fields_are_errors() {
    assert!(dispatch("preview_nothing", &json!([])).is_err());
    let physics = dispatch("preview_physics", &json!([settings()])).expect("valid settings");
    let request = json!({
        "takeoff": {"point": [0.0, 0.0, 0.0], "direction": [0.0, 1.0], "jumping": true, "margin": 0.0, "run_up": 1.0},
        "heights": [0.0],
        "air_control": false,
        "shooter": [0.0, 0.0],
        "portals": null,
    });
    assert!(dispatch("jump_preview", &json!([physics, request])).is_err());
}
