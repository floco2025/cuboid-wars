use bevy::prelude::*;

// Marker for the primary 3D camera (first-person / top-down view of the game world).
#[derive(Component)]
pub struct MainCameraMarker;

// Marker for the rearview-mirror 3D camera (separate viewport, shown in the corner).
#[derive(Component)]
pub struct RearviewCameraMarker;

// Marker for the window-facing 2D camera that draws the scene image and the HUD.
#[derive(Component)]
pub struct CompositorCameraMarker;

// Marker for the fullscreen sprite showing the scene image.
#[derive(Component)]
pub struct SceneSpriteMarker;
