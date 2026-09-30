use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterCollision, CharacterLength, KinematicCharacterController},
    parry::shape::Capsule,
    prelude::{ColliderHandle, Vector},
};

use super::{
    geometry::{character_movement_pose, character_movement_shape},
    ladder::{LadderMode, evaluate_ladder_interaction},
    support::{character_ground_hit, grounding_diagnostics, rider_carry, snap_character_to_ground},
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
    physics::{
        PortalSet,
        world::{CollisionWorld, LadderVolume},
    },
    protocol::{CarrierId, FieldId, Position},
};

const CHARACTER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
// Rapier's own zero-length threshold for a character move.
const CHARACTER_RESTING_MOVEMENT: f32 = 1e-5;

// One fixed-tick request. Ladder decisions read only `intent_velocity`, so
// a shove or a launch can move the body without impersonating what it means
// to do; `displacement` is motion that is not a velocity at all.
#[derive(Debug, Clone, Copy)]
pub struct CharacterStep {
    pub start: Position,
    pub vertical_velocity: f32,
    pub intent_velocity: Vec3,
    pub velocity: Vec3,
    pub displacement: Vec3,
    pub delta: f32,
}

// The world the step happens in. `gravity` is the per-map acceleration
// magnitude, already resolved by the caller (`MapSettings::gravity_for`
// picks the low-gravity value when the power-up is active).
#[derive(Clone, Copy)]
pub struct CharacterEnvironment<'a> {
    pub collision_world: &'a CollisionWorld,
    pub gravity: f32,
    pub passable_fields: &'a [FieldId],
    pub physics: CharacterPhysicsConfig,
    pub ladder_mode: LadderMode,
    // Portal pass-through: while the body overlaps a linked aperture, its
    // backing colliders are excluded from this step's collision and support
    // queries. `None` for characters that cannot use portals (actors).
    pub portals: Option<&'a PortalSet>,
    // The carriers at this tick's pose, already applied to
    // `collision_world`; a body standing on one rides with it.
    pub carriers: &'a Carriers,
}

// What the motor learns about the body before it moves, probed once where
// the carrier ride puts it this tick: the carrier colliders already sit at
// this tick's pose, so a probe at the previous position would start inside
// a lift. The player policy reads the same answers, so the two never
// disagree about a stand.
pub struct CharacterStart<'a> {
    pub carrier: CarrierId,
    pub carry: Vec3,
    pub floor_velocity: Vec3,
    // The aperture backing the body may pass through this tick.
    pub exclusions: Vec<ColliderHandle>,
    pub grounded: bool,
    pub ladder: Option<&'a LadderVolume>,
}

impl<'a> CharacterStart<'a> {
    #[must_use]
    pub fn probe(step: &CharacterStep, env: &CharacterEnvironment<'a>) -> Self {
        let shape = character_movement_shape(env.physics);
        let carry = rider_carry(step, env, &shape);
        let carried = Position::from(Vec3::from(step.start) + carry.displacement);
        let exclusions = env.portals.map_or_else(Vec::new, |portals| {
            portals.collision_exclusions(carried.into(), env.physics)
        });
        let grounded = step.vertical_velocity <= 0.0
            && character_ground_hit(
                env.collision_world,
                &shape,
                &carried,
                env.passable_fields,
                &exclusions,
                env.physics,
            )
            .is_some();
        let ladder = env
            .collision_world
            .ladder_volume_at(&carried)
            .filter(|_| env.ladder_mode != LadderMode::Disabled);
        Self {
            carrier: carry.carrier,
            carry: carry.displacement,
            floor_velocity: carry.floor_velocity,
            exclusions,
            grounded,
            ladder,
        }
    }
}

#[must_use]
pub fn step_character_movement(step: CharacterStep, env: &CharacterEnvironment) -> CharacterMovementResult {
    step_character_movement_from(&CharacterStart::probe(&step, env), step, env)
}

// The step from a start already probed. The body follows the carrier's rise
// or drop before the move; the horizontal part rides the move instead, so a
// wall still blocks a body the carrier pushes into it.
#[must_use]
pub fn step_character_movement_from(
    start: &CharacterStart<'_>,
    step: CharacterStep,
    env: &CharacterEnvironment,
) -> CharacterMovementResult {
    let shape = character_movement_shape(env.physics);
    let mut step = step;
    step.start.y += start.carry.y;
    let request = prepare_movement_request(step, env, start);
    let movement_excluded = env.portals.map_or_else(Vec::new, |portals| {
        portals.movement_collision_exclusions(
            Vec3::from(step.start),
            from_rapier(request.requested_total),
            env.physics,
        )
    });
    let collision = resolve_character_collision(step, env, &movement_excluded, &shape, &request);
    finish_character_movement(step, env, &movement_excluded, request, collision, start)
}

// Where a body that does not walk this tick goes: its ride, its shove, and
// its fall, unresolved against the world. A stand-in for a body whose own
// move is not decided yet.
#[must_use]
pub fn character_passive_motion(step: &CharacterStep, env: &CharacterEnvironment) -> Vec3 {
    let carry = rider_carry(step, env, &character_movement_shape(env.physics)).displacement;
    carry + step.displacement + Vec3::Y * (step.vertical_velocity * step.delta)
}

struct MovementRequest {
    next_vertical_velocity: f32,
    requested_horizontal: Vector,
    requested_vertical: Vector,
    requested_total: Vector,
    carried: Vector,
    can_follow_ground: bool,
    ascending_ladder: bool,
    ladder_supported: bool,
    // A carrier moved the body vertically before the request.
    lifted: bool,
}

