use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Cuboid,
    },
    prelude::{Pose, Vector},
};

use crate::{config::CharacterPhysicsConfig, protocol::Position};

// The canonical character collision cuboid. Note the Y extent uses
// `collision_height()`, not `collider.height` — keep every hit/spawn/movement
// query going through this so the body box can't silently diverge between them.
#[must_use]
pub fn character_shape(physics: CharacterPhysicsConfig) -> Cuboid {
    Cuboid::new(Vector::new(
        physics.collider.width / 2.0,
        physics.collision_height() / 2.0,
        physics.collider.depth / 2.0,
    ))
}

#[must_use]
pub fn character_center(pos: Position, physics: CharacterPhysicsConfig) -> Vec3 {
    Vec3::new(pos.x, physics.collider_center_y(pos.y), pos.z)
}

#[must_use]
pub fn character_vertical_ranges_overlap(
    pos1: Position,
    physics1: CharacterPhysicsConfig,
    pos2: Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let bottom1 = pos1.y + physics1.collider.bottom_y_offset();
    let top1 = pos1.y + physics1.collider.top_y_offset();
    let bottom2 = pos2.y + physics2.collider.bottom_y_offset();
    let top2 = pos2.y + physics2.collider.top_y_offset();
    bottom1 <= top2 && bottom2 <= top1
}

pub(super) fn character_support_probe_shape(physics: CharacterPhysicsConfig) -> Cuboid {
    Cuboid::new(Vector::new(
        physics.support_probe.width / 2.0,
        physics.collision_height() / 2.0,
        physics.support_probe.depth / 2.0,
    ))
}

pub(super) fn character_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    Pose::translation(pos.x, physics.collider_center_y(pos.y), pos.z)
}

pub(super) fn character_support_probe_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    character_pose(pos, physics)
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
    let shape1 = character_shape(physics1);
    let shape2 = character_shape(physics2);
    let velocity1 = Vector::new(end1.x - start1.x, end1.y - start1.y, end1.z - start1.z);
    let velocity2 = Vector::new(end2.x - start2.x, end2.y - start2.y, end2.z - start2.z);
    if character_positions_intersect(start1, physics1, start2, physics2) {
        return true;
    }

    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    cast_shapes(
        &character_pose(start1, physics1),
        velocity1,
        &shape1,
        &character_pose(start2, physics2),
        velocity2,
        &shape2,
        options,
    )
    .is_ok_and(|hit| hit.is_some())
}

pub(super) fn character_positions_intersect(
    pos1: &Position,
    physics1: CharacterPhysicsConfig,
    pos2: &Position,
    physics2: CharacterPhysicsConfig,
) -> bool {
    let shape1 = character_shape(physics1);
    let shape2 = character_shape(physics2);
    intersection_test(
        &character_pose(pos1, physics1),
        &shape1,
        &character_pose(pos2, physics2),
        &shape2,
    )
    .is_ok_and(|overlaps| overlaps)
}

#[must_use]
pub fn character_overlaps_item(character_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = character_pos.x - item_pos.x;
    let dy = character_pos.y - item_pos.y;
    let dz = character_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}
