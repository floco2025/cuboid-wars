#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Timer, TimerMode};

use crate::constants::*;

// ============================================================================
// Shared Game Components
// ============================================================================

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
