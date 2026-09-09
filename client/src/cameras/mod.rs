mod aim;
mod components;
mod follow;
mod rearview;
mod resources;
mod scene_target;
mod setup;
mod third_person;
mod top_down;
mod visibility;

pub use aim::camera_aim_system;
pub use components::{CompositorCameraMarker, MainCameraMarker, RearviewCameraMarker, SkyDiscRenderLayer};
pub(crate) use components::{
    RENDER_LAYER_CHARACTER_LABEL, RENDER_LAYER_LOCAL_PLAYER, RENDER_LAYER_MAIN_VIEW, RENDER_LAYER_PORTAL_VIEW_START,
    RENDER_LAYER_REARVIEW,
};
pub use follow::local_player_camera_sync_system;
pub use rearview::{local_player_rearview_sync_system, local_player_rearview_viewport_system};
pub use resources::{CameraAim, CameraInputState, CameraViewMode, FollowCamera, SceneRenderTarget, TopDownCameraYaw};
pub use scene_target::scene_render_target_system;
pub use setup::{clamp_msaa_to_device_system, setup_cameras_system, supported_msaa_samples};
pub use visibility::local_player_view_mode_system;
pub(crate) use visibility::{
    LocalPlayerLabelMarker, local_player_light_layer_system, local_player_render_layer_system,
};

mod plugin;

pub use plugin::camera_plugin;
