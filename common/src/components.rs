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
pub const PROJECTILE_SPAWN_OFFSET: f32 = 50.0; // meters in front of shooter
pub const PROJECTILE_SPAWN_HEIGHT: f32 = 60.0; // meters above ground (eye level)

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
            face_dir.sin() * PROJECTILE_SPEED,
            0.0,
            face_dir.cos() * PROJECTILE_SPEED,
        );
        
        Self {
            velocity,
            lifetime: Timer::from_seconds(PROJECTILE_LIFETIME, TimerMode::Once),
        }
    }
    
    // Calculate spawn position in front of shooter
    // Returns Vec3 position in meters
    pub fn calculate_spawn_position(shooter_pos: Vec3, face_dir: f32) -> Vec3 {
        Vec3::new(
            shooter_pos.x + face_dir.sin() * PROJECTILE_SPAWN_OFFSET,
            PROJECTILE_SPAWN_HEIGHT,
            shooter_pos.z + face_dir.cos() * PROJECTILE_SPAWN_OFFSET,
        )
    }
}
