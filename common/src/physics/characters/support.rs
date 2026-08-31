use bevy_math::Vec3;
use rapier3d::{
    parry::shape::Cuboid,
    prelude::{ColliderHandle, Vector},
};

use super::geometry::{character_pose, character_support_probe_pose, character_support_probe_shape};
use crate::{
    config::CharacterPhysicsConfig,
    constants::{CHARACTER_GROUND_SNAP_DISTANCE, CHARACTER_PERCH_SLIDE_SPEED, CHARACTER_STEP_HEIGHT, PHYSICS_EPSILON},
    physics::world::{CollisionWorld, ShapeCastHit},
    protocol::{BarrierKindId, Position},
};

#[must_use]
pub fn position_has_floor_support(
    collision_world: &CollisionWorld,
    pos: &Position,
    physics: CharacterPhysicsConfig,
) -> bool {
    let shape = character_support_probe_shape(physics);
    character_ground_hit(collision_world, &shape, pos, &[], &[], physics).is_some()
}

pub(super) fn character_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    excluded_colliders: &[ColliderHandle],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    let pose = character_support_probe_pose(pos, physics);
    collision_world.ground_hit(
        shape,
        &pose,
        CHARACTER_GROUND_SNAP_DISTANCE + physics.collider.bottom_y_offset(),
        0.0,
        passable_kinds,
        excluded_colliders,
    )
}

pub(super) fn snap_position_to_ground(pos: &mut Position, ground: ShapeCastHit, physics: CharacterPhysicsConfig) {
    pos.y -= ground.t - physics.collider.bottom_y_offset();
}

pub(super) fn perch_slide_displacement(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    physics: CharacterPhysicsConfig,
    delta: f32,
) -> Vector {
    // A full-footprint contact without center-line support is an unstable
    // edge perch. The contact witness can drift along the non-overhang axis,
    // but any outward component clears the edge; a degenerate direction can
    // safely skip the slide for one tick.
    character_perch_hit(collision_world, shape, pos, passable_kinds, physics)
        .and_then(|hit| Vec3::new(pos.x - hit.contact.x, 0.0, pos.z - hit.contact.z).try_normalize())
        .map_or(Vector::ZERO, |direction| {
            Vector::new(direction.x, 0.0, direction.z) * CHARACTER_PERCH_SLIDE_SPEED * delta
        })
}

fn character_perch_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    passable_kinds: &[BarrierKindId],
    physics: CharacterPhysicsConfig,
) -> Option<ShapeCastHit> {
    // Keep this reach shorter than the normal support probe so a descending
    // airborne character is not nudged sideways before reaching the edge.
    let pose = character_pose(pos, physics);
    collision_world.ground_hit(
        shape,
        &pose,
        physics.collider.bottom_y_offset() + CHARACTER_STEP_HEIGHT,
        0.0,
        passable_kinds,
        &[],
    )
}

pub(super) fn project_move_onto_support(input_move: Vector, support_normal: Vec3) -> Vector {
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
