use super::*;
use crate::map::{GroundsSettings, ROCK_VARIANTS, RockClass};
use rapier3d::prelude::Ray;

#[test]
fn field_user_data_round_trips_the_piece_its_field_and_its_carrier() {
    let carrier = CarrierId(300);
    for piece in [ColliderKind::Barrier, ColliderKind::Bridge] {
        let user_data = piece.field_user_data(FieldId(u16::MAX), carrier);
        assert_eq!(ColliderKind::from_user_data(user_data), Some(piece));
        assert_eq!(ColliderKind::field_from_user_data(user_data), Some(FieldId(u16::MAX)));
        assert_eq!(ColliderKind::carrier_from_user_data(user_data), carrier);
    }
    let floor = ColliderKind::Floor.user_data(carrier);
    assert_eq!(ColliderKind::from_user_data(floor), Some(ColliderKind::Floor));
    assert_eq!(ColliderKind::field_from_user_data(floor), None);
    assert_eq!(ColliderKind::carrier_from_user_data(floor), carrier);
    assert_eq!(
        ColliderKind::carrier_from_user_data(ColliderKind::Wall.user_data(CarrierId::WORLD)),
        CarrierId::WORLD
    );
}

#[test]
fn characters_query_all_surface_and_barrier_groups() {
    let groups = character_collision_groups();
    assert!(groups.contains(surface_collision_groups() | BARRIER_COLLISION_GROUP));
    assert!((BARRIER_COLLISION_GROUP & surface_collision_groups()).is_empty());
}

#[test]
fn passability_lets_a_body_through_every_piece_of_the_named_field_alone() {
    let piece = |kind: ColliderKind, field| {
        ColliderBuilder::ball(1.0)
            .user_data(kind.field_user_data(FieldId(field), CarrierId(300)))
            .build()
    };
    let wall = ColliderBuilder::ball(1.0)
        .user_data(ColliderKind::Wall.user_data(CarrierId(300)))
        .build();
    let passable = [FieldId(7)];
    assert!(!field_blocks(&piece(ColliderKind::Barrier, 7), &passable));
    assert!(!field_blocks(&piece(ColliderKind::Bridge, 7), &passable));
    assert!(field_blocks(&piece(ColliderKind::Barrier, 8), &passable));
    assert!(field_blocks(&piece(ColliderKind::Bridge, 8), &passable));
    assert!(field_blocks(&wall, &passable));
    assert!(field_blocks(&piece(ColliderKind::Barrier, 7), &[]));
}

#[test]
fn cached_rock_hulls_preserve_scaled_and_rotated_collision_surfaces() {
    let grounds = Grounds::new([(-20.0, 20.0, -30.0, 30.0)], 4.4, GroundsSettings { level: 1 });
    let mut colliders = ColliderSet::new();
    let handles = insert_grounds_colliders(&mut colliders, &grounds);
    let decorations = grounds.collidable_decorations();
    assert_eq!(handles.len(), decorations.len() + 1);
    let mut checked = HashMap::new();
    for (decoration, handle) in decorations.iter().zip(&handles[1..]) {
        let DecorationKind::Rock(class) = decoration.kind else {
            continue;
        };
        let count = checked.entry((class, decoration.variant)).or_insert(0);
        if *count == 2 {
            continue;
        }
        *count += 1;
        let points: Vec<_> = rock_shape(class, decoration.variant, ROCK_HULL_SUBDIVISIONS)
            .vertices
            .into_iter()
            .map(|point| to_rapier(decoration.rotation * (point * decoration.scale)))
            .collect();
        let expected = ColliderBuilder::convex_hull(&points)
            .expect("reference rock hull")
            .translation(to_rapier(decoration.position))
            .build();
        let actual = &colliders[*handle];
        for direction in [Vec3::X, Vec3::Y, Vec3::Z, Vec3::new(1.0, 2.0, 3.0).normalize()] {
            for sign in [-1.0, 1.0] {
                let direction = direction * sign;
                let ray = Ray::new(to_rapier(decoration.position + direction * 8.0), to_rapier(-direction));
                let hit = |collider: &Collider| {
                    collider
                        .shape()
                        .cast_ray_and_get_normal(collider.position(), &ray, 16.0, true)
                        .expect("ray misses the rock")
                };
                let expected = hit(&expected);
                let actual = hit(actual);
                assert!((actual.time_of_impact - expected.time_of_impact).abs() < 0.001);
                assert!(actual.normal.dot(expected.normal) > 0.999);
            }
        }
    }
    for class in [RockClass::Stone, RockClass::Boulder] {
        for variant in 0..ROCK_VARIANTS {
            assert_eq!(checked.get(&(class, variant)), Some(&2));
        }
    }
}
