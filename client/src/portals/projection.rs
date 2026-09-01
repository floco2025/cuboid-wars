use bevy::{
    camera::{CameraProjection, SubCameraView},
    math::Vec3A,
    prelude::*,
};

use crate::constants::PORTAL_VIEW_CLIP_OFFSET;
use common::{
    constants::{PORTAL_HALF_HEIGHT, PORTAL_HALF_WIDTH},
    physics::{PortalFrame, traverse_vector},
};

#[derive(Clone, Debug)]
pub(super) struct PortalProjection {
    left: f32,
    right: f32,
    bottom: f32,
    top: f32,
    near: f32,
    far: f32,
}

impl Default for PortalProjection {
    fn default() -> Self {
        Self {
            left: -PORTAL_HALF_WIDTH,
            right: PORTAL_HALF_WIDTH,
            bottom: -PORTAL_HALF_HEIGHT,
            top: PORTAL_HALF_HEIGHT,
            near: 0.1,
            far: 1000.0,
        }
    }
}

impl PortalProjection {
    fn through_aperture(eye: Vec3, exit: &PortalFrame, plane_distance: f32, far: f32) -> Self {
        let to_center = exit.center - eye;
        let camera_right = -exit.right;
        let center_x = to_center.dot(camera_right);
        let center_y = to_center.dot(exit.up);
        let near = plane_distance + PORTAL_VIEW_CLIP_OFFSET;
        let near_scale = near / plane_distance;
        Self {
            left: (center_x - PORTAL_HALF_WIDTH) * near_scale,
            right: (center_x + PORTAL_HALF_WIDTH) * near_scale,
            bottom: (center_y - PORTAL_HALF_HEIGHT) * near_scale,
            top: (center_y + PORTAL_HALF_HEIGHT) * near_scale,
            near,
            far: far.max(near + 1.0),
        }
    }

    fn matrix(&self) -> Mat4 {
        let x = 2.0 * self.near / (self.right - self.left);
        let y = 2.0 * self.near / (self.top - self.bottom);
        let a = (self.right + self.left) / (self.right - self.left);
        let b = (self.top + self.bottom) / (self.top - self.bottom);
        Mat4::from_cols(
            Vec4::new(x, 0.0, 0.0, 0.0),
            Vec4::new(0.0, y, 0.0, 0.0),
            Vec4::new(a, b, 0.0, -1.0),
            Vec4::new(0.0, 0.0, self.near, 0.0),
        )
    }

    fn corners_at(&self, z: f32) -> [Vec3A; 4] {
        let scale = z.abs() / self.near;
        [
            Vec3A::new(self.right * scale, self.bottom * scale, z),
            Vec3A::new(self.right * scale, self.top * scale, z),
            Vec3A::new(self.left * scale, self.top * scale, z),
            Vec3A::new(self.left * scale, self.bottom * scale, z),
        ]
    }
}

impl CameraProjection for PortalProjection {
    fn get_clip_from_view(&self) -> Mat4 {
        self.matrix()
    }

    fn get_clip_from_view_for_sub(&self, _sub_view: &SubCameraView) -> Mat4 {
        self.matrix()
    }

    fn update(&mut self, _width: f32, _height: f32) {}

    fn far(&self) -> f32 {
        self.far
    }

    fn get_frustum_corners(&self, z_near: f32, z_far: f32) -> [Vec3A; 8] {
        let near = self.corners_at(z_near);
        let far = self.corners_at(z_far);
        [near[0], near[1], near[2], near[3], far[0], far[1], far[2], far[3]]
    }
}

pub(super) fn portal_camera_view(
    eye: Vec3,
    entry: &PortalFrame,
    exit: &PortalFrame,
    far: f32,
) -> Option<(Transform, PortalProjection)> {
    let plane_distance = (eye - entry.center).dot(entry.normal);
    if plane_distance <= PORTAL_VIEW_CLIP_OFFSET {
        return None;
    }
    let mapped_eye = exit.center + traverse_vector(entry, exit, eye - entry.center);
    let transform = Transform::from_translation(mapped_eye).looking_to(exit.normal, exit.up);
    let projection = PortalProjection::through_aperture(mapped_eye, exit, plane_distance, far);
    Some((transform, projection))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_vec3_close(actual: Vec3, expected: Vec3) {
        assert!(actual.distance(expected) < 1e-5, "{actual:?} != {expected:?}");
    }

    #[test]
    fn camera_eye_maps_through_same_wall_pair() {
        let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
        let exit = PortalFrame::from_surface(Vec3::new(5.0, 0.0, 0.0), Vec3::Z, 0.0);
        let eye = entry.center + entry.right * 0.2 + entry.up * 0.3 + entry.normal * 3.0;
        let (transform, _) = portal_camera_view(eye, &entry, &exit, 100.0).expect("eye in front of portal");
        let expected = exit.center - exit.right * 0.2 + exit.up * 0.3 - exit.normal * 3.0;
        assert_vec3_close(transform.translation, expected);
        assert_vec3_close(transform.forward().as_vec3(), exit.normal);
        assert_vec3_close(transform.up().as_vec3(), exit.up);
    }

    #[test]
    fn projection_maps_aperture_corners_to_viewport_corners() {
        let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
        let exit = PortalFrame::from_surface(Vec3::new(5.0, 1.0, 0.0), Vec3::Z, 0.0);
        let eye = entry.center + entry.right * 0.25 + entry.up * 0.4 + entry.normal * 3.0;
        let (transform, projection) = portal_camera_view(eye, &entry, &exit, 100.0).expect("eye in front of portal");
        let view_from_world = transform.to_matrix().inverse();

        for (point, expected) in [
            (
                exit.center + exit.right * PORTAL_HALF_WIDTH - exit.up * PORTAL_HALF_HEIGHT,
                Vec2::new(-1.0, -1.0),
            ),
            (
                exit.center - exit.right * PORTAL_HALF_WIDTH + exit.up * PORTAL_HALF_HEIGHT,
                Vec2::new(1.0, 1.0),
            ),
        ] {
            let clip = projection.matrix() * view_from_world * point.extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!((ndc.x - expected.x).abs() < 1e-4, "{ndc:?}");
            assert!((ndc.y - expected.y).abs() < 1e-4, "{ndc:?}");
        }
    }

    #[test]
    fn camera_view_rejects_eye_behind_aperture() {
        let entry = PortalFrame::from_surface(Vec3::ZERO, Vec3::Z, 0.0);
        let exit = PortalFrame::from_surface(Vec3::X, Vec3::NEG_Z, 0.0);
        assert!(portal_camera_view(Vec3::NEG_Z, &entry, &exit, 100.0).is_none());
    }
}
