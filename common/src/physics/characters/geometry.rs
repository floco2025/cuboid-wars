use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::details::distance_segment_segment,
        shape::{Capsule, Cuboid, Segment},
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
    // Upright capsules reduce to a relative path against one vertical capsule.
    // The generic convex cast can miss unequal capsules moving almost
    // horizontally; segment distance also catches crossings with clear ends.
    let first = physics1.movement_collider;
    let second = physics2.movement_collider;
    let center_offset = Vec3::Y * ((second.height - first.height) * 0.5);
    let relative = Segment::new(
        to_rapier(Vec3::from(*start2) - Vec3::from(*start1) + center_offset),
        to_rapier(Vec3::from(*end2) - Vec3::from(*end1) + center_offset),
    );
    let half_length = first.segment_half_height() + second.segment_half_height();
    let axis = Segment::new(-Vector::Y * half_length, Vector::Y * half_length);
    distance_segment_segment(&Pose::IDENTITY, &relative, &axis) <= first.radius() + second.radius()
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
    let radius = physics1.movement_collider.radius() + physics2.movement_collider.radius();
    character_axis_separation(pos1, physics1, pos2, physics2).length_squared() <= radius * radius
}

#[cfg(test)]
#[path = "tests/geometry.rs"]
mod tests;
