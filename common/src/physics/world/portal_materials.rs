use std::{collections::BTreeMap, sync::OnceLock};

use bevy_math::{Mat3, Quat, Vec3};
use rapier3d::prelude::{Collider, ColliderHandle, Pose, SharedShape, Vector};

use super::{
    CollisionWorld, WorldSurfaceHit,
    colliders::{ColliderKind, query_filter, world_collision_groups},
};
use crate::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_RIM_SCALE},
    physics::PortalFrame,
    protocol::{MapLayout, TextureSettings},
};

pub(super) const MATERIAL_INDEX_SHIFT: u32 = 40;
const SURFACE_DEPTH: f32 = 0.02;
const RIM_SEGMENTS: usize = 64;

impl CollisionWorld {
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
        let query = self.broad_phase.as_query_pipeline(
            self.narrow_phase.query_dispatcher(),
            &self.bodies,
            &self.colliders,
            query_filter(world_collision_groups()),
        );
        let rotation = Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal)).to_scaled_axis();
        let center = frame.center - frame.normal * SURFACE_DEPTH / 2.0;
        let pose = Pose::new(Vector::from(center.to_array()), Vector::from(rotation.to_array()));
        let outward = Vector::from(frame.normal.to_array());
        let plane = frame.center.dot(frame.normal);
        query.intersect_shape(pose, aperture_backing_shape().as_ref()).all(
            |(_, collider): (ColliderHandle, &Collider)| {
                let front = collider
                    .shape()
                    .as_support_map()
                    .expect("world surface has no support map")
                    .support_point(collider.position(), outward);
                (Vec3::from(front.to_array()).dot(frame.normal) - plane).abs() > SURFACE_DEPTH
                    || material_allows(collider, frame.normal, layout, textures)
            },
        )
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
    let local = collider.position().rotation.inverse() * Vector::from(normal.to_array());
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
        let scale = PORTAL_RIM_SCALE / (std::f32::consts::PI / RIM_SEGMENTS as f32).cos();
        let points: Vec<_> = [-SURFACE_DEPTH / 2.0, SURFACE_DEPTH / 2.0]
            .into_iter()
            .flat_map(|z| {
                (0..RIM_SEGMENTS).map(move |i| {
                    let angle = i as f32 * std::f32::consts::TAU / RIM_SEGMENTS as f32;
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
