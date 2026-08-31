use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::shape::Cuboid,
    prelude::{ColliderHandle, Vector},
};
use std::f32::consts::FRAC_PI_3;

use super::{
    geometry::{character_pose, character_shape, character_support_probe_shape},
    ladder::evaluate_ladder_interaction,
    support::{
        character_ground_hit, perch_slide_displacement, position_has_floor_support, project_move_onto_support,
        snap_position_to_ground,
    },
    types::{CharacterMovementResult, CharacterSupport},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_STEP_HEIGHT, CHARACTER_STEP_MIN_WIDTH, CHARACTER_TERMINAL_VELOCITY, PHYSICS_EPSILON},
    physics::world::CollisionWorld,
    protocol::Position,
};

const CHARACTER_CONTACT_OFFSET: f32 = 0.01;
const CHARACTER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
const CHARACTER_AUTOSTEP_EPSILON: f32 = 0.01;

#[must_use]
pub fn player_jump_velocity(
    vertical_velocity: f32,
    collision_world: &CollisionWorld,
    physics: CharacterPhysicsConfig,
    jump_speed: f32,
    pos: &Position,
) -> Option<f32> {
    // Jumping is how a character detaches mid-climb, so it must work even
    // while the ladder is supplying upward velocity.
    let on_ladder = collision_world.ladder_volume_at(pos).is_some();
    if !on_ladder && (vertical_velocity > 0.0 || !position_has_floor_support(collision_world, pos, physics)) {
        return None;
    }

    Some(jump_speed)
}

// One fixed-tick request. Ladder decisions read only `control_velocity`;
// reconciliation and knockback ride `external_displacement` so they can move
// the body without impersonating player/actor intent.
#[derive(Debug, Clone, Copy)]
pub struct CharacterStep {
    pub start: Position,
    pub vertical_velocity: f32,
    pub control_velocity: Vec3,
    pub external_displacement: Vec3,
    pub delta: f32,
}

// The world the step happens in. `gravity` is the per-map acceleration
// magnitude, already resolved by the caller (`MapSettings::gravity_for`
// picks the low-gravity value when the power-up is active).
#[derive(Clone, Copy)]
pub struct CharacterEnvironment<'a> {
    pub collision_world: &'a CollisionWorld,
    pub gravity: f32,
    pub passable_kinds: &'a [crate::protocol::BarrierKindId],
    pub physics: CharacterPhysicsConfig,
    pub ladder_climb_ratio: f32,
    // Portal pass-through: while the body overlaps a linked aperture, its
    // backing colliders are excluded from this step's collision and support
    // queries. `None` for characters that cannot use portals (actors).
    pub portals: Option<&'a super::super::portals::PortalSet>,
}

#[must_use]
pub fn step_character_movement(step: CharacterStep, env: &CharacterEnvironment) -> CharacterMovementResult {
    let character_shape = character_shape(env.physics);
    let support_shape = character_support_probe_shape(env.physics);
    let excluded = env.portals.map_or_else(Vec::new, |portals| {
        portals.collision_exclusions(Vec3::new(step.start.x, step.start.y, step.start.z), env.physics)
    });
    let request = prepare_movement_request(step, env, &excluded, &character_shape, &support_shape);
    let collision = resolve_character_collision(step, env, &excluded, &character_shape, &request);
    finish_character_movement(step, env, &excluded, &support_shape, request, collision)
}

struct MovementRequest {
    next_vertical_velocity: f32,
    requested_horizontal: Vector,
    requested_vertical: Vector,
    requested_total: Vector,
    can_follow_ground: bool,
    ascending_ladder: bool,
    ladder_supported: bool,
}

