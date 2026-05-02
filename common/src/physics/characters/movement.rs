use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::shape::Cuboid,
    prelude::Vector,
};

use super::{
    geometry::{character_pose, character_shape, character_support_probe_pose, character_support_probe_shape},
    types::CharacterMovementResult,
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        PHYSICS_EPSILON, PLAYER_GRAVITY, PLAYER_GROUND_SNAP_DISTANCE, PLAYER_JUMP_SPEED, PLAYER_STEP_HEIGHT,
        PLAYER_STEP_MIN_WIDTH, PLAYER_TERMINAL_VELOCITY,
    },
    physics::world::{CollisionWorld, ShapeCastHit},
    protocol::Position,
};

const CHARACTER_CONTACT_OFFSET: f32 = 0.01;
const CHARACTER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
const CHARACTER_AUTOSTEP_EPSILON: f32 = 0.01;

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
    if *vertical_velocity > 0.0 || !is_character_grounded(collision_world, &ground_probe_pos, physics) {
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
        character_ground_hit(collision_world, &support_shape, start_pos, has_phasing, physics)
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
    let controller = character_controller();

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
        character_ground_hit(collision_world, &support_shape, &resolved, has_phasing, physics)
    } else {
        None
    };
    if let Some(ground) = resolved_ground {
        resolved.y -= ground.t - physics.collider.bottom_y_offset();
    }
    let mut vertical_velocity = next_vertical_velocity;
    // Rapier reports a side contact while auto-stepping over slab/trim edges.
    // That is normal movement, not a wall hit, so don't expose it as blocked.
    let stepped_up = movement.grounded && movement.translation.y > requested_move.y + CHARACTER_AUTOSTEP_EPSILON;
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
    if desired_len <= CHARACTER_BLOCKED_MOVEMENT_EPSILON {
        return false;
    }

    let actual_xz = Vec3::new(actual.x, 0.0, actual.z);
    let desired_dir = desired_xz / desired_len;
    let actual_along_desired = actual_xz.dot(desired_dir);
    actual_along_desired < desired_len - CHARACTER_BLOCKED_MOVEMENT_EPSILON
}

fn is_character_grounded(collision_world: &CollisionWorld, pos: &Position, physics: CharacterPhysicsConfig) -> bool {
    let shape = character_support_probe_shape(physics);
    character_ground_hit(collision_world, &shape, pos, false, physics).is_some()
}

fn character_ground_hit(
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
        PLAYER_GROUND_SNAP_DISTANCE + physics.collider.bottom_y_offset(),
        0.0,
        has_phasing,
    )
}

fn character_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(CHARACTER_CONTACT_OFFSET),
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

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
