use super::script::{Action, Script};
use serde_json::{Value, json};
use std::fs;
use tempfile::TempDir;

// A test-owned map with no actions: 4 m cells under 4 m walls, so one wall
// section backs a portal, and both weapons from the start.
pub(super) fn chamber(layout: Value, spawn: [f32; 3]) -> (TempDir, Script) {
    let folder = TempDir::new().expect("experiment directory");
    let settings = json!({
        "textures": {
            "solid": {"material": "steelplate1", "portalable": false},
            "portal": {"material": "titanium-scuffed", "portalable": true},
        },
        "grounds": null,
        "random_items": null,
        "placed_items": null,
        "quests": [],
        "weather": "clear",
        "geometry": {"grid_cell_size": 4.0, "level_height": 4.4, "floor_thickness": 0.4, "wall_thickness": 0.4},
        "portals": "both",
        "power_ups": {"single_shot": {"mode": "always"}, "portal_gun": {"mode": "always"}},
        "combat": {"health": {
            "actors": {"turret": {"max": 50.0, "regen_rate": 0.0}},
            "player": {"max": 500.0, "regen_rate": 0.0},
        }},
    });
    let script = json!({"gameplay": "gameplay.json", "settings": "settings.json", "layout": "layout.json",
        "spawn": spawn, "actions": []});
    for (name, value) in [
        ("gameplay.json", gameplay()),
        ("settings.json", settings),
        ("layout.json", layout),
        ("experiment.json", script),
    ] {
        fs::write(folder.path().join(name), value.to_string()).expect("write chamber file");
    }
    let script = Script::load(&folder.path().join("experiment.json")).expect("chamber script");
    (folder, script)
}

// Every cell of a `size` by `size` grid, floored.
pub(super) fn floors(size: i32) -> Vec<Value> {
    (0..size)
        .flat_map(|col| (0..size).map(move |row| json!({"col": col, "row": row, "all": "solid"})))
        .collect()
}

// A flat floor with a wall between the spawn and a turret, and a portalable
// wall to one side of each.
pub(super) fn turret_room() -> (TempDir, Script) {
    let layout = json!({"map": {"fireworks": null, "grid_cols": 8, "grid_rows": 8,
        "checkpoints": [{"level": 0, "cols": [1, 2], "rows": [4, 5], "type": "individual", "number": 0}],
        "actor_spawn_zones": [{"level": 0, "cols": [5, 6], "rows": [4, 5], "kind": "turret", "count": [1],
            "respawn_secs": null, "beam_in_secs": 0.1}],
        "levels": [{"name": "Chamber", "floors": floors(8), "walls": [
            {"c0": 4, "r0": 3, "c1": 4, "r1": 4, "all": "solid"},
            {"c0": 4, "r0": 4, "c1": 4, "r1": 5, "all": "solid"},
            {"c0": 1, "r0": 2, "c1": 2, "r1": 2, "all": "portal"},
            {"c0": 5, "r0": 6, "c1": 6, "r1": 6, "all": "portal"},
        ]}],
    }});
    chamber(layout, [-10.0, 0.0, 2.0])
}

// A floor with a portalable wall north of the spawn and a wall of texture
// `east_wall` east of it.
pub(super) fn walled_floor(east_wall: &str) -> (TempDir, Script) {
    let layout = json!({"map": {"fireworks": null, "grid_cols": 10, "grid_rows": 10,
        "checkpoints": [{"level": 0, "cols": [2, 3], "rows": [3, 4], "number": 0, "type": "individual"}],
        "levels": [{"name": "Walled", "floors": floors(10), "walls": [
            {"c0": 2, "r0": 2, "c1": 3, "r1": 2, "all": "portal"},
            {"c0": 7, "r0": 3, "c1": 7, "r1": 4, "all": east_wall},
        ]}],
    }});
    chamber(layout, [-10.0, 0.0, -6.0])
}

pub(super) fn walk(direction: [f32; 2], ticks: u32) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump: false,
    }
}

pub(super) fn jump(direction: [f32; 2], ticks: u32) -> Action {
    Action::Move {
        direction,
        ticks,
        crouch: false,
        jump: true,
    }
}

pub(super) fn events(report: &Value) -> impl Iterator<Item = &Value> {
    report["steps"]
        .as_array()
        .expect("steps missing from the report")
        .iter()
        .flat_map(|step| step["events"].as_array().expect("events missing from a step"))
}

fn gameplay() -> Value {
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
    gameplay["movement"]["low_gravity"] = json!(13.2);
    gameplay["player_fall"] = json!({"safe_distance": 8.0, "lethal_distance": 15.0});
    gameplay["movement"]["projectile_speed"] = json!(90.0);
    gameplay["movement"]["gravity"] = json!(25.0);
    gameplay["combat"]["damage"]["projectile"] = json!(60.0);
    let player = json!({
        "move_speed": 5.1,
        "move_speed_ladder": 0.45,
        "move_speed_power_up": 1.818,
        "jump_speed": 12.0,
        "ground_acceleration": 20.0,
        "ground_deceleration": 30.0,
        "ground_lateral_deceleration": 40.0,
        "air_acceleration": 3.0,
        "air_deceleration": 0.0,
        "air_lateral_deceleration": 0.0,
    });
    for (key, value) in player.as_object().expect("movement rates") {
        gameplay["movement"]["player"][key] = value.clone();
    }
    gameplay
}
