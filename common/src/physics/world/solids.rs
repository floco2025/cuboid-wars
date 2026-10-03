use std::collections::HashMap;

use bevy_math::Vec3;
use rapier3d::{
    parry::shape::{Ball, TypedShape},
    prelude::{Collider, Pose, Vector},
};

use super::{
    CollisionWorld,
    colliders::{ColliderKind, query_filter, world_collision_groups},
};
use crate::{
    map::Carriers,
    math::{from_rapier, to_rapier},
    protocol::CarrierId,
};

// One flat face of a solid: its outward normal and its corners in order
// around it.
#[derive(Clone, Debug, PartialEq)]
pub struct SolidFace {
    pub normal: Vec3,
    pub corners: Vec<Vec3>,
}

// A convex piece of the built world in its carrier's frame: what a wall, a
// slab, a trim strip, or a ramp is once compiled, whichever record it came
// from. A reader that needs to know where the structure is asks for these
// and never for the layout's records, which are authoring data.
#[derive(Clone, Debug, PartialEq)]
pub struct Solid {
    pub carrier: CarrierId,
    pub faces: Vec<SolidFace>,
    pub min: Vec3,
    pub max: Vec3,
}

impl Solid {
    // Whether the solid fills its bounds: an axis-aligned box.
    #[must_use]
    pub fn is_box(&self) -> bool {
        self.faces.len() == 6 && self.faces.iter().all(|face| face.normal.abs().max_element() > 0.999)
    }
}

impl CollisionWorld {
    // Every structural solid in its carrier's frame as built: walls, slabs,
    // and ramps. Terrain and its decorations, fields, and plates are not
    // structure.
    #[must_use]
    pub fn structural_solids(&self) -> Vec<Solid> {
        let local_poses: HashMap<_, _> = self
            .carrier_colliders
            .iter()
            .flatten()
            .map(|(handle, pose)| (*handle, *pose))
            .collect();
        self.colliders
            .iter()
            .filter_map(|(handle, collider)| {
                let pose = local_poses
                    .get(&handle)
                    .copied()
                    .unwrap_or_else(|| *collider.position());
                solid(collider, pose)
            })
            .collect()
    }

    // The structural solids within `radius` of the world point `center`, in
    // their carriers' frames where the carriers stand now.
    #[must_use]
    pub fn structural_solids_near(&self, carriers: &Carriers, center: Vec3, radius: f32) -> Vec<Solid> {
        if !center.is_finite() || !radius.is_finite() || radius <= 0.0 {
            return Vec::new();
        }
        self.query_pipeline(query_filter(world_collision_groups()))
            .intersect_shape(Pose::from_translation(to_rapier(center)), &Ball::new(radius))
            .filter_map(|(_, collider)| {
                let carrier = ColliderKind::carrier_from_user_data(collider.user_data);
                let offset = Pose::from_translation(to_rapier(-carriers.pose(carrier).translation));
                solid(collider, offset * *collider.position())
            })
            .collect()
    }
}

fn solid(collider: &Collider, pose: Pose) -> Option<Solid> {
    if !matches!(
        ColliderKind::from_user_data(collider.user_data)?,
        ColliderKind::Wall | ColliderKind::Floor | ColliderKind::Ramp
    ) {
        return None;
    }
    let local_faces: Vec<(Vector, Vec<Vector>)> = match collider.shape().as_typed_shape() {
        TypedShape::Cuboid(shape) => cuboid_faces(shape.half_extents),
        TypedShape::ConvexPolyhedron(shape) => shape
            .faces()
            .iter()
            .map(|face| {
                let first = face.first_vertex_or_edge as usize;
                let count = face.num_vertices_or_edges as usize;
                let corners = shape.vertices_adj_to_face()[first..first + count]
                    .iter()
                    .map(|&index| shape.points()[index as usize])
                    .collect();
                (face.normal, corners)
            })
            .collect(),
        _ => return None,
    };
    let faces: Vec<SolidFace> = local_faces
        .into_iter()
        .map(|(normal, corners)| SolidFace {
            normal: from_rapier(pose.rotation * normal),
            corners: corners.into_iter().map(|corner| from_rapier(pose * corner)).collect(),
        })
        .collect();
    let (min, max) = faces
        .iter()
        .flat_map(|face| &face.corners)
        .fold((Vec3::INFINITY, Vec3::NEG_INFINITY), |(min, max), &corner| {
            (min.min(corner), max.max(corner))
        });
    Some(Solid {
        carrier: ColliderKind::carrier_from_user_data(collider.user_data),
        faces,
        min,
        max,
    })
}

// A box's six faces, each with its corners in order around it.
fn cuboid_faces(half: Vector) -> Vec<(Vector, Vec<Vector>)> {
    let mut faces = Vec::with_capacity(6);
    for axis in 0..3 {
        let (across, along) = ((axis + 1) % 3, (axis + 2) % 3);
        for sign in [-1.0, 1.0] {
            let mut normal = Vector::ZERO;
            normal[axis] = sign;
            let corners = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)]
                .into_iter()
                .map(|(a, b)| {
                    let mut corner = Vector::ZERO;
                    corner[axis] = sign * half[axis];
                    corner[across] = a * half[across];
                    corner[along] = b * half[along];
                    corner
                })
                .collect();
            faces.push((normal, corners));
        }
    }
    faces
}

#[cfg(test)]
#[path = "tests/solids.rs"]
mod tests;
