use super::*;
use crate::{
    map::Carriers,
    protocol::{CarrierId, FaceMaterials, PressurePlate, Ramp, RampDirection, RampShape, SwitchId, SwitchState},
};

#[test]
fn ground_surface_below_hits_floor_instead_of_wall_top() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());

    let hit = world
        .ground_surface_below(Vec3::new(2.0, LEVEL_HEIGHT + WALL_HEIGHT + 1.0, 0.0), WALL_HEIGHT + 2.0)
        .expect("expected floor below the wall");

    assert!((hit.point.y - LEVEL_HEIGHT).abs() < 0.001, "hit was {hit:?}");
    assert_eq!(hit.normal, Vec3::Y);
}

#[test]
fn ground_surface_below_returns_ramp_normal() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());

    let hit = world
        .ground_surface_below(Vec3::new(2.0, LEVEL_HEIGHT + 2.0, 6.0), LEVEL_HEIGHT + 2.0)
        .expect("expected ramp below the ray");

    assert!(hit.normal.y > 0.1, "hit was {hit:?}");
    assert_ne!(hit.normal, Vec3::Y);
}

#[test]
fn ramp_surfaces_rise_toward_their_direction_whatever_the_footprint() {
    for (x2, z2) in [(4.0, 4.0), (8.0, 2.0)] {
        for (direction, toward) in [
            (RampDirection::North, Vec3::NEG_Z),
            (RampDirection::South, Vec3::Z),
            (RampDirection::East, Vec3::X),
            (RampDirection::West, Vec3::NEG_X),
        ] {
            let ramp = Ramp {
                x1: 0.0,
                z1: 0.0,
                x2,
                z2,
                y: 0.0,
                height: 2.0,
                direction,
                shape: RampShape::Solid,
                thickness: 0.4,
                level: 0,
                levels: 1,
                carrier: CarrierId::WORLD,
            };
            let world = CollisionWorld::from_map_layout(&MapLayout {
                ramps: vec![ramp],
                ..Default::default()
            });
            let probe = |point: Vec3| {
                world
                    .ground_surface_below(point.with_y(5.0), 6.0)
                    .expect("ramp missing below the probe")
                    .point
                    .y
            };
            let center = Vec3::new(x2 / 2.0, 0.0, z2 / 2.0);
            let (low, high) = (center - toward * 0.5, center + toward * 0.5);

            assert!(
                (probe(low) - ramp.surface_at(low.x, low.z)).abs() < 0.001,
                "{direction:?}"
            );
            assert!(probe(high) > probe(low) + 0.1, "{direction:?}");
        }
    }
}

#[test]
fn ground_surface_below_returns_none_over_void() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());

    assert!(
        world
            .ground_surface_below(Vec3::new(20.0, LEVEL_HEIGHT, 20.0), LEVEL_HEIGHT)
            .is_none()
    );
}

#[test]
fn world_surface_along_ray_hits_wall_between_points() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());

    let hit = world
        .world_surface_along_ray(Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0), Vec3::NEG_Z, 4.0)
        .expect("expected the wall to intercept the ray");

    assert!((hit.point.z - WALL_THICKNESS / 2.0).abs() < 0.001, "hit was {hit:?}");
}

#[test]
fn world_surface_along_ray_hits_floor_unlike_wall_filter() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0);

    // A downward-pitched beam must clip at the floor; the walls-only filter
    // would let it pierce through.
    assert!(world.world_surface_along_ray(origin, Vec3::NEG_Y, 3.0).is_some());
    assert!(world.wall_surface_along_ray(origin, Vec3::NEG_Y, 3.0).is_none());
}

#[test]
fn world_surface_along_ray_returns_none_in_the_open() {
    let world = CollisionWorld::from_map_layout(&test_map_layout());

    assert!(
        world
            .world_surface_along_ray(Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0), Vec3::Z, 4.0)
            .is_none()
    );
}

