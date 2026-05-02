use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Cuboid,
    },
    prelude::{Pose, Vector},
};

use super::world::{CollisionWorld, ShapeCastHit};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        PHYSICS_EPSILON, PLAYER_GRAVITY, PLAYER_GROUND_SNAP_DISTANCE, PLAYER_JUMP_SPEED, PLAYER_STEP_HEIGHT,
        PLAYER_STEP_MIN_WIDTH, PLAYER_TERMINAL_VELOCITY,
    },
    protocol::Position,
};

const PLAYER_CONTACT_OFFSET: f32 = 0.01;
const PLAYER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
const PLAYER_AUTOSTEP_EPSILON: f32 = 0.01;

// Component attached to character entities tracking persistent gravity-axis
// motion. X/Z movement is derived from intent each tick. Running on a ramp can
// add vertical displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalMotion(pub f32);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterMovementResult {
    pub position: Position,
    pub vertical_velocity: f32,
    // True when static-world collision materially blocked requested movement.
    // Side contacts that Rapier resolves by auto-stepping are not treated as blocked.
    pub blocked: bool,
}

// Represents a character's intended movement after static-world collision but
// before character-character collision.
#[derive(Copy, Clone)]
pub struct PlannedCharacterMove {
    pub entity: Entity,
    pub start: Position,
    pub target: Position,
    pub target_vertical_velocity: f32,
    pub physics: CharacterPhysicsConfig,
    pub blocked: bool,
}

// Check if a planned move would overlap with any other character's planned position.
#[must_use]
pub fn overlaps_other_character(candidate: &PlannedCharacterMove, planned_moves: &[PlannedCharacterMove]) -> bool {
    overlapping_character(candidate, planned_moves).is_some()
}

#[must_use]
pub fn overlapping_character<'a>(
    candidate: &PlannedCharacterMove,
    planned_moves: &'a [PlannedCharacterMove],
) -> Option<&'a PlannedCharacterMove> {
    planned_moves
        .iter()
        .find(|other| other.entity != candidate.entity && planned_character_moves_intersect(candidate, other))
}

#[must_use]
pub fn planned_character_moves_intersect(candidate: &PlannedCharacterMove, other: &PlannedCharacterMove) -> bool {
    if character_positions_intersect_with_clearance(&candidate.start, candidate.physics, &other.start, other.physics) {
        return !planned_character_moves_separate(candidate, other);
    }

    character_paths_intersect_with_clearance(
        &candidate.start,
        &candidate.target,
        candidate.physics,
        &other.start,
        &other.target,
        other.physics,
    )
}

fn planned_character_moves_separate(candidate: &PlannedCharacterMove, other: &PlannedCharacterMove) -> bool {
    let start_distance_sq = position_distance_sq(&candidate.start, &other.start);
    let target_distance_sq = position_distance_sq(&candidate.target, &other.target);
    target_distance_sq > start_distance_sq + PHYSICS_EPSILON * PHYSICS_EPSILON
}

fn position_distance_sq(a: &Position, b: &Position) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    let dz = a.z - b.z;
    dx.mul_add(dx, dy.mul_add(dy, dz * dz))
}

#[must_use]
pub fn try_start_player_jump(
    vertical_velocity: &mut f32,
    collision_world: &CollisionWorld,
    physics: CharacterPhysicsConfig,
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    let ground_probe_pos = Position { x, y: pos.y, z };
    if *vertical_velocity > 0.0 || !is_player_grounded(collision_world, &ground_probe_pos, physics) {
        return false;
    }

    *vertical_velocity = PLAYER_JUMP_SPEED;
    true
}

