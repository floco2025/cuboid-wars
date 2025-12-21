use bevy::prelude::*;

use common::protocol::{FaceDirection, Position};

// ============================================================================
// Components
// ============================================================================

// Track bump flash effect state for local player
#[derive(Component, Default)]
pub struct BumpFlashState {
    pub was_colliding: bool,
    pub flash_timer: f32,
}

// ============================================================================
// Query Bundles
// ============================================================================

// Common query for player movement (read-only)
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub struct PlayerMovement {
    pub position: &'static Position,
    pub face_direction: &'static FaceDirection,
}

// Common query for player movement (mutable)
#[derive(bevy::ecs::query::QueryData)]
#[query_data(mutable)]
pub struct PlayerMovementMut {
    pub velocity: &'static mut common::protocol::Velocity,
    pub face_direction: &'static mut FaceDirection,
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
