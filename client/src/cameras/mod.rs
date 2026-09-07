mod components;
mod resources;
mod scene_target;
mod setup;

pub use components::{CompositorCameraMarker, MainCameraMarker, RearviewCameraMarker, SkyDiscRenderLayer};
pub(crate) use components::{
    RENDER_LAYER_CHARACTER_LABEL, RENDER_LAYER_LOCAL_PLAYER, RENDER_LAYER_MAIN_VIEW, RENDER_LAYER_PORTAL_VIEW_START,
    RENDER_LAYER_REARVIEW,
};
pub use resources::{CameraViewMode, SceneRenderTarget, TopDownCameraYaw};
pub use scene_target::scene_render_target_system;
pub use setup::{clamp_msaa_to_device_system, setup_cameras_system, supported_msaa_samples};
