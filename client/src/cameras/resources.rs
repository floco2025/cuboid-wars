use bevy::prelude::*;

// Camera view mode.
#[derive(Resource, Default, PartialEq, Eq, Clone, Copy, Debug)]
pub enum CameraViewMode {
    #[default]
    FirstPerson,
    TopDown,
}

impl CameraViewMode {
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::FirstPerson => Self::TopDown,
            Self::TopDown => Self::FirstPerson,
        }
    }

    #[must_use]
    pub const fn is_first_person(self) -> bool {
        matches!(self, Self::FirstPerson)
    }

    #[must_use]
    pub const fn is_top_down(self) -> bool {
        !self.is_first_person()
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
