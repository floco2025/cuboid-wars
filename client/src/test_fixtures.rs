// Reference sizes for tests that lay out a world by hand: the shipped maps'
// values, so hand-built fixtures agree with what the server ships.
use std::collections::HashMap;

use common::{
    config::{GameplayConfig, KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    map::MapGeometry,
    protocol::{MapSettings, PortalMode},
};

use crate::config::FollowCameraConfig;

// The client projection of the shipped `gameplay.json`, for tests that need
// the real player body and weapon tuning.
pub(crate) fn gameplay_config() -> GameplayConfig {
    let source: serde_json::Value = serde_json::from_str(include_str!("../../config/server/gameplay.json"))
        .expect("server gameplay JSON is invalid");
    serde_json::from_value(serde_json::json!({
        "player": source["player"],
        "actors": source["actors"]["kinds"],
        "projectiles": source["weapons"]["projectiles"],
        "missiles": source["weapons"]["missiles"],
        "portals": source["weapons"]["portals"],
    }))
    .expect("client gameplay config is invalid")
}

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;

pub(crate) fn geometry(cols: i32, rows: i32) -> MapGeometry {
    MapGeometry::new(cols, rows, sizes())
}

pub(crate) fn sizes() -> MapGeometryConfig {
    MapGeometryConfig {
        grid_cell_size: CELL,
        level_height: LEVEL_HEIGHT,
        floor_thickness: FLOOR_THICKNESS,
        wall_thickness: WALL_THICKNESS,
    }
}

// The settings resource for systems that only read `geometry`.
pub(crate) fn map_settings() -> MapSettings {
    MapSettings {
        skybox: "test".to_owned(),
        textures: Default::default(),

        geometry: sizes(),
        movement: MapMovementConfig {
            player: PlayerMovementConfig {
                walk_speed: 4.0,
                run_speed: 7.0,
                speed_power_up: 1.5,
                jump_speed: 12.0,
            },
            actors: HashMap::new(),
            missile_speed: 20.0,
            projectile_speed: 30.0,
            gravity: 20.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.6,
            knockback: KnockbackConfig {
                max_speed: 10.0,
                up_speed: 4.0,
                deceleration: 12.0,
            },
        },
        portals: PortalMode::Both,
        barrier_kinds: Vec::new(),
        bridge_kinds: Vec::new(),
    }
}

// A follow camera with room to zoom, so arm tests are not bound to the
// shipped `max_distance`.
pub(crate) fn follow_camera() -> FollowCameraConfig {
    FollowCameraConfig {
        max_distance: 6.0,
        first_person_distance: 0.7,
        pivot_height: 1.4,
        shoulder_offset: 0.0,
        collision_radius: 0.2,
        obstruction_return_rate: 8.0,
    }
}
