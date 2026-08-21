use crate::constants::{FLOOR_THICKNESS, LADDER_OVERSHOOT, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS};
use crate::protocol::{Barrier, BarrierKindId, Floor, Ladder, MapLayout, Position, Ramp, Wall};

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
fn ladder_volume_is_symmetric_and_covers_overshoot() {
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
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
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
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());

    assert_eq!(world.solid_count(), 0);
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

#[test]
fn ground_surface_below_hits_floor_instead_of_wall_top() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    let hit = world
        .ground_surface_below(
            bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + WALL_HEIGHT + 1.0, 0.0),
            WALL_HEIGHT + 2.0,
        )
        .expect("expected floor below the wall");

    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 0.001, "hit was {hit:?}");
    assert_eq!(hit.normal, bevy_math::Vec3::Y);
}

#[test]
fn ground_surface_below_returns_ramp_normal() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    let hit = world
        .ground_surface_below(bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + 2.0, 6.0), LEVEL_HEIGHT + 2.0)
        .expect("expected ramp below the ray");

    assert!(hit.normal.y > 0.1, "hit was {hit:?}");
    assert_ne!(hit.normal, bevy_math::Vec3::Y);
}

#[test]
fn ground_surface_below_returns_none_over_void() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    assert!(
        world
            .ground_surface_below(bevy_math::Vec3::new(20.0, LEVEL_HEIGHT, 20.0), LEVEL_HEIGHT)
            .is_none()
    );
}

#[test]
fn world_surface_along_ray_hits_wall_between_points() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    let hit = world
        .world_surface_along_ray(
            bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0),
            bevy_math::Vec3::NEG_Z,
            4.0,
        )
        .expect("expected the wall to intercept the ray");

    assert!((hit.point.z - WALL_THICKNESS / 2.0).abs() < 0.001, "hit was {hit:?}");
}

#[test]
fn world_surface_along_ray_hits_floor_unlike_wall_filter() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());
    let origin = bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0);

    // A downward-pitched beam must clip at the floor; the walls-only filter
    // would let it pierce through.
    assert!(
        world
            .world_surface_along_ray(origin, bevy_math::Vec3::NEG_Y, 3.0)
            .is_some()
    );
    assert!(
        world
            .wall_surface_along_ray(origin, bevy_math::Vec3::NEG_Y, 3.0)
            .is_none()
    );
}

#[test]
fn world_surface_along_ray_returns_none_in_the_open() {
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());

    assert!(
        world
            .world_surface_along_ray(
                bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0),
                bevy_math::Vec3::Z,
                4.0,
            )
            .is_none()
    );
}

#[test]
fn wall_surface_along_ray_ignores_barrier() {
    let mut layout = test_map_layout();
    layout.barriers.push(Barrier {
        x1: 0.0,
        z1: 1.0,
        x2: 4.0,
        z2: 1.0,
        level: 1,
        kind: BarrierKindId(0),
    });
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());

    let hit = world
        .wall_surface_along_ray(
            bevy_math::Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0),
            bevy_math::Vec3::NEG_Z,
            3.0,
        )
        .expect("expected wall behind the barrier");

    assert!((hit.point.z - WALL_THICKNESS / 2.0).abs() < 0.001, "hit was {hit:?}");
    assert_eq!(hit.normal, bevy_math::Vec3::Z);
}
