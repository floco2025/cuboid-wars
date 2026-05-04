use bevy::prelude::*;

// ============================================================================
// Player Markers
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayerMarker;

// Marker component for character model entities (for animation/render settings)
#[derive(Component)]
pub struct CharacterModelMarker;

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

// Marker component for upper-level floor slabs. Level-0 ground is not tagged.
#[derive(Component)]
pub struct RoofMarker;

// Marker component for the ground plane (level-0 floor).
#[derive(Component)]
pub struct GroundMarker;

// Marker component for ramps
#[derive(Component)]
pub struct RampMarker;

// Marker component for wall light scene and light entities.
#[derive(Component)]
pub struct WallLightMarker;

// Records which level a map entity belongs to. Walls and floors get the
// level they sit on; ramps get the lower of the two levels they connect.
// Used by the level-focus toggle to hide entities not at the local
// player's current level.
#[derive(Component, Copy, Clone, Debug)]
pub struct MapLevel(pub u8);

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

// Marker component for character label text in UI render targets.
#[derive(Component)]
pub struct CharacterLabelTextMarker;

// Marker component for character label meshes in the world.
#[derive(Component)]
pub struct CharacterLabelMeshMarker;

// Lives on the character entity (player or actor). Stores the Entity id of
// its dedicated label-texture camera so the shared visibility system can
// toggle the camera's `is_active` flag based on distance + Health change.
#[derive(Component)]
pub struct LabelCamera(pub Entity);

#[derive(Component)]
pub struct HealthBarFillMarker {
    pub tracked_entity: Entity,
    pub max_health: f32,
}
