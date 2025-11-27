#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Time, Timer, TimerMode};

use crate::{constants::*, protocol::{Movement, Position, Velocity}};

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

// ============================================================================
// Shared Game Systems
// ============================================================================

// Movement system - integrates movement into position.
// Position uses meters in 3D space (X, Y=up/down, Z=forward/back).
// Y is always 0 for now (flat 2D gameplay).
// This runs on both client and server to ensure deterministic movement.
pub fn movement_system(time: Res<Time>, mut query: Query<(&mut Position, &Movement)>) {
    let delta = time.delta_secs();

    for (mut pos, mov) in query.iter_mut() {
        // Calculate actual velocity from movement state
        let speed_m_per_sec = match mov.vel {
            Velocity::Idle => 0.0,
            Velocity::Walk => WALK_SPEED,
            Velocity::Run => RUN_SPEED,
        };

        if speed_m_per_sec > 0.0 {
            // Calculate velocity vector from movement direction and speed
            // Using face_dir directly with Bevy's coordinate system
            let vel_x = mov.move_dir.sin() * speed_m_per_sec;
            let vel_z = mov.move_dir.cos() * speed_m_per_sec;

            // Integrate into position
            pos.x += vel_x * delta;
            pos.z += vel_z * delta;
            // pos.y stays at 0 for 2D gameplay
        }
    }
}

// Update projectile positions and despawn expired projectiles
pub fn projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile)>,
) {
    let delta = time.delta_secs();
    
    for (entity, mut pos, mut projectile) in projectile_query.iter_mut() {
        // Update position based on velocity
        pos.x += projectile.velocity.x * delta;
        pos.y += projectile.velocity.y * delta;
        pos.z += projectile.velocity.z * delta;
        
        // Update lifetime
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
