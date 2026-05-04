mod hits;
mod marker;
mod motion;

pub use hits::{HitDirection, ProjectileCharacterHit, projectile_character_hit, projectile_hits_character};
pub use marker::ProjectileMarker;
pub use motion::ProjectileMotion;

#[cfg(test)]
mod tests;
