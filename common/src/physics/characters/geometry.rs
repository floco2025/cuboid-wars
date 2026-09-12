use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::{Capsule, Cuboid},
    },
    prelude::{Pose, Vector},
};

use crate::{config::CharacterPhysicsConfig, constants::CHARACTER_CONTACT_OFFSET, math::to_rapier, protocol::Position};

#[must_use]
pub fn character_movement_shape(physics: CharacterPhysicsConfig) -> Capsule {
    let body = physics.movement_collider;
    Capsule::new_y(body.segment_half_height(), body.radius())
}

#[must_use]
pub fn character_hitbox_shape(physics: CharacterPhysicsConfig) -> Cuboid {
    let hitbox = physics.hitbox;
    Cuboid::new(Vector::new(hitbox.width, hitbox.height, hitbox.depth) / 2.0)
}

// The skin lifts the shape while the shared position remains at the feet.
#[must_use]
pub fn character_movement_center(pos: Position, physics: CharacterPhysicsConfig) -> Vec3 {
    Vec3::new(
        pos.x,
        pos.y + physics.movement_collider.height / 2.0 + CHARACTER_CONTACT_OFFSET,
        pos.z,
    )
}

#[must_use]
pub fn character_hitbox_center(pos: Position, physics: CharacterPhysicsConfig) -> Vec3 {
    Vec3::new(pos.x, physics.hitbox_center_y(pos.y), pos.z)
}

pub fn character_movement_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    Pose::from_translation(to_rapier(character_movement_center(*pos, physics)))
}

#[must_use]
pub fn character_paths_intersect(
    start1: &Position,
    end1: &Position,
    physics1: CharacterPhysicsConfig,
    start2: &Position,
    end2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let shape1 = character_movement_shape(physics1);
    let shape2 = character_movement_shape(physics2);
    let velocity1 = to_rapier(Vec3::from(*end1) - Vec3::from(*start1));
    let velocity2 = to_rapier(Vec3::from(*end2) - Vec3::from(*start2));
    if character_positions_intersect(start1, physics1, start2, physics2) {
        return true;
    }

    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        // Near-touching bodies can report a zero-time hit even while separating.
        stop_at_penetration: false,
        ..ShapeCastOptions::default()
    };

    cast_shapes(
        &character_movement_pose(start1, physics1),
        velocity1,
        &shape1,
        &character_movement_pose(start2, physics2),
        velocity2,
        &shape2,
        options,
    )
    .is_ok_and(|hit| hit.is_some())
}

pub fn character_axis_separation(
    first: &Position,
    physics: CharacterPhysicsConfig,
    second: &Position,
    other_physics: CharacterPhysicsConfig,
) -> Vec3 {
    let body = physics.movement_collider;
    let other = other_physics.movement_collider;
    let bottom = first.y + body.radius();
    let top = first.y + body.height - body.radius();
    let other_bottom = second.y + other.radius();
    let other_top = second.y + other.height - other.radius();
    let mut separation = Vec3::from(*second) - Vec3::from(*first);
    // Upright capsules separate by their closest axis points, not by the distance between their feet.
    separation.y = (other_bottom - top).max(0.0) - (bottom - other_top).max(0.0);
    separation
}

pub fn character_positions_intersect(
    pos1: &Position,
    physics1: CharacterPhysicsConfig,
    pos2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let shape1 = character_movement_shape(physics1);
    let shape2 = character_movement_shape(physics2);
    intersection_test(
        &character_movement_pose(pos1, physics1),
        &shape1,
        &character_movement_pose(pos2, physics2),
        &shape2,
    )
    .is_ok_and(|overlaps| overlaps)
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
