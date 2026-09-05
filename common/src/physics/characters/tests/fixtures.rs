pub(super) use super::super::*;
pub(super) use crate::test_geometry::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};
pub(super) use crate::{
    config::CharacterPhysicsConfig,
    map::{MovingFloors, ramp_surface_at},
    physics::{CollisionWorld, character_overlaps_item},
    protocol::{Floor, Ladder, MapLayout, Position, Ramp, Wall},
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

// Edge plane at z = 0 spanning x -0.5..0.5, climbable from the -Z rail side.
// Spans level 0 -> 1; the landing fixtures put floors on the +Z side.
pub(crate) fn test_ladder() -> Ladder {
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
    }
}

// Ground in front of `test_ladder` (the climb side).
pub(crate) fn ladder_front_base_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 0.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    }
}

// Two-storey variant of `test_ladder` on the same edge.
pub(crate) fn test_ladder_two_storey() -> Ladder {
    Ladder {
        levels: 2,
        height: 2.0 * LEVEL_HEIGHT,
        ..test_ladder()
    }
}

// Ground behind `test_ladder`, same storey as the front base floor. The
// fence still blocks front-side crossings here; back-side walkers pass
// through onto the front floor.
pub(crate) fn ladder_back_base_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    }
}

// Landing behind `test_ladder`, one storey up.
pub(crate) fn ladder_back_landing_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

// X-facing variant of `test_ladder`: edge plane at x = 0 spanning z -0.5..0.5
// — the character collider's wide (1.0 m) axis faces this plane.
pub(crate) fn test_ladder_facing_x() -> Ladder {
    Ladder {
        x1: 0.0,
        z1: -0.5,
        x2: 0.0,
        z2: 0.5,
        nx: -1.0,
        nz: 0.0,
        level: 0,
        levels: 1,
        y: 0.0,
        height: LEVEL_HEIGHT,
    }
}

// Landing behind `test_ladder_facing_x`, one storey up.
pub(crate) fn ladder_back_landing_floor_x() -> Floor {
    Floor {
        x1: 0.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

pub(crate) fn ladder_collision_world(floors: &[Floor], ladders: &[Ladder]) -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            floors: floors.to_vec(),
            ladders: ladders.to_vec(),
            ..Default::default()
        },
        &crate::protocol::BarrierKindTable::default(),
    )
}

pub(crate) fn test_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
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
        y: 0.0,
        height: WALL_HEIGHT,
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
        y: 2.0 * LEVEL_HEIGHT,
        height: WALL_HEIGHT,
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
    crate::config::gameplay::load_test_gameplay()
        .expect("default gameplay config should load")
        .player
        .physics()
}

pub(crate) fn test_ladders() -> f32 {
    0.4
}

pub(crate) fn player_speed() -> f32 {
    9.0
}

pub(crate) fn character_step_toward(
    start: Position,
    vertical_velocity: f32,
    target_x: f32,
    target_z: f32,
    delta: f32,
) -> CharacterStep {
    CharacterStep {
        start,
        vertical_velocity,
        control_velocity: Vec3::new((target_x - start.x) / delta, 0.0, (target_z - start.z) / delta),
        external_displacement: Vec3::ZERO,
        delta,
    }
}

pub(crate) fn planned_move(entity: Entity, start: Position, target: Position) -> CharacterMovePlan {
    CharacterMovePlan::from_target(entity, start, target, 0.0, player_physics(), false)
}
