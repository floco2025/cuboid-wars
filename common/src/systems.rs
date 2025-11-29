#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Time, Timer, TimerMode};

use crate::{
    constants::*,
    protocol::Position,
};

// ============================================================================
// Shared Game Components
// ============================================================================

// Component attached to projectile entities to track velocity and lifetime.
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
}

impl Projectile {
    // Create a new projectile traveling along the provided facing direction.
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

    // Calculate the spawn position in front of a shooter.
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

// Update projectile positions and despawn them once their timer elapses.
pub fn projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile)>,
) {
    let delta_seconds = time.delta_secs();
    let frame_time = time.delta();

    for (entity, mut pos, mut projectile) in projectile_query.iter_mut() {
        pos.x += projectile.velocity.x * delta_seconds;
        pos.y += projectile.velocity.y * delta_seconds;
        pos.z += projectile.velocity.z * delta_seconds;

        projectile.lifetime.tick(frame_time);
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
