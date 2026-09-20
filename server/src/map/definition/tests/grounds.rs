use super::*;
use crate::map::definition::{
    compile_map,
    tests::{
        cell_def, compile_settings, empty_kind_table, level, map_with_zones, no_nested, surface_mesh, switch_table,
    },
};
use common::{
    physics::CollisionWorld,
    protocol::{Position, RampDirection, Wall},
};

#[test]
fn grounds_fit_a_small_base_below_an_obby_style_elevated_course() {
    let mut base = level(Vec::new());
    base.terrain = (10..13)
        .flat_map(|row| (0..3).map(move |col| cell_def(col, row)))
        .collect();
    let upper = level(vec![[1, 8], [24, 10]]);
    let mut map = map_with_zones(
        100,
        vec![base, level(Vec::new()), level(Vec::new()), level(Vec::new()), upper],
        Vec::new(),
        Vec::new(),
    );
    map.grid_rows = 40;
    let kinds = empty_kind_table();
    let mut settings = compile_settings(&kinds);
    settings.geometry = MapGeometryConfig {
        grid_cell_size: 4.0,
        level_height: 2.4,
        floor_thickness: 0.4,
        wall_thickness: 0.4,
    };
    settings.grounds = Some(GroundsSettings { level: 0 });
    let (layout, _) =
        compile_map(&map, 30, &settings, &no_nested(), &kinds, &switch_table(&kinds)).expect("small base compiles");
    let grounds = layout.grounds.as_ref().expect("grounds missing");
    for (x, z) in [(-200.2, -34.0), (-187.8, -34.0), (-194.0, -40.2), (-194.0, -27.8)] {
        assert!(
            grounds.distance_outside_footprint(x, z).abs() < 0.0001,
            "cutout includes the slab trim, not the grid"
        );
    }
    let world = CollisionWorld::from_map_layout(&layout);
    let beside_pad = bevy::math::Vec3::new(-187.0, 5.0, -34.0);
    let hit = world
        .ground_surface_below(beside_pad, 10.0)
        .expect("ground inside the overall grid");
    assert!(hit.point.y.abs() < 0.001);
    let terrain_only = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds.clone()),
        ..Default::default()
    });
    assert!(
        terrain_only
            .ground_surface_below(bevy::math::Vec3::new(-194.0, 5.0, -34.0), 10.0)
            .is_none()
    );

    // This case has a huge empty authoring grid; only the base-to-meadow
    // connection under test belongs in this navigation region.
    use crate::actors::{
        navigation::surface::{SurfaceBounds, SurfaceMesh},
        test_kinds,
    };
    let mesh = SurfaceMesh::bake_in(
        &world.collision_meshes().expect("collision export"),
        CarrierId::WORLD,
        test_kinds::physics(test_kinds::CONTACT),
        &[],
        Some(SurfaceBounds {
            min: bevy::math::Vec3::new(-205.0, -2.0, -45.0),
            max: bevy::math::Vec3::new(-175.0, 4.0, -23.0),
        }),
        &[],
    )
    .expect("base navigation mesh");
    let pad = common::protocol::Position {
        x: -194.0,
        y: 0.0,
        z: -34.0,
    };
    let meadow = common::protocol::Position {
        x: -182.0,
        y: 0.0,
        z: -34.0,
    };
    assert!(mesh.locate(meadow, 1.0).is_some());
    assert!(mesh.route(pad, meadow, 1.0).is_ok());
}

#[test]
fn irregular_bases_compile_walkable_outdoor_gaps_without_filling_enclosed_voids() {
    let mut base = level(Vec::new());
    for row in 1..10 {
        for col in 1..16 {
            let left = col < 9 && (row < 3 || col >= 7);
            let right = col >= 12 && !(col == 13 && (4..7).contains(&row));
            if left || right {
                base.terrain.push(cell_def(col, row));
            }
        }
    }
    let mut map = map_with_zones(20, vec![base], Vec::new(), Vec::new());
    map.grid_rows = 14;
    let kinds = empty_kind_table();
    let mut settings = compile_settings(&kinds);
    settings.grounds = Some(GroundsSettings { level: 0 });
    let (layout, config) =
        compile_map(&map, 30, &settings, &no_nested(), &kinds, &switch_table(&kinds)).expect("irregular base compiles");
    let geometry = config.root_grid().geometry;
    let center = |col, row| Position {
        x: geometry.cell_center_x(col),
        y: 0.0,
        z: geometry.cell_center_z(row),
    };
    let grounds = layout.grounds.as_ref().expect("grounds");
    let world = CollisionWorld::from_map_layout(&layout);
    let mesh = surface_mesh(&layout, &config, CarrierId::WORLD, &[]);
    let start = center(1, 1);
    for (col, row) in [(3, 7), (10, 5)] {
        let point = center(col, row);
        assert!(!grounds.is_inside_footprint(point.x, point.z));
        let hit = world
            .ground_surface_below(bevy::math::Vec3::new(point.x, 0.5, point.z), 1.0)
            .expect("ground in the outdoor gap");
        assert!(hit.point.y.abs() < 0.001);
        assert!(mesh.locate(point, 1.0).is_some(), "outdoor navigation surface");
        assert!(mesh.route(start, point, 1.0).is_ok());
    }
    let enclosed = center(13, 5);
    assert!(grounds.is_inside_footprint(enclosed.x, enclosed.z));
    assert!(
        world
            .ground_surface_below(bevy::math::Vec3::new(enclosed.x, 0.5, enclosed.z), 1.0)
            .is_none()
    );
}

