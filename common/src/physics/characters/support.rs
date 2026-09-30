use bevy_math::Vec3;
use rapier3d::{parry::shape::Capsule, prelude::ColliderHandle};

use super::{
    GroundingDiagnostics,
    geometry::{character_movement_pose, character_movement_shape},
    ladder::{LadderMode, evaluate_ladder_interaction},
    movement::{CharacterEnvironment, CharacterStep},
};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{
        CHARACTER_CARRIER_RIDE_TOLERANCE, CHARACTER_CARRIER_TIE_EPSILON, CHARACTER_CONTACT_OFFSET,
        CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE,
    },
    map::Carriers,
    physics::world::{CollisionWorld, ShapeCastHit},
    protocol::{CarrierId, FieldId, Position},
};

// The support probe's reach. It matches the carrier ride tolerance so a body
// still carried at a tick's start stood on the tile at the last tick's end and
// takes its velocity once; `rider_carry` says why the two must agree.
const GROUND_PROBE_DISTANCE: f32 = CHARACTER_CARRIER_RIDE_TOLERANCE + CHARACTER_CONTACT_OFFSET * 3.0;

#[must_use]
pub fn position_has_floor_support(
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
    passable_fields: &[FieldId],
) -> bool {
    let shape = character_movement_shape(physics);
    character_ground_hit(collision_world, &shape, pos, passable_fields, &[], physics).is_some()
}

pub(super) fn character_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_fields: &[FieldId],
    excluded_colliders: &[ColliderHandle],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    probe_character_ground(
        collision_world,
        shape,
        pos,
        passable_fields,
        excluded_colliders,
        physics,
        GROUND_PROBE_DISTANCE,
    )
    .filter(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
}

fn probe_character_ground(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_fields: &[FieldId],
    excluded_colliders: &[ColliderHandle],
    physics: CharacterPhysicsConfig,
    distance: f32,
) -> Option<ShapeCastHit> {
    let mut pose = character_movement_pose(pos, physics);
    pose.translation.y += CHARACTER_CONTACT_OFFSET * 2.0;
    // Cast to the surface and subtract the skin afterward to avoid near-contact distance noise.
    collision_world
        .ground_hit(shape, &pose, distance, 0.0, passable_fields, excluded_colliders)
        .map(|mut hit| {
            hit.t -= CHARACTER_CONTACT_OFFSET * 2.0 + CHARACTER_CONTACT_OFFSET / hit.normal.y;
            hit
        })
}