// Steps one character from `start_pos` toward the requested horizontal target
// `target_x`/`target_z`.
//
// `start_pos` is the character position at the start of the frame. The caller
// supplies only the desired horizontal target because X/Z movement comes from
// intent. Vertical movement is calculated here from `start_vertical_velocity`,
// gravity, support following, and floor/ceiling collision. Static-world
// collision may block, slide, step, or otherwise adjust the requested movement
// before the final position is returned.
#[must_use]
pub fn step_character_movement(
    start_pos: &Position,
    start_vertical_velocity: f32,
    collision_world: &CollisionWorld,
    has_phasing: bool,
    physics: CharacterPhysicsConfig,
    target_x: f32,
    target_z: f32,
    delta: f32,
) -> CharacterMovementResult {
    let character_shape = character_shape(physics);
    let character_pos = character_pose(start_pos, physics);
    let support_shape = character_support_probe_shape(physics);
    let current_ground = if start_vertical_velocity <= 0.0 {
        player_ground_hit(collision_world, &support_shape, start_pos, has_phasing, physics)
    } else {
        None
    };
    let mut next_vertical_velocity = start_vertical_velocity;
    let can_follow_ground = next_vertical_velocity <= 0.0;
    if current_ground.is_some() && can_follow_ground {
        next_vertical_velocity = 0.0;
    } else {
        // Apply gravity for this frame, but cap falling speed so large falls
        // remain stable and predictable.
        next_vertical_velocity -= PLAYER_GRAVITY * delta;
        next_vertical_velocity = next_vertical_velocity.max(-PLAYER_TERMINAL_VELOCITY);
    }
    let controller = player_controller();

    let requested_target = Position {
        x: target_x,
        y: next_vertical_velocity.mul_add(delta, start_pos.y),
        z: target_z,
    };
    let requested_horizontal_move =
        Vector::new(requested_target.x - start_pos.x, 0.0, requested_target.z - start_pos.z);
    let requested_vertical_move = Vector::new(0.0, requested_target.y - start_pos.y, 0.0);
    let supported_horizontal_move = current_ground.map_or(requested_horizontal_move, |ground| {
        project_input_move_onto_support(requested_horizontal_move, ground.normal)
    });
    let requested_move = supported_horizontal_move + requested_vertical_move;
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let movement = collision_world.move_character(
        delta,
        &controller,
        &character_shape,
        &character_pos,
        requested_move,
        has_phasing,
        |collision| {
            let normal = vec3(collision.hit.normal1);
            let is_side_contact = normal.y.abs() <= 0.5;
            let is_ceiling = normal.y < -0.5 && requested_vertical_move.y > 0.0;
            if is_side_contact {
                saw_side_contact = true;
            }
            if is_ceiling {
                hit_ceiling = true;
            }
        },
    );
    let mut resolved = Position {
        x: start_pos.x + movement.translation.x,
        y: start_pos.y + movement.translation.y,
        z: start_pos.z + movement.translation.z,
    };
    let resolved_ground = if can_follow_ground {
        player_ground_hit(collision_world, &support_shape, &resolved, has_phasing, physics)
    } else {
        None
    };
    if let Some(ground) = resolved_ground {
        resolved.y -= ground.t - physics.low_obstacle_clearance;
    }
    let mut vertical_velocity = next_vertical_velocity;
    // Rapier reports a side contact while auto-stepping over slab/trim edges.
    // That is normal movement, not a wall hit, so don't expose it as blocked.
    let stepped_up = movement.grounded && movement.translation.y > requested_move.y + PLAYER_AUTOSTEP_EPSILON;
    let blocked = saw_side_contact
        && !stepped_up
        && movement_progress_was_blocked(supported_horizontal_move, movement.translation);

    let grounded = resolved_ground.is_some();
    if grounded && vertical_velocity < 0.0
        || hit_ceiling && vertical_velocity > 0.0
        || requested_vertical_move.y > 0.0 && movement.translation.y < requested_move.y - PHYSICS_EPSILON
    {
        vertical_velocity = 0.0;
    }

    CharacterMovementResult {
        position: resolved,
        vertical_velocity,
        blocked,
    }
}

fn movement_progress_was_blocked(desired: Vector, actual: Vector) -> bool {
    let desired_xz = Vec3::new(desired.x, 0.0, desired.z);
    let desired_len = desired_xz.length();
    if desired_len <= PLAYER_BLOCKED_MOVEMENT_EPSILON {
        return false;
    }

    let actual_xz = Vec3::new(actual.x, 0.0, actual.z);
    let desired_dir = desired_xz / desired_len;
    let actual_along_desired = actual_xz.dot(desired_dir);
    actual_along_desired < desired_len - PLAYER_BLOCKED_MOVEMENT_EPSILON
}

