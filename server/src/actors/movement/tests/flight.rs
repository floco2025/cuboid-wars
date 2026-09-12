use super::*;
use crate::actors::{
    movement::tests::{actor_info, context, test_entity},
    navigation::air::FlightState,
};
use common::{
    physics::{CollisionWorld, character_positions_intersect},
    protocol::{MapLayout, Position},
};

#[test]
fn diagonal_flight_uses_total_speed() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let pos = Position::default();
    let mut info = actor_info();
    info.flight = Some(FlightState {
        route: [Position::from(Vec3::new(3.0, 4.0, 0.0))].into(),
        ..Default::default()
    });
    let context = context(test_entity(1), &pos, &world, &[], &[]);
    let selected = select_flying_move(&context, &info, 2.0, 4.0);
    assert!((Vec3::from(selected.step.position).length() - 0.2).abs() < 0.001);
    assert!(selected.step.position.y > 0.0);
    assert_eq!(selected.intent.speed(), Some(2.0));
}

#[test]
fn ascending_flyer_cannot_pass_through_a_character_above_it() {
    use crate::actors::movement::tests::actor_physics;
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let pos = Position::default();
    let above = Vec3::new(0.0, actor_physics().movement_collider.height + 0.05, 0.0).into();
    let starts = [(test_entity(2), above, actor_physics())];
    let context = context(test_entity(1), &pos, &world, &[], &starts);
    assert!(evaluate(&context, Vec3::Y * 4.0).is_none());
    let mut info = actor_info();
    info.flight = Some(FlightState {
        route: [Vec3::new(0.0, 10.0, 0.0).into()].into(),
        ..Default::default()
    });
    let selected = select_flying_move(&context, &info, 4.0, 4.0);
    assert!(!character_positions_intersect(
        &selected.step.position,
        context.actor_physics,
        &above,
        actor_physics()
    ));
}
