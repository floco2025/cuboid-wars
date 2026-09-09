use std::{
    collections::BTreeMap,
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

use bevy_math::{Mat3, Quat, Vec3};
use rapier3d::{
    parry::shape::Cuboid,
    prelude::{Collider, ColliderHandle, SharedShape, Vector},
};

use super::{
    CollisionWorld, WorldSurfaceHit,
    colliders::{ColliderKind, query_filter, world_collision_groups},
};
use crate::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_RIM_SCALE},
    math::{from_rapier, rapier_pose, to_rapier},
    physics::PortalFrame,
    protocol::{CarrierId, MapLayout, TextureSettings},
};

pub(super) const MATERIAL_INDEX_SHIFT: u32 = 40;
const SURFACE_DEPTH: f32 = 0.02;
const RIM_SEGMENTS: usize = 64;
// Coplanar grid faces can protrude slightly past the portal plane through floating-point noise.
const PORTAL_BACKING_FLUSH_EPSILON: f32 = 0.02;

impl CollisionWorld {
    // Portal backing colliders: what the aperture's backing volume touches
    // that lies entirely behind the surface plane and belongs to the
    // portal's own carrier. An adjoining ramp, or the floor a wall portal
    // stands on, reaches in front of the plane and must remain solid while
    // the surface itself opens for transit; a stacked wall's trim strip is
    // flush with the wall faces and opens with them. Another carrier's
    // collider passing behind the plane is not backing: it moves on, and
    // the set is computed once.
    #[must_use]
    pub(crate) fn portal_backing_colliders(
        &self,
        surface_center: Vec3,
        surface_normal: Vec3,
        half_extents: Vec3,
        rotation: Quat,
        carrier: CarrierId,
    ) -> Vec<ColliderHandle> {
        let Some(surface_normal) = surface_normal.try_normalize() else {
            return Vec::new();
        };
        let plane_reach = surface_center.dot(surface_normal) + PORTAL_BACKING_FLUSH_EPSILON;
        let outward = to_rapier(surface_normal);
        let shape = Cuboid::new(to_rapier(half_extents));
        let pose = rapier_pose(surface_center - surface_normal * half_extents.z, rotation);
        self.query_pipeline(query_filter(world_collision_groups()))
            .intersect_shape(pose, &shape)
            .filter_map(|(handle, collider)| {
                if ColliderKind::carrier_from_user_data(collider.user_data) != carrier {
                    return None;
                }
                let front = collider
                    .shape()
                    .as_support_map()
                    .expect("world surface has no support map")
                    .support_point(collider.position(), outward);
                (from_rapier(front).dot(surface_normal) <= plane_reach).then_some(handle)
            })
            .collect()
    }

    pub(crate) fn portal_surface_allows(
        &self,
        hit: &WorldSurfaceHit,
        layout: &MapLayout,
        textures: &BTreeMap<String, TextureSettings>,
    ) -> bool {
        material_allows(&self.colliders[hit.collider], hit.normal, layout, textures)
    }

    pub(crate) fn portal_materials_allow(
        &self,
        frame: &PortalFrame,
        layout: &MapLayout,
        textures: &BTreeMap<String, TextureSettings>,
    ) -> bool {
        let pose = rapier_pose(
            frame.center - frame.normal * SURFACE_DEPTH / 2.0,
            Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal)),
        );
        let outward = to_rapier(frame.normal);
        let plane = frame.center.dot(frame.normal);
        self.query_pipeline(query_filter(world_collision_groups()))
            .intersect_shape(pose, aperture_backing_shape().as_ref())
            .all(|(_, collider): (ColliderHandle, &Collider)| {
                let front = collider
                    .shape()
                    .as_support_map()
                    .expect("world surface has no support map")
                    .support_point(collider.position(), outward);
                (from_rapier(front).dot(frame.normal) - plane).abs() > SURFACE_DEPTH
                    || material_allows(collider, frame.normal, layout, textures)
            })
    }
}

fn material_allows(
    collider: &Collider,
    normal: Vec3,
    layout: &MapLayout,
    textures: &BTreeMap<String, TextureSettings>,
) -> bool {
    let index = (collider.user_data >> MATERIAL_INDEX_SHIFT) as usize;
    let materials = match ColliderKind::from_user_data(collider.user_data) {
        Some(ColliderKind::Wall) => layout.wall_materials.get(index),
        Some(ColliderKind::Floor) => layout.floor_materials.get(index),
        Some(ColliderKind::Ramp) => layout.ramp_materials.get(index),
        _ => None,
    };
    let Some(materials) = materials else {
        return false;
    };
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
    textures.get(alias).is_some_and(|texture| texture.portalable)
}

fn aperture_backing_shape() -> &'static SharedShape {
    static SHAPE: OnceLock<SharedShape> = OnceLock::new();
    SHAPE.get_or_init(|| {
        // Circumscribe the oval so a narrow forbidden patch cannot hide between rim samples.
        let scale = PORTAL_RIM_SCALE / (PI / RIM_SEGMENTS as f32).cos();
        let points: Vec<_> = [-SURFACE_DEPTH / 2.0, SURFACE_DEPTH / 2.0]
            .into_iter()
            .flat_map(|z| {
                (0..RIM_SEGMENTS).map(move |i| {
                    let angle = i as f32 * TAU / RIM_SEGMENTS as f32;
                    Vector::new(
                        PORTAL_HALF_WIDTH * scale * angle.cos(),
                        PORTAL_HALF_HEIGHT * scale * angle.sin(),
                        z,
                    )
                })
            })
            .collect();
        SharedShape::convex_hull(&points).expect("portal backing oval has no convex hull")
    })
}