#[must_use]
pub fn grounding_diagnostics(
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
    passable_fields: &[FieldId],
    excluded_colliders: &[ColliderHandle],
) -> GroundingDiagnostics {
    let hit = probe_character_ground(
        collision_world,
        &character_movement_shape(physics),
        pos,
        passable_fields,
        excluded_colliders,
        physics,
        GROUND_PROBE_DISTANCE,
    );
    GroundingDiagnostics {
        origin: Vec3::from(*pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET * 2.0,
        distance: GROUND_PROBE_DISTANCE,
        supported: hit.is_some_and(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos()),
        hit,
    }
}

// The rider's carry: how far the body follows a carrier this tick, and
// the velocity the step reports for it.
pub(super) struct RiderCarry {
    pub carrier: CarrierId,
    pub displacement: Vec3,
    pub floor_velocity: Vec3,
}

// A body standing on a carrier follows it by the ride rule
// (`supporting_carrier`); a body on a carried ladder follows the ladder's
// carrier. In the corridor of a carried aperture the ride rule applies
// only to a body that ended the last tick standing: on that carrier in
// front of its wall portal, or on top of a floor portal it sinks into
// this tick, once the motor excludes the backing. The sink is a departure
// like any other and takes the carrier's motion into the body's own
// velocity, so a body already in the air here is on its own: its velocity
// keeps it with a sliding aperture and nothing is added twice. The probe
// alone cannot tell a stand from a body falling into the hole or rising
// out of it, which pass within its reach too; a stand has its feet in
// front of the plane and no vertical velocity, since a landing zeroes it
// (`finish_character_movement`) and standing never adds any. A jump on the
// very tick a body would sink leaves without the ride. The gate's carrier
// is the frame an unsupported body's position is reported in.
pub(super) fn rider_carry(step: &CharacterStep, env: &CharacterEnvironment, shape: &Capsule) -> RiderCarry {
    let transit = env
        .portals
        .and_then(|portals| portals.transit(Vec3::from(step.start), env.physics));
    let carrier = match transit {
        Some(transit) => {
            let standing = (step.vertical_velocity == 0.0 && transit.feet_distance > -CHARACTER_CONTACT_OFFSET)
                .then(|| {
                    supporting_carrier(
                        env.collision_world,
                        shape,
                        &step.start,
                        env.passable_fields,
                        env.physics,
                        env.carriers,
                    )
                })
                .flatten();
            match standing {
                Some(supporting) => Some(supporting),
                None => {
                    return RiderCarry {
                        carrier: transit.carrier,
                        displacement: Vec3::ZERO,
                        floor_velocity: Vec3::ZERO,
                    };
                }
            }
        }
        None if env.carriers.is_static() => None,
        None => env
            .collision_world
            .carried_ladder_at_previous_pose(&step.start, env.carriers)
            .filter(|_| env.ladder_mode != LadderMode::Disabled)
            .and_then(|(carrier, ladder)| {
                let grounded = step.vertical_velocity <= 0.0
                    && character_ground_hit(
                        env.collision_world,
                        shape,
                        &step.start,
                        env.passable_fields,
                        &[],
                        env.physics,
                    )
                    .is_some();
                evaluate_ladder_interaction(
                    Some(&ladder),
                    env.ladder_mode,
                    &step.start,
                    step.vertical_velocity,
                    step.intent_velocity,
                    step.delta,
                    grounded,
                )
                .is_supported()
                .then_some(carrier)
            })
            .or_else(|| {
                supporting_carrier(
                    env.collision_world,
                    shape,
                    &step.start,
                    env.passable_fields,
                    env.physics,
                    env.carriers,
                )
            }),
    };
    let displacement = carrier.map_or(Vec3::ZERO, |carrier| env.carriers.displacement(carrier));
    RiderCarry {
        carrier: carrier.unwrap_or(CarrierId::WORLD),
        displacement,
        floor_velocity: if step.delta <= 0.0 {
            Vec3::ZERO
        } else {
            displacement / step.delta
        },
    }
}

// The ride rule: the body rides the nearest carrier whose surface is within
// `CHARACTER_CARRIER_RIDE_TOLERANCE` under its feet, probed in that carrier's previous
// frame because the body has not received this tick's carry yet. Vertical
// velocity is ignored so a takeoff tick still receives the carry; the
// movement support probe reaches the same height as the tolerance, so
// a body still carried at a tick's start stood on the tile at the last
// tick's end and takes its velocity once. A world surface above the lifted
// probe is what the body stands on and ends the ride; a coincident static
// floor (a tile sliding through it) does not interrupt it.
fn supporting_carrier(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_fields: &[FieldId],
    physics: CharacterPhysicsConfig,
    carriers: &Carriers,
) -> Option<CarrierId> {
    let bottom = CHARACTER_CONTACT_OFFSET;
    let (carrier, current_distance) = carriers
        .carried_ids()
        .filter_map(|carrier| {
            let travel = carriers.displacement(carrier);
            let carried_pos = Position::from(Vec3::from(*pos) + travel);
            let mut pose = character_movement_pose(&carried_pos, physics);
            pose.translation.y += CHARACTER_CONTACT_OFFSET * 2.0;
            let mut hit = collision_world.ground_hit_on_carrier(
                shape,
                &pose,
                bottom + CHARACTER_CARRIER_RIDE_TOLERANCE + CHARACTER_CONTACT_OFFSET * 2.0,
                passable_fields,
                carrier,
            )?;
            hit.t -= CHARACTER_CONTACT_OFFSET * 2.0;
            ((hit.t - bottom).abs() <= CHARACTER_CARRIER_RIDE_TOLERANCE && hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
                .then_some((carrier, hit.t - travel.y))
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))?;
    let rise = carriers.displacement(carrier).y.max(0.0);
    let lifted = Position {
        y: pos.y + rise,
        ..*pos
    };
    let pose = character_movement_pose(&lifted, physics);
    let carried_distance = current_distance + rise;
    let world_above = collision_world
        .ground_hit_on_carrier(shape, &pose, carried_distance, passable_fields, CarrierId::WORLD)
        .is_some_and(|hit| hit.t + CHARACTER_CARRIER_TIE_EPSILON < carried_distance);
    (!world_above).then_some(carrier)
}

pub(super) fn snap_character_to_ground(
    collision_world: &CollisionWorld,
    pos: &mut Position,
    physics: CharacterPhysicsConfig,
    passable_fields: &[FieldId],
    excluded_colliders: &[ColliderHandle],
) {
    if let Some(hit) = probe_character_ground(
        collision_world,
        &character_movement_shape(physics),
        pos,
        passable_fields,
        excluded_colliders,
        physics,
        CHARACTER_GROUND_SNAP_DISTANCE + CHARACTER_CONTACT_OFFSET * 3.0,
    )
    .filter(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
    {
        pos.y -= hit.t;
    }
}
