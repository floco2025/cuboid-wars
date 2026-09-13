use std::collections::HashMap;

use common::{
    celestial::{CelestialMapSettings, LocalTime, Season},
    config::{KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    constants::BARRIER_THICKNESS_FRACTION,
    map::MapGeometry,
    protocol::{MapSettings, PortalMode},
};

pub(crate) const CELL: f32 = 3.4;
pub(crate) const LEVEL_HEIGHT: f32 = 4.4;
pub(crate) const FLOOR_THICKNESS: f32 = 0.4;
pub(crate) const WALL_THICKNESS: f32 = 0.3;
pub(crate) const WALL_HEIGHT: f32 = LEVEL_HEIGHT - FLOOR_THICKNESS;
pub(crate) const BARRIER_THICKNESS: f32 = WALL_THICKNESS * BARRIER_THICKNESS_FRACTION;

pub(crate) fn sizes() -> MapGeometryConfig {
    MapGeometryConfig {
        grid_cell_size: CELL,
        level_height: LEVEL_HEIGHT,
        floor_thickness: FLOOR_THICKNESS,
        wall_thickness: WALL_THICKNESS,
    }
}

pub(crate) fn geometry(grid_cols: i32, grid_rows: i32) -> MapGeometry {
    MapGeometry::new(grid_cols, grid_rows, sizes())
}

pub(crate) fn map_settings() -> MapSettings {
    MapSettings {
        grounds: None,
        celestial: CelestialMapSettings {
            latitude_degrees: 40.0,
            season: Season::Summer,
            north_yaw_degrees: 0.0,
            start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
            start_moon_phase: 0.25,
        },
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
            gravity: 25.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.4,
            knockback: KnockbackConfig {
                max_speed: 10.0,
                up_speed: 4.0,
                deceleration: 12.0,
            },
        },
        portals: PortalMode::Both,
        switches: Vec::new(),
        barrier_kinds: Vec::new(),
        bridge_kinds: Vec::new(),
    }
}
