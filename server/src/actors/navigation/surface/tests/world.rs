use super::*;
use crate::actors::navigation::surface::{RouteFailure, fixtures};
use common::{map::Carriers, protocol::Position};

#[test]
fn authored_surfaces_keep_headroom_permissions_and_rebuild_only_for_relevant_changes() {
    let config = fixtures::config();
    let generated = fixtures::generate("fixture", 30, &config.settings).expect("compile authored scene");
    let mut world = CollisionWorld::from_map_layout(&generated.layout);
    let mut navigation = SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[])
        .expect("bake authored scene");
    let small = config.expect_actor("scuttler").character.physics();
    let tall = config.expect_actor("bruiser").character.physics();
    let ceiling = Position {
        x: -9.0,
        y: 1.5,
        z: 6.0,
    };
    assert!(
        world
            .support_surface_on_carrier(Vec3::from(ceiling) + Vec3::Y * 0.1, 0.2, CarrierId::WORLD, &[])
            .is_some()
    );
    for physics in [small, tall] {
        let (mesh, _) = navigation.mesh(CarrierId::WORLD, physics).expect("body mesh");
        let below = mesh
            .locate(Position { x: 4.5, y: 0.0, z: 0.0 }, 0.3)
            .expect("underpass");
        let above = mesh
            .locate(Position { x: 4.5, y: 3.0, z: 0.0 }, 0.3)
            .expect("upper floor");
        assert_ne!(below.polygon, above.polygon);
        assert!(mesh.locate(ceiling, 0.2).is_none(), "inaccessible roof became walkable");
        assert_eq!(
            mesh.locate(Position { y: 0.0, ..ceiling }, 0.2).is_some(),
            physics.movement_collider.height < 1.3
        );
    }
    let (mesh, before) = navigation.mesh(CarrierId::WORLD, small).expect("initial mesh");
    let start = Position { x: 4.5, y: 3.0, z: 0.0 };
    let goal = Position {
        x: 13.5,
        y: 3.0,
        z: 0.0,
    };
    mesh.route(start, goal, 0.7).expect("enabled bridge route");
    let mut carriers = Carriers::from_layout(&generated.layout);
    carriers.advance(10, &SwitchState::default());
    world.set_carrier_poses(&carriers);
    navigation
        .refresh(&world, &[FieldId(99)], &[])
        .expect("unrelated field");
    assert_eq!(navigation.mesh(CarrierId::WORLD, small).expect("same mesh").1, before);
    navigation.refresh(&world, &[FieldId(0)], &[]).expect("bridge off");
    settle(&mut navigation, &world, &[FieldId(0)]);
    let (mesh, after) = navigation.mesh(CarrierId::WORLD, small).expect("updated mesh");
    assert_ne!(before, after);
    assert_eq!(
        mesh.route(start, goal, 0.7).expect_err("route over removed bridge"),
        RouteFailure::Disconnected
    );
    navigation.refresh(&world, &[], &[]).expect("bridge on");
    settle(&mut navigation, &world, &[]);
    navigation
        .mesh(CarrierId::WORLD, small)
        .expect("restored mesh")
        .0
        .route(start, goal, 0.7)
        .expect("restored route");
    navigation
        .refresh(&world, &[FieldId(0)], &[])
        .expect("begin obsolete bake");
    navigation
        .refresh(&world, &[], &[])
        .expect("bridge changed while baking");
    settle(&mut navigation, &world, &[]);
    navigation
        .mesh(CarrierId::WORLD, small)
        .expect("latest mesh")
        .0
        .route(start, goal, 0.7)
        .expect("obsolete result did not restore the closed connection");
}

fn settle(navigation: &mut SurfaceNavigation, world: &CollisionWorld, open: &[FieldId]) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while navigation.running.is_some() || navigation.meshes.values().any(|entry| entry.dirty) {
        assert!(std::time::Instant::now() < deadline, "navigation bake did not finish");
        std::thread::sleep(std::time::Duration::from_millis(1));
        navigation.refresh(world, open, &[]).expect("background bake");
    }
}

