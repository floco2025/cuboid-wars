#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use common::protocol::PlayerId;

// ============================================================================
// Bevy Components
// ============================================================================

// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;

// Marker component for the player list UI
#[derive(Component)]
pub struct PlayerListUI;

// Marker component for individual player entries
#[derive(Component)]
pub struct PlayerEntryUI(pub PlayerId);

// Marker component for the crosshair UI
#[derive(Component)]
pub struct CrosshairUI;

// Camera shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
}

// Cuboid shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CuboidShake {
    pub timer: Timer,
    pub intensity: f32,
}
