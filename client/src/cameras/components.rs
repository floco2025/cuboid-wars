use bevy::prelude::*;

// Layer 0 contains the shared world geometry visible from every scene camera.
pub(crate) const RENDER_LAYER_MAIN_VIEW: usize = 1;
pub(crate) const RENDER_LAYER_LOCAL_PLAYER: usize = 2;
// Billboards face the main camera, so other views must hide them.
pub(crate) const RENDER_LAYER_CHARACTER_LABEL: usize = 3;
pub(crate) const RENDER_LAYER_PORTAL_VIEW_START: usize = 4;

// Marker for the primary 3D camera.
#[derive(Component)]
pub struct MainCameraMarker;

// Marker for the window-facing 2D camera that draws the scene image and the HUD.
#[derive(Component)]
pub struct CompositorCameraMarker;

// The layer a 3D camera's own procedural sky renders on — one only that camera has.
#[derive(Component)]
pub struct SkyRenderLayer(pub usize);
