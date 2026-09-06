use crate::config::{
    CharacterColliderAnchor, CharacterColliderConfig, CharacterPhysicsConfig, CharacterSupportProbeConfig,
};
use crate::constants::LADDER_OVERSHOOT;
use crate::protocol::{Barrier, BarrierKindId, Floor, Ladder, MapLayout, Position, Ramp, Wall};
use crate::test_geometry::{
    BARRIER_THICKNESS, BRIDGE_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS,
};

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
            y: LEVEL_HEIGHT,
            height: WALL_HEIGHT,
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
            y: 0.0,
            height: LEVEL_HEIGHT,
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
        levels: 1,
        kind: BarrierKindId(0),
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        width: BARRIER_THICKNESS,
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

// A wall end at the origin, running north (negative z) along x = 0.
fn wall_end_world() -> CollisionWorld {
    CollisionWorld::from_map_layout(
        &MapLayout {
            walls: vec![Wall {
                x1: 0.0,
                z1: 0.0,
                x2: 0.0,
                z2: -8.0,
                width: WALL_THICKNESS,
                level: 0,
                y: 0.0,
                height: WALL_HEIGHT,
            }],
            floors: vec![Floor {
                x1: -8.0,
                z1: -8.0,
                x2: 8.0,
                z2: 8.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
            }],
            ..Default::default()
        },
        &crate::protocol::BarrierKindTable::default(),
    )
}

fn wide_body() -> CharacterPhysicsConfig {
    CharacterPhysicsConfig {
        collider: CharacterColliderConfig {
            width: 1.8,
            height: 1.0,
            depth: 1.4,
            y_offset: 0.45,
            y_offset_anchor: CharacterColliderAnchor::Bottom,
        },
        support_probe: CharacterSupportProbeConfig { width: 0.2, depth: 0.2 },
    }
}

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
    let world = CollisionWorld::from_map_layout(&test_map_layout(), &crate::protocol::BarrierKindTable::default());
    let start = Position { x: 2.0, y: 0.0, z: 9.0 };
    let target = Position { x: 2.0, y: 0.0, z: 3.0 };

    assert!(!world.character_sweep_hits_wall(&start, &target, wide_body()));
}

#[test]
fn light_bridge_supports_a_character_only_while_powered() {
    use crate::config::CharacterPhysicsConfig;
    use crate::protocol::{BridgeKindId, LightBridge};
    use rapier3d::prelude::Pose;

    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            level: 1,
            kind: BridgeKindId(0),
            thickness: BRIDGE_THICKNESS,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    assert_eq!(world.solid_kinds(), vec![ColliderKind::Bridge]);

    let physics: CharacterPhysicsConfig = wide_body();
    let shape = crate::physics::characters::character_shape(physics);
    let pose = Pose::translation(2.0, LEVEL_HEIGHT + physics.collider.bottom_y_offset() + 0.05, 2.0);
    let probe = |world: &CollisionWorld| world.ground_hit(&shape, &pose, 1.0, 0.0, &[], &[]);

    assert!(probe(&world).is_none(), "an unpowered bridge is not ground");
    world.set_powered_bridges(&[BridgeKindId(1)]);
    assert!(probe(&world).is_none(), "another powered kind is not this bridge");
    world.set_powered_bridges(&[BridgeKindId(0)]);
    assert!(probe(&world).is_some(), "a powered bridge is ground");
    world.set_powered_bridges(&[]);
    assert!(probe(&world).is_none(), "power switches off again");
}

