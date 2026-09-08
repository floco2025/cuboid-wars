use bevy_math::Vec3;
use rapier3d::{parry::shape::Capsule, prelude::ColliderHandle};

use super::GroundingDiagnostics;
use super::geometry::{character_movement_pose, character_movement_shape};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_CONTACT_OFFSET, CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_MAX_SLOPE},
    map::Carriers,
    physics::world::{CollisionWorld, ShapeCastHit},
    protocol::{BarrierKindId, CarrierId, Position},
};

// Ground snap leaves the feet slightly above the surface they ride.
pub const CARRIER_RIDE_TOLERANCE: f32 = 0.05;
// Coincident static and carried surfaces must tolerate shape-cast depth noise.
const CARRIER_SURFACE_TIE_EPSILON: f32 = 0.01;

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

// Move the probe with its carrier to query support in the carrier's previous frame.
// Takeoff still receives carry; a coincident static floor does not interrupt the ride.
pub(super) fn supporting_carrier(
    collision_world: &CollisionWorld,
    shape: &Capsule,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    physics: CharacterPhysicsConfig,
    carriers: &Carriers,
) -> Option<CarrierId> {
    let bottom = CHARACTER_CONTACT_OFFSET;
    let (carrier, current_distance) = (0..carriers.carried_count())
        .filter_map(|index| {
            let carrier = CarrierId(index as u16 + 1);
            let travel = carriers.displacement(carrier);
            let carried_pos = Position::from(Vec3::from(*pos) + travel);
            let mut pose = character_movement_pose(&carried_pos, physics);
            pose.translation.y += CHARACTER_CONTACT_OFFSET * 2.0;
            let mut hit = collision_world.ground_hit_on_carrier(
                shape,
                &pose,
                bottom + CARRIER_RIDE_TOLERANCE + CHARACTER_CONTACT_OFFSET * 2.0,
                passable_kinds,
                carrier,
            )?;
            hit.t -= CHARACTER_CONTACT_OFFSET * 2.0;
            ((hit.t - bottom).abs() <= CARRIER_RIDE_TOLERANCE && hit.normal.y >= CHARACTER_MAX_SLOPE.cos())
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
        .is_some_and(|hit| hit.t + CARRIER_SURFACE_TIE_EPSILON < carried_distance);
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
