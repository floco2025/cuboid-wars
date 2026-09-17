use std::{
    collections::BTreeMap,
    f32::consts::{PI, TAU},
    sync::OnceLock,
};

use bevy_math::{Mat3, Quat, Vec3};
use rapier3d::{
    parry::{
        query::intersection_test,
        shape::{Cuboid, Shape},
    },
    prelude::{Collider, ColliderHandle, Pose, SharedShape, Vector},
};

use super::{
    CollisionWorld, WorldSurfaceHit,
    colliders::{ColliderKind, query_filter, world_collision_groups},
    surface_materials::collider_material,
};
use crate::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH, PORTAL_RIM_SCALE},
    math::{from_rapier, rapier_pose, to_rapier},
    physics::PortalFrame,
    protocol::{CarrierId, MapLayout, TextureSettings},
};

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
                // Backing opens whole, so only a convex solid can be one:
                // the grounds trimesh is the entire landscape.
                let front = collider
                    .shape()
                    .as_support_map()?
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
        self.surface_material(hit, layout)
            .and_then(|alias| textures.get(alias))
            .is_some_and(|texture| texture.portalable)
    }

    pub(crate) fn portal_materials_allow(
        &self,
        frame: &PortalFrame,
        layout: &MapLayout,
        textures: &BTreeMap<String, TextureSettings>,
    ) -> bool {
        let pose = rapier_pose(
            frame.center,
            Quat::from_mat3(&Mat3::from_cols(frame.right, frame.up, frame.normal)),
        );
        let outward = to_rapier(frame.normal);
        let plane = frame.center.dot(frame.normal);
        let face = aperture_face_shape().as_ref();
        self.query_pipeline(query_filter(world_collision_groups()))
            .intersect_shape(pose, face)
            .all(|(_, collider): (ColliderHandle, &Collider)| {
                let Some(reach) = surface_reach(collider, &pose, face, outward) else {
                    return true;
                };
                (reach - plane).abs() > SURFACE_DEPTH
                    || collider_material(collider, frame.normal, layout)
                        .and_then(|alias| textures.get(alias))
                        .is_some_and(|texture| texture.portalable)
            })
    }
}

// How far along `outward` the collider's surface reaches where `probe`
// touches it. A convex solid reaches to its support point. The grounds
// trimesh is one collider for the whole landscape, so only the triangles the
// probe touches answer: terrain level with a floor's top is a face on that
// plane, while terrain meeting a wall at its base reaches past the wall's
// plane and is no face of it.
fn surface_reach(collider: &Collider, probe_pose: &Pose, probe: &dyn Shape, outward: Vector) -> Option<f32> {
    let shape = collider.shape();
    let position = collider.position();
    if let Some(support_map) = shape.as_support_map() {
        return Some(support_map.support_point(position, outward).dot(outward));
    }
    let composite = shape.as_composite_shape()?;
    let local_probe = probe.compute_aabb(&position.inv_mul(probe_pose));
    composite
        .bvh()
        .intersect_aabb(&local_probe)
        .filter_map(|part| {
            let mut reach = None;
            composite.map_part_at(part, &mut |part_pose, part_shape, _| {
                let part_position = part_pose.map_or(*position, |part_pose| position * part_pose);
                if intersection_test(probe_pose, probe, &part_position, part_shape).unwrap_or(false)
                    && let Some(support_map) = part_shape.as_support_map()
                {
                    reach = Some(support_map.support_point(&part_position, outward).dot(outward));
                }
            });
            reach
        })
        .max_by(f32::total_cmp)
}

// The probe for the surfaces an aperture lies on. It reaches `SURFACE_DEPTH`
// to either side of the plane, so a surface level with the plane lies
// inside it instead of touching its face, which an intersection test
// reports inconsistently.
fn aperture_face_shape() -> &'static SharedShape {
    static SHAPE: OnceLock<SharedShape> = OnceLock::new();
    SHAPE.get_or_init(|| {
        // Circumscribe the oval so a narrow forbidden patch cannot hide between rim samples.
        let scale = PORTAL_RIM_SCALE / (PI / RIM_SEGMENTS as f32).cos();
        let points: Vec<_> = [-SURFACE_DEPTH, SURFACE_DEPTH]
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
        SharedShape::convex_hull(&points).expect("portal face oval has no convex hull")
    })
}
