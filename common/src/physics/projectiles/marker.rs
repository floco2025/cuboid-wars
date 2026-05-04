use bevy_ecs::prelude::*;

// Tagged on every projectile spawn so movement and collision systems can query
// projectiles in isolation.
#[derive(Component, Debug, Default)]
pub struct ProjectileMarker;