#[test]
fn a_powered_light_bridge_stays_out_of_sight_and_ground_probes() {
    use crate::protocol::{BridgeKindId, LightBridge};

    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: LEVEL_HEIGHT,
            level: 1,
            kind: BridgeKindId(0),
            thickness: BRIDGE_THICKNESS,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    world.set_powered_bridges(&[BridgeKindId(0)]);
    let above = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);
    let below = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT - 1.0, 0.0);

    assert!(
        world.cast_moving_ball(above, below - above, 0.1).is_some(),
        "a surface query sees it"
    );
    assert!(world.line_of_sight_clear(above, below), "sight reaches through it");
    assert!(
        world.ground_surface_below(above, 2.0).is_none(),
        "rain and scorch probes ignore it"
    );
    assert!(
        world
            .world_surface_along_ray(above, bevy_math::Vec3::NEG_Y, 2.0)
            .is_none(),
        "the world ray ignores it"
    );
}

#[test]
fn portal_surface_ray_hits_a_tile_and_names_it() {
    use crate::protocol::MovingFloorId;

    let mut layout = slider_layout();
    layout.floors.push(Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: LEVEL_HEIGHT - 2.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    });
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    let above = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);

    let (hit, anchor) = world
        .portal_surface_along_ray(above, bevy_math::Vec3::NEG_Y, 4.0)
        .expect("the tile did not stop the portal ray");
    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(anchor, Some(MovingFloorId(0)));

    let beside = bevy_math::Vec3::new(1.8, LEVEL_HEIGHT + 1.0, 0.0);
    let (hit, anchor) = world
        .portal_surface_along_ray(beside, bevy_math::Vec3::NEG_Y, 4.0)
        .expect("the floor under the tile's edge was missed");
    assert!((hit.point.y - (LEVEL_HEIGHT - 2.0)).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(anchor, None);
}

fn slider_layout() -> MapLayout {
    use crate::protocol::MovingFloor;

    MapLayout {
        moving_floors: vec![MovingFloor {
            x1: 0.0,
            y1: LEVEL_HEIGHT,
            z1: 0.0,
            x2: 8.0,
            y2: LEVEL_HEIGHT,
            z2: 0.0,
            half_x: 1.5,
            half_z: 1.5,
            thickness: FLOOR_THICKNESS,
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
            level: 1,
            levels: 0,
        }],
        ..Default::default()
    }
}

#[test]
fn moving_floor_collider_follows_its_current_center() {
    use rapier3d::prelude::Pose;

    let mut world = CollisionWorld::from_map_layout(&slider_layout(), &crate::protocol::BarrierKindTable::default());
    assert_eq!(world.solid_kinds(), vec![ColliderKind::MovingFloor]);
    let physics = wide_body();
    let shape = crate::physics::characters::character_shape(physics);
    let probe = |world: &CollisionWorld, x: f32| {
        let pose = Pose::translation(x, LEVEL_HEIGHT + physics.collider.bottom_y_offset() + 0.05, 0.0);
        world.ground_hit(&shape, &pose, 1.0, 0.0, &[], &[])
    };

    assert!(probe(&world, 0.0).is_some(), "the tile starts at its first end");
    world.set_moving_floor_centers(&[bevy_math::Vec3::new(8.0, LEVEL_HEIGHT - FLOOR_THICKNESS / 2.0, 0.0)]);
    assert!(probe(&world, 0.0).is_none(), "the tile left its first end");
    assert!(probe(&world, 8.0).is_some(), "the tile arrived at its second end");
}

#[test]
fn moving_floor_is_a_surface_but_not_world_geometry() {
    let world = CollisionWorld::from_map_layout(&slider_layout(), &crate::protocol::BarrierKindTable::default());
    let above = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);
    let below = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT - 1.0, 0.0);

    assert!(
        world.cast_moving_ball(above, below - above, 0.1).is_some(),
        "a surface query sees it"
    );
    assert!(world.line_of_sight_clear(above, below), "sight reaches through it");
    assert!(
        world.ground_surface_below(above, 2.0).is_none(),
        "rain and scorch probes ignore it"
    );
    assert!(
        world
            .world_surface_along_ray(above, bevy_math::Vec3::NEG_Y, 2.0)
            .is_none(),
        "a portal never lands on it"
    );
}
