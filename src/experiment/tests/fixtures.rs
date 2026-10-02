use super::script::Script;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use tempfile::TempDir;

// The courses authored for the instant walk/sprint model keep its rates.
pub(super) fn scenario(name: &str) -> (TempDir, Script) {
    scenario_with_movement(
        name,
        json!({
            "move_speed": 6.0,
            "jump_speed": 12.0,
            "ground_acceleration": 60.0,
            "ground_deceleration": 24.0,
            "ground_lateral_deceleration": 60.0,
            "air_acceleration": 5.0,
            "air_deceleration": 0.0,
            "air_lateral_deceleration": 0.0,
        }),
    )
}

// The persistent-velocity rates the current courses are proved against.
pub(super) fn tuned_scenario(name: &str) -> (TempDir, Script) {
    scenario_with_movement(
        name,
        json!({
            "move_speed": 8.0,
            "jump_speed": 12.0,
            "ground_acceleration": 20.0,
            "ground_deceleration": 30.0,
            "ground_lateral_deceleration": 40.0,
            "air_acceleration": 5.0,
            "air_deceleration": 5.0,
            "air_lateral_deceleration": 5.0,
        }),
    )
}

fn scenario_with_movement(name: &str, player: Value) -> (TempDir, Script) {
    let folder = TempDir::new().expect("experiment directory");
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("config/server/maps")
        .join(name);
    let mut script = Script::load(&example.join("experiment.json")).expect("example script");
    let mut gameplay: Value =
        serde_json::from_str(include_str!("../../../config/server/gameplay.json")).expect("gameplay defaults");
    gameplay["network"] = json!({"server_hz": 30, "update_hz": 30, "snapshot_hz": 4});
    gameplay["player"]["eye_height"] = json!(1.62);
    gameplay["player"]["respawn_secs"] = json!(2.0);
    gameplay["player"]["movement_collider"] = json!({"diameter": 0.6, "height": 1.8});
    gameplay["player"]["hitbox"] = json!({"width": 0.6, "height": 1.78, "depth": 0.25, "bottom_offset": 0.02});
    gameplay["actors"]["turret"]["hitbox"] = json!({"width": 0.5, "height": 1.61, "depth": 0.5, "bottom_offset": 0.01});
    gameplay["actors"]["turret"]["movement_collider"] = json!({"diameter": 0.5, "height": 1.62});
    gameplay["actors"]["turret"]["immovable"] = json!(true);
    gameplay["weapons"]["projectiles"]["radius"] = json!(0.08);
    gameplay["weapons"]["projectiles"]["spawn_offset"] = json!(1.0);
    gameplay["weapons"]["projectiles"]["cooldown_secs"] = json!(0.1);
    gameplay["weapons"]["projectiles"]["lifetime_secs"] = json!(8.0);
    gameplay["weapons"]["projectiles"]["gravity_scale"] = json!(0.4);
    gameplay["weapons"]["projectiles"]["drag_factor"] = json!(0.011);
    gameplay["weapons"]["projectiles"]["bounce_retention"] = json!(0.85);
    gameplay["weapons"]["portals"]["range"] = json!(100.0);
    gameplay["weapons"]["portals"]["size"] = json!({"width": 1.4, "height": 2.6});
    gameplay["weapons"]["portals"]["funnel"] = json!({"capture_margin": 0.6, "capture_growth": 0.8});
    gameplay["movement"]["low_gravity"] = json!(5.0);
    gameplay["player_fall"] = json!({"safe_distance": 8.0, "lethal_distance": 15.0});
    gameplay["movement"]["projectile_speed"] = json!(90.0);
    gameplay["movement"]["gravity"] = json!(25.0);
    gameplay["combat"]["damage"]["projectile"] = json!(60.0);
    for (key, value) in player.as_object().expect("movement rates") {
        gameplay["movement"]["player"][key] = value.clone();
    }
    script.gameplay = folder.path().join("gameplay.json");
    fs::write(&script.gameplay, gameplay.to_string()).expect("write test defaults");
    (folder, script)
}
