use super::*;
use crate::map::{GroundsSettings, ROCK_VARIANTS, RockClass};
use rapier3d::prelude::Ray;

#[test]
fn barrier_user_data_round_trips_kind() {
    let kind = BarrierId(7);
    let user_data = ColliderKind::barrier_user_data(kind, CarrierId::WORLD);
    assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Barrier));
    assert_eq!(
        ColliderKind::field_kind_from_user_data(user_data),
        Some(FieldKind::Barrier(kind))
    );
}

#[test]
fn bridge_user_data_is_not_a_barrier() {
    let user_data = ColliderKind::bridge_user_data(BridgeId(2), CarrierId::WORLD);
    assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Bridge));
    assert_eq!(
        ColliderKind::field_kind_from_user_data(user_data),
        Some(FieldKind::Bridge(BridgeId(2)))
    );
}

#[test]
fn user_data_round_trips_kind_and_carrier() {
    let carrier = CarrierId(300);
    let floor = ColliderKind::Floor.user_data(carrier);
    assert_eq!(ColliderKind::from_user_data(floor), Some(ColliderKind::Floor));
    assert_eq!(ColliderKind::carrier_from_user_data(floor), carrier);
    let barrier = ColliderKind::barrier_user_data(BarrierId(70000), carrier);
    assert_eq!(
        ColliderKind::field_kind_from_user_data(barrier),
        Some(FieldKind::Barrier(BarrierId(70000)))
    );
    assert_eq!(ColliderKind::carrier_from_user_data(barrier), carrier);
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
fn passability_excludes_only_the_named_barrier_instance() {
    let a = ColliderBuilder::ball(1.0)
        .user_data(ColliderKind::barrier_user_data(BarrierId(70000), CarrierId(300)))
        .build();
    let b = ColliderBuilder::ball(1.0)
        .user_data(ColliderKind::barrier_user_data(BarrierId(70001), CarrierId(300)))
        .build();
    assert!(!barrier_blocks(&a, &[BarrierId(70000)]));
    assert!(barrier_blocks(&b, &[BarrierId(70000)]));
    assert!(barrier_blocks(&a, &[]));
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
                        .expect("ray through rock hits its surface")
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
