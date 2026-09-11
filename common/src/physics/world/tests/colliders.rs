use super::*;

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
