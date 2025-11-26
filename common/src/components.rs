#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

// ============================================================================
// Shared Game Components
// ============================================================================

// Projectile constants
pub const PROJECTILE_SPEED: f32 = 2000.0; // meters per second
pub const PROJECTILE_LIFETIME: f32 = 2.5; // seconds
pub const PROJECTILE_SPAWN_OFFSET: f32 = 500.0; // millimeters in front of shooter

// Component for projectiles
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
}

impl Projectile {
    // Create a new projectile with standard parameters
    // face_dir is the shooter's facing direction in radians
    pub fn new(face_dir: f32) -> Self {
        let velocity = Vec3::new(
            -face_dir.sin() * PROJECTILE_SPEED,
            0.0,
            -face_dir.cos() * PROJECTILE_SPEED,
        );
        
        Self {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        }
    }
    
    // Calculate spawn position in front of shooter
    // Returns (x, y) in millimeters
    pub fn calculate_spawn_position(shooter_x: i32, shooter_y: i32, face_dir: f32) -> (f32, f32) {
        let spawn_x = shooter_x as f32 + (-face_dir.sin()) * PROJECTILE_SPAWN_OFFSET;
        let spawn_y = shooter_y as f32 + (-face_dir.cos()) * PROJECTILE_SPAWN_OFFSET;
        (spawn_x, spawn_y)
    }
}
