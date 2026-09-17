use super::*;
use crate::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    map::{Grounds, GroundsSettings},
};
use bevy_math::{Mat3, Quat};
use rapier3d::prelude::ColliderHandle;

fn floor_with_grounds() -> MapLayout {
    let floor = Floor {
        x1: -6.0,
        z1: -6.0,
        x2: 6.0,
        z2: 6.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    MapLayout {
        grounds: Some(Grounds::new([floor.bounds_xz()], floor.y, GroundsSettings { level: 0 })),
        floors: vec![floor],
        ..Default::default()
    }
}

fn floor_portal_backing(world: &CollisionWorld, center: Vec3) -> Vec<ColliderHandle> {
    world.portal_backing_colliders(
        center,
        Vec3::Y,
        Vec3::new(PORTAL_HALF_WIDTH, PORTAL_HALF_HEIGHT, 0.25),
        Quat::from_mat3(&Mat3::from_cols(Vec3::X, Vec3::NEG_Z, Vec3::Y)),
        CarrierId::WORLD,
    )
}

#[test]
fn terrain_level_with_a_floor_never_backs_a_portal() {
    let world = CollisionWorld::from_map_layout(&floor_with_grounds());
    assert_eq!(floor_portal_backing(&world, Vec3::new(6.0, 0.0, 0.0)).len(), 1);
    assert!(floor_portal_backing(&world, Vec3::new(9.0, 0.0, 0.0)).is_empty());
}
