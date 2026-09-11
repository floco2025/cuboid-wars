use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterCollision, CharacterLength, KinematicCharacterController},
    parry::shape::Capsule,
    prelude::{ColliderHandle, Vector},
};

use super::{
    geometry::{character_movement_pose, character_movement_shape},
    ladder::{LadderMode, evaluate_ladder_interaction},
    support::{
        RiderCarry, character_ground_hit, grounding_diagnostics, position_has_floor_support, rider_carry,
        snap_character_to_ground,
    },
    types::{CharacterMovementResult, CharacterSupport},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        CHARACTER_CONTACT_OFFSET, CHARACTER_MAX_SLOPE, CHARACTER_STEP_HEIGHT, CHARACTER_STEP_MIN_WIDTH,
        CHARACTER_TERMINAL_VELOCITY,
    },
    map::Carriers,
    math::from_rapier,
    physics::{PortalSet, world::CollisionWorld},
    protocol::{BarrierId, CarrierId, Position},
};

const CHARACTER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
// Rapier's own zero-length threshold for a character move.
const CHARACTER_RESTING_MOVEMENT: f32 = 1e-5;

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
// knockback and portal momentum ride `external_displacement` so they can move
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
    pub passable_kinds: &'a [BarrierId],
    pub physics: CharacterPhysicsConfig,
    pub ladder_climb_ratio: f32,
    pub ladder_mode: LadderMode,
    // Portal pass-through: while the body overlaps a linked aperture, its
    // backing colliders are excluded from this step's collision and support
    // queries. `None` for characters that cannot use portals (actors).
    pub portals: Option<&'a PortalSet>,
    // The carriers at this tick's pose, already applied to
    // `collision_world`; a body standing on one rides with it.
    pub carriers: &'a Carriers,
}

#[must_use]
pub fn step_character_movement(step: CharacterStep, env: &CharacterEnvironment) -> CharacterMovementResult {
    let shape = character_movement_shape(env.physics);
    let RiderCarry {
        carrier,
        displacement: carry,
        floor_velocity,
    } = rider_carry(&step, env, &shape);
    // The carrier's colliders already sit at this tick's pose, and a probe
    // that starts inside a collider finds no ground, so the body follows the
    // carrier's rise or drop before anything probes. The horizontal part
    // rides the move instead, so a wall still blocks a body the carrier
    // pushes into it.
    let mut step = step;
    step.start.y += carry.y;
    let support_excluded = env.portals.map_or_else(Vec::new, |portals| {
        portals.collision_exclusions(Vec3::from(step.start), env.physics)
    });
    let request = prepare_movement_request(step, env, carry, &support_excluded, &shape);
    let movement_excluded = env.portals.map_or_else(Vec::new, |portals| {
        portals.movement_collision_exclusions(
            Vec3::from(step.start),
            from_rapier(request.requested_total),
            env.physics,
        )
    });
    let collision = resolve_character_collision(step, env, &movement_excluded, &shape, &request);
    finish_character_movement(
        step,
        env,
        &movement_excluded,
        request,
        collision,
        carrier,
        floor_velocity,
    )
}

struct MovementRequest {
    next_vertical_velocity: f32,
    requested_horizontal: Vector,
    requested_vertical: Vector,
    requested_total: Vector,
    carried: Vector,
    can_follow_ground: bool,
    started_grounded: bool,
    ascending_ladder: bool,
    ladder_supported: bool,
    // A carrier moved the body vertically before the request.
    lifted: bool,
}

