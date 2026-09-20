use anyhow::{Result, bail};
use bevy_math::Vec3;
use rapier3d::parry::shape::TypedShape;
use std::collections::HashMap;

use super::{CollisionWorld, colliders::ColliderKind, surface_materials::MATERIAL_INDEX_SHIFT};
use crate::{
    math::from_rapier,
    protocol::{CarrierId, FieldId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionSource {
    Wall(usize),
    Floor(usize),
    Ramp(usize),
    Barrier(FieldId),
    Bridge(FieldId),
    Grounds,
    Decoration,
    PressurePlate,
}

pub struct CollisionMesh {
    pub carrier: CarrierId,
    pub source: CollisionSource,
    pub field: Option<FieldId>,
    pub vertices: Vec<Vec3>,
    pub triangles: Vec<[u32; 3]>,
}

impl CollisionWorld {
    // Export the shapes already used by physics. Moving colliders keep their
    // original local poses so baking never depends on the platform's current tick.
    pub fn collision_meshes(&self) -> Result<Vec<CollisionMesh>> {
        let local_poses: HashMap<_, _> = self
            .carrier_colliders
            .iter()
            .flatten()
            .map(|(handle, pose)| (*handle, pose))
            .collect();
        self.colliders
            .iter()
            .filter(|(_, collider)| !collider.collision_groups().memberships.is_empty())
            .map(|(handle, collider)| {
                let carrier = ColliderKind::carrier_from_user_data(collider.user_data);
                let local_pose = local_poses.get(&handle).copied().unwrap_or_else(|| collider.position());
                let (vertices, triangles) = match collider.shape().as_typed_shape() {
                    TypedShape::Cuboid(shape) => shape.to_trimesh(),
                    TypedShape::ConvexPolyhedron(shape) => shape.to_trimesh(),
                    TypedShape::TriMesh(shape) => (shape.vertices().to_vec(), shape.indices().to_vec()),
                    TypedShape::Cylinder(shape) => {
                        let (mut vertices, triangles) = shape.to_trimesh(16);
                        let scale = (std::f32::consts::PI / 16.0).cos().recip();
                        for point in &mut vertices {
                            point.x *= scale;
                            point.z *= scale;
                        }
                        (vertices, triangles)
                    }
                    _ => bail!("collision mesh export does not support this shape"),
                };
                let index = (collider.user_data >> MATERIAL_INDEX_SHIFT) as usize;
                let field = ColliderKind::field_from_user_data(collider.user_data);
                let source = match ColliderKind::from_user_data(collider.user_data) {
                    Some(ColliderKind::Wall) => CollisionSource::Wall(index),
                    Some(ColliderKind::Floor) => CollisionSource::Floor(index),
                    Some(ColliderKind::Ramp) => CollisionSource::Ramp(index),
                    Some(ColliderKind::Barrier) => {
                        CollisionSource::Barrier(field.expect("field missing from barrier collider"))
                    }
                    Some(ColliderKind::Bridge) => {
                        CollisionSource::Bridge(field.expect("field missing from bridge collider"))
                    }
                    Some(ColliderKind::Grounds) => CollisionSource::Grounds,
                    Some(ColliderKind::Decoration) => CollisionSource::Decoration,
                    Some(ColliderKind::PressurePlate) => CollisionSource::PressurePlate,
                    None => bail!("collision mesh has no geometry source"),
                };
                Ok(CollisionMesh {
                    carrier,
                    source,
                    field,
                    vertices: vertices
                        .into_iter()
                        .map(|point| from_rapier(*local_pose * point))
                        .collect(),
                    triangles,
                })
            })
            .collect()
    }
}
