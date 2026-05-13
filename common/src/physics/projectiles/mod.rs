mod hits;
mod marker;
mod motion;
mod spawning;

pub use hits::{HitDirection, ProjectileCharacterHit, projectile_character_hit, projectile_hits_character};
pub use marker::ProjectileMarker;
pub use motion::ProjectileMotion;
pub use spawning::{ProjectileSpawnInfo, calculate_projectile_spawns};

#[cfg(test)]
mod tests;
