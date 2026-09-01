use bevy_math::Vec3;

use crate::{constants::PORTAL_UP_DEGENERACY_LIMIT, math::direction_from_yaw_pitch, protocol::Portal};

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
    #[must_use]
    pub fn from_portal(portal: &Portal) -> Self {
        Self::from_surface(
            portal.pos.into(),
            Vec3::new(portal.nx, portal.ny, portal.nz),
            portal.yaw,
        )
    }

    #[must_use]
    pub fn from_surface(center: Vec3, normal: Vec3, yaw: f32) -> Self {
        let normal = normal.normalize();
        // World-up projected onto the plane orients the frame; only a
        // near-vertical normal is degenerate, and there the shooter's
        // placement yaw supplies the in-plane up instead.
        let reference = if normal.y.abs() < PORTAL_UP_DEGENERACY_LIMIT {
            Vec3::Y
        } else {
            direction_from_yaw_pitch(yaw, 0.0)
        };
        let up = (reference - normal * reference.dot(normal)).normalize();
        Self {
            center,
            normal,
            up,
            right: up.cross(normal),
        }
    }
}
