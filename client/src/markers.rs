use bevy::prelude::*;

// ============================================================================
// Player Markers
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayerMarker;

// Marker component for player model entities (for animation)
#[derive(Component)]
pub struct PlayerModelMarker;

// Marker component for sentry model entities (for animation)
#[derive(Component)]
pub struct SentryModelMarker;

// ============================================================================
// Camera Markers
// ============================================================================

// Marker component for the main camera
#[derive(Component)]
pub struct MainCameraMarker;

// Marker component for the rearview mirror camera
#[derive(Component)]
pub struct RearviewCameraMarker;

// ============================================================================
// Map Markers
// ============================================================================

// Marker component for walls
#[derive(Component)]
pub struct WallMarker;

// Marker component for roofs
#[derive(Component)]
pub struct RoofMarker;

// Marker component for roof walls
#[derive(Component)]
pub struct RoofWallMarker;

// Marker component for ramps
#[derive(Component)]
pub struct RampMarker;

// ============================================================================
// UI Markers
// ============================================================================

// Marker component for the player list UI
#[derive(Component)]
pub struct PlayerListUIMarker;

// Marker component for the crosshair UI
#[derive(Component)]
pub struct CrosshairUIMarker;

// Marker component for the RTT display
#[derive(Component)]
pub struct RttUIMarker;

// Marker component for the FPS display
#[derive(Component)]
pub struct FpsUIMarker;

// Marker component for the bump flash overlay
#[derive(Component)]
pub struct BumpFlashUIMarker;

// Marker component for player entry rows
#[derive(Component)]
pub struct PlayerEntryMarker;

// Marker component for player ID text in UI
#[derive(Component)]
pub struct PlayerIdTextMarker;

// Marker component for player ID text mesh in world
#[derive(Component)]
pub struct PlayerIdTextMeshMarker;
