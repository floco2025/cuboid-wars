use super::*;
use crate::actors::navigation::surface::fixtures;

#[test]
fn exterior_terrain_with_notched_region_boundaries_can_be_routed() {
    use common::{
        map::{Grounds, GroundsSettings},
        physics::CollisionWorld,
        protocol::MapLayout,
    };
    let world = CollisionWorld::from_map_layout(&MapLayout {
        grounds: Some(Grounds::new(
            [(-51.0, 51.0, -51.0, 51.0)],
            4.4,
            GroundsSettings { level: 1 },
        )),
        ..Default::default()
    });
    let geometry = world.collision_meshes().expect("terrain collision geometry");
    let mut physics = fixtures::config().expect_actor("bruiser").character.physics();
    physics.movement_collider.diameter = 1.6422;
    physics.movement_collider.height = 1.6675;
    let mesh = SurfaceMesh::bake_in(
        &geometry,
        CarrierId::WORLD,
        physics,
        &[],
        Some(SurfaceBounds {
            min: Vec3::new(-256.0, -100.0, -96.0),
            max: Vec3::new(-128.0, 100.0, 32.0),
        }),
        &[],
    )
    .expect("bake the region needed by the return journey");
    let support = |x, z| {
        world
            .support_surface_on_carrier(Vec3::new(x, 100.0, z), 200.0, CarrierId::WORLD, &[])
            .expect("terrain support")
            .point
            .into()
    };
    mesh.route(support(-193.0, -22.0), support(-145.0, -26.0), 1.0)
        .expect("cross the terrain region toward home");
}

#[test]
fn rolling_ground_keeps_its_interior_height_when_polygons_are_simplified() {
    // The perimeter is level; the middle rises three metres. A polygon's
    // boundary alone cannot describe the walkable surface inside it.
    let mut vertices = Vec::new();
    for z in 0..=8 {
        for x in 0..=8 {
            let height =
                3.0 * (std::f32::consts::PI * x as f32 / 8.0).sin() * (std::f32::consts::PI * z as f32 / 8.0).sin();
            vertices.push(Vec3::new(x as f32 * 3.0, height, z as f32 * 3.0));
        }
    }
    let mut triangles = Vec::new();
    for z in 0..8 {
        for x in 0..8 {
            let a = z * 9 + x;
            triangles.extend([[a, a + 9, a + 1], [a + 1, a + 9, a + 10]]);
        }
    }
    let geometry = vec![CollisionMesh {
        carrier: CarrierId::WORLD,
        source: CollisionSource::Grounds,
        field: None,
        vertices,
        triangles,
    }];
    let physics = fixtures::config().expect_actor("bruiser").character.physics();
    let mesh = SurfaceMesh::bake(&geometry, CarrierId::WORLD, physics, &[]).expect("hill mesh");
    let summit = Position {
        x: 12.0,
        y: 3.0,
        z: 12.0,
    };
    let located = mesh.locate(summit, 0.3).expect("physical summit remains on navigation");
    assert!((located.position.y - summit.y).abs() < 0.2, "{located:?}");
    let lower = Position {
        x: 3.0,
        y: 3.0 * (std::f32::consts::PI / 8.0).sin().powi(2),
        z: 3.0,
    };
    mesh.route(summit, lower, 0.3)
        .expect("an actor on the hill can resume pursuit");
}
