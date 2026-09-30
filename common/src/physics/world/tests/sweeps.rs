use super::*;

#[test]
fn projectile_paths_reject_embedded_starts_even_when_stationary_or_heading_out() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());
    let inside_wall = Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 0.0);
    assert!(!world.projectile_path_clear(inside_wall, Vec3::ZERO, 0.3, &[]));
    assert!(!world.projectile_path_clear(inside_wall, Vec3::Z * 2.0, 0.3, &[]));
    let clear = inside_wall + Vec3::Z;
    assert!(world.projectile_path_clear(clear, Vec3::ZERO, 0.3, &[]));
    assert!(!world.projectile_path_clear(clear, Vec3::NEG_Z, 0.3, &[]));
}
