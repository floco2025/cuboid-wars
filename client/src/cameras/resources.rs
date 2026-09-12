use bevy::prelude::*;

use crate::{config::FollowCameraConfig, constants::INPUT_ZOOM_SENSITIVITY_BASE};

// Camera view mode. `Debug` is a free orbit around the character's centre
// that passes through geometry and never locks the facing.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CameraViewMode {
    #[default]
    FirstPerson,
    ThirdPerson,
    Debug,
}

impl CameraViewMode {
    #[must_use]
    pub const fn is_first_person(self) -> bool {
        matches!(self, Self::FirstPerson)
    }

    #[must_use]
    pub const fn is_debug(self) -> bool {
        matches!(self, Self::Debug)
    }
}

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
    pub debug_distance: f32,
}

impl FollowCamera {
    pub fn pivot_blend(&self) -> f32 {
        SmoothStepCurve.sample_clamped(self.distance)
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

    pub fn toggle_debug(&mut self, view: CameraViewMode, far_distance: f32) -> CameraViewMode {
        self.previous_pivot = None;
        match view {
            CameraViewMode::FirstPerson | CameraViewMode::ThirdPerson => {
                self.debug_distance = far_distance;
                CameraViewMode::Debug
            }
            CameraViewMode::Debug => {
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
    // up with. In the debug view it moves the debug distance alone, uncapped.
    pub fn zoom(&mut self, view: CameraViewMode, wheel: f32, sensitivity: f32, config: FollowCameraConfig) {
        if wheel == 0.0 {
            return;
        }
        if view.is_debug() {
            self.debug_distance = (self.debug_distance - wheel * INPUT_ZOOM_SENSITIVITY_BASE * sensitivity).max(0.0);
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
            debug_distance: 0.0,
        }
    }
}

#[derive(Resource, Debug, Default)]
pub struct CameraInputState {
    pub released: bool,
    pub suppress_fire: bool,
    pub mouse_delta: Vec2,
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
