use super::*;
use crate::config::fixtures;
use bevy::math::Vec3;

#[test]
fn capsule_surface_distance_accounts_for_vertical_separation_and_round_ends() {
    let physics = fixtures::server_config().gameplay_config().player.physics();
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

#[test]
fn a_body_passing_through_a_hovering_item_collects_it_at_any_height() {
    let physics = fixtures::server_config().gameplay_config().player.physics();
    let height = physics.movement_collider.height;
    let item = Position::default();
    let feet = |y: f32| Position { y, ..item };
    for y in [0.0, ITEM_HOVER_HEIGHT, ITEM_HOVER_HEIGHT - height] {
        assert!(character_overlaps_item(&feet(y), physics, &item, 1.0));
    }
    assert!(!character_overlaps_item(
        &feet(ITEM_HOVER_HEIGHT + 1.1),
        physics,
        &item,
        1.0
    ));
    assert!(!character_overlaps_item(
        &feet(ITEM_HOVER_HEIGHT - height - 1.1),
        physics,
        &item,
        1.0
    ));
    let beside = Position { x: 1.1, ..item };
    assert!(!character_overlaps_item(&beside, physics, &item, 1.0));
}