fn prepare_movement_request(
    step: CharacterStep,
    env: &CharacterEnvironment,
    excluded_colliders: &[ColliderHandle],
    character_shape: &Cuboid,
    support_shape: &Cuboid,
) -> MovementRequest {
    let start_pos = &step.start;
    let collision_world = env.collision_world;
    let passable_kinds = env.passable_kinds;
    let physics = env.physics;

    let ground_probe = if step.vertical_velocity <= 0.0 {
        character_ground_hit(
            collision_world,
            support_shape,
            start_pos,
            passable_kinds,
            excluded_colliders,
            physics,
        )
    } else {
        None
    };
    let ladder = evaluate_ladder_interaction(
        collision_world,
        start_pos,
        step.vertical_velocity,
        step.control_velocity,
        step.delta,
        ground_probe.is_some(),
        env.ladder_climb_ratio,
    );
    let ascending_ladder = ladder.is_ascending();
    // Climbing suppresses ground following: without this, the ground snap
    // below would glue the first climb tick back onto the base floor.
    let can_follow_ground = step.vertical_velocity <= 0.0 && !ascending_ladder;
    let current_ground = if can_follow_ground { ground_probe } else { None };
    let next_vertical_velocity = if let Some(vertical_velocity) = ladder.vertical_velocity() {
        vertical_velocity
    } else if current_ground.is_some() {
        0.0
    } else {
        (step.vertical_velocity - env.gravity * step.delta).max(-CHARACTER_TERMINAL_VELOCITY)
    };

    let perch_slide_move = if can_follow_ground && current_ground.is_none() {
        perch_slide_displacement(
            collision_world,
            character_shape,
            start_pos,
            passable_kinds,
            physics,
            step.delta,
        )
    } else {
        Vector::ZERO
    };

    let funnel = env.portals.map_or(Vec3::ZERO, |portals| {
        portals.funnel_displacement(
            Vec3::new(start_pos.x, start_pos.y, start_pos.z),
            physics,
            step.control_velocity,
            step.vertical_velocity,
            step.delta,
        )
    });
    let target_x = step.control_velocity.x.mul_add(step.delta, start_pos.x) + step.external_displacement.x + funnel.x;
    let target_z = step.control_velocity.z.mul_add(step.delta, start_pos.z) + step.external_displacement.z + funnel.z;
    let (target_x, target_z) = ladder.constrain_target(start_pos, target_x, target_z, collision_world, physics);
    let requested_target = Position {
        x: target_x,
        y: next_vertical_velocity.mul_add(step.delta, start_pos.y),
        z: target_z,
    };
    let requested_horizontal_move =
        Vector::new(requested_target.x - start_pos.x, 0.0, requested_target.z - start_pos.z);
    let requested_vertical_move = Vector::new(0.0, requested_target.y - start_pos.y, 0.0);
    let supported_horizontal_move = current_ground.map_or(requested_horizontal_move, |ground| {
        project_move_onto_support(requested_horizontal_move, ground.normal)
    });
    // The perch slide is a separate term (not folded into
    // `supported_horizontal_move`) so the blocked/bump check below keeps
    // comparing requested body movement against actual movement — an idle
    // player perched against a wall must not hear bump feedback.
    let requested_move = supported_horizontal_move + perch_slide_move + requested_vertical_move;

    MovementRequest {
        next_vertical_velocity,
        requested_horizontal: supported_horizontal_move,
        requested_vertical: requested_vertical_move,
        requested_total: requested_move,
        can_follow_ground,
        ascending_ladder,
        ladder_supported: ladder.is_supported(),
    }
}

struct CharacterCollisionResult {
    translation: Vector,
    grounded: bool,
    saw_side_contact: bool,
    hit_ceiling: bool,
}