fn prepare_movement_request(
    step: CharacterStep,
    env: &CharacterEnvironment,
    carry: Vec3,
    excluded_colliders: &[ColliderHandle],
    shape: &Capsule,
) -> MovementRequest {
    let carry_xz = carry.with_y(0.0);
    let start_pos = &step.start;
    let collision_world = env.collision_world;
    let passable_kinds = env.passable_kinds;
    let physics = env.physics;

    let ground_probe = if step.vertical_velocity <= 0.0 {
        character_ground_hit(
            collision_world,
            shape,
            start_pos,
            passable_kinds,
            excluded_colliders,
            physics,
        )
    } else {
        None
    };
    let ladder_pos = Position {
        x: start_pos.x + carry_xz.x,
        z: start_pos.z + carry_xz.z,
        ..*start_pos
    };
    let ladder = evaluate_ladder_interaction(
        collision_world
            .ladder_volume_at(&ladder_pos)
            .filter(|_| env.ladder_mode != LadderMode::Disabled),
        env.ladder_mode,
        &ladder_pos,
        step.vertical_velocity,
        step.control_velocity,
        step.delta,
        ground_probe.is_some(),
        env.ladder_climb_ratio,
    );
    let ascending_ladder = ladder.is_ascending();
    // Climbing suppresses ground following: without this, the ground snap
    // below would glue the first climb tick back onto the base floor.
    let can_follow_ground = step.vertical_velocity <= 0.0
        && !ascending_ladder
        && !(matches!(env.ladder_mode, LadderMode::Climb | LadderMode::Exit) && ladder.is_supported());
    let next_vertical_velocity = if let Some(vertical_velocity) = ladder.vertical_velocity() {
        vertical_velocity
    } else if ground_probe.is_some() {
        // Ground support balances gravity; repeatedly casting into it amplifies capsule contact noise.
        0.0
    } else {
        (step.vertical_velocity - env.gravity * step.delta).max(-CHARACTER_TERMINAL_VELOCITY)
    };

    let portal_funnel = env.portals.map_or(Vec3::ZERO, |portals| {
        portals.funnel_displacement(
            Vec3::from(*start_pos),
            physics,
            step.control_velocity,
            step.vertical_velocity,
            step.delta,
        )
    });
    // Actor mount waypoints align them; pulling adjacent climbers together can stop both moves.
    let ladder_funnel = if env.ladder_mode == LadderMode::Automatic {
        ladder.funnel_displacement(&ladder_pos, step.delta)
    } else {
        Vec3::ZERO
    };
    let target_x = step.control_velocity.x.mul_add(step.delta, start_pos.x)
        + step.external_displacement.x
        + carry_xz.x
        + portal_funnel.x
        + ladder_funnel.x;
    let target_z = step.control_velocity.z.mul_add(step.delta, start_pos.z)
        + step.external_displacement.z
        + carry_xz.z
        + portal_funnel.z
        + ladder_funnel.z;
    let (target_x, target_z) = if matches!(env.ladder_mode, LadderMode::Automatic | LadderMode::Climb) {
        ladder.constrain_target(&ladder_pos, target_x, target_z, collision_world, physics)
    } else {
        (target_x, target_z)
    };
    let requested_target = Position {
        x: target_x,
        y: next_vertical_velocity.mul_add(step.delta, start_pos.y),
        z: target_z,
    };
    let requested_horizontal_move =
        Vector::new(requested_target.x - start_pos.x, 0.0, requested_target.z - start_pos.z);
    let requested_vertical_move = Vector::new(0.0, requested_target.y - start_pos.y, 0.0);
    let carried = Vector::new(carry_xz.x, 0.0, carry_xz.z);

    MovementRequest {
        next_vertical_velocity,
        requested_horizontal: requested_horizontal_move,
        requested_vertical: requested_vertical_move,
        requested_total: requested_horizontal_move + requested_vertical_move,
        carried,
        can_follow_ground,
        started_grounded: ground_probe.is_some(),
        ascending_ladder,
        ladder_supported: ladder.is_supported(),
        lifted: carry.y != 0.0,
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
    shape: &Capsule,
    request: &MovementRequest,
) -> CharacterCollisionResult {
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let controller = character_controller();
    let pose = character_movement_pose(&step.start, env.physics);
    let mut observe = |collision: CharacterCollision| {
        let normal = from_rapier(collision.hit.normal1);
        let is_side_contact = normal.y.abs() <= 0.5;
        let is_ceiling = normal.y < -0.5 && request.requested_vertical.y > 0.0;
        if is_side_contact {
            saw_side_contact = true;
        }
        if is_ceiling {
            hit_ceiling = true;
        }
    };
    let mut carried = if request.carried == Vector::ZERO {
        Vector::ZERO
    } else {
        env.collision_world
            .move_character(
                step.delta,
                &controller,
                shape,
                &pose,
                request.carried,
                env.passable_kinds,
                excluded_colliders,
                &mut observe,
            )
            .translation
    };
    let mut motion_start = pose;
    motion_start.translation += carried;
    if !env.carriers.is_static() {
        let push = env.collision_world.push_character_from_carriers(
            step.delta,
            &controller,
            shape,
            &motion_start,
            env.carriers,
            env.passable_kinds,
            excluded_colliders,
            &mut observe,
        );
        carried += push;
        motion_start.translation += push;
    }
    // Resolve incoming geometry first so control input cannot cancel the push while still inside it.
    let requested = request.requested_total - request.carried;
    // A body asking for no move stays where the carrier push left it: the
    // controller would otherwise push it out of every overlap, undoing the
    // push and hiding a crush. Its support comes from the ground probe.
    let movement = (requested.length() > CHARACTER_RESTING_MOVEMENT).then(|| {
        env.collision_world.move_character(
            step.delta,
            &controller,
            shape,
            &motion_start,
            requested,
            env.passable_kinds,
            excluded_colliders,
            observe,
        )
    });

    CharacterCollisionResult {
        translation: carried + movement.as_ref().map_or(Vector::ZERO, |movement| movement.translation),
        grounded: movement.is_some_and(|movement| movement.grounded),
        saw_side_contact,
        hit_ceiling,
    }
}

fn finish_character_movement(
    step: CharacterStep,
    env: &CharacterEnvironment,
    excluded_colliders: &[ColliderHandle],
    request: MovementRequest,
    collision: CharacterCollisionResult,
    carrier: CarrierId,
    floor_velocity: Vec3,
) -> CharacterMovementResult {
    let mut resolved = Position {
        x: step.start.x + collision.translation.x,
        y: step.start.y + collision.translation.y,
        z: step.start.z + collision.translation.z,
    };
    if request.can_follow_ground && request.started_grounded {
        snap_character_to_ground(
            env.collision_world,
            &mut resolved,
            env.physics,
            env.passable_kinds,
            excluded_colliders,
        );
    }
    let grounding = grounding_diagnostics(
        env.collision_world,
        &resolved,
        env.physics,
        env.passable_kinds,
        excluded_colliders,
    );
    let resolved_ground = grounding
        .hit
        .filter(|_| request.can_follow_ground && grounding.supported);
    let mut vertical_velocity = request.next_vertical_velocity;
    let side_movement_blocked = collision.saw_side_contact
        && horizontal_shortfall(request.requested_horizontal, collision.translation)
            > CHARACTER_BLOCKED_MOVEMENT_EPSILON;
    // A climb whose rise was cut short hit something overhead (e.g. riding
    // the wrong side of a ladder into the floor above) — surface it as
    // blocked. Scoped to climbing: ordinary jumps against ceilings stay
    // silent.
    let climb_rise_blocked = request.ascending_ladder
        && request.requested_vertical.y > 0.0
        && collision.translation.y < request.requested_vertical.y - CHARACTER_BLOCKED_MOVEMENT_EPSILON;
    let blocked = side_movement_blocked || climb_rise_blocked;

    let grounded = resolved_ground.is_some() || collision.grounded;
    let landed_while_falling = grounded && vertical_velocity < 0.0;
    // Only a contact from above ends a rise. A shortfall in the achieved
    // rise cannot: sliding up a wall loses rise in proportion to the push
    // speed and the slant of a corner contact, which reads as a ceiling at
    // run speed.
    let hit_ceiling_while_rising = collision.hit_ceiling && vertical_velocity > 0.0;
    if landed_while_falling || hit_ceiling_while_rising {
        vertical_velocity = 0.0;
    }

    let support = if request.ladder_supported && env.collision_world.ladder_volume_at(&resolved).is_some() {
        CharacterSupport::Ladder
    } else if grounded {
        CharacterSupport::Ground
    } else {
        CharacterSupport::Airborne
    };
    // Ground probes can zero velocity before the cast; preserve that incoming impact too.
    let impact_speed = if support == CharacterSupport::Ground {
        (-step.vertical_velocity.min(request.next_vertical_velocity)).max(0.0)
    } else {
        0.0
    };
    // Leaving a tile keeps its rise or drop: a jump off a rising lift goes
    // higher, the way it does off a real one.
    if support == CharacterSupport::Airborne {
        vertical_velocity += floor_velocity.y;
    }
    // A carrier moving into a body the collision could not push clear
    // (a lift descending onto a body on the floor) leaves the body inside
    // the carrier's collider after the step, and one lifting a body into a
    // ceiling leaves the ceiling inside the body. The controller would
    // otherwise let the body through next tick.
    let crushed = !env.carriers.is_static()
        && env.collision_world.character_crushed(
            &resolved,
            env.physics,
            env.passable_kinds,
            excluded_colliders,
            request.lifted,
        );

    CharacterMovementResult {
        grounding,
        position: resolved,
        vertical_velocity,
        impact_speed,
        support,
        blocked,
        carrier,
        floor_velocity,
        lifted: request.lifted,
        crushed,
    }
}

// How far short of the requested horizontal move the body fell, measured
// along the requested direction; sliding sideways off a wall counts as
// lost progress, moving further than asked does not.
fn horizontal_shortfall(desired: Vector, actual: Vector) -> f32 {
    let desired_xz = Vec3::new(desired.x, 0.0, desired.z);
    let desired_len = desired_xz.length();
    if desired_len <= CHARACTER_BLOCKED_MOVEMENT_EPSILON {
        return 0.0;
    }

    let actual_xz = Vec3::new(actual.x, 0.0, actual.z);
    let actual_along_desired = actual_xz.dot(desired_xz / desired_len);
    (desired_len - actual_along_desired).max(0.0)
}

fn character_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(CHARACTER_CONTACT_OFFSET),
        autostep: Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(CHARACTER_STEP_HEIGHT),
            min_width: CharacterLength::Absolute(CHARACTER_STEP_MIN_WIDTH),
            include_dynamic_bodies: false,
        }),
        max_slope_climb_angle: CHARACTER_MAX_SLOPE,
        min_slope_slide_angle: CHARACTER_MAX_SLOPE,
        // The motor follows the ground itself (`snap_character_to_ground`,
        // which casts from above the skin); rapier's snap casts from the
        // touching capsule, where the cast can stagnate and sink the body.
        snap_to_ground: None,
        ..KinematicCharacterController::default()
    }
}