fn prepare_movement_request(
    step: CharacterStep,
    env: &CharacterEnvironment,
    start: &CharacterStart<'_>,
) -> MovementRequest {
    let carry_xz = start.carry.with_y(0.0);
    let start_pos = &step.start;
    let collision_world = env.collision_world;
    let physics = env.physics;

    let ladder_pos = Position {
        x: start_pos.x + carry_xz.x,
        z: start_pos.z + carry_xz.z,
        ..*start_pos
    };
    let ladder = evaluate_ladder_interaction(
        start.ladder,
        env.ladder_mode,
        &ladder_pos,
        step.vertical_velocity,
        step.intent_velocity,
        step.delta,
        start.grounded,
    );
    let ascending_ladder = ladder.is_ascending();
    // Climbing suppresses ground following: without this, the ground snap
    // below would glue the first climb tick back onto the base floor.
    let can_follow_ground = step.vertical_velocity <= 0.0
        && !ascending_ladder
        && !(matches!(env.ladder_mode, LadderMode::Climb | LadderMode::Exit) && ladder.is_supported());
    // Gravity is integrated in two half-steps, the second after the move in
    // `finish_character_movement`, so the move uses the tick's mean velocity.
    let next_vertical_velocity = if let Some(vertical_velocity) = ladder.vertical_velocity() {
        vertical_velocity
    } else if start.grounded {
        // Ground support balances gravity; repeatedly casting into it amplifies capsule contact noise.
        0.0
    } else {
        fall(step.vertical_velocity, env.gravity, step.delta)
    };

    // Actor mount waypoints align them; pulling adjacent climbers together can stop both moves.
    let ladder_funnel = if env.ladder_mode == LadderMode::Automatic {
        ladder.funnel_displacement(&ladder_pos, step.delta)
    } else {
        Vec3::ZERO
    };
    let travel_x = step.velocity.x.mul_add(step.delta, start_pos.x) + carry_xz.x + ladder_funnel.x;
    let travel_z = step.velocity.z.mul_add(step.delta, start_pos.z) + carry_xz.z + ladder_funnel.z;
    let displacement = step.displacement;
    let (target_x, target_z) = if !matches!(env.ladder_mode, LadderMode::Automatic | LadderMode::Climb) {
        (travel_x + displacement.x, travel_z + displacement.z)
    } else if ladder.is_supported() {
        // A shove moves a held body freely, so letting go or a blast can carry
        // it through the rungs; a walker's whole move stays fenced.
        let (x, z) = ladder.constrain_target(&ladder_pos, travel_x, travel_z, collision_world, physics);
        (x + displacement.x, z + displacement.z)
    } else {
        ladder.constrain_target(
            &ladder_pos,
            travel_x + displacement.x,
            travel_z + displacement.z,
            collision_world,
            physics,
        )
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
        ascending_ladder,
        ladder_supported: ladder.is_supported(),
        lifted: start.carry.y != 0.0,
    }
}

// Half a tick of gravity, held at the terminal fall speed.
fn fall(vertical_velocity: f32, gravity: f32, delta: f32) -> f32 {
    (vertical_velocity - gravity * delta * 0.5).max(-CHARACTER_TERMINAL_VELOCITY)
}

struct CharacterCollisionResult {
    translation: Vector,
    normals: [Vec3; 5],
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
    let mut normals = [Vec3::ZERO; 5];
    let mut count = 0;
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let controller = character_controller();
    let pose = character_movement_pose(&step.start, env.physics);
    let mut observe = |collision: CharacterCollision| {
        let normal = from_rapier(collision.hit.normal1);
        let is_side_contact = normal.y.abs() <= 0.5;
        if count < normals.len() && !normals[..count].iter().any(|n: &Vec3| n.dot(normal) > 0.999) {
            normals[count] = normal;
            count += 1;
        }
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
                env.passable_fields,
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
            env.passable_fields,
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
            env.passable_fields,
            excluded_colliders,
            observe,
        )
    });

    CharacterCollisionResult {
        translation: carried + movement.as_ref().map_or(Vector::ZERO, |movement| movement.translation),
        normals,
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
    start: &CharacterStart<'_>,
) -> CharacterMovementResult {
    let mut resolved = Position {
        x: step.start.x + collision.translation.x,
        y: step.start.y + collision.translation.y,
        z: step.start.z + collision.translation.z,
    };
    if request.can_follow_ground && start.grounded {
        snap_character_to_ground(
            env.collision_world,
            &mut resolved,
            env.physics,
            env.passable_fields,
            excluded_colliders,
        );
    }
    let grounding = grounding_diagnostics(
        env.collision_world,
        &resolved,
        env.physics,
        env.passable_fields,
        excluded_colliders,
    );
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

    // Rapier's grounded flag accepts near-vertical wall seams. Require a standable surface below the feet,
    // with the carrier ride tolerance so a slow takeoff cannot inherit platform velocity twice.
    let grounded = grounding.supported;
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
        vertical_velocity = fall(vertical_velocity + start.floor_velocity.y, env.gravity, step.delta);
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
            env.passable_fields,
            excluded_colliders,
            request.lifted,
        );

    CharacterMovementResult {
        contact_normals: collision.normals,
        grounding,
        position: resolved,
        vertical_velocity,
        impact_speed,
        support,
        blocked,
        carrier: start.carrier,
        floor_velocity: start.floor_velocity,
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
