mod hits;
mod marker;
mod motion;
mod spawning;

pub use hits::{
    HitDirection, ProjectileCharacterHit, ball_character_hit, ball_overlaps_character, projectile_character_hit,
    projectile_overlaps_character,
};
pub use marker::ProjectileMarker;
pub use motion::{BarrierImpact, ProjectileMotion, WorldBounces};
pub use spawning::{ProjectileSpawnInfo, calculate_projectile_spawns};

#[cfg(test)]
mod tests;
