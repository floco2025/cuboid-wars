use super::*;
use bevy::math::Vec3;

use crate::config::ServerGameplayConfig;

#[test]
fn capsule_surface_distance_accounts_for_vertical_separation_and_round_ends() {
    let physics = ServerGameplayConfig::load_default()
        .expect("server gameplay config missing")
        .gameplay_config()
        .player
        .physics();
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
