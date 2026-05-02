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
    constants::{
        PHYSICS_EPSILON, PLAYER_DEPTH, PLAYER_GRAVITY, PLAYER_GROUND_SNAP_DISTANCE, PLAYER_HEIGHT, PLAYER_JUMP_SPEED,
        PLAYER_LOW_OBSTACLE_CLEARANCE, PLAYER_STEP_HEIGHT, PLAYER_STEP_MIN_WIDTH, PLAYER_SUPPORT_PROBE_DEPTH,
        PLAYER_SUPPORT_PROBE_WIDTH, PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH,
    },
    protocol::Position,
};

const PLAYER_CONTACT_OFFSET: f32 = 0.01;
const PLAYER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
const PLAYER_AUTOSTEP_EPSILON: f32 = 0.01;

// Component attached to character entities tracking persistent gravity-axis
// motion. X/Z movement is derived from intent each tick. Running on a ramp can
// add Y displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct CharacterVerticalMotion {
    pub vertical_velocity: f32,
}

impl CharacterVerticalMotion {
    pub fn apply_gravity(&mut self, delta: f32) {
        self.vertical_velocity -= PLAYER_GRAVITY * delta;
    }

    pub fn apply_terminal_velocity(&mut self) {
        if self.vertical_velocity < -PLAYER_TERMINAL_VELOCITY {
            self.vertical_velocity = -PLAYER_TERMINAL_VELOCITY;
        }
    }
}

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

fn planned_character_moves_intersect(candidate: &PlannedCharacterMove, other: &PlannedCharacterMove) -> bool {
    if character_positions_intersect(&candidate.start, &other.start) {
        return !planned_character_moves_separate(candidate, other);
    }

    character_paths_intersect(&candidate.start, &candidate.target, &other.start, &other.target)
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
    motion: &mut CharacterVerticalMotion,
    collision_world: &CollisionWorld,
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    let ground_probe_pos = Position { x, y: pos.y, z };
    if motion.vertical_velocity > 0.0 || !is_player_grounded(collision_world, &ground_probe_pos) {
        return false;
    }

    motion.vertical_velocity = PLAYER_JUMP_SPEED;
    true
}

// Steps one character from `pos` toward the requested horizontal target `x`/`z`.
//
// `pos` is the character position at the start of the frame. The caller supplies
// only the desired horizontal target because X/Z movement comes from intent. Y
// movement is calculated here from `CharacterVerticalMotion`, gravity, support
// following, and floor/ceiling collision. Static-world collision may block,
// slide, step, or otherwise adjust the requested movement before the final
// position is returned.
#[must_use]
pub fn step_character_movement(
    pos: &Position,
    motion: &CharacterVerticalMotion,
    collision_world: &CollisionWorld,
    has_phasing: bool,
    x: f32,
    z: f32,
    delta: f32,
) -> CharacterMovementResult {
    let character_shape = player_shape();
    let character_pos = player_pose(pos);
    let support_shape = player_support_probe_shape();
    let current_ground = if motion.vertical_velocity <= 0.0 {
        player_ground_hit(collision_world, &support_shape, pos, has_phasing)
    } else {
        None
    };
    let mut next_motion = CharacterVerticalMotion {
        vertical_velocity: motion.vertical_velocity,
    };
    let can_follow_ground = next_motion.vertical_velocity <= 0.0;
    if current_ground.is_some() && can_follow_ground {
        next_motion.vertical_velocity = 0.0;
    } else {
        next_motion.apply_gravity(delta);
        next_motion.apply_terminal_velocity();
    }
    let controller = player_controller();

    let target = Position {
        x,
        y: next_motion.vertical_velocity.mul_add(delta, pos.y),
        z,
    };
    let input_move = Vector::new(target.x - pos.x, 0.0, target.z - pos.z);
    let gravity_axis_move = Vector::new(0.0, target.y - pos.y, 0.0);
    let supported_input_move = current_ground.map_or(input_move, |ground| {
        project_input_move_onto_support(input_move, ground.normal)
    });
    let requested_move = supported_input_move + gravity_axis_move;
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
            let is_ceiling = normal.y < -0.5 && gravity_axis_move.y > 0.0;
            if is_side_contact {
                saw_side_contact = true;
            }
            if is_ceiling {
                hit_ceiling = true;
            }
        },
    );
    let mut resolved = Position {
        x: pos.x + movement.translation.x,
        y: pos.y + movement.translation.y,
        z: pos.z + movement.translation.z,
    };
    let resolved_ground = if can_follow_ground {
        player_ground_hit(collision_world, &support_shape, &resolved, has_phasing)
    } else {
        None
    };
    if let Some(ground) = resolved_ground {
        resolved.y -= ground.t - PLAYER_LOW_OBSTACLE_CLEARANCE;
    }
    let mut vertical_velocity = next_motion.vertical_velocity;
    // Rapier reports a side contact while auto-stepping over slab/trim edges.
    // That is normal movement, not a wall hit, so don't expose it as blocked.
    let stepped_up = movement.grounded && movement.translation.y > requested_move.y + PLAYER_AUTOSTEP_EPSILON;
    let blocked =
        saw_side_contact && !stepped_up && movement_progress_was_blocked(supported_input_move, movement.translation);

    let grounded = resolved_ground.is_some();
    if grounded && vertical_velocity < 0.0
        || hit_ceiling && vertical_velocity > 0.0
        || gravity_axis_move.y > 0.0 && movement.translation.y < requested_move.y - PHYSICS_EPSILON
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

