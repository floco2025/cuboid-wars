use crate::protocol::{BarrierId, BridgeId};
use std::f32::consts::FRAC_PI_2;

use super::*;
use crate::{
    constants::PORTAL_LIGHT_CLEARANCE,
    protocol::{Barrier, BridgeKindId, LightBridge, PressurePlate, SwitchId, WallLight},
    test_geometry::{BARRIER_THICKNESS, BRIDGE_THICKNESS},
};

#[test]
fn placement_accepts_a_clear_wall_center() {
    let layout = placement_layout();
    let placement =
        place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI).expect("clear wall center rejected");
    assert!((placement.normal - Vec3::Z).length() < 1e-4);
}

#[test]
fn opening_a_barrier_exposes_a_fitting_portal_surface_behind_it() {
    let mut layout = placement_layout();
    layout.barriers.push(Barrier {
        id: Default::default(),

        switch: None,
        switch_inverted: false,

        x1: -6.0,
        z1: 2.0,
        x2: 6.0,
        z2: 2.0,
        width: BARRIER_THICKNESS,
        y: 0.0,
        height: WALL_HEIGHT,
        level: 0,
        levels: 1,
        kind: BarrierKindId(0),
        carrier: CarrierId::WORLD,
    });
    let kinds = BarrierKindTable::from_ids(vec!["gate".into(), "other".into()]).expect("barrier catalog rejected");
    let world = CollisionWorld::from_map_layout(&layout, &kinds);
    for open in [vec![], vec![BarrierId(1)], vec![BarrierId(0)], vec![]] {
        let placement = place_on_geometry(
            Vec3::new(0.0, 1.6, 4.0),
            Vec3::NEG_Z,
            0.0,
            10.0,
            &world,
            &layout,
            &Carriers::default(),
            &open,
        );
        assert_eq!(placement.is_some(), open.contains(&BarrierId(0)));
        if let Some(placement) = placement {
            assert!((placement.pos.z - WALL_THICKNESS / 2.0).abs() < 1e-4);
            assert!(placement.normal.abs_diff_eq(Vec3::Z, 1e-4));
        }
    }
}

