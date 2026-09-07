use bevy::prelude::*;

// Layer 0 contains the shared world geometry visible from every scene camera.
pub(crate) const RENDER_LAYER_MAIN_VIEW: usize = 1;
pub(crate) const RENDER_LAYER_LOCAL_PLAYER: usize = 2;
pub(crate) const RENDER_LAYER_REARVIEW: usize = 3;
// Billboards face the main camera, so other views must hide them.
pub(crate) const RENDER_LAYER_CHARACTER_LABEL: usize = 4;
pub(crate) const RENDER_LAYER_PORTAL_VIEW_START: usize = 5;

// Marker for the primary 3D camera (first-person / top-down view of the game world).
#[derive(Component)]
pub struct MainCameraMarker;

// Marker for the rearview-mirror 3D camera (separate viewport, shown in the corner).
#[derive(Component)]
pub struct RearviewCameraMarker;

// Marker for the window-facing 2D camera that draws the scene image and the HUD.
#[derive(Component)]
pub struct CompositorCameraMarker;

// The layer a 3D camera's own sky disc renders on — one only that camera has.
#[derive(Component)]
pub struct SkyDiscRenderLayer(pub usize);
