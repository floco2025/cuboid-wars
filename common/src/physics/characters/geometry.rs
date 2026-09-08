use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::{Capsule, Cuboid},
    },
    prelude::{Pose, Vector},
};

use crate::{config::CharacterPhysicsConfig, constants::CHARACTER_CONTACT_OFFSET, protocol::Position};

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

#[must_use]
// The skin lifts the shape while the shared position remains at the feet.
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

#[must_use]
pub fn character_surface_distance(
    a: Position,
    a_physics: CharacterPhysicsConfig,
    b: Position,
    b_physics: CharacterPhysicsConfig,
) -> f32 {
    let ac = a_physics.movement_collider;
    let bc = b_physics.movement_collider;
    let vertical_gap = ((a.y + ac.radius()) - (b.y + bc.height - bc.radius()))
        .max((b.y + bc.radius()) - (a.y + ac.height - ac.radius()))
        .max(0.0);
    (a.horizontal_distance_sq(&b) + vertical_gap * vertical_gap).sqrt() - ac.radius() - bc.radius()
}

pub fn character_movement_pose(pos: &Position, physics: CharacterPhysicsConfig) -> Pose {
    let center = character_movement_center(*pos, physics);
    Pose::translation(center.x, center.y, center.z)
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
    let velocity1 = Vector::new(end1.x - start1.x, end1.y - start1.y, end1.z - start1.z);
    let velocity2 = Vector::new(end2.x - start2.x, end2.y - start2.y, end2.z - start2.z);
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

pub(super) fn character_positions_intersect(
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

#[must_use]
pub fn character_overlaps_item(character_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = character_pos.x - item_pos.x;
    let dy = character_pos.y - item_pos.y;
    let dz = character_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::gameplay::load_test_gameplay, physics::ball_overlaps_character};

    #[test]
    fn movement_and_damage_dimensions_are_independent() {
        let mut physics = load_test_gameplay()
            .expect("test gameplay config rejected")
            .player
            .physics();
        let movement = character_movement_shape(physics);
        let position = Position::default();
        let target = Position { x: 0.8, y: 1.0, z: 0.0 };
        assert!(!ball_overlaps_character(&target, 0.05, &position, 0.0, physics));
        physics.hitbox.width = 2.0;
        assert!(ball_overlaps_character(&target, 0.05, &position, 0.0, physics));
        let after = character_movement_shape(physics);
        assert_eq!(after.radius, movement.radius);
        assert_eq!(after.segment.a, movement.segment.a);
        assert_eq!(after.segment.b, movement.segment.b);
        let hitbox = character_hitbox_shape(physics);
        physics.movement_collider.diameter = 0.8;
        assert_eq!(character_hitbox_shape(physics), hitbox);
    }

    #[test]
    fn capsule_surface_distance_accounts_for_vertical_separation_and_round_ends() {
        let physics = load_test_gameplay()
            .expect("test gameplay config rejected")
            .player
            .physics();
        let at = Position::default();
        let radius = physics.movement_collider.radius();
        for direction in [Vec3::X, Vec3::Z, Vec3::new(1.0, 0.0, 1.0).normalize()] {
            let other = Position::from(direction * (radius * 2.0 + 0.1));
            assert!((character_surface_distance(at, physics, other, physics) - 0.1).abs() < 1e-5);
        }
        let above = Position {
            y: physics.movement_collider.height + 0.1,
            ..at
        };
        assert!((character_surface_distance(at, physics, above, physics) - 0.1).abs() < 1e-5);
    }
}
