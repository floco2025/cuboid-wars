use super::*;
use crate::{constants::LADDER_OVERSHOOT, protocol::Ladder};

#[test]
fn collision_world_contains_solids_for_walls_floors_and_ramps() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &BarrierKindTable::default());

    assert_eq!(world.solid_count(), 3);
    assert_eq!(
        world.solid_kinds(),
        vec![ColliderKind::Wall, ColliderKind::Floor, ColliderKind::Ramp]
    );
}

#[test]
fn wall_solid_uses_wall_level_height() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &BarrierKindTable::default());
    let (_, wall_collider) = world
        .colliders
        .iter()
        .find(|(_, collider)| ColliderKind::from_user_data(collider.user_data) == Some(ColliderKind::Wall))
        .expect("expected wall collider");
    let wall_center_y = wall_collider.position().translation.y;

    assert_eq!(wall_center_y, LEVEL_HEIGHT + WALL_HEIGHT / 2.0);
}

#[test]
fn ladder_volume_covers_the_front_and_overshoot() {
    let layout = MapLayout {
        ladders: vec![Ladder {
            x1: -0.5,
            z1: 0.0,
            x2: 0.5,
            z2: 0.0,
            nx: 0.0,
            nz: -1.0,
            level: 0,
            levels: 2,
            y: 0.0,
            height: 2.0 * LEVEL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let in_volume = |x: f32, y: f32, z: f32| world.ladder_volume_at(&Position { x, y, z }).is_some();

    assert!(in_volume(0.0, 0.0, -0.4));
    assert!(in_volume(0.0, 2.0 * LEVEL_HEIGHT + LADDER_OVERSHOOT - 0.01, -0.4));
    assert!(!in_volume(0.0, 2.0 * LEVEL_HEIGHT + LADDER_OVERSHOOT + 0.1, -0.4));
    // Only the front (the normal's side, -Z here) is a ladder; the back and
    // anything beyond the front depth are not.
    assert!(!in_volume(0.0, 1.0, 0.4));
    assert!(!in_volume(0.0, 1.0, -1.2));
    assert!(!in_volume(2.0, 1.0, -0.4));
}

#[test]
fn ladder_volume_is_not_a_solid() {
    let layout = MapLayout {
        ladders: vec![Ladder {
            x1: -0.5,
            z1: 0.0,
            x2: 0.5,
            z2: 0.0,
            nx: 0.0,
            nz: -1.0,
            level: 0,
            levels: 1,
            y: 0.0,
            height: LEVEL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());

    assert_eq!(world.solid_count(), 0);
}
