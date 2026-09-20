use super::*;
use crate::actors::navigation::surface::fixtures;

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
