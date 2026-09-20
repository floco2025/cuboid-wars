use anyhow::{Result, bail};
use bevy_math::Vec3;
use rapier3d::{
    parry::{
        query::{ShapeCastOptions, cast_shapes},
        shape::{Triangle, TypedShape},
    },
    prelude::{Pose, Vector},
};
use std::collections::HashMap;

use super::{CollisionWorld, colliders::ColliderKind, surface_materials::MATERIAL_INDEX_SHIFT};
use crate::{
    config::CharacterPhysicsConfig,
    constants::CHARACTER_CONTACT_OFFSET,
    math::{from_rapier, to_rapier},
    physics::characters::{character_movement_pose, character_movement_shape},
    protocol::{CarrierId, FieldId, Position},
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

impl CollisionMesh {
    // Bakes query the exported geometry in its carrier's frame, independently
    // of the live collision world's moving poses.
    pub fn character_path_clear(&self, start: Position, target: Position, physics: CharacterPhysicsConfig) -> bool {
        let from = Vec3::from(start);
        let to = Vec3::from(target);
        if !from.is_finite() || !to.is_finite() {
            return false;
        }
        let body = physics.movement_collider;
        let min = from.min(to) + Vec3::new(-body.radius(), CHARACTER_CONTACT_OFFSET, -body.radius());
        let max = from.max(to) + Vec3::new(body.radius(), body.height + CHARACTER_CONTACT_OFFSET, body.radius());
        let pose = character_movement_pose(&start, physics);
        let shape = character_movement_shape(physics);
        let velocity = to_rapier(to - from);
        let options = ShapeCastOptions {
            max_time_of_impact: 1.0,
            stop_at_penetration: false,
            ..Default::default()
        };
        self.triangles.iter().all(|indices| {
            let [a, b, c] = indices.map(|index| self.vertices[index as usize]);
            if a.min(b).min(c).cmpgt(max).any() || a.max(b).max(c).cmplt(min).any() {
                return true;
            }
            cast_shapes(
                &pose,
                velocity,
                &shape,
                &Pose::IDENTITY,
                Vector::ZERO,
                &Triangle::new(to_rapier(a), to_rapier(b), to_rapier(c)),
                options,
            )
            .is_ok_and(|hit| hit.is_none())
        })
    }
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
