use super::*;

#[test]
fn character_sweep_hits_wall_end_clipped_by_the_body_edge() {
    let world = wall_end_world();
    // Centre line passes 0.8 m west of the wall; the 0.9 m half-width does not.
    let start = Position {
        x: -0.8,
        y: 0.0,
        z: 1.0,
    };
    let target = Position {
        x: -0.8,
        y: 0.0,
        z: -3.0,
    };

    assert!(world.character_sweep_hits_wall(&start, &target, wide_body()));
}

#[test]
fn character_sweep_is_clear_past_a_wall_end_with_room_for_the_body() {
    let world = wall_end_world();
    let start = Position {
        x: -1.5,
        y: 0.0,
        z: 1.0,
    };
    let target = Position {
        x: -1.5,
        y: 0.0,
        z: -3.0,
    };

    assert!(!world.character_sweep_hits_wall(&start, &target, wide_body()));
}

#[test]
fn character_sweep_ignores_floors_and_ramps() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &BarrierKindTable::default());
    let start = Position { x: 2.0, y: 0.0, z: 9.0 };
    let target = Position { x: 2.0, y: 0.0, z: 3.0 };

    assert!(!world.character_sweep_hits_wall(&start, &target, wide_body()));
}

#[test]
fn projectile_paths_reject_embedded_starts_even_when_stationary_or_heading_out() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &BarrierKindTable::default());
    let inside_wall = Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 0.0);
    assert!(!world.projectile_path_clear(inside_wall, Vec3::ZERO, 0.3, &[]));
    assert!(!world.projectile_path_clear(inside_wall, Vec3::Z * 2.0, 0.3, &[]));
    let clear = inside_wall + Vec3::Z;
    assert!(world.projectile_path_clear(clear, Vec3::ZERO, 0.3, &[]));
    assert!(!world.projectile_path_clear(clear, Vec3::NEG_Z, 0.3, &[]));
}
