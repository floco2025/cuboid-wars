#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;

// ============================================================================
// Client Components
// ============================================================================

/// Marker component for the local player (yourself)
#[derive(Component)]
pub struct LocalPlayer;
