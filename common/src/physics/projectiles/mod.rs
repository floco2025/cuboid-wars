mod hits;
mod motion;
mod spawning;

pub use hits::{projectile_character_hit, projectile_overlaps_character};
pub use motion::{BarrierImpact, ProjectileMotion, WorldBounces};
pub use spawning::{ProjectileSpawnInfo, calculate_projectile_spawns};

#[cfg(test)]
mod tests;
