use crate::{
    config::ServerGameplayConfig,
    map::{GeneratedMap, generation::generate_map_at},
};
use anyhow::Result;
use common::protocol::MapSettings;
use serde_json::json;
use std::fs;

pub(crate) fn config() -> ServerGameplayConfig {
    let mut config = crate::config::fixtures::server_config();
    config.random_items = None;
    config.placed_items = None;
    config.quests.clear();
    config.settings.grounds = None;
    config.settings.geometry.grid_cell_size = 3.0;
    config.settings.geometry.level_height = 1.5;
    config.settings.geometry.floor_thickness = 0.2;
    config.settings.geometry.wall_thickness = 0.3;
    config.settings.movement.gravity = 25.0;
    for (kind, diameter, height) in [("scuttler", 0.6, 0.9), ("bruiser", 0.8, 1.8)] {
        let body = common::config::CharacterPhysicsConfig {
            movement_collider: common::config::MovementColliderConfig { diameter, height },
            hitbox: common::config::HitboxConfig {
                width: diameter,
                height,
                depth: diameter,
                bottom_offset: 0.0,
            },
        };
        let actor = config.actors.get_mut(kind).expect("fixture actor");
        actor.character.character.movement_collider = body.movement_collider;
        actor.character.character.hitbox = body.hitbox;
        actor.character.character.eye_height = body.movement_collider.height * 0.8;
        actor.character.can_use_ladders = false;
        actor.vision_range = 100.0;
        actor.threat_memory_secs = 10.0;
        let speeds = config.settings.movement.actors.get_mut(kind).expect("fixture speed");
        speeds.roam_speed = 2.0;
        speeds.active_speed = 3.0;
    }
    config
}

pub(crate) fn generate(_: &str, hz: u32, settings: &MapSettings) -> Result<GeneratedMap> {
    let floor = |col, row| json!({"col":col,"row":row,"all":"basement-floor"});
    let file = json!({"map":{
        "grid_cols":10,"grid_rows":8,"fireworks":null,
        "levels":[
            {"floors":(0..8).flat_map(|row| (0..7).map(move |col| floor(col,row))).collect::<Vec<_>>()},
            {"inaccessible_floors":(5..7).flat_map(|row| (1..3).map(move |col| floor(col,row))).collect::<Vec<_>>()},
            {"floors":(3..5).flat_map(|row| [6,8,9].map(move |col| floor(col,row))).collect::<Vec<_>>(),
                "light_bridges":[{"col":7,"row":3,"field":"bridge"},{"col":7,"row":4,"field":"bridge"}]}
        ],
        "ramps":[{"lower_level":0,"levels":2,"cols":[2,6],"rows":[3,5],"direction":"E","shape":"plank","all":"basement-floor"}],
        "checkpoints":[{"level":0,"cols":[5,6],"rows":[6,7],"number":0,"type":"individual"}],
        "actor_spawn_zones":[
            {"level":0,"cols":[5,6],"rows":[1,2],"kind":"scuttler","count":[1],"respawn_secs":0.1,"roam_distance":20},
            {"level":0,"cols":[1,2],"rows":[1,2],"kind":"bruiser","count":[0],"respawn_secs":null,"roam_distance":20}
        ],
        "fields":[{"id":"bridge","color":"#30d8ff","switch":"bridge"}],
        "switches":[{"id":"bridge","activation":"toggle","held":"any","reset_on_player_death":"never"}],
        "pressure_plates":[{"level":0,"col":0,"row":7,"switch":"bridge"}]
    }});
    compile(file, hz, settings)
}

pub(crate) fn compile(file: serde_json::Value, hz: u32, settings: &MapSettings) -> Result<GeneratedMap> {
    let directory = std::env::temp_dir().join(format!("surface_scene_{}", rand::random::<u64>()));
    fs::create_dir(&directory)?;
    let path = directory.join("layout.json");
    fs::write(&path, file.to_string())?;
    let generated = generate_map_at(&path, "surface_fixture", hz, settings);
    fs::remove_dir_all(directory)?;
    generated
}

pub(crate) fn shuttle() -> serde_json::Value {
    json!({"map": {
        "grid_cols":8,"grid_rows":4,"fireworks":null,
        "levels":[{"floors":[
            {"col":0,"row":1,"all":"basement-floor"},{"col":1,"row":1,"all":"basement-floor"},
            {"col":6,"row":1,"all":"basement-floor"},{"col":7,"row":1,"all":"basement-floor"}
        ]}],
        "checkpoints":[{"level":0,"cols":[0,1],"rows":[1,2],"number":0,"type":"individual"}],
        "actor_spawn_zones":[{"level":0,"cols":[0,1],"rows":[1,2],"kind":"scuttler","count":[1],"respawn_secs":null}],
        "nested_maps":[{"map":"shuttle","level":0,"from":[2,1],"to":[5,1],"travel_secs":2.0,"pause_secs":3.0,"phase_secs":0.0}],
        "nested_geometry":{"shuttle":{"grid_cols":1,"grid_rows":1,"levels":[{"floors":[{"col":0,"row":0,"all":"basement-floor"}]}]}}
    }})
}
