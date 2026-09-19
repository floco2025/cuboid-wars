use super::*;
use common::protocol::{CarrierId, RampDirection, RampShape};

fn ramp(shape: RampShape, direction: RampDirection) -> Ramp {
    Ramp {
        x1: 2.0,
        z1: -3.0,
        x2: 6.0,
        z2: 5.0,
        y: 1.0,
        height: 4.0,
        direction,
        shape,
        thickness: 0.4,
        level: 0,
        levels: 1,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn every_face_is_wound_outward_and_lands_on_its_own_material() {
    let materials = FaceMaterials::from_six("top", "bottom", "north", "south", "east", "west");
    for (direction, high, low, sides) in [
        (RampDirection::North, "north", "south", ["east", "west"]),
        (RampDirection::South, "south", "north", ["east", "west"]),
        (RampDirection::East, "east", "west", ["north", "south"]),
        (RampDirection::West, "west", "east", ["north", "south"]),
    ] {
        for shape in [RampShape::Solid, RampShape::Plank] {
            let ramp = ramp(shape, direction);
            let centre = ramp.prism().points().sum::<Vec3>() / ramp.prism().points().count() as f32;
            for face in ramp_faces(&ramp) {
                for [a, b, c] in &face.triangles {
                    let wound = (*b - *a).cross(*c - *a).normalize();
                    assert!(wound.dot(face.normal) > 0.999, "{shape:?} {direction:?}");
                    assert!(face.normal.dot(*a - centre) > 0.0, "{shape:?} {direction:?}");
                }
            }

            let triangles = |alias: &str| {
                build_ramp_meshes(&ramp, &materials, |_| 1.0)
                    .iter()
                    .find(|(mesh_alias, _)| mesh_alias == alias)
                    .map_or(0, |(_, mesh)| mesh.count_vertices() / 3)
            };
            // A wedge has a tall back face and two triangular sides; a plank a
            // face at each end and two parallelogram sides.
            let (low_end, side) = match shape {
                RampShape::Solid => (0, 1),
                RampShape::Plank => (2, 2),
            };
            assert_eq!(triangles("top"), 2, "{shape:?} {direction:?}");
            assert_eq!(triangles("bottom"), 2, "{shape:?} {direction:?}");
            assert_eq!(triangles(high), 2, "{shape:?} {direction:?}");
            assert_eq!(triangles(low), low_end, "{shape:?} {direction:?}");
            assert_eq!(sides.map(triangles), [side; 2], "{shape:?} {direction:?}");
        }
    }
}