fn is_player_grounded(collision_world: &CollisionWorld, pos: &Position, physics: CharacterPhysicsConfig) -> bool {
    let shape = character_support_probe_shape(physics);
    player_ground_hit(collision_world, &shape, pos, false, physics).is_some()
}

fn player_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    has_phasing: bool,
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    let pose = character_support_probe_pose(pos, physics);
    collision_world.ground_hit(
        shape,
        &pose,
        PLAYER_GROUND_SNAP_DISTANCE + physics.low_obstacle_clearance,
        0.0,
        has_phasing,
    )
}

fn player_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(PLAYER_CONTACT_OFFSET),
        autostep: Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(PLAYER_STEP_HEIGHT),
            min_width: CharacterLength::Absolute(PLAYER_STEP_MIN_WIDTH),
            include_dynamic_bodies: false,
        }),
        min_slope_slide_angle: std::f32::consts::FRAC_PI_3,
        snap_to_ground: None,
        ..KinematicCharacterController::default()
    }
}

fn character_shape(physics: CharacterPhysicsConfig) -> Cuboid {
    Cuboid::new(Vector::new(
        physics.collider.width / 2.0,
        character_collision_height(physics) / 2.0,
        physics.collider.depth / 2.0,
    ))
}

fn character_collision_height(physics: CharacterPhysicsConfig) -> f32 {
    // The logical foot position remains at `Position.y`; the collider starts
    // above it so low obstacle contacts do not block movement.
    physics.collider.height - physics.low_obstacle_clearance
}

fn character_support_probe_shape(physics: CharacterPhysicsConfig) -> Cuboid {
    Cuboid::new(Vector::new(
        physics.support_probe.width / 2.0,
        character_collision_height(physics) / 2.0,
        physics.support_probe.depth / 2.0,
    ))
}

fn project_input_move_onto_support(input_move: Vector, support_normal: Vec3) -> Vector {
    if input_move.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON {
        return input_move;
    }

    let input_move = Vec3::new(input_move.x, input_move.y, input_move.z);
    let tangent = input_move - support_normal * input_move.dot(support_normal);
    let Some(tangent_dir) = tangent.try_normalize() else {
        return Vector::new(input_move.x, input_move.y, input_move.z);
    };

    let surface_move = tangent_dir * input_move.length();
    Vector::new(surface_move.x, surface_move.y, surface_move.z)
}

fn character_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    Pose::translation(
        pos.x,
        pos.y + physics.low_obstacle_clearance + character_collision_height(physics) / 2.0,
        pos.z,
    )
}

fn character_support_probe_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    character_pose(pos, physics)
}

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[must_use]
pub fn character_paths_intersect(
    start1: &Position,
    end1: &Position,
    physics1: CharacterPhysicsConfig,
    start2: &Position,
    end2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    character_paths_intersect_with_clearance(start1, end1, physics1, start2, end2, physics2)
}

fn character_paths_intersect_with_clearance(
    start1: &Position,
    end1: &Position,
    physics1: CharacterPhysicsConfig,
    start2: &Position,
    end2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let shape1 = character_shape(physics1);
    let shape2 = character_shape(physics2);
    let velocity1 = Vector::new(end1.x - start1.x, end1.y - start1.y, end1.z - start1.z);
    let velocity2 = Vector::new(end2.x - start2.x, end2.y - start2.y, end2.z - start2.z);
    if character_positions_intersect_with_clearance(start1, physics1, start2, physics2) {
        return true;
    }

    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    cast_shapes(
        &character_pose(start1, physics1),
        velocity1,
        &shape1,
        &character_pose(start2, physics2),
        velocity2,
        &shape2,
        options,
    )
    .is_ok_and(|hit| hit.is_some())
}

fn character_positions_intersect_with_clearance(
    pos1: &Position,
    physics1: CharacterPhysicsConfig,
    pos2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let shape1 = character_shape(physics1);
    let shape2 = character_shape(physics2);
    intersection_test(
        &character_pose(pos1, physics1),
        &shape1,
        &character_pose(pos2, physics2),
        &shape2,
    )
    .is_ok_and(|overlaps| overlaps)
}

#[must_use]
pub fn overlap_player_vs_item(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dy = player_pos.y - item_pos.y;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}

#[cfg(test)]
mod tests;
