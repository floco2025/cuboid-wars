use super::*;
use crate::config::gameplay::load_test_gameplay;

#[test]
fn hitbox_and_movement_shapes_are_independent() {
    let mut physics = load_test_gameplay()
        .expect("test gameplay config rejected")
        .player
        .physics();
    let movement = character_movement_shape(physics);
    physics.hitbox.width = 2.0;
    let after = character_movement_shape(physics);
    assert_eq!(after.radius, movement.radius);
    assert_eq!(after.segment.a, movement.segment.a);
    assert_eq!(after.segment.b, movement.segment.b);
    let hitbox = character_hitbox_shape(physics);
    physics.movement_collider.diameter = 0.8;
    assert_eq!(character_hitbox_shape(physics), hitbox);
}
