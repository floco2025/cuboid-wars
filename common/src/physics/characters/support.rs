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
        CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE, TICK_SECS,
    },
    map::Carriers,
    physics::world::{CollisionWorld, ShapeCastHit},
    protocol::{BarrierKindId, CarrierId, Position},
};

#[must_use]
pub fn position_has_floor_support(
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
) -> bool {
    let shape = character_movement_shape(physics);
    character_ground_hit(collision_world, &shape, pos, &[], &[], physics).is_some()
}

pub(super) fn character_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    excluded_colliders: &[ColliderHandle],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    probe_character_ground(
        collision_world,
        shape,
        pos,
        passable_kinds,
        excluded_colliders,
        physics,
        CHARACTER_CONTACT_OFFSET * 5.0,
    )
    .filter(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
}

fn probe_character_ground(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    excluded_colliders: &[ColliderHandle],
    physics: CharacterPhysicsConfig,
    distance: f32,
) -> Option<ShapeCastHit> {
    let mut pose = character_movement_pose(pos, physics);
    pose.translation.y += CHARACTER_CONTACT_OFFSET * 2.0;
    // Cast to the surface and subtract the skin afterward to avoid near-contact distance noise.
    collision_world
        .ground_hit(shape, &pose, distance, 0.0, passable_kinds, excluded_colliders)
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
    passable_kinds: &[BarrierKindId],
    excluded_colliders: &[ColliderHandle],
) -> GroundingDiagnostics {
    let hit = probe_character_ground(
        collision_world,
        &character_movement_shape(physics),
        pos,
        passable_kinds,
        excluded_colliders,
        physics,
        CHARACTER_CONTACT_OFFSET * 5.0,
    );
    GroundingDiagnostics {
        origin: Vec3::from(*pos) + Vec3::Y * CHARACTER_CONTACT_OFFSET * 2.0,
        distance: CHARACTER_CONTACT_OFFSET * 5.0,
        supported: hit.is_some_and(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos()),
        hit,
    }
}

// The rider's carry: how far the body follows a carrier this tick, and
// the velocity the step reports for it.
pub(super) struct RiderCarry {
    pub displacement: Vec3,
    pub floor_velocity: Vec3,
}

// A body standing on a carrier follows it by the ride rule
// (`supporting_carrier`); a body on a carried ladder follows the ladder's
// carrier. A body passing through an aperture mounted on a carrier follows
// that carrier instead, until it crosses: the ride rule lets go the tick
// the feet leave the surface, and a fast carrier would pull the aperture
// out from under a sinking body. In transit the carrier's velocity is not
// reported, so a rising floor does not pump the fall and the slide is not
// gathered as momentum; the body exits the pair with carrier-relative
// velocity. A body in the corridor that another carrier supports (standing
// under a lift's ceiling portal) stays put, and the plane reaching it is
// what the relative crossing test catches; the portal's own carrier
// supporting it (its floor in front of its wall portal) is still the ride.
pub(super) fn rider_carry(step: &CharacterStep, env: &CharacterEnvironment, shape: &Capsule) -> RiderCarry {
    let transit = env
        .portals
        .and_then(|portals| portals.transit_carrier(Vec3::from(step.start), env.physics));
    let displacement = match transit {
        Some((carrier, backing)) => {
            let supported_elsewhere = character_ground_hit(
                env.collision_world,
                shape,
                &step.start,
                env.passable_kinds,
                backing,
                env.physics,
            )
            .is_some_and(|hit| hit.carrier != carrier);
            if supported_elsewhere {
                Vec3::ZERO
            } else {
                env.carriers.displacement(carrier)
            }
        }
        None if env.carriers.is_static() => Vec3::ZERO,
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
                        env.passable_kinds,
                        &[],
                        env.physics,
                    )
                    .is_some();
                evaluate_ladder_interaction(
                    Some(&ladder),
                    env.ladder_mode,
                    &step.start,
                    step.vertical_velocity,
                    step.control_velocity,
                    step.delta,
                    grounded,
                    env.ladder_climb_ratio,
                )
                .is_supported()
                .then_some(carrier)
            })
            .or_else(|| {
                supporting_carrier(
                    env.collision_world,
                    shape,
                    &step.start,
                    env.passable_kinds,
                    env.physics,
                    env.carriers,
                )
            })
            .map_or(Vec3::ZERO, |carrier| env.carriers.displacement(carrier)),
    };
    RiderCarry {
        displacement,
        floor_velocity: if transit.is_some() {
            Vec3::ZERO
        } else {
            displacement / TICK_SECS
        },
    }
}

// The ride rule: the body rides the nearest carrier whose surface is within
// `CHARACTER_CARRIER_RIDE_TOLERANCE` under its feet, probed in that carrier's previous
// frame because the body has not received this tick's carry yet. Vertical
// velocity is ignored so a takeoff tick still receives the carry; the
// controller's grounded reach ends at the same height as the tolerance, so
// a body still carried at a tick's start stood on the tile at the last
// tick's end and takes its velocity once. A world surface above the lifted
// probe is what the body stands on and ends the ride; a coincident static
// floor (a tile sliding through it) does not interrupt it.
fn supporting_carrier(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
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
                passable_kinds,
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
        .ground_hit_on_carrier(shape, &pose, carried_distance, passable_kinds, CarrierId::WORLD)
        .is_some_and(|hit| hit.t + CHARACTER_CARRIER_TIE_EPSILON < carried_distance);
    (!world_above).then_some(carrier)
}

pub(super) fn snap_character_to_ground(
    collision_world: &CollisionWorld,
    pos: &mut Position,
    physics: CharacterPhysicsConfig,
    passable_kinds: &[BarrierKindId],
    excluded_colliders: &[ColliderHandle],
) {
    if let Some(hit) = probe_character_ground(
        collision_world,
        &character_movement_shape(physics),
        pos,
        passable_kinds,
        excluded_colliders,
        physics,
        CHARACTER_GROUND_SNAP_DISTANCE + CHARACTER_CONTACT_OFFSET * 3.0,
    )
    .filter(|hit| hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
    {
        pos.y -= hit.t;
    }
}

// Accepted positions already include their carry; use the step's lift flag rather than probing again.
#[must_use]
pub fn character_crushed_at(pos: Position, env: &CharacterEnvironment, lifted: bool) -> bool {
    if env.carriers.is_static() {
        return false;
    }
    let excluded = env.portals.map_or_else(Vec::new, |portals| {
        portals.collision_exclusions(Vec3::from(pos), env.physics)
    });
    env.collision_world
        .character_crushed(&pos, env.physics, env.passable_kinds, &excluded, lifted)
}
