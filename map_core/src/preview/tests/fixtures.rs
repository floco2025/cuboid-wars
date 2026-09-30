use serde_json::json;

use super::physics::PreviewPhysics;

// Round numbers: speed 1 (2 with the pickup), jump 2, gravity 2 (1 low).
pub(super) fn physics_with(air_rates: f32) -> PreviewPhysics {
    physics(1.0, 2.0, 2.0, air_rates)
}

// The proportions of a shipped map, where a tick moves a body a long way.
pub(super) fn game_physics() -> PreviewPhysics {
    physics(9.0, 12.0, 25.0, 5.0)
}

fn physics(move_speed: f32, jump_speed: f32, gravity: f32, air_rates: f32) -> PreviewPhysics {
    let physics: PreviewPhysics = serde_json::from_value(json!({
        "server_hz": 30,
        "gravity": gravity,
        "low_gravity": gravity * 0.5,
        "player": {
            "move_speed": move_speed,
            "move_speed_power_up": 2.0,
            "move_speed_ladder": 0.5,
            "jump_speed": jump_speed,
            "ground_acceleration": 20.0,
            "ground_deceleration": 30.0,
            "ground_lateral_deceleration": 40.0,
            "air_acceleration": air_rates,
            "air_deceleration": air_rates,
            "air_lateral_deceleration": air_rates,
        },
        "player_fall": {"safe_distance": 4.0, "lethal_distance": 12.0},
        "max_health": 100.0,
        "body": {"diameter": 0.6, "height": 1.8},
        "portal_size": {"width": 1.4, "height": 2.6},
        "funnel": {"capture_margin": 0.6},
    }))
    .expect("test physics is valid");
    physics.validate().expect("test physics is valid");
    physics
}
