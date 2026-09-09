use bevy::prelude::*;

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
