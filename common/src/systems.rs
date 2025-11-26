#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_time::Time;

use crate::{components::Projectile, constants::*, protocol::{Movement, Position, Velocity}};

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
pub fn update_projectiles_system(
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
