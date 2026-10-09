pub(super) use bevy::prelude::*;
pub(super) use common::{
    config::CharacterPhysicsConfig,
    map::Carriers,
    physics::{CharacterSupport, CollisionWorld, KnockbackVelocity, PortalSet},
    protocol::{
        Carrier, CarrierId, Floor, Ladder, MapLayout, MapSettings, PlayerMoveIntent, PlayerStance, Position, Ramp,
    },
};

pub(super) use super::super::{HorizontalVelocity, PlayerJump, PlayerMovementStep, player_jump, step_player_movement};
pub(super) use crate::test_fixtures::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, gameplay_config};
use common::{
    celestial::{CelestialMapSettings, LocalTime, Season},
    config::{KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    constants::{BRIDGE_THICKNESS_FRACTION, LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE},
    protocol::{PortalMode, SwitchState, Wall},
};
use std::collections::HashMap;

pub(super) const TEST_GRAVITY: f32 = 25.0;
pub(super) const TEST_PLAYER_SPEED: f32 = 9.0;
pub(super) const BRIDGE_THICKNESS: f32 = FLOOR_THICKNESS * BRIDGE_THICKNESS_FRACTION;
pub(super) const TILE: CarrierId = CarrierId(1);

pub(super) fn test_movement() -> MapMovementConfig {
    MapMovementConfig {
        player: PlayerMovementConfig {
            move_speed: TEST_PLAYER_SPEED,
            move_speed_power_up: 1.5,
            move_speed_ladder: 0.4,
            jump_speed: 12.0,
            ground_acceleration: 20.0,
            ground_deceleration: 30.0,
            ground_lateral_deceleration: 40.0,
            air_acceleration: 5.0,
            air_deceleration: 5.0,
            air_lateral_deceleration: 5.0,
        },
        actors: HashMap::new(),
        missile_speed: 16.0,
        projectile_speed: 90.0,
        gravity: TEST_GRAVITY,
        low_gravity: 5.0,
        knockback: KnockbackConfig {
            max_speed: 15.0,
            up_speed: 7.0,
            deceleration: 35.0,
        },
    }
}

// The map every player-step test runs on unless it pins its own numbers.
pub(super) fn map_settings() -> MapSettings {
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
        geometry: MapGeometryConfig {
            grid_cell_size: 2.0,
            level_height: 2.0,
            floor_thickness: 0.2,
            wall_thickness: 0.2,
        },
        movement: MapMovementConfig {
            player: PlayerMovementConfig {
                move_speed: 4.375,
                move_speed_power_up: 1.5,
                move_speed_ladder: 0.6,
                jump_speed: 5.809475,
                ground_acceleration: 43.75,
                ground_deceleration: 17.5,
                ground_lateral_deceleration: 43.75,
                air_acceleration: 21.875,
                air_deceleration: 0.0,
                air_lateral_deceleration: 0.0,
            },
            actors: HashMap::new(),
            missile_speed: 20.0,
            projectile_speed: 30.0,
            gravity: 15.0,
            low_gravity: 5.0,
            knockback: KnockbackConfig {
                max_speed: 10.0,
                up_speed: 4.0,
                deceleration: 12.0,
            },
        },
        portals: PortalMode::Both,
        switches: Vec::new(),
        fields: Vec::new(),
    }
}

pub(super) fn player_physics() -> CharacterPhysicsConfig {
    gameplay_config().player.physics()
}

pub(super) fn lower_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// Edge plane at z = 0 spanning x -0.5..0.5, climbable from the -Z rail side.
// Spans level 0 -> 1; the landing fixtures put floors on the +Z side.
pub(super) fn test_ladder() -> Ladder {
    Ladder {
        x1: -0.5,
        z1: 0.0,
        x2: 0.5,
        z2: 0.0,
        nx: 0.0,
        nz: -1.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

// Two-storey variant of `test_ladder` on the same edge.
pub(super) fn test_ladder_two_storey() -> Ladder {
    Ladder {
        levels: 2,
        height: 2.0 * LEVEL_HEIGHT,
        ..test_ladder()
    }
}

// Ground in front of `test_ladder` (the climb side).
pub(super) fn ladder_front_base_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// Landing behind `test_ladder`, one storey up.
pub(super) fn ladder_back_landing_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    }
}

// Where the plane clamp holds the player against `test_ladder`, measured
// from the rail plane, which sits at z = -LADDER_RAIL_INSET.
pub(super) fn player_hold_distance() -> f32 {
    player_physics().movement_collider.radius() + LADDER_STANDOFF_CLEARANCE
}

pub(super) fn rail_plane_z() -> f32 {
    -LADDER_RAIL_INSET
}

pub(super) fn test_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    }
}

pub(super) fn ladder_collision_world(floors: &[Floor], ladders: &[Ladder]) -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        floors: floors.to_vec(),
        ladders: ladders.to_vec(),
        ..Default::default()
    })
}

// The world `tick` ticks into its carriers' cycles: `previous` is the pose
// one tick earlier, and the colliders already sit at `current`.
pub(super) fn world_at(layout: &MapLayout, tick: u32) -> (CollisionWorld, Carriers) {
    let mut world = CollisionWorld::from_map_layout(layout);
    let mut carriers = Carriers::from_layout(layout);
    carriers.advance(tick.wrapping_sub(1), &SwitchState::default());
    carriers.advance(tick, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    (world, carriers)
}

// Slides from the origin four meters along +X in two seconds: 2 m/s, one
// fifteenth of a meter per tick. The tile is a slab centered on the
// carrier's origin.
pub(super) fn slider() -> (Carrier, Floor) {
    (
        Carrier {
            motion: Default::default(),
            initially_on: true,
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            switch: None,
        },
        Floor {
            x1: -1.5,
            z1: -1.5,
            x2: 1.5,
            z2: 1.5,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: TILE,
        },
    )
}
