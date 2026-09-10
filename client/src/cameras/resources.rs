use bevy::prelude::*;

use crate::{config::FollowCameraConfig, constants::INPUT_ZOOM_SENSITIVITY_BASE};

// Camera view mode.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CameraViewMode {
    #[default]
    FirstPerson,
    ThirdPerson,
    TopDown,
}

impl CameraViewMode {
    #[must_use]
    pub const fn is_first_person(self) -> bool {
        matches!(self, Self::FirstPerson)
    }

    #[must_use]
    pub const fn is_top_down(self) -> bool {
        matches!(self, Self::TopDown)
    }
}

// Horizontal rotation of the top-down camera around the current level center.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct TopDownCameraYaw(pub f32);

// The offscreen image the 3D cameras render into; the compositor camera
// upscales it to the window. `size` mirrors the image so consumers don't
// need `Assets<Image>`.
#[derive(Resource)]
pub struct SceneRenderTarget {
    pub handle: Handle<Image>,
    pub size: UVec2,
}

#[derive(Resource, Debug)]
pub struct FollowCamera {
    pub locked: bool,
    pub distance: f32,
    pub arm_distance: f32,
    pub previous_pivot: Option<Vec3>,
}

impl FollowCamera {
    pub fn pivot_blend(&self) -> f32 {
        let blend = self.distance.clamp(0.0, 1.0);
        blend * blend * (3.0 - 2.0 * blend)
    }

    // The shoulder offset eases in with the pivot, so a slight zoom-out does
    // not jump the camera sideways.
    pub fn shoulder_offset(&self, config: FollowCameraConfig) -> f32 {
        config.shoulder_offset * self.pivot_blend()
    }

    pub fn visible_distance(&self, config: FollowCameraConfig) -> f32 {
        // Clamped like the arm in `third_person_transform`, so body visibility
        // and the camera agree when the stored distance exceeds the maximum.
        let distance = self.distance.min(config.max_distance);
        if self.previous_pivot.is_none() || distance <= 0.0 {
            return distance;
        }
        // The collision arm includes the shoulder offset; zoom measures only its rearward component.
        let arm = distance.hypot(self.shoulder_offset(config));
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

    // Wheel zoom updates the requested distance and the facing lock only; the
    // follow camera derives the first/third-person view from the arm it ends
    // up with.
    pub fn zoom(&mut self, view: CameraViewMode, wheel: f32, sensitivity: f32, config: FollowCameraConfig) {
        if view.is_top_down() || wheel == 0.0 {
            return;
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
        } else {
            if wheel > 0.0 {
                self.distance = 0.0;
            }
            self.locked = true;
        }
    }
}

impl Default for FollowCamera {
    fn default() -> Self {
        Self {
            locked: true,
            distance: 0.0,
            arm_distance: 0.0,
            previous_pivot: None,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct CameraInputState {
    pub released: bool,
    pub suppress_fire: bool,
}

#[derive(Resource, Debug, Default)]
pub struct CameraAim {
    pub origin: Vec3,
    pub direction: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub crosshair_height_offset: f32,
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