fn is_player_grounded(collision_world: &CollisionWorld, pos: &Position) -> bool {
    let shape = player_support_probe_shape();
    player_ground_hit(collision_world, &shape, pos, false).is_some()
}

fn player_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    has_phasing: bool,
) -> Option<ShapeCastHit> {
    let pose = player_support_probe_pose(pos);
    collision_world.ground_hit(
        shape,
        &pose,
        PLAYER_GROUND_SNAP_DISTANCE + PLAYER_LOW_OBSTACLE_CLEARANCE,
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

fn player_shape() -> Cuboid {
    Cuboid::new(Vector::new(
        PLAYER_WIDTH / 2.0,
        player_collision_height() / 2.0,
        PLAYER_DEPTH / 2.0,
    ))
}

fn player_collision_height() -> f32 {
    // The logical foot position remains at `Position.y`; the collider starts
    // above it so low obstacle contacts do not block movement.
    PLAYER_HEIGHT - PLAYER_LOW_OBSTACLE_CLEARANCE
}

fn player_support_probe_shape() -> Cuboid {
    Cuboid::new(Vector::new(
        PLAYER_SUPPORT_PROBE_WIDTH / 2.0,
        player_collision_height() / 2.0,
        PLAYER_SUPPORT_PROBE_DEPTH / 2.0,
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

fn player_pose(pos: &Position) -> Pose {
    Pose::translation(
        pos.x,
        pos.y + PLAYER_LOW_OBSTACLE_CLEARANCE + player_collision_height() / 2.0,
        pos.z,
    )
}

fn player_support_probe_pose(pos: &Position) -> Pose {
    player_pose(pos)
}

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[must_use]
pub fn character_paths_intersect(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    let shape = player_shape();
    let velocity1 = Vector::new(end1.x - start1.x, end1.y - start1.y, end1.z - start1.z);
    let velocity2 = Vector::new(end2.x - start2.x, end2.y - start2.y, end2.z - start2.z);
    if character_positions_intersect(start1, start2) {
        return true;
    }

    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    cast_shapes(
        &player_pose(start1),
        velocity1,
        &shape,
        &player_pose(start2),
        velocity2,
        &shape,
        options,
    )
    .is_ok_and(|hit| hit.is_some())
}

fn character_positions_intersect(pos1: &Position, pos2: &Position) -> bool {
    let shape = player_shape();
    intersection_test(&player_pose(pos1), &shape, &player_pose(pos2), &shape).is_ok_and(|overlaps| overlaps)
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