#[test]
fn bridge_power_controls_portal_placement_on_the_floor_and_ceiling_beyond_it() {
    let floor = Floor {
        x1: -3.0,
        z1: -3.0,
        x2: 3.0,
        z2: 3.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let ceiling_y = 2.0 * LEVEL_HEIGHT;
    let layout = MapLayout {
        floors: vec![
            floor,
            Floor {
                y: ceiling_y,
                level: 2,
                ..floor
            },
        ],
        light_bridges: vec![LightBridge {
            id: Default::default(),
            switch: None,
            switch_inverted: false,

            x1: -3.0,
            z1: -3.0,
            x2: 3.0,
            z2: 3.0,
            y: LEVEL_HEIGHT,
            thickness: BRIDGE_THICKNESS,
            level: 1,
            kind: BridgeKindId(0),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    for powered in [false, true, false] {
        world.set_powered_bridges(if powered { &[BridgeId(0)] } else { &[] });
        for (origin_y, direction, surface_y) in [
            (LEVEL_HEIGHT + 1.5, Vec3::NEG_Y, 0.0),
            (LEVEL_HEIGHT - 1.5, Vec3::Y, ceiling_y - FLOOR_THICKNESS),
        ] {
            let placement = place_on_geometry(
                Vec3::Y * origin_y,
                direction,
                0.0,
                20.0,
                &world,
                &layout,
                &Carriers::default(),
                &[],
            );
            assert_eq!(placement.is_some(), !powered);
            if let Some(placement) = placement {
                assert!((placement.pos.y - surface_y).abs() < 1e-4);
                assert!(placement.normal.abs_diff_eq(-direction, 1e-4));
            }
        }
    }
}

#[test]
fn placement_rejects_overlap_with_another_portal() {
    let placement = PortalPlacement {
        pos: Vec3::new(0.0, 1.6, 0.0),
        normal: Vec3::Z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let existing = [portal(PortalEnd::A, Vec3::new(0.5, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(portal_placement_overlaps(
        &placement.portal(PortalPairId(2), PortalEnd::B, &Carriers::default()),
        &existing,
        &Carriers::default()
    ));
}

#[test]
fn placement_allows_clear_space_and_replacing_its_own_end() {
    let placement = PortalPlacement {
        pos: Vec3::new(0.0, 1.6, 0.0),
        normal: Vec3::Z,
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    };
    let clear = [portal(PortalEnd::A, Vec3::new(2.0, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(!portal_placement_overlaps(
        &placement.portal(PortalPairId(2), PortalEnd::B, &Carriers::default()),
        &clear,
        &Carriers::default()
    ));

    let replaced = [portal(PortalEnd::B, Vec3::new(0.0, 1.6, 0.0), Vec3::Z, 0.0)];
    assert!(!portal_placement_overlaps(
        &placement.portal(PortalPairId(1), PortalEnd::B, &Carriers::default()),
        &replaced,
        &Carriers::default()
    ));
}

#[test]
fn low_wall_shot_nudges_up_until_the_aperture_fits() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 0.5, 0.0), PI)
        .expect("low shot did not nudge up onto the wall");
    assert!(placement.pos.y > 1.3);
    assert!(placement.pos.x.abs() < 0.3);
}

#[test]
fn high_wall_shot_nudges_until_the_visible_rim_has_backing() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(0.0, 2.7, 3.0), Vec3::new(0.0, 2.7, 0.0), PI)
        .expect("high wall shot did not nudge below the wall top");

    assert!(placement.pos.y + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE < WALL_HEIGHT);
}

#[test]
fn ramp_side_portal_rim_can_meet_the_slope() {
    let ramp_length = 6.0;
    let slope = LEVEL_HEIGHT / ramp_length;
    let z = 1.5;
    let surface_y = slope * z;
    let rim_half_height = PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    let rim_half_width = PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE;
    let ellipse_support = (rim_half_height.powi(2) + (slope * rim_half_width).powi(2)).sqrt();
    let center_y = surface_y + ellipse_support + 0.01;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -2.0,
            z1: 0.0,
            x2: -2.0,
            z2: ramp_length,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let placement = place(&layout, Vec3::new(0.0, center_y, z), Vec3::new(-2.0, center_y, z), 0.0)
        .expect("ramp-side portal placement failed");

    assert!((placement.pos.y - center_y).abs() < 0.03, "{placement:?}");
    assert!((placement.pos.z - z).abs() < 0.03, "{placement:?}");
}

#[test]
fn wall_portal_near_ramp_excludes_only_wall_backing() {
    let ramp_length = 6.0;
    let slope = LEVEL_HEIGHT / ramp_length;
    let z = 1.5;
    let rim_half_height = PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    let rim_half_width = PORTAL_HALF_WIDTH * PORTAL_RIM_SCALE;
    let ellipse_support = (rim_half_height.powi(2) + (slope * rim_half_width).powi(2)).sqrt();
    let center_y = slope * z + ellipse_support + 0.01;
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -2.0,
            z1: 0.0,
            x2: -2.0,
            z2: ramp_length,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(-1.85, center_y, z), Vec3::X, 0.0),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(-1.5, center_y - physics.movement_collider.height / 2.0, z);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 1);
}

#[test]
fn wall_portal_across_a_stacked_wall_opens_its_trim_strip() {
    let wall = |level: u8, y: f32| Wall {
        x1: -3.0,
        z1: 0.0,
        x2: 3.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level,
        y,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        walls: vec![wall(0, 0.0), wall(1, LEVEL_HEIGHT)],
        floors: vec![Floor {
            x1: -3.0,
            z1: -WALL_THICKNESS / 2.0,
            x2: 3.0,
            z2: WALL_THICKNESS / 2.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(
                PortalEnd::A,
                Vec3::new(0.0, LEVEL_HEIGHT, -WALL_THICKNESS / 2.0),
                -Vec3::Z,
                0.0,
            ),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(0.0, LEVEL_HEIGHT - physics.movement_collider.height / 2.0, -0.5);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 3);
}

#[test]
fn wall_portal_keeps_the_floor_it_stands_on_solid() {
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -3.0,
            z1: 0.0,
            x2: 3.0,
            z2: 0.0,
            width: WALL_THICKNESS,
            level: 0,
            y: 0.0,
            height: WALL_HEIGHT,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -4.0,
            z1: -4.0,
            x2: 4.0,
            z2: 4.0,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let set = PortalSet::rebuild(
        &[
            portal(PortalEnd::A, Vec3::new(0.0, 1.0, -WALL_THICKNESS / 2.0), -Vec3::Z, 0.0),
            portal(PortalEnd::B, Vec3::new(10.0, 1.6, 10.0), Vec3::Z, 0.0),
        ],
        &world,
        &Carriers::default(),
    );
    let physics = player_physics();
    let origin = Vec3::new(0.0, 1.0 - physics.movement_collider.height / 2.0, -0.5);

    assert_eq!(set.collision_exclusions(origin, physics).len(), 1);
}

#[test]
fn shot_past_the_walls_end_nudges_back_onto_it() {
    let layout = placement_layout();
    let placement = place(&layout, Vec3::new(5.9, 1.6, 3.0), Vec3::new(5.9, 1.6, 0.0), PI)
        .expect("edge shot did not nudge back onto the wall");
    assert!(placement.pos.x < 5.45);
}

#[test]
fn ramp_lip_shot_nudges_the_whole_aperture_onto_the_slope() {
    let ramp_length = 6.0;
    let layout = MapLayout {
        ramps: vec![Ramp {
            x1: -2.0,
            y1: 0.0,
            z1: 0.0,
            x2: 2.0,
            y2: LEVEL_HEIGHT,
            z2: ramp_length,
            carrier: CarrierId::WORLD,
        }],
        floors: vec![Floor {
            x1: -4.0,
            z1: ramp_length,
            x2: 4.0,
            z2: 12.0,
            y: LEVEL_HEIGHT,
            thickness: FLOOR_THICKNESS,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let slope = LEVEL_HEIGHT / ramp_length;
    let target = Vec3::new(0.0, slope * 5.05, 5.05);
    let normal = Vec3::new(0.0, 1.0, -slope).normalize();
    let placement = place(&layout, target + normal * 3.0, target, 0.0).expect("ramp-lip shot did not nudge");
    let frame = PortalFrame::from_surface(placement.pos, placement.normal, placement.yaw);

    assert!((frame.center + frame.up * PORTAL_HALF_HEIGHT).z <= ramp_length);
}

#[test]
fn floor_shot_under_a_crossing_wall_nudges_clear_of_it() {
    let mut layout = placement_layout();
    // A second wall crossing the floor at z = 3.2 cuts any aperture that
    // straddles it — including between rim probes.
    layout.walls.push(Wall {
        x1: -6.0,
        z1: 3.2,
        x2: 6.0,
        z2: 3.2,
        width: WALL_THICKNESS,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(2.0, 1.6, 2.5), Vec3::new(2.0, 0.0, 2.5), 0.0)
        .expect("wall-cut floor shot did not nudge clear");
    assert!(placement.pos.z < 1.68);
    assert!((placement.pos.x - 2.0).abs() < 0.01);
}

#[test]
fn shot_with_no_fitting_spot_anywhere_fizzles() {
    let mut layout = placement_layout();
    // A 1 m stub wall can never back the 1.4 m aperture, nudged or not.
    layout.walls[0].x1 = -0.5;
    layout.walls[0].x2 = 0.5;
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI);
    assert!(placement.is_none());
}

#[test]
fn shot_at_a_wall_light_nudges_clear_of_it() {
    let mut layout = placement_layout();
    layout.wall_lights.push(WallLight {
        kind: "test-light".into(),
        pos: Position { x: 0.0, y: 1.6, z: 0.2 },
        yaw: 0.0,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI)
        .expect("shot at the light did not nudge clear");
    // The nudged aperture leaves the light outside its grown keep-out oval.
    let across = placement.pos.x / (PORTAL_HALF_WIDTH + PORTAL_LIGHT_CLEARANCE);
    let along_up = (placement.pos.y - 1.6) / (PORTAL_HALF_HEIGHT + PORTAL_LIGHT_CLEARANCE);
    assert!(across * across + along_up * along_up > 0.99);
    // Far enough along the wall the light does not even nudge the shot.
    let clear = place(&layout, Vec3::new(4.0, 1.6, 3.0), Vec3::new(4.0, 1.6, 0.0), PI).expect("clear shot rejected");
    assert!((clear.pos.x - 4.0).abs() < 0.01);
}

#[test]
fn wall_light_on_the_other_face_does_not_block_placement() {
    let mut layout = placement_layout();
    layout.wall_lights.push(WallLight {
        kind: "test-light".into(),
        pos: Position {
            x: 0.0,
            y: 1.6,
            z: -0.17,
        },
        yaw: PI,
        carrier: CarrierId::WORLD,
    });
    let placement = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), PI)
        .expect("opposite-face light rejected placement");

    assert!(placement.pos.x.abs() < 0.01);
    assert!((placement.pos.y - 1.6).abs() < 0.01);
}

// A powered bridge slab crossing the oval is as solid as a floor to a
// traveller, so front clearance must see it; backing stays bridge-blind.
#[test]
fn placement_front_clearance_rejects_a_powered_light_bridge() {
    let mut layout = placement_layout();
    layout.walls.push(Wall {
        x1: -6.0,
        z1: 0.0,
        x2: 6.0,
        z2: 0.0,
        width: WALL_THICKNESS,
        level: 1,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    layout.light_bridges.push(LightBridge {
        id: Default::default(),
        switch: None,
        switch_inverted: false,

        x1: -6.0,
        z1: 0.0,
        x2: 6.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        level: 1,
        kind: BridgeKindId(0),
        thickness: BRIDGE_THICKNESS,
        carrier: CarrierId::WORLD,
    });
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let origin = Vec3::new(0.0, 3.7, 3.0);
    let aim = Vec3::new(0.0, 3.7, 0.0);
    let shoot = |world: &CollisionWorld| {
        place_on_geometry(
            origin,
            (aim - origin).normalize(),
            PI,
            40.0,
            world,
            &layout,
            &Carriers::default(),
            &[],
        )
    };

    let ghost = shoot(&world).expect("an unpowered bridge blocked the shot");
    assert!(
        (ghost.pos.y - aim.y).abs() < 1e-3,
        "ghost bridge moved the portal to {ghost:?}"
    );

    world.set_powered_bridges(&[BridgeId(0)]);
    let solid = shoot(&world).expect("no fitting spot below the powered bridge");
    let rim_top = solid.pos.y + PORTAL_HALF_HEIGHT * PORTAL_RIM_SCALE;
    assert!(
        rim_top <= LEVEL_HEIGHT - BRIDGE_THICKNESS,
        "portal rim at {rim_top} still crosses the powered bridge"
    );
}

#[test]
fn placement_rejects_a_floor_portal_covering_a_pressure_plate() {
    let mut layout = placement_layout();
    layout.pressure_plates.push(PressurePlate {
        level: 0,
        center_x: 3.0,
        center_z: 3.0,
        switch: SwitchId(0),
        center_y: 0.0,
        carrier: CarrierId::WORLD,
    });
    assert!(place(&layout, Vec3::new(3.0, 1.6, 3.0), Vec3::new(3.0, 0.0, 3.0), 0.0).is_none());
    // The same shot well away from the plate lands.
    assert!(place(&layout, Vec3::new(-3.0, 1.6, 3.0), Vec3::new(-3.0, 0.0, 3.0), 0.0).is_some());
}

#[test]
fn vertical_placement_yaw_snaps_to_quarter_turns() {
    let layout = placement_layout();
    let floor = place(&layout, Vec3::new(-3.0, 1.6, 3.0), Vec3::new(-3.0, 0.0, 3.0), 1.0).expect("floor shot rejected");
    assert!((floor.yaw - FRAC_PI_2).abs() < 1e-4);
    let wall = place(&layout, Vec3::new(0.0, 1.6, 3.0), Vec3::new(0.0, 1.6, 0.0), 1.0).expect("wall shot rejected");
    assert!((wall.yaw - 1.0).abs() < 1e-4);
}
