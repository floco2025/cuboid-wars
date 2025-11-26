#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_time::Time;

use crate::protocol::{Movement, Position, Velocity};

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
