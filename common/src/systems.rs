#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_time::Time;

use crate::{components::Projectile, protocol::{Movement, Position, Velocity}};

// ============================================================================
// Shared Game Systems
// ============================================================================

// Movement system - integrates movement into position.
// Position uses millimeter fixed-point scale (i32 = millimeters).
// This runs on both client and server to ensure deterministic movement.
pub fn movement_system(time: Res<Time>, mut query: Query<(&mut Position, &Movement)>) {
    let delta = time.delta_secs();

    for (mut pos, mov) in query.iter_mut() {
        // Calculate actual velocity from movement state
        let speed_mm_per_sec = match mov.vel {
            Velocity::Idle => 0.0,
            Velocity::Walk => 200_000.0, // mm/sec
            Velocity::Run => 300_000.0,  // mm/sec
        };

        if speed_mm_per_sec > 0.0 {
            // Calculate velocity vector from movement direction and speed
            // move_dir of 0 means moving in -Y direction (forward when camera_rot=0)
            let vel_x = mov.move_dir.sin() * speed_mm_per_sec;
            let vel_y = -mov.move_dir.cos() * speed_mm_per_sec;

            // Integrate into position
            let dx = (vel_x * delta) as i32;
            let dy = (vel_y * delta) as i32;

            if dx != 0 || dy != 0 {
                pos.x += dx;
                pos.y += dy;
            }
        }
    }
}

// Update projectile positions and despawn expired projectiles
// This system doesn't use Transform - it only updates the lifetime
// The Position component would need to be added separately if tracking is needed
pub fn update_projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Projectile)>,
) {
    for (entity, mut projectile) in projectile_query.iter_mut() {
        projectile.lifetime.tick(time.delta());
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
