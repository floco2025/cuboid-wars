pub(super) use super::super::*;
pub(super) use crate::{
    config::{CharacterPhysicsConfig, GameplayConfig},
    constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_THICKNESS},
    map::ramp_surface_at,
    physics::{CollisionWorld, character_overlaps_item},
    protocol::{Floor, MapLayout, Position, Ramp, Wall},
};
pub(super) use bevy_ecs::prelude::Entity;
pub(super) use bevy_math::Vec3;

// Gravity magnitude for movement tests (matches the shipping map's setting).
pub(crate) const TEST_GRAVITY: f32 = 25.0;

pub(crate) fn test_ramp() -> Ramp {
    Ramp {
        x1: 0.0,
        y1: 0.0,
        z1: 0.0,
        x2: 4.0,
        y2: LEVEL_HEIGHT,
        z2: 8.0,
    }
}

pub(crate) fn upper_floor_west_of_ramp() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 0.0,
        z2: 8.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

pub(crate) fn lower_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    }
}

pub(crate) fn upper_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

pub(crate) fn low_overhead_floor() -> Floor {
    let player_physics = player_physics();
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: player_physics.collider.top_y_offset() - 0.05,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

pub(crate) fn test_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
    }
}

pub(crate) fn horizontal_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        width: 0.2,
        level: 0,
    }
}

pub(crate) fn upper_horizontal_wall() -> Wall {
    Wall {
        x1: 27.85,
        z1: 32.0,
        x2: 35.85,
        z2: 32.0,
        width: 0.3,
        level: 2,
    }
}

pub(crate) fn collision_world(floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    collision_world_with(&[], floors, ramps)
}

pub(crate) fn collision_world_with(walls: &[Wall], floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: walls.to_vec(),
            ramps: ramps.to_vec(),
            floors: floors.to_vec(),
            ..Default::default()
        },
        &crate::protocol::BarrierKindTable::default(),
    )
}

pub(crate) fn test_entity(index: u32) -> Entity {
    Entity::from_raw_u32(index).expect("test entity index should be valid")
}

pub(crate) fn player_physics() -> CharacterPhysicsConfig {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .player
        .physics()
}

pub(crate) fn player_speed() -> f32 {
    GameplayConfig::load_default()
        .expect("default gameplay config should load")
        .player
        .run_speed
}

pub(crate) fn planned_move(entity: Entity, start: Position, target: Position) -> CharacterMovePlan {
    CharacterMovePlan::from_target(entity, start, target, 0.0, player_physics(), false)
}
