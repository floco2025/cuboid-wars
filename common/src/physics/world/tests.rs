use crate::config::{
    CharacterColliderAnchor, CharacterColliderConfig, CharacterPhysicsConfig, CharacterSupportProbeConfig,
    PortalShotSettings, gameplay::load_test_gameplay,
};
use crate::constants::LADDER_OVERSHOOT;
use crate::protocol::{
    Barrier, BarrierKindId, BarrierKindTable, BridgeKindId, CarrierId, Floor, Ladder, LightBridge, MapLayout, Position,
    Ramp, Wall,
};
use crate::test_geometry::{
    BARRIER_THICKNESS, BRIDGE_THICKNESS, FLOOR_THICKNESS, LEVEL_HEIGHT, WALL_HEIGHT, WALL_THICKNESS,
};
use bevy_math::Vec3;
use rapier3d::{
    control::KinematicCharacterController,
    prelude::{Pose, Vector},
};

use super::{CollisionWorld, colliders::ColliderKind};
use crate::{
    constants::TICK_SECS,
    map::Carriers,
    physics::characters::{character_center, character_shape},
    protocol::Carrier,
};

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
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 0.0,
            x2: 4.0,
            y2: LEVEL_HEIGHT,
            z2: 8.0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    }
}

#[test]
fn carrier_pushes_respect_barrier_passability_bridge_power_and_portal_exclusions() {
    let carrier = CarrierId(1);
    let wall = Wall {
        x1: -2.0,
        x2: 2.0,
        z1: 0.0,
        z2: 0.0,
        width: 0.4,
        y: 0.0,
        height: 3.0,
        level: 0,
        carrier,
    };
    let barrier = Barrier {
        x1: wall.x1,
        x2: wall.x2,
        z1: wall.z1,
        z2: wall.z2,
        width: wall.width,
        y: wall.y,
        height: wall.height,
        level: 0,
        levels: 1,
        kind: BarrierKindId(0),
        carrier,
    };
    let bridge = LightBridge {
        x1: -2.0,
        x2: 2.0,
        z1: -2.0,
        z2: 0.2,
        y: 1.5,
        thickness: 1.0,
        level: 0,
        kind: BridgeKindId(0),
        carrier,
    };
    let table = BarrierKindTable::from_ids(vec!["red".into()]).expect("barrier catalog invalid");
    let physics = load_test_gameplay()
        .expect("test gameplay config invalid")
        .player
        .physics();
    let shape = character_shape(physics);
    let center = character_center(
        Position {
            x: 0.0,
            y: 0.0,
            z: 0.2 + physics.collider.depth / 2.0 + 0.01,
        },
        physics,
    );
    let pose = Pose::translation(center.x, center.y, center.z);
    for kind in [ColliderKind::Wall, ColliderKind::Barrier, ColliderKind::Bridge] {
        let layout = MapLayout {
            walls: if kind == ColliderKind::Wall { vec![wall] } else { vec![] },
            barriers: if kind == ColliderKind::Barrier {
                vec![barrier]
            } else {
                vec![]
            },
            light_bridges: if kind == ColliderKind::Bridge {
                vec![bridge]
            } else {
                vec![]
            },
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::default(),
                to: Position { x: 0.0, y: 0.0, z: 4.0 },
                travel_ticks: 60,
                pause_ticks: 0,
                phase_ticks: 0,
            }],
            ..Default::default()
        };
        let mut world = CollisionWorld::from_map_layout(&layout, &table);
        let mut carriers = Carriers::from_layout(&layout);
        carriers.advance(1);
        world.set_carrier_poses(&carriers);
        let handles: Vec<_> = world.colliders.iter().map(|(handle, _)| handle).collect();
        for powered in [false, true] {
            world.set_powered_bridges(if powered { &[BridgeKindId(0)] } else { &[] });
            for passable in [vec![], vec![BarrierKindId(0)]] {
                for excluded in [&[][..], handles.as_slice()] {
                    let movement = world.push_character_from_carriers(
                        TICK_SECS,
                        &KinematicCharacterController::default(),
                        &shape,
                        &pose,
                        &carriers,
                        &passable,
                        excluded,
                        |_| {},
                    );
                    let solid = excluded.is_empty()
                        && (kind != ColliderKind::Barrier || passable.is_empty())
                        && (kind != ColliderKind::Bridge || powered);
                    assert_eq!(
                        movement.z > 0.05,
                        solid,
                        "kind {kind:?}, powered {powered}, passable {passable:?}, excluded {excluded:?}: {movement:?}"
                    );
                    if !solid {
                        assert_eq!(movement, Vector::ZERO);
                    }
                }
            }
        }
    }
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

#[test]
fn portal_shots_only_pass_blocking_barriers_when_the_kind_is_globally_open() {
    let mut layout = test_map_layout();
    layout.floors.clear();
    layout.ramps.clear();
    layout.barriers.push(Barrier {
        x1: 0.0,
        z1: 2.0,
        x2: 4.0,
        z2: 2.0,
        width: BARRIER_THICKNESS,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        level: 1,
        levels: 1,
        kind: BarrierKindId(0),
        carrier: CarrierId::WORLD,
    });
    let table = BarrierKindTable::from_ids(vec!["red".into(), "blue".into()]).expect("barrier catalog rejected");
    let world = CollisionWorld::from_map_layout(&layout, &table);
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 1.5, 4.0);
    for barriers_block in [false, true] {
        for light_bridges_block in [false, true] {
            let settings = PortalShotSettings {
                barriers_block,
                light_bridges_block,
            };
            for open in [vec![], vec![BarrierKindId(1)], vec![BarrierKindId(0)]] {
                let hit = world.portal_surface_along_ray(origin, Vec3::NEG_Z, 10.0, settings, &open);
                assert_eq!(hit.is_some(), !barriers_block || open.contains(&BarrierKindId(0)));
                if let Some(hit) = hit {
                    assert!(hit.point.z < 1.0, "portal landed on the barrier instead of the wall");
                }
            }
        }
    }
}

