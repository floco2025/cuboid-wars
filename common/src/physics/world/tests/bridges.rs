use super::*;
use crate::physics::passable_fields;

const BRIDGE: FieldId = FieldId(0);

#[test]
fn a_light_bridge_supports_a_character_unless_it_is_passable() {
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: 0.0,
            z1: 0.0,
            x2: 4.0,
            z2: 4.0,
            y: LEVEL_HEIGHT,
            level: 1,
            field: FieldId(0),
            thickness: BRIDGE_THICKNESS,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    assert_eq!(world.solid_kinds(), vec![ColliderKind::Bridge]);

    let shape = character_movement_shape(wide_body());
    let pose = Pose::translation(2.0, LEVEL_HEIGHT + 0.0 + 0.05, 2.0);
    let probe = |passable: &[FieldId]| world.ground_hit(&shape, &pose, 1.0, 0.0, passable, &[]);

    assert!(probe(&[]).is_some(), "a bridge that is on is ground");
    assert!(probe(&[BRIDGE]).is_none(), "a passable bridge is not ground");
    assert!(
        probe(&[FieldId(1), FieldId(2)]).is_some(),
        "another field being passable leaves this bridge solid"
    );
    assert!(
        probe(&passable_fields(&[FieldId(0)], &[])).is_none(),
        "the key of its field drops its holder through"
    );
    assert!(probe(&passable_fields(&[FieldId(1)], &[])).is_some());
}

#[test]
fn a_solid_light_bridge_stays_out_of_sight_and_ground_probes() {
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -2.0,
            z1: -2.0,
            x2: 2.0,
            z2: 2.0,
            y: LEVEL_HEIGHT,
            level: 1,
            field: FieldId(0),
            thickness: BRIDGE_THICKNESS,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let above = Vec3::new(0.0, LEVEL_HEIGHT + 1.0, 0.0);
    let below = Vec3::new(0.0, LEVEL_HEIGHT - 1.0, 0.0);

    assert!(
        world.cast_moving_ball(above, below - above, 0.1, &[]).is_some(),
        "a surface query sees it"
    );
    assert!(
        world.cast_moving_ball(above, below - above, 0.1, &[BRIDGE]).is_none(),
        "unless it is off"
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
fn a_solid_bridge_blocks_attacks_and_beams_without_blocking_awareness() {
    let layout = MapLayout {
        light_bridges: vec![LightBridge {
            x1: -3.0,
            z1: -3.0,
            x2: 3.0,
            z2: 3.0,
            y: 2.0,
            thickness: BRIDGE_THICKNESS,
            level: 1,
            field: FieldId(0),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    for solid in [false, true] {
        let open: &[FieldId] = if solid { &[] } else { &[BRIDGE] };
        for (from, to) in [(Vec3::Y * 4.0, Vec3::ZERO), (Vec3::ZERO, Vec3::Y * 4.0)] {
            assert!(world.line_of_sight_clear(from, to));
            assert_eq!(world.attack_path_clear(from, to, open), !solid);
            let hit = world.attack_surface_along_ray(from, to - from, 4.0, open);
            assert_eq!(hit.is_some(), solid);
            if let Some(hit) = hit {
                assert!(hit.point.y <= 2.0 && hit.point.y >= 2.0 - BRIDGE_THICKNESS - 1e-4);
            }
            assert_eq!(world.projectile_path_clear(from, to - from, 0.1, open), !solid);
        }
    }
}

#[test]
fn portal_shots_only_stop_at_solid_bridges() {
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
        field: FieldId(0),
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 4.0, 2.0);
    for solid in [false, true] {
        let open: &[FieldId] = if solid { &[] } else { &[BRIDGE] };
        let hit = world.portal_surface_along_ray(origin, Vec3::NEG_Y, 10.0, open);
        assert_eq!(hit.is_some(), !solid);
        if let Some(hit) = hit {
            assert!((hit.point.y - LEVEL_HEIGHT).abs() < 1e-4, "portal landed on a bridge");
        }
    }
}
