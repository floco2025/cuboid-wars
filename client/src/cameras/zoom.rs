use super::{CameraViewMode, FollowCamera};
use crate::{config::FollowCameraConfig, constants::INPUT_ZOOM_SENSITIVITY_BASE};

impl FollowCamera {
    pub fn visible_distance(&self, config: FollowCameraConfig) -> f32 {
        // Clamped like the arm in `third_person_transform`, so body visibility
        // and the camera agree when the stored distance exceeds the maximum.
        let distance = self.distance.min(config.max_distance);
        if self.previous_pivot.is_none() || distance <= 0.0 {
            return distance;
        }
        // The collision arm includes the shoulder offset; zoom measures only its rearward component.
        let arm = distance.hypot(config.shoulder_offset * self.pivot_blend());
        distance.min(self.arm_distance * distance / arm)
    }

    pub fn toggle_top_down(&mut self, view: CameraViewMode) -> CameraViewMode {
        self.previous_pivot = None;
        match view {
            CameraViewMode::FirstPerson | CameraViewMode::ThirdPerson => CameraViewMode::TopDown,
            CameraViewMode::TopDown => {
                if self.distance == 0.0 {
                    CameraViewMode::FirstPerson
                } else {
                    CameraViewMode::ThirdPerson
                }
            }
        }
    }

    pub fn zoom(
        &mut self,
        view: CameraViewMode,
        wheel: f32,
        sensitivity: f32,
        config: FollowCameraConfig,
    ) -> CameraViewMode {
        if view.is_top_down() || wheel == 0.0 {
            return view;
        }
        let initial = if wheel > 0.0 && self.previous_pivot.is_some() && self.distance > 0.0 {
            if view.is_first_person() {
                0.0
            } else {
                self.visible_distance(config)
            }
        } else {
            self.distance
        };
        let distance = (initial - wheel * INPUT_ZOOM_SENSITIVITY_BASE * sensitivity).clamp(0.0, config.max_distance);
        self.distance = distance;
        if distance > config.first_person_distance {
            if initial <= config.first_person_distance {
                self.previous_pivot = None;
            }
            CameraViewMode::ThirdPerson
        } else {
            if wheel > 0.0 {
                self.distance = 0.0;
            }
            self.locked = true;
            CameraViewMode::FirstPerson
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_geometry::follow_camera as config;
    use bevy::prelude::Vec3;

    #[test]
    fn obstruction_does_not_reduce_requested_distance_when_scrolling_out() {
        let config = config();
        let mut camera = FollowCamera {
            distance: 4.0,
            arm_distance: 1.5,
            previous_pivot: Some(Vec3::ZERO),
            ..Default::default()
        };
        camera.zoom(
            CameraViewMode::ThirdPerson,
            -1.0,
            0.2 / INPUT_ZOOM_SENSITIVITY_BASE,
            config,
        );
        assert!((camera.distance - 4.2).abs() < 1e-5);
    }

    #[test]
    fn zoom_into_first_person_locks_and_zooming_out_keeps_lock() {
        let config = config();
        let mut camera = FollowCamera {
            distance: 1.0,
            locked: false,
            ..Default::default()
        };
        assert_eq!(
            camera.zoom(
                CameraViewMode::ThirdPerson,
                2.0,
                0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
                config
            ),
            CameraViewMode::FirstPerson
        );
        assert!(camera.locked);
        assert_eq!(
            camera.zoom(
                CameraViewMode::FirstPerson,
                -2.0,
                0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
                config
            ),
            CameraViewMode::ThirdPerson
        );
        assert!(camera.locked);
    }
    #[test]
    fn tiny_scroll_events_accumulate_without_flipping_modes() {
        let config = config();
        let mut camera = FollowCamera::default();
        for _ in 0..5 {
            assert_eq!(
                camera.zoom(
                    CameraViewMode::FirstPerson,
                    -0.25,
                    0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
                    config
                ),
                CameraViewMode::FirstPerson
            );
        }
        assert_eq!(
            camera.zoom(
                CameraViewMode::FirstPerson,
                -0.25,
                0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
                config
            ),
            CameraViewMode::ThirdPerson
        );
        assert_eq!(
            camera.zoom(
                CameraViewMode::ThirdPerson,
                0.02,
                0.5 / INPUT_ZOOM_SENSITIVITY_BASE,
                config
            ),
            CameraViewMode::ThirdPerson
        );
    }
    #[test]
    fn top_down_restores_zoom_and_ignores_scroll() {
        let config = config();
        for (view, distance) in [(CameraViewMode::FirstPerson, 0.0), (CameraViewMode::ThirdPerson, 3.0)] {
            let mut camera = FollowCamera {
                distance,
                ..Default::default()
            };
            assert_eq!(camera.toggle_top_down(view), CameraViewMode::TopDown);
            assert_eq!(
                camera.zoom(CameraViewMode::TopDown, 10.0, 0.5 / INPUT_ZOOM_SENSITIVITY_BASE, config),
                CameraViewMode::TopDown
            );
            assert_eq!(camera.toggle_top_down(CameraViewMode::TopDown), view);
            assert_eq!(camera.distance, distance);
        }
    }
    #[test]
    fn sensitivity_scales_zoom_and_distance_is_bounded() {
        let config = config();
        let mut camera = FollowCamera {
            distance: 3.0,
            ..Default::default()
        };
        camera.zoom(
            CameraViewMode::ThirdPerson,
            -1.0,
            1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
            config,
        );
        assert_eq!(camera.distance, 4.0);
        camera.zoom(
            CameraViewMode::ThirdPerson,
            -100.0,
            1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
            config,
        );
        assert_eq!(camera.distance, config.max_distance);
        camera.zoom(
            CameraViewMode::ThirdPerson,
            100.0,
            1.0 / INPUT_ZOOM_SENSITIVITY_BASE,
            config,
        );
        assert_eq!(camera.distance, 0.0);
    }
}
