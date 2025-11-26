#[allow(clippy::wildcard_imports)]
use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use bevy_time::Timer;

// ============================================================================
// Shared Game Components
// ============================================================================

// Component for projectiles
#[derive(Component)]
pub struct Projectile {
    pub velocity: Vec3,
    pub lifetime: Timer,
}
