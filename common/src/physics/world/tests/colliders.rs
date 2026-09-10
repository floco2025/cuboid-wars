use super::*;

fn barrier_mask(count: u16) -> Group {
    let mut mask = Group::empty();
    for i in 0..count {
        mask |= barrier_collision_group(BarrierKindId(i));
    }
    mask
}

#[test]
fn barrier_collision_group_is_unique_per_kind() {
    let g0 = barrier_collision_group(BarrierKindId(0));
    let g1 = barrier_collision_group(BarrierKindId(1));
    let g2 = barrier_collision_group(BarrierKindId(2));
    assert_ne!(g0, g1);
    assert_ne!(g1, g2);
    assert_ne!(g0, g2);
    // Barrier bits must not overlap reserved world groups.
    assert!((g0 & (WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP)).is_empty());
    assert!((g1 & (WALL_COLLISION_GROUP | FLOOR_COLLISION_GROUP | RAMP_COLLISION_GROUP)).is_empty());
}

#[test]
fn barrier_user_data_round_trips_kind() {
    let kind = BarrierKindId(7);
    let user_data = ColliderKind::barrier_user_data(kind, CarrierId::WORLD);
    assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Barrier));
    assert_eq!(
        ColliderKind::field_kind_from_user_data(user_data),
        Some(FieldKind::Barrier(kind))
    );
}

#[test]
fn bridge_user_data_is_not_a_barrier() {
    let user_data = ColliderKind::bridge_user_data(BridgeKindId(2), CarrierId::WORLD);
    assert_eq!(ColliderKind::from_user_data(user_data), Some(ColliderKind::Bridge));
    assert_eq!(
        ColliderKind::field_kind_from_user_data(user_data),
        Some(FieldKind::Bridge(BridgeKindId(2)))
    );
}

#[test]
fn user_data_round_trips_kind_and_carrier() {
    let carrier = CarrierId(300);
    let floor = ColliderKind::Floor.user_data(carrier);
    assert_eq!(ColliderKind::from_user_data(floor), Some(ColliderKind::Floor));
    assert_eq!(ColliderKind::carrier_from_user_data(floor), carrier);
    let barrier = ColliderKind::barrier_user_data(BarrierKindId(27), carrier);
    assert_eq!(
        ColliderKind::field_kind_from_user_data(barrier),
        Some(FieldKind::Barrier(BarrierKindId(27)))
    );
    assert_eq!(ColliderKind::carrier_from_user_data(barrier), carrier);
    assert_eq!(
        ColliderKind::carrier_from_user_data(ColliderKind::Wall.user_data(CarrierId::WORLD)),
        CarrierId::WORLD
    );
}

#[test]
fn every_barrier_kind_is_disjoint_from_the_shared_surface_groups() {
    let max = u16::try_from(BarrierKindId::MAX.expect("barrier kinds carry no collision-group cap"))
        .expect("barrier kind cap exceeds u16");
    let reserved = surface_collision_groups();
    let mut seen = Group::empty();
    for idx in 0..max {
        let group = barrier_collision_group(BarrierKindId(idx));
        assert!((group & (reserved | seen)).is_empty(), "barrier {idx} overlaps");
        seen |= group;
    }
}

#[test]
fn character_collision_groups_includes_the_bridge_group() {
    let groups = character_collision_groups(&[], barrier_mask(2));
    assert!(groups.contains(BRIDGE_COLLISION_GROUP));
    assert!(groups.contains(FLOOR_COLLISION_GROUP));
}

#[test]
fn character_collision_groups_keeps_all_barriers_without_passable_kinds() {
    let all = barrier_mask(3);
    let groups = character_collision_groups(&[], all);
    assert_eq!(groups & all, all);
    assert!(groups.contains(WALL_COLLISION_GROUP));
}

#[test]
fn character_collision_groups_removes_held_key_kinds() {
    let all = barrier_mask(3);
    let held = [BarrierKindId(1)];
    let groups = character_collision_groups(&held, all);
    assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
    assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
    assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
    assert!(groups.contains(WALL_COLLISION_GROUP));
}

// After pressure plates, callers union `held_keys` with `open_kinds` via
// `passable_barrier_kinds` and pass the result here. Validate that
// merged input removes BOTH groups — same as if the caller had
// hand-rolled the union.
#[test]
fn character_collision_groups_removes_union_of_held_and_open() {
    let all = barrier_mask(4);
    let held = [BarrierKindId(1)];
    let open = [BarrierKindId(3)];
    let merged = crate::physics::passable_barrier_kinds(&held, &open);
    let groups = character_collision_groups(&merged, all);
    assert!(!groups.contains(barrier_collision_group(BarrierKindId(1))));
    assert!(!groups.contains(barrier_collision_group(BarrierKindId(3))));
    assert!(groups.contains(barrier_collision_group(BarrierKindId(0))));
    assert!(groups.contains(barrier_collision_group(BarrierKindId(2))));
}
