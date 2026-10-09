use super::*;
use crate::actors::navigation::air::FlightState;

#[test]
fn actor_plan_order_prioritizes_shorter_remaining_route_then_lower_id() {
    let mut order = vec![order(1, 9.0, 1), order(2, 4.0, 5), order(3, 1.0, 2), order(4, 4.0, 3)];

    sort_actor_plan_order(&mut order);

    assert_eq!(
        order.iter().map(|entry| entry.id).collect::<Vec<_>>(),
        vec![ActorId(2), ActorId(3), ActorId(5), ActorId(1)]
    );
}

#[test]
fn flying_route_order_measures_the_remaining_three_dimensional_length() {
    let mut actor = actor_info();
    actor.flight = Some(FlightState {
        route: [
            Position { x: 3.0, y: 4.0, z: 0.0 },
            Position {
                x: 3.0,
                y: 4.0,
                z: 12.0,
            },
        ]
        .into(),
        ..Default::default()
    });
    assert_eq!(actor_route_distance(&Position::default(), Some(&actor)), 17.0);
    assert_eq!(actor_route_distance(&Position::default(), None), f32::INFINITY);
}
