use bevy_math::Vec3;

use crate::{map::Carriers, math::direction_from_yaw_pitch, protocol::Portal};

// Near-vertical normals need placement yaw because world-up has no usable in-plane projection.
const PORTAL_UP_DEGENERACY_LIMIT: f32 = 0.99;

// Orthonormal aperture frame of one portal end: `normal` points out of the
// surface into the room, `up`/`right` span the plane with (right, up, normal)
// right-handed. Everything downstream — traversal, triggers, rendering —
// reads this frame; nothing asks what kind of surface the portal is on.
#[derive(Debug, Clone, Copy)]
pub struct PortalFrame {
    pub center: Vec3,
    pub normal: Vec3,
    pub up: Vec3,
    pub right: Vec3,
}

impl PortalFrame {
    // The world frame of a placed end at this tick: its carrier-local
    // position and normal placed by the carrier's pose.
    #[must_use]
    pub fn from_portal(portal: &Portal, carriers: &Carriers) -> Self {
        let pose = carriers.pose(portal.carrier);
        Self::from_surface(
            pose.transform_point(Vec3::from(portal.pos)),
            pose.transform_vector(Vec3::new(portal.nx, portal.ny, portal.nz)),
            portal.yaw,
        )
    }

    // The same between the last two ticks, for render-rate interpolation.
    #[must_use]
    pub fn from_portal_between(portal: &Portal, carriers: &Carriers, alpha: f32) -> Self {
        let pose = carriers.pose_between(portal.carrier, alpha);
        Self::from_surface(
            pose.transform_point(Vec3::from(portal.pos)),
            pose.transform_vector(Vec3::new(portal.nx, portal.ny, portal.nz)),
            portal.yaw,
        )
    }

    #[must_use]
    pub fn from_surface(center: Vec3, normal: Vec3, yaw: f32) -> Self {
        let normal = normal.normalize();
        // World-up projected onto the plane orients the frame; only a
        // near-vertical normal is degenerate, and there the shooter's
        // placement yaw supplies the in-plane up instead.
        let reference = if Self::up_is_degenerate(normal) {
            direction_from_yaw_pitch(yaw, 0.0)
        } else {
            Vec3::Y
        };
        let up = reference.reject_from_normalized(normal).normalize();
        Self {
            center,
            normal,
            up,
            right: up.cross(normal),
        }
    }

    // Whether a unit surface normal is too vertical for world-up to have an
    // in-plane projection, so the frame's up comes from a yaw instead.
    #[must_use]
    pub(super) fn up_is_degenerate(normal: Vec3) -> bool {
        normal.y.abs() >= PORTAL_UP_DEGENERACY_LIMIT
    }
}