#[test]
fn world_surfaces_along_ray_lists_each_solid_entered_nearest_first_and_no_field() {
    let mut layout = test_map_layout();
    layout.walls.push(Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 4.0,
        z2: -2.0,
        width: WALL_THICKNESS,
        level: 1,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    });
    layout.barriers.push(Barrier {
        id: Default::default(),
        switch: None,
        initially_on: true,
        x1: 0.0,
        z1: 2.0,
        x2: 4.0,
        z2: 2.0,
        level: 1,
        levels: 1,
        kind: FieldKindId(0),
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        width: BARRIER_THICKNESS,
        carrier: CarrierId::WORLD,
    });
    layout.light_bridges.push(LightBridge {
        id: Default::default(),
        switch: None,
        initially_on: true,
        x1: 0.0,
        z1: 2.5,
        x2: 4.0,
        z2: 3.5,
        y: LEVEL_HEIGHT + 1.0,
        level: 1,
        kind: FieldKindId(0),
        thickness: BRIDGE_THICKNESS,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 1.0 - BRIDGE_THICKNESS * 0.25, 6.0);

    // The solid bridge is the first thing a shot would meet.
    let cover = world
        .attack_surface_along_ray(origin, Vec3::NEG_Z, 10.0, &[])
        .expect("expected the bridge to be cover");
    assert!((cover.point.z - 3.5).abs() < 0.001, "cover was {cover:?}");

    let hits = world.world_surfaces_along_ray(origin, Vec3::NEG_Z, 10.0);
    let entries: Vec<f32> = hits.iter().map(|hit| hit.point.z).collect();
    assert_eq!(entries.len(), 2, "hits were {hits:?}");
    assert!((entries[0] - WALL_THICKNESS / 2.0).abs() < 0.001, "hits were {hits:?}");
    assert!(
        (entries[1] + 2.0 - WALL_THICKNESS / 2.0).abs() < 0.001,
        "hits were {hits:?}"
    );
    assert!(hits.iter().all(|hit| hit.normal == Vec3::Z), "hits were {hits:?}");

    // Down through the floor slab onto the ramp: entry faces only.
    let hits = world.world_surfaces_along_ray(Vec3::new(2.0, LEVEL_HEIGHT + 2.0, 2.0), Vec3::NEG_Y, 10.0);
    let heights: Vec<f32> = hits.iter().map(|hit| hit.point.y).collect();
    assert_eq!(heights.len(), 2, "hits were {hits:?}");
    assert!((heights[0] - LEVEL_HEIGHT).abs() < 0.001, "hits were {hits:?}");
    assert!((heights[1] - LEVEL_HEIGHT / 4.0).abs() < 0.01, "hits were {hits:?}");
}

