use super::*;

#[test]
fn actor_plan_order_prioritizes_closer_go_to_target() {
    let mut order = vec![order(1, 9.0, 1), order(2, 1.0, 2), order(3, 4.0, 3)];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(3), ActorId(1),]
    );
}

#[test]
fn actor_plan_order_uses_actor_id_as_tie_breaker() {
    let mut order = vec![order(1, 4.0, 3), order(2, 4.0, 1), order(3, 4.0, 2)];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(1), ActorId(2), ActorId(3),]
    );
}

#[test]
fn actor_without_go_to_position_plans_after_targeted_actor() {
    let pos = Position::default();
    let targeted = ActorInfo {
        entity: Entity::from_bits(1),
        spawn_zone_index: 0,
        spawn_kind: TEST_KIND.into(),
        direction_timer: 0.0,
        patrol_intent: ActorMoveIntent::Idle,
        go_to_position: Some(Position { x: 1.0, y: 0.0, z: 0.0 }),
        go_to_position_is_chase: true,
        is_returning_to_spawn: false,
        return_path: Default::default(),
        chase_reacquire_timer: 0.0,
        avoidance_state: ActorAvoidanceState::None,
        last_damager: None,
    };

    assert!(actor_target_distance_sq(&pos, Some(&targeted)).is_finite());
    assert_eq!(actor_target_distance_sq(&pos, None), f32::INFINITY);
}
