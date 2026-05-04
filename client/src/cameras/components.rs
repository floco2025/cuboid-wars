use bevy::prelude::*;

// Marker for the primary 3D camera (first-person / top-down view of the game world).
#[derive(Component)]
pub struct MainCameraMarker;

// Marker for the rearview-mirror 3D camera (separate viewport, shown in the corner).
#[derive(Component)]
pub struct RearviewCameraMarker;