#[test]
fn an_exterior_basement_ramp_is_not_capped_by_ground_infill() {
    let geometry = crate::test_geometry::sizes();
    let mut layout = MapLayout {
        floors: vec![Floor {
            x1: -2.0,
            x2: 8.0,
            z1: 0.0,
            z2: 8.0,
            y: geometry.level_y(1),
            thickness: geometry.floor_thickness,
            level: 1,
            carrier: CarrierId::WORLD,
        }],
        ramps: vec![Ramp {
            x1: -2.0,
            z1: -8.0,
            x2: 2.0,
            z2: 0.0,
            y: 0.0,
            height: geometry.level_y(1),
            direction: RampDirection::South,
            shape: RampShape::Solid,
            thickness: 0.4,
            level: 0,
            levels: 1,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let grounds = compile_grounds(&layout, &GroundsSettings { level: 1 }, geometry);
    assert!(grounds.is_inside_footprint(0.0, -4.0));
    assert!(!grounds.is_inside_footprint(6.0, -4.0));
    layout.grounds = Some(grounds);
    let world = CollisionWorld::from_map_layout(&layout);
    let ramp = world
        .ground_surface_below(bevy::math::Vec3::new(0.0, geometry.level_y(1) + 1.0, -4.0), 10.0)
        .expect("basement ramp");
    assert!(
        ramp.point.y < geometry.level_y(1) - 0.5,
        "no terrain seals the slope at ground level"
    );
    let beside = world
        .ground_surface_below(bevy::math::Vec3::new(6.0, geometry.level_y(1) + 1.0, -4.0), 10.0)
        .expect("ground beside the ramp");
    assert!((beside.point.y - geometry.level_y(1)).abs() < 0.001);
}

#[test]
fn a_plank_standing_on_the_grounds_leaves_them_under_it_while_a_wedge_or_a_ramp_from_below_cuts_them() {
    let geometry = crate::test_geometry::sizes();
    let ramp = |shape, level, levels| Ramp {
        x1: 20.0,
        z1: 0.0,
        x2: 24.0,
        z2: 8.0,
        y: geometry.level_y(level),
        height: f32::from(levels) * geometry.level_height,
        direction: RampDirection::South,
        shape,
        thickness: geometry.floor_thickness,
        level,
        levels,
        carrier: CarrierId::WORLD,
    };
    for (shape, level, levels, cut) in [
        (RampShape::Plank, 1, 1, false),
        (RampShape::Solid, 1, 1, true),
        (RampShape::Plank, 0, 2, true),
        (RampShape::Plank, 2, 1, false),
    ] {
        let layout = MapLayout {
            floors: vec![Floor {
                x1: 0.0,
                x2: 8.0,
                z1: 0.0,
                z2: 8.0,
                y: geometry.level_y(1),
                thickness: geometry.floor_thickness,
                level: 1,
                carrier: CarrierId::WORLD,
            }],
            ramps: vec![ramp(shape, level, levels)],
            ..Default::default()
        };
        let grounds = compile_grounds(&layout, &GroundsSettings { level: 1 }, geometry);
        assert_eq!(
            grounds.is_inside_footprint(22.0, 4.0),
            cut,
            "{shape:?} from level {level} over {levels}"
        );
    }
}

#[test]
fn wall_cutouts_meet_the_sides_and_end_caps_without_a_gap() {
    let geometry = crate::test_geometry::sizes();
    let layout = MapLayout {
        walls: vec![Wall {
            x1: -6.0,
            x2: 6.0,
            z1: 3.0,
            z2: 3.0,
            width: 0.4,
            y: 0.0,
            height: geometry.wall_height(),
            level: 0,
            carrier: CarrierId::WORLD,
        }],
        ..Default::default()
    };
    let grounds = compile_grounds(&layout, &GroundsSettings { level: 0 }, geometry);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds),
        ..Default::default()
    });
    for (x, z) in [(-6.01, 3.0), (6.01, 3.0), (0.0, 2.79), (0.0, 3.21)] {
        let hit = world
            .ground_surface_below(bevy::math::Vec3::new(x, 1.0, z), 2.0)
            .expect("terrain meets every wall face");
        assert!(hit.point.y.abs() < 0.001);
    }
    assert!(
        world
            .ground_surface_below(bevy::math::Vec3::new(0.0, 1.0, 3.0), 2.0)
            .is_none()
    );
}

#[test]
fn empty_ground_level_does_not_inherit_an_upper_floor_footprint() {
    let geometry = crate::test_geometry::sizes();
    let upper = Floor {
        x1: -5.0,
        x2: 5.0,
        z1: -5.0,
        z2: 5.0,
        y: geometry.level_y(2),
        thickness: geometry.floor_thickness,
        level: 2,
        carrier: CarrierId::WORLD,
    };
    let layout = MapLayout {
        floors: vec![
            upper,
            Floor {
                level: 0,
                carrier: CarrierId(1),
                ..upper
            },
        ],
        ..Default::default()
    };
    let grounds = compile_grounds(&layout, &GroundsSettings { level: 0 }, geometry);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds),
        ..Default::default()
    });
    let hit = world
        .ground_surface_below(bevy::math::Vec3::new(0.0, 5.0, 0.0), 10.0)
        .expect("filled base beneath the upper floor");
    assert!(hit.point.y.abs() < 0.001);
}
