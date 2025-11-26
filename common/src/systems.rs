#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_time::Time;

use crate::protocol::{Position, Velocity};

// ============================================================================
// Shared Game Systems
// ============================================================================

// Movement system - integrates velocity into position.
// Position uses millimeter fixed-point scale (i32 = millimeters).
// This runs on both client and server to ensure deterministic movement.
pub fn movement_system(
    time: Res<Time>,
    mut query: Query<(&mut Position, &Velocity)>,
) {
    let delta = time.delta_secs();
    
    for (mut pos, vel) in query.iter_mut() {
        // Velocity is in mm/sec, delta is in seconds
        // Result is in millimeters
        let dx = (vel.x * delta) as i32;
        let dy = (vel.y * delta) as i32;
        
        // Update position only if it changed
        if dx != 0 || dy != 0 {
            pos.x += dx;
            pos.y += dy;
        }
    }
}