#[test]
fn navigation_regions_follow_collision_geometry_and_reload_evicted_areas() {
    use crate::actors::SurfaceGoal;
    let config = fixtures::config();
    let mut generated = fixtures::compile(serde_json::json!({"map": {
        "grid_cols":4,"grid_rows":4,"fireworks":null,
        "levels":[{"floors":[{"col":1,"row":1,"all":"basement-floor"}]}],
        "checkpoints":[{"level":0,"cols":[1,2],"rows":[1,2],"number":0,"type":"individual"}],
        "actor_spawn_zones":[{"level":0,"cols":[1,2],"rows":[1,2],"kind":"scuttler","count":[1],"respawn_secs":null}]
    }}), 30, &config.settings).expect("small authored region");
    // A normal collision floor extends beyond the authoring grid. Region
    // loading must work without a procedural Grounds object or terrain mode.
    generated.layout.floors.push(common::protocol::Floor {
        x1: -3000.0,
        x2: 3000.0,
        z1: -4.0,
        z2: 4.0,
        y: 0.0,
        thickness: 0.2,
        level: 0,
        carrier: CarrierId::WORLD,
    });
    let world = CollisionWorld::from_map_layout(&generated.layout);
    let mut navigation =
        SurfaceNavigation::build(&generated.config, &generated.layout, &config, &world, &[], &[]).expect("navigation");
    let physics = config.expect_actor("scuttler").character.physics();
    let point = |x| SurfaceGoal {
        carrier: CarrierId::WORLD,
        position: Position { x, y: 0.0, z: 0.0 },
    };
    for index in 1..=40 {
        let from = point(index as f32 * 64.0);
        let to = point(from.position.x + 12.0);
        assert_eq!(
            navigation.prepare_route(from, to, physics, false),
            None,
            "unloaded navigation is pending"
        );
        settle(&mut navigation, &world, &[]);
        assert_eq!(navigation.prepare_route(from, to, physics, false), Some(true));
        navigation
            .route(from, to, physics, false, 4096)
            .expect("route outside the authoring grid");
    }
    assert!(
        navigation.mesh_at(point(64.0), physics).is_none(),
        "unused regions were kept forever"
    );
    assert_eq!(navigation.prepare_route(point(64.0), point(76.0), physics, false), None);
    settle(&mut navigation, &world, &[]);
    navigation
        .route(point(64.0), point(76.0), physics, false, 4096)
        .expect("evicted region reloads");
    navigation
        .route(point(2560.0), point(2572.0), physics, false, 4096)
        .expect("recent region remains usable");
}

#[test]
fn moving_collision_export_and_navigation_stay_in_the_carriers_local_frame() {
    let config = fixtures::config();
    let generated = fixtures::compile(fixtures::shuttle(), 30, &config.settings).expect("authored shuttle");
    let mut world = CollisionWorld::from_map_layout(&generated.layout);
    let mut carriers = Carriers::from_layout(&generated.layout);
    world.set_carrier_poses(&carriers);
    let initial = world.collision_meshes().expect("initial export");
    let carrier = CarrierId::from_carried_index(0);
    let origin = carriers.pose(carrier).transform_position(&Position::default());
    carriers.advance(45, &Default::default());
    world.set_carrier_poses(&carriers);
    assert_ne!(origin, carriers.pose(carrier).transform_position(&Position::default()));
    let moved = world.collision_meshes().expect("moved export");
    assert_eq!(initial.len(), moved.len());
    for (a, b) in initial.iter().zip(&moved) {
        assert_eq!(a.vertices, b.vertices);
    }
    let physics = config.expect_actor("scuttler").character.physics();
    let mesh = SurfaceMesh::bake(&moved, carrier, physics, &[]).expect("local carrier mesh");
    let location = mesh.locate(Position::default(), 0.2).expect("carrier local origin");
    assert_eq!(location.carrier, carrier);
    assert!(location.position.y.abs() < 0.1);
}
