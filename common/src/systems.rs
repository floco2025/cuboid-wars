use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::{Time, Timer, TimerMode};

use crate::{constants::*, protocol::Position};

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
    #[must_use]
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
    #[must_use]
    pub fn calculate_spawn_position(shooter_pos: Vec3, face_dir: f32) -> Vec3 {
        Vec3::new(
            face_dir.sin().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.x),
            PROJECTILE_SPAWN_HEIGHT,
            face_dir.cos().mul_add(PROJECTILE_SPAWN_OFFSET, shooter_pos.z),
        )
    }
}

// ============================================================================
// Projectiles Movement System
// ============================================================================

// Update projectile positions and despawn them once their timer elapses.
pub fn projectiles_system(
    mut commands: Commands,
    time: Res<Time>,
    mut projectile_query: Query<(Entity, &mut Position, &mut Projectile)>,
) {
    let delta_seconds = time.delta_secs();
    let frame_time = time.delta();

    for (entity, mut pos, mut projectile) in &mut projectile_query {
        pos.x += projectile.velocity.x * delta_seconds;
        pos.y += projectile.velocity.y * delta_seconds;
        pos.z += projectile.velocity.z * delta_seconds;

        projectile.lifetime.tick(frame_time);
        if projectile.lifetime.is_finished() {
            commands.entity(entity).despawn();
        }
    }
}