#[test]
fn portal_shots_only_stop_at_powered_bridges_when_configured() {
    let mut layout = test_map_layout();
    layout.walls.clear();
    layout.ramps.clear();
    layout.light_bridges.push(LightBridge {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT + 2.0,
        thickness: BRIDGE_THICKNESS,
        level: 2,
        kind: BridgeKindId(0),
        carrier: CarrierId::WORLD,
    });
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 4.0, 2.0);
    for powered in [false, true, false] {
        world.set_powered_bridges(if powered { &[BridgeKindId(0)] } else { &[] });
        for barriers_block in [false, true] {
            for light_bridges_block in [false, true] {
                let settings = PortalShotSettings {
                    barriers_block,
                    light_bridges_block,
                };
                let hit = world.portal_surface_along_ray(origin, Vec3::NEG_Y, 10.0, settings, &[]);
                assert_eq!(hit.is_some(), !powered || !light_bridges_block);
                if let Some(hit) = hit {
                    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-4, "portal landed on a bridge");
                }
            }
        }
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
            carrier: CarrierId::WORLD,
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
            carrier: CarrierId::WORLD,
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
        carrier: CarrierId::WORLD,
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
                carrier: CarrierId::WORLD,
            }],
            floors: vec![Floor {
                x1: -8.0,
                z1: -8.0,
                x2: 8.0,
                z2: 8.0,
                y: 0.0,
                thickness: FLOOR_THICKNESS,
                level: 0,
                carrier: CarrierId::WORLD,
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
            carrier: CarrierId::WORLD,
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
            carrier: CarrierId::WORLD,
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

fn slider_layout() -> MapLayout {
    use crate::protocol::Carrier;

    MapLayout {
        carriers: vec![Carrier {
            parent: CarrierId::WORLD,
            level: 1,
            levels: 0,
            from: Position {
                x: 0.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            to: Position {
                x: 8.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
        }],
        floors: vec![Floor {
            x1: -1.5,
            z1: -1.5,
            x2: 1.5,
            z2: 1.5,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId(1),
        }],
        ..Default::default()
    }
}

#[test]
fn world_surface_ray_names_the_carrier_it_hits() {
    let mut layout = slider_layout();
    layout.floors.push(Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: LEVEL_HEIGHT - 2.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    let above = bevy_math::Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);

    let hit = world
        .world_surface_along_ray(above, bevy_math::Vec3::NEG_Y, 4.0)
        .expect("the tile did not stop the ray");
    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(hit.carrier, CarrierId(1));

    let beside = bevy_math::Vec3::new(1.8, LEVEL_HEIGHT + 1.0, 0.0);
    let hit = world
        .world_surface_along_ray(beside, bevy_math::Vec3::NEG_Y, 4.0)
        .expect("the floor under the tile's edge was missed");
    assert!((hit.point.y - (LEVEL_HEIGHT - 2.0)).abs() < 1e-3, "hit was {hit:?}");
    assert_eq!(hit.carrier, CarrierId::WORLD);
}

#[test]
fn carrier_colliders_follow_the_carrier_pose() {
    use crate::map::Carriers;
    use rapier3d::prelude::Pose;

    let layout = slider_layout();
    let mut world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    assert_eq!(world.solid_kinds(), vec![ColliderKind::Floor]);
    let physics = wide_body();
    let shape = crate::physics::characters::character_shape(physics);
    let probe = |world: &CollisionWorld, x: f32| {
        let pose = Pose::translation(x, LEVEL_HEIGHT + physics.collider.bottom_y_offset() + 0.05, 0.0);
        world.ground_hit(&shape, &pose, 1.0, 0.0, &[], &[])
    };

    assert!(probe(&world, 0.0).is_some(), "the tile starts at its first end");
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(60);
    world.set_carrier_poses(&carriers);
    assert!(probe(&world, 0.0).is_none(), "the tile left its first end");
    assert!(probe(&world, 8.0).is_some(), "the tile arrived at its second end");
}

#[test]
fn ground_hit_names_the_carrier_under_the_feet() {
    use rapier3d::prelude::Pose;

    let mut layout = slider_layout();
    layout.floors.push(Floor {
        x1: 4.0,
        z1: -2.0,
        x2: 12.0,
        z2: 2.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout, &crate::protocol::BarrierKindTable::default());
    let physics = wide_body();
    let shape = crate::physics::characters::character_shape(physics);
    let probe = |x: f32| {
        let pose = Pose::translation(x, LEVEL_HEIGHT + physics.collider.bottom_y_offset() + 0.05, 0.0);
        world
            .ground_hit(&shape, &pose, 1.0, 0.0, &[], &[])
            .map(|hit| hit.carrier)
    };

    assert_eq!(probe(0.0), Some(CarrierId(1)));
    assert_eq!(probe(8.0), Some(CarrierId::WORLD));
}
