use super::*;

#[test]
fn light_bridge_supports_a_character_only_while_powered() {
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
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    assert_eq!(world.solid_kinds(), vec![ColliderKind::Bridge]);

    let shape = character_movement_shape(wide_body());
    let pose = Pose::translation(2.0, LEVEL_HEIGHT + 0.0 + 0.05, 2.0);
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
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    world.set_powered_bridges(&[BridgeKindId(0)]);
    let above = Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);
    let below = Vec3::new(0.0, LEVEL_HEIGHT - 1.0, 0.0);

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
        world.world_surface_along_ray(above, Vec3::NEG_Y, 2.0).is_none(),
        "the world ray ignores it"
    );
}

#[test]
fn bridge_power_blocks_attacks_and_beams_without_blocking_awareness() {
    let kind = BridgeKindId(0);
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -3.0,
            z1: -3.0,
            x2: 3.0,
            z2: 3.0,
            y: 2.0,
            thickness: BRIDGE_THICKNESS,
            level: 1,
            kind,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    for powered in [false, true, false] {
        let powered_kinds = [kind];
        world.set_powered_bridges(if powered { &powered_kinds } else { &[] });
        for (from, to) in [(Vec3::Y * 4.0, Vec3::ZERO), (Vec3::ZERO, Vec3::Y * 4.0)] {
            assert!(world.line_of_sight_clear(from, to));
            assert_eq!(world.attack_path_clear(from, to, &[]), !powered);
            let hit = world.attack_surface_along_ray(from, to - from, 4.0, &[]);
            assert_eq!(hit.is_some(), powered);
            if let Some(hit) = hit {
                assert!(hit.point.y <= 2.0 && hit.point.y >= 2.0 - BRIDGE_THICKNESS - 1e-4);
            }
            assert_eq!(world.projectile_path_clear(from, to - from, 0.1, &[]), !powered);
        }
    }
}

#[test]
fn portal_shots_only_stop_at_powered_bridges() {
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
        let hit = world.portal_surface_along_ray(origin, Vec3::NEG_Y, 10.0, &[]);
        assert_eq!(hit.is_some(), !powered);
        if let Some(hit) = hit {
            assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-4, "portal landed on a bridge");
        }
    }
}
