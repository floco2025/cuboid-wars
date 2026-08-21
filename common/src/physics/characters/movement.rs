use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::shape::Cuboid,
    prelude::Vector,
};
use std::f32::consts::FRAC_PI_3;

use super::{
    geometry::{character_pose, character_shape, character_support_probe_pose, character_support_probe_shape},
    types::CharacterMovementResult,
};
use crate::{
    config::{CharacterPhysicsConfig, LaddersConfig},
    constants::{
        CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_PERCH_SLIDE_SPEED, CHARACTER_STEP_HEIGHT, CHARACTER_STEP_MIN_WIDTH,
        CHARACTER_TERMINAL_VELOCITY, LADDER_CLIMB_FACING_FRACTION, LADDER_CLIMB_MIN_SPEED, LADDER_STANDOFF_CLEARANCE,
        PHYSICS_EPSILON,
    },
    physics::world::{CollisionWorld, LadderVolume, ShapeCastHit},
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
    // must work regardless of the climb's vertical velocity. Front side only
    // — the back of a ladder is not a ladder.
    let on_ladder = collision_world
        .ladder_volume_at(&ground_probe_pos)
        .is_some_and(|ladder| ladder.offset_from_plane(x, z) > 0.0);
    if !on_ladder && (*vertical_velocity > 0.0 || !is_character_grounded(collision_world, &ground_probe_pos, physics)) {
        return false;
    }

    *vertical_velocity = jump_speed;
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
//
// `gravity` is the per-map acceleration magnitude, already resolved by the
// caller (`MapSettings::gravity_for` picks the low-gravity value when the
// power-up is active).
// One frame of requested motion for one character. X/Z come from intent as
// a horizontal target; vertical motion is derived inside the step.
#[derive(Debug, Clone, Copy)]
pub struct CharacterStep {
    pub start: Position,
    pub vertical_velocity: f32,
    pub target_x: f32,
    pub target_z: f32,
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
        target_x,
        target_z,
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
    // Ladders are one-sided: only the FRONT (the rail side, positive plane
    // offset) is a ladder. From the back it is nothing at all — no ride, no
    // fence, no latch — so a back-side walker passes through the plane and
    // emerges on the front face, where the usual rules take over. That
    // one-way membrane is what makes mounting mid-ladder from a balcony
    // behind the ladder work: walk off through it and you're hanging on the
    // front, already descending if you keep pushing.
    let ladder = collision_world
        .ladder_volume_at(start_pos)
        .filter(|ladder| ladder.offset_from_plane(start_pos.x, start_pos.z) > 0.0);
    // The ladder converts front-side intent along its plane normal into
    // vertical motion, per tick with no persistent state: pushing toward
    // the plane ascends, pushing away descends, both at the intent speed ×
    // `climb_speed_ratio` (walk, run, and actor speeds all carry through).
    // Two gates keep it deliberate: the move must point mostly along the
    // normal, at real speed — so a grazing walk past the ladder or a
    // reconciliation micro-nudge never triggers. Whatever hangs over the
    // front is an ordinary collision; the ladder never inspects the
    // surrounding geometry. Derived from position + intent alone, so server
    // and client prediction agree without any wire state.
    let ladder_ride_velocity = ladder.and_then(|ladder| {
        // Standing ON the top landing walks normally; the ride still runs
        // above the landing while airborne so a crest can finish.
        if ground_probe.is_some() && start_pos.y >= ladder.top_landing_y() - PHYSICS_EPSILON {
            return None;
        }
        let move_x = target_x - start_pos.x;
        let move_z = target_z - start_pos.z;
        let toward_plane = -(move_x * ladder.normal_x + move_z * ladder.normal_z);
        let aligned = toward_plane.abs() >= move_x.hypot(move_z) * LADDER_CLIMB_FACING_FRACTION;
        let speed = toward_plane / delta;
        (aligned && speed.abs() >= LADDER_CLIMB_MIN_SPEED).then_some(speed * env.ladders.climb_speed_ratio)
    });
    let climb_velocity = ladder_ride_velocity.filter(|&v| v > 0.0);
    let climbing = climb_velocity.is_some();
    // Descending only while not moving upward, so a jump off the ladder
    // keeps its arc.
    let descend_velocity = ladder_ride_velocity.filter(|&v| v < 0.0 && start_vertical_velocity <= 0.0);
    // Climbing suppresses ground following: without this, the ground snap
    // below would glue the first climb tick back onto the base floor.
    let can_follow_ground = start_vertical_velocity <= 0.0 && !climbing;
    let current_ground = if can_follow_ground { ground_probe } else { None };
    let mut next_vertical_velocity = start_vertical_velocity;
    let mut ladder_descend_hold = None;
    if let Some(climb_velocity) = climb_velocity {
        next_vertical_velocity = climb_velocity;
    } else if current_ground.is_some() {
        next_vertical_velocity = 0.0;
    } else if let Some(on_ladder) = ladder.filter(|_| start_vertical_velocity <= 0.0) {
        // On the ladder, airborne, not ascending: pressing away climbs down
        // (horizontal motion pinned to the hold line below so backing up
        // doesn't shear off the ladder); otherwise latch in place — standing
        // still on a ladder stays put, and a fall through the volume is
        // caught the same way. Jump arcs (vv > 0) fall through to gravity
        // so detaching works.
        if let Some(descend_velocity) = descend_velocity {
            // The ladder ends at its bottom: descending never carries the
            // feet below the volume, so a hanging ladder leaves you at the
            // last rung instead of dropping you. Jump to let go.
            next_vertical_velocity = descend_velocity.max((on_ladder.bottom_y() - start_pos.y) / delta);
            ladder_descend_hold = Some(on_ladder);
        } else {
            next_vertical_velocity = 0.0;
        }
    } else {
        // Apply gravity for this frame, but cap falling speed so large falls
        // remain stable and predictable (terminal velocity is unchanged by
        // per-map gravity — it's a velocity cap, not an acceleration cap).
        next_vertical_velocity -= gravity * delta;
        next_vertical_velocity = next_vertical_velocity.max(-CHARACTER_TERMINAL_VELOCITY);
    }

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

    let (target_x, target_z) = match ladder_descend_hold {
        Some(ladder) => ladder.with_plane_offset(target_x, target_z, ladder_hold_standoff(ladder, physics)),
        None => (target_x, target_z),
    };
    let (target_x, target_z) = clamp_move_at_ladder_plane(start_pos, target_x, target_z, collision_world, physics);
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
    // comparing input-only intent against actual movement — an idle player
    // perched against a wall must not hear bump feedback.
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

// Where the plane clamp holds this character's center: leading face (half
// extent along the plane normal — the collider is wider than it is deep)
// plus a small air gap to the rails.
fn ladder_hold_standoff(ladder: &LadderVolume, physics: CharacterPhysicsConfig) -> f32 {
    let half_extent_toward_plane = if ladder.normal_x != 0.0 {
        physics.collider.width / 2.0
    } else {
        physics.collider.depth / 2.0
    };
    half_extent_toward_plane + LADDER_STANDOFF_CLEARANCE
}

// The ladder's rail plane is a fence from the ladder's base up to its top
// landing, for FRONT-side characters only: below that height, their
// horizontal motion may not cross the plane or bring the leading face
// closer than the hold stand-off. A back-side character is not on a ladder
// at all and walks through freely (that crossing is the mid-ladder mount).
// At or above the top landing the plane is open for everyone (the band
// ends there), which is what lets a climb crest over the top. Adjusting
// the target before the sweep keeps the `blocked` bump feedback quiet.
fn clamp_move_at_ladder_plane(
    start: &Position,
    target_x: f32,
    target_z: f32,
    collision_world: &CollisionWorld,
    physics: CharacterPhysicsConfig,
) -> (f32, f32) {
    let Some(ladder) = collision_world.ladder_band_at(target_x, target_z, start.y) else {
        return (target_x, target_z);
    };
    if ladder.offset_from_plane(start.x, start.z) <= 0.0 {
        return (target_x, target_z);
    }
    let standoff = ladder_hold_standoff(ladder, physics);
    if ladder.offset_from_plane(target_x, target_z) >= standoff {
        return (target_x, target_z);
    }
    ladder.with_plane_offset(target_x, target_z, standoff)
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
