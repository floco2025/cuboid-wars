use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::shape::Cuboid,
    prelude::Vector,
};
use std::f32::consts::FRAC_PI_3;

use super::{
    geometry::{character_pose, character_shape, character_support_probe_pose, character_support_probe_shape},
    ladder::evaluate_ladder_interaction,
    types::CharacterMovementResult,
};
use crate::{
    config::{CharacterPhysicsConfig, LaddersConfig},
    constants::{
        CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_PERCH_SLIDE_SPEED, CHARACTER_STEP_HEIGHT, CHARACTER_STEP_MIN_WIDTH,
        CHARACTER_TERMINAL_VELOCITY, PHYSICS_EPSILON,
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
    jump_speed: f32,
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    let ground_probe_pos = Position { x, y: pos.y, z };
    // A ladder counts as support: jumping is how you detach mid-climb, so it
    // must work regardless of the climb's vertical velocity. (The climb
    // volume covers only the ladder's front — its back is not a ladder.)
    let on_ladder = collision_world.ladder_volume_at(&ground_probe_pos).is_some();
    if !on_ladder && (*vertical_velocity > 0.0 || !is_character_grounded(collision_world, &ground_probe_pos, physics)) {
        return false;
    }

    *vertical_velocity = jump_speed;
    true
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
    pub ladders: LaddersConfig,
}

#[must_use]
pub fn step_character_movement(step: CharacterStep, env: &CharacterEnvironment) -> CharacterMovementResult {
    let CharacterStep {
        start,
        vertical_velocity: start_vertical_velocity,
        control_velocity,
        external_displacement,
        delta,
    } = step;
    let start_pos = &start;
    let collision_world = env.collision_world;
    let gravity = env.gravity;
    let passable_kinds = env.passable_kinds;
    let physics = env.physics;

    let character_shape = character_shape(physics);
    let character_pos = character_pose(start_pos, physics);
    let support_shape = character_support_probe_shape(physics);
    let ground_probe = if start_vertical_velocity <= 0.0 {
        character_ground_hit(collision_world, &support_shape, start_pos, passable_kinds, physics)
    } else {
        None
    };
    let ladder = evaluate_ladder_interaction(
        collision_world,
        start_pos,
        start_vertical_velocity,
        control_velocity,
        delta,
        ground_probe.is_some(),
        env.ladders,
    );
    let climbing = ladder.climbing();
    // Climbing suppresses ground following: without this, the ground snap
    // below would glue the first climb tick back onto the base floor.
    let can_follow_ground = start_vertical_velocity <= 0.0 && !climbing;
    let current_ground = if can_follow_ground { ground_probe } else { None };
    let next_vertical_velocity = if let Some(vertical_velocity) = ladder.vertical_velocity() {
        vertical_velocity
    } else if current_ground.is_some() {
        0.0
    } else {
        (start_vertical_velocity - gravity * delta).max(-CHARACTER_TERMINAL_VELOCITY)
    };

    // "Perched": the center-line probe reads airborne while the collider
    // still rests on an edge sliver — never a stable state. Slide away from
    // the support contact so the fall actually happens; input (walk/run
    // speed) overrides the slide to walk back on. The witness point of a
    // flat box contact is indeterminate along the non-overhang axis, so the
    // direction can be diagonal — its outward component still clears the
    // band within a few ticks. `try_normalize` fails only in geometric
    // corner cases; skipping the slide that tick is safe (the end-of-tick
    // vv-zeroing keeps the perch from pumping fall velocity).
    let perch_slide_move = if can_follow_ground && current_ground.is_none() {
        character_perch_hit(collision_world, &character_shape, start_pos, passable_kinds, physics)
            .and_then(|hit| Vec3::new(start_pos.x - hit.contact.x, 0.0, start_pos.z - hit.contact.z).try_normalize())
            .map_or(Vector::ZERO, |dir| {
                Vector::new(dir.x, 0.0, dir.z) * CHARACTER_PERCH_SLIDE_SPEED * delta
            })
    } else {
        Vector::ZERO
    };
    let controller = character_controller();

    let target_x = control_velocity.x.mul_add(delta, start_pos.x) + external_displacement.x;
    let target_z = control_velocity.z.mul_add(delta, start_pos.z) + external_displacement.z;
    let (target_x, target_z) = ladder.constrain_target(start_pos, target_x, target_z, collision_world, physics);
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
    // The perch slide is a separate term (not folded into
    // `supported_horizontal_move`) so the blocked/bump check below keeps
    // comparing requested body movement against actual movement — an idle
    // player perched against a wall must not hear bump feedback.
    let requested_move = supported_horizontal_move + perch_slide_move + requested_vertical_move;
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let movement = collision_world.move_character(
        delta,
        &controller,
        &character_shape,
        &character_pos,
        requested_move,
        passable_kinds,
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
        character_ground_hit(collision_world, &support_shape, &resolved, passable_kinds, physics)
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
    // A climb whose rise was cut short hit something overhead (e.g. riding
    // the wrong side of a ladder into the floor above) — surface it as
    // blocked so the bump feedback fires. Scoped to climbing: ordinary jumps
    // against ceilings stay silent.
    let climb_rise_blocked = climbing
        && requested_vertical_move.y > 0.0
        && movement.translation.y < requested_vertical_move.y - PHYSICS_EPSILON;
    let blocked = saw_side_contact
        && !stepped_up
        && movement_progress_was_blocked(supported_horizontal_move, movement.translation)
        || climb_rise_blocked;

    let grounded = resolved_ground.is_some();
    // `movement.grounded` covers support the center-line probe can't see
    // (resting on an edge sliver). The perch slide makes that state
    // transient, but a blocked slide (doorway lip, inside corner) can
    // persist — without this, gravity would pump fall velocity for seconds
    // while the body never moves.
    if (grounded || movement.grounded) && vertical_velocity < 0.0
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
    position_has_floor_support(collision_world, pos, physics)
}

// Public predicate for "is there floor under `pos` within the character's
// normal support reach." Same probe `is_character_grounded` uses internally;
// exposed so callers (e.g. actor patrol ledge avoidance) can ask the
// question for a hypothetical position without reimplementing the probe.
#[must_use]
pub fn position_has_floor_support(
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
) -> bool {
    let shape = character_support_probe_shape(physics);
    character_ground_hit(collision_world, &shape, pos, &[], physics).is_some()
}

fn character_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    passable_kinds: &[crate::protocol::BarrierKindId],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    let pose = character_support_probe_pose(pos, physics);
    collision_world.ground_hit(
        shape,
        &pose,
        CHARACTER_GROUND_SNAP_DISTANCE + physics.collider.bottom_y_offset(),
        0.0,
        passable_kinds,
    )
}

// Full-footprint ground contact within a step's height of rest — the support
// the KCC can stand on even when the center-line probe misses. Deliberately
// shorter reach than the probe: the slide must only engage once the collider
// is (about to be) resting, not nudge a descending jump mid-air.
fn character_perch_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    passable_kinds: &[crate::protocol::BarrierKindId],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    let pose = character_pose(pos, physics);
    collision_world.ground_hit(
        shape,
        &pose,
        physics.collider.bottom_y_offset() + CHARACTER_STEP_HEIGHT,
        0.0,
        passable_kinds,
    )
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
