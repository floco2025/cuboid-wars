// Reference sizes for tests that lay out a world by hand: the shipped maps'
// values, so hand-built fixtures agree with what the server ships.
use std::collections::HashMap;

use common::{
    config::{KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    constants::BRIDGE_THICKNESS_FRACTION,
    protocol::{MapSettings, MapWeaponSettings, PortalMode},
};

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;
pub(crate) const BRIDGE_THICKNESS: f32 = FLOOR_THICKNESS * BRIDGE_THICKNESS_FRACTION;

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
        weapons: MapWeaponSettings {
            projectiles: true,
            missiles: false,
            portals: PortalMode::None,
        },
        barrier_kinds: Vec::new(),
        bridge_kinds: Vec::new(),
    }
}