fn resolve_character_collision(
    step: CharacterStep,
    env: &CharacterEnvironment,
    excluded_colliders: &[ColliderHandle],
    character_shape: &Cuboid,
    request: &MovementRequest,
) -> CharacterCollisionResult {
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let movement = env.collision_world.move_character(
        step.delta,
        &character_controller(),
        character_shape,
        &character_pose(&step.start, env.physics),
        request.requested_total,
        env.passable_kinds,
        excluded_colliders,
        |collision| {
            let normal = vec3(collision.hit.normal1);
            let is_side_contact = normal.y.abs() <= 0.5;
            let is_ceiling = normal.y < -0.5 && request.requested_vertical.y > 0.0;
            if is_side_contact {
                saw_side_contact = true;
            }
            if is_ceiling {
                hit_ceiling = true;
            }
        },
    );

    CharacterCollisionResult {
        translation: movement.translation,
        grounded: movement.grounded,
        saw_side_contact,
        hit_ceiling,
    }
}

fn finish_character_movement(
    step: CharacterStep,
    env: &CharacterEnvironment,
    excluded_colliders: &[ColliderHandle],
    support_shape: &Cuboid,
    request: MovementRequest,
    collision: CharacterCollisionResult,
) -> CharacterMovementResult {
    let mut resolved = Position {
        x: step.start.x + collision.translation.x,
        y: step.start.y + collision.translation.y,
        z: step.start.z + collision.translation.z,
    };
    let resolved_ground = if request.can_follow_ground {
        character_ground_hit(
            env.collision_world,
            support_shape,
            &resolved,
            env.passable_kinds,
            excluded_colliders,
            env.physics,
        )
    } else {
        None
    };
    if let Some(ground) = resolved_ground {
        snap_position_to_ground(&mut resolved, ground, env.physics);
    }
    let mut vertical_velocity = request.next_vertical_velocity;
    // Rapier reports a side contact while auto-stepping over slab/trim edges.
    // That is normal movement, not a wall hit, so don't expose it as blocked.
    let stepped_up =
        collision.grounded && collision.translation.y > request.requested_total.y + CHARACTER_AUTOSTEP_EPSILON;
    let side_movement_blocked = collision.saw_side_contact
        && !stepped_up
        && movement_progress_was_blocked(request.requested_horizontal, collision.translation);
    // A climb whose rise was cut short hit something overhead (e.g. riding
    // the wrong side of a ladder into the floor above) — surface it as
    // blocked so the bump feedback fires. Scoped to climbing: ordinary jumps
    // against ceilings stay silent.
    let climb_rise_blocked = request.ascending_ladder
        && request.requested_vertical.y > 0.0
        && collision.translation.y < request.requested_vertical.y - PHYSICS_EPSILON;
    let blocked = side_movement_blocked || climb_rise_blocked;

    let grounded = resolved_ground.is_some() || collision.grounded;
    // `movement.grounded` covers support the center-line probe can't see
    // (resting on an edge sliver). The perch slide makes that state
    // transient, but a blocked slide (doorway lip, inside corner) can
    // persist — without this, gravity would pump fall velocity for seconds
    // while the body never moves.
    let landed_while_falling = grounded && vertical_velocity < 0.0;
    let hit_ceiling_while_rising = collision.hit_ceiling && vertical_velocity > 0.0;
    let upward_move_was_blocked =
        request.requested_vertical.y > 0.0 && collision.translation.y < request.requested_total.y - PHYSICS_EPSILON;
    if landed_while_falling || hit_ceiling_while_rising || upward_move_was_blocked {
        vertical_velocity = 0.0;
    }

    let support = if request.ladder_supported {
        CharacterSupport::Ladder
    } else if grounded {
        CharacterSupport::Ground
    } else {
        CharacterSupport::Airborne
    };

    CharacterMovementResult {
        position: resolved,
        vertical_velocity,
        support,
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

fn character_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(CHARACTER_CONTACT_OFFSET),
        autostep: Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(CHARACTER_STEP_HEIGHT),
            min_width: CharacterLength::Absolute(CHARACTER_STEP_MIN_WIDTH),
            include_dynamic_bodies: false,
        }),
        min_slope_slide_angle: FRAC_PI_3,
        snap_to_ground: None,
        ..KinematicCharacterController::default()
    }
}

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}
