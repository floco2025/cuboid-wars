use bevy::prelude::*;

// ============================================================================
// Components
// ============================================================================

// Debounces the local player's collision (bump) sound: `was_colliding` gates
// re-triggering while still in contact, and `release_timer` keeps that gate
// closed for a short window after the last blocked frame.
#[derive(Component, Default)]
pub struct BumpFeedbackState {
    pub was_colliding: bool,
    pub release_timer: f32,
}

// ============================================================================
// Camera and Visual Effects
// ============================================================================

// Camera shake effect - tracks duration and intensity. `dir_{x,y,z}` is the
// shake "direction" each axis is modulated against, allowing the same
// component to serve both horizontal hits (small `dir_y` for a slight
// vertical jolt) and vertical impacts like fall damage (`dir_x = dir_z =
// 0`, larger `dir_y`).
#[derive(Component)]
pub struct CameraShake {
    pub timer: Timer,
    pub intensity: f32,
    pub dir_x: f32,
    pub dir_y: f32,
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
