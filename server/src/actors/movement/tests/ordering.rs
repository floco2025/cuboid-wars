use super::*;

#[test]
fn actor_plan_order_prioritizes_shorter_remaining_route() {
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
fn actor_without_route_plans_after_routed_actor() {
    let pos = Position::default();
    let mut targeted = ActorInfo::new(Entity::from_bits(1), 0, TEST_KIND.into(), CarrierId::WORLD);
    targeted.route = Some(route(Position { x: 1.0, y: 0.0, z: 0.0 }));

    assert!(actor_route_distance(&pos, Some(&targeted)).is_finite());
    assert_eq!(actor_route_distance(&pos, None), f32::INFINITY);
}

#[test]
fn multi_waypoint_queue_plans_front_actor_before_rear_actor() {
    let target = Position {
        x: 10.0,
        y: 0.0,
        z: 0.0,
    };
    let mut rear = ActorInfo::new(Entity::from_bits(1), 0, TEST_KIND.into(), CarrierId::WORLD);
    rear.route = Some(ActorRoute {
        waypoints: [Position { x: 1.0, y: 0.0, z: 0.0 }, target]
            .map(NavWaypoint::walk)
            .into(),
        destination: target,
        destination_node: NavNode {
            level: 0,
            row: 0,
            col: 2,
        },
    });
    let mut front = ActorInfo::new(Entity::from_bits(2), 0, TEST_KIND.into(), CarrierId::WORLD);
    front.route = Some(ActorRoute {
        waypoints: [Position { x: 9.0, y: 0.0, z: 0.0 }, target]
            .map(NavWaypoint::walk)
            .into(),
        destination: target,
        destination_node: NavNode {
            level: 0,
            row: 0,
            col: 2,
        },
    });
    let mut order = vec![
        order(1, actor_route_distance(&Position::default(), Some(&rear)), 1),
        order(
            2,
            actor_route_distance(&Position { x: 8.0, y: 0.0, z: 0.0 }, Some(&front)),
            2,
        ),
    ];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(1)]
    );
}

#[test]
fn ground_and_flying_routes_compare_the_same_remaining_three_dimensional_length() {
    use crate::actors::navigation::air::FlightState;
    let points = [
        Position { x: 3.0, y: 4.0, z: 0.0 },
        Position {
            x: 3.0,
            y: 4.0,
            z: 12.0,
        },
    ];
    let mut ground = actor_info();
    ground.route = Some(ActorRoute {
        waypoints: points.map(NavWaypoint::walk).into(),
        destination: points[1],
        destination_node: NavNode {
            level: 0,
            row: 0,
            col: 0,
        },
    });
    let mut air = actor_info();
    air.flight = Some(FlightState {
        route: points.into(),
        ..Default::default()
    });
    assert_eq!(actor_route_distance(&Position::default(), Some(&ground)), 17.0);
    assert_eq!(actor_route_distance(&Position::default(), Some(&air)), 17.0);
}