#[test]
fn world_surfaces_along_ray_skips_a_pressure_plate_lying_on_its_floor() {
    let mut layout = test_map_layout();
    layout.pressure_plates.push(PressurePlate {
        level: 1,
        center_x: 2.0,
        center_y: LEVEL_HEIGHT,
        center_z: 2.0,
        side: 1.0,
        switch: SwitchId(0),
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 0.05, 3.5);

    // The plate stands in the way of a body at foot height.
    let blocked = world
        .world_surface_along_ray(origin, Vec3::NEG_Z, 3.0)
        .expect("plate missing from the world query");
    assert!((blocked.point.z - 2.5).abs() < 0.001, "hit was {blocked:?}");

    assert!(world.world_surfaces_along_ray(origin, Vec3::NEG_Z, 3.0).is_empty());
}

#[test]
fn wall_surface_along_ray_ignores_barrier() {
    let mut layout = test_map_layout();
    layout.barriers.push(Barrier {
        id: Default::default(),

        switch: None,
        initially_on: true,

        x1: 0.0,
        z1: 1.0,
        x2: 4.0,
        z2: 1.0,
        level: 1,
        levels: 1,
        kind: FieldKindId(0),
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        width: BARRIER_THICKNESS,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);

    let hit = world
        .wall_surface_along_ray(Vec3::new(2.0, LEVEL_HEIGHT + 1.0, 2.0), Vec3::NEG_Z, 3.0)
        .expect("expected wall behind the barrier");

    assert!((hit.point.z - WALL_THICKNESS / 2.0).abs() < 0.001, "hit was {hit:?}");
    assert_eq!(hit.normal, Vec3::Z);
}

#[test]
fn portal_shots_only_pass_blocking_barriers_when_the_kind_is_globally_open() {
    let mut layout = test_map_layout();
    layout.floors.clear();
    layout.ramps.clear();
    layout.barriers.push(Barrier {
        id: Default::default(),

        switch: None,
        initially_on: true,

        x1: 0.0,
        z1: 2.0,
        x2: 4.0,
        z2: 2.0,
        width: BARRIER_THICKNESS,
        y: LEVEL_HEIGHT,
        height: WALL_HEIGHT,
        level: 1,
        levels: 1,
        kind: FieldKindId(0),
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&layout);
    let origin = Vec3::new(2.0, LEVEL_HEIGHT + 1.5, 4.0);
    for open in [
        vec![],
        vec![FieldId::Barrier(BarrierId(1))],
        vec![FieldId::Barrier(BarrierId(0))],
    ] {
        let hit = world.portal_surface_along_ray(origin, Vec3::NEG_Z, 10.0, &open);
        assert_eq!(hit.is_some(), open.contains(&FieldId::Barrier(BarrierId(0))));
        if let Some(hit) = hit {
            assert!(hit.point.z < 1.0, "portal landed on the barrier instead of the wall");
        }
    }
}

#[test]
fn barriers_are_transparent_cover_until_globally_opened() {
    let barrier = FieldId::Barrier(BarrierId(0));
    let layout = MapLayout {
        barriers: vec![Barrier {
            id: Default::default(),

            switch: None,
            initially_on: true,

            x1: -3.0,
            z1: 0.0,
            x2: 3.0,
            z2: 0.0,
            y: 0.0,
            height: 4.0,
            width: BARRIER_THICKNESS,
            level: 0,
            levels: 1,
            kind: FieldKindId(0),
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    for (from, to) in [
        (Vec3::new(0.0, 1.0, -2.0), Vec3::new(0.0, 1.0, 2.0)),
        (Vec3::new(0.0, 1.0, 2.0), Vec3::new(0.0, 1.0, -2.0)),
    ] {
        assert!(world.line_of_sight_clear(from, to));
        for open in [&[][..], &[FieldId::Barrier(BarrierId(1))], &[barrier]] {
            let blocked = !open.contains(&barrier);
            assert_eq!(!world.attack_path_clear(from, to, open), blocked);
            assert_eq!(
                world.attack_surface_along_ray(from, to - from, 4.0, open).is_some(),
                blocked
            );
            assert_eq!(!world.projectile_path_clear(from, to - from, 0.1, open), blocked);
        }
    }
}
#[test]
fn support_rays_report_top_materials_on_floors_ramps_and_walls() {
    let mut layout = test_map_layout();
    layout.floor_materials = vec![FaceMaterials::uniform("floor")];
    layout.wall_materials = vec![FaceMaterials::uniform("wall")];
    layout.ramp_materials = vec![FaceMaterials::uniform("ramp")];
    let world = CollisionWorld::from_map_layout(&layout);
    for (origin, expected) in [
        (Vec3::new(2.0, LEVEL_HEIGHT + 0.2, 2.0), "floor"),
        (Vec3::new(2.0, LEVEL_HEIGHT + 2.0, 6.0), "ramp"),
        (Vec3::new(2.0, LEVEL_HEIGHT + WALL_HEIGHT + 0.2, 0.0), "wall"),
    ] {
        let hit = world
            .support_surface_on_carrier(origin, 10.0, CarrierId::WORLD, &[])
            .expect("support surface missing");
        assert_eq!(world.surface_material(&hit, &layout), Some(expected));
        assert!(
            world
                .support_surface_on_carrier(origin, 10.0, CarrierId(1), &[])
                .is_none()
        );
    }
}

#[test]
fn support_material_follows_a_moving_carrier() {
    let mut layout = slider_layout();
    layout.floor_materials = vec![FaceMaterials::uniform("steel")];
    let mut world = CollisionWorld::from_map_layout(&layout);
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(60, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    assert!(
        world
            .support_surface_on_carrier(Vec3::new(0.0, LEVEL_HEIGHT + 0.1, 0.0), 1.0, CarrierId(1), &[])
            .is_none()
    );
    let hit = world
        .support_surface_on_carrier(Vec3::new(8.0, LEVEL_HEIGHT + 0.1, 0.0), 1.0, CarrierId(1), &[])
        .expect("moved support missing");
    assert_eq!(world.surface_material(&hit, &layout), Some("steel"));
}
