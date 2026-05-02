use bevy::prelude::*;

// ============================================================================
// Components
// ============================================================================

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
    pub release_timer: f32,
}

#[derive(Component)]
pub struct CharacterVisualTurn {
    pub start_yaw: f32,
    pub target_yaw: f32,
    pub elapsed: f32,
    pub duration: f32,
}

impl CharacterVisualTurn {
    pub const fn settled(yaw: f32) -> Self {
        Self {
            start_yaw: yaw,
            target_yaw: yaw,
            elapsed: 0.0,
            duration: 0.0,
        }
    }
}

// ============================================================================
// Camera and Visual Effects
// ============================================================================

// Camera shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32, // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_y: f32,
    pub offset_z: f32,
}

// Cuboid shake effect - tracks duration and intensity
#[derive(Component)]
pub struct CuboidShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32, // Direction of impact
    pub dir_z: f32,
    pub offset_x: f32, // Current shake offset
    pub offset_z: f32,
}
