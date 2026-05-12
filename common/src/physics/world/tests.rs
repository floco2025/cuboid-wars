use crate::constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};
use crate::protocol::{Floor, MapLayout, Ramp, Wall};

use super::{CollisionWorld, colliders::ColliderKind};

fn test_map_layout() -> MapLayout {
    MapLayout {
        walls: vec![Wall {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 1,
        }],
        floors: vec![Floor {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
        }],
        ramps: vec![Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 8.0,
        }],
        ..Default::default()
    }
}

#[test]
fn collision_world_contains_solids_for_walls_floors_and_ramps() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    assert_eq!(world.solid_count(), 3);
    assert_eq!(
        world.solid_kinds(),
        vec![ColliderKind::Wall, ColliderKind::Floor, ColliderKind::Ramp]
    );
}

#[test]
fn wall_solid_uses_wall_level_height() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());
    let (_, wall_collider) = world
        .colliders
        .iter()
        .find(|(_, collider)| ColliderKind::from_user_data(collider.user_data) == Some(ColliderKind::Wall))
        .expect("expected wall collider");
    let wall_center_y = wall_collider.position().translation.y;

    assert_eq!(wall_center_y, LEVEL_HEIGHT + WALL_HEIGHT / 2.0);
}

#[test]
fn ramp_converts_to_collider() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    assert!(world.solid_kinds().contains(&ColliderKind::Ramp));
}
