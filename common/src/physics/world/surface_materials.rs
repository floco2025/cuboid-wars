use bevy_math::Vec3;
use rapier3d::prelude::Collider;

use super::{CollisionWorld, WorldSurfaceHit, colliders::ColliderKind};
use crate::{math::to_rapier, protocol::MapLayout};

pub(super) const MATERIAL_INDEX_SHIFT: u32 = 40;

impl CollisionWorld {
    pub fn surface_material<'a>(&self, hit: &WorldSurfaceHit, layout: &'a MapLayout) -> Option<&'a str> {
        collider_material(&self.colliders[hit.collider], hit.normal, layout)
    }
}

pub(super) fn collider_material<'a>(collider: &Collider, normal: Vec3, layout: &'a MapLayout) -> Option<&'a str> {
    let index = (collider.user_data >> MATERIAL_INDEX_SHIFT) as usize;
    let materials = match ColliderKind::from_user_data(collider.user_data) {
        Some(ColliderKind::Wall) => layout.wall_materials.get(index),
        Some(ColliderKind::Floor) => layout.floor_materials.get(index),
        Some(ColliderKind::Ramp) => layout.ramp_materials.get(index),
        _ => None,
    };
    let materials = materials?;
    let local = collider.position().rotation.inverse() * to_rapier(normal);
    let alias = if local.y > 0.001 {
        &materials.top
    } else if local.y < -0.001 {
        &materials.bottom
    } else if local.x.abs() > local.z.abs() {
        if local.x > 0.0 {
            &materials.east
        } else {
            &materials.west
        }
    } else if local.z > 0.0 {
        &materials.south
    } else {
        &materials.north
    };
    Some(alias)
}
