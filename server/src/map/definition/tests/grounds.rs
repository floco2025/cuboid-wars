use super::*;
use crate::map::definition::{
    compile_map,
    tests::{
        cell_def, compile_settings, empty_kind_table, level, map_with_zones, no_bridges, no_nested, player_zone,
        switch_table,
    },
};
use common::{physics::CollisionWorld, protocol::Wall};

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
        vec![player_zone(4, 1, 8)],
        Vec::new(),
    );
    map.grid_rows = 40;
    let kinds = empty_kind_table();
    let bridges = no_bridges();
    let mut settings = compile_settings(&kinds, &bridges);
    settings.geometry = MapGeometryConfig {
        grid_cell_size: 4.0,
        level_height: 2.4,
        floor_thickness: 0.4,
        wall_thickness: 0.4,
    };
    settings.grounds = Some(GroundsSettings { level: 0 });
    let (layout, config) = compile_map(
        &map,
        30,
        &settings,
        &no_nested(),
        &kinds,
        &bridges,
        &switch_table(&kinds, &bridges),
    )
    .expect("small base compiles");
    let grounds = layout.grounds.as_ref().expect("grounds missing");
    assert_eq!(grounds.center, [-194.0, -34.0]);
    for size in grounds.half_size {
        assert!(
            (size - 6.2).abs() < 0.0001,
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

    let mut graphs = crate::actors::navigation::NavGraphs::new(&config);
    graphs.add_grounds(&layout);
    let graph = graphs.get(CarrierId::WORLD);
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
    assert!(graph.nearest_node_for_position(&meadow).is_some());
    assert!(graph.engagement_route(&[], &pad, &meadow, 0.15, 0.15).is_some());
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
    assert_eq!(grounds.half_size, [0.0, 0.0]);
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(grounds),
        ..Default::default()
    });
    let hit = world
        .ground_surface_below(bevy::math::Vec3::new(0.0, 5.0, 0.0), 10.0)
        .expect("filled base beneath the upper floor");
    assert!(hit.point.y.abs() < 0.001);
}
