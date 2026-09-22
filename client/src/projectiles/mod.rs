mod audio;
mod collision;
mod events;
mod hits;
mod motion;
mod movement;
mod spawn;
mod spawning;
mod step;
mod transform_sync;

pub use audio::LastBounceSound;
pub(crate) use events::{PROJECTILE_EVENT_LIMIT, ProjectileEvent, earliest_projectile_event};
pub use hits::{projectile_character_hit, projectile_overlaps_character};
pub use motion::{FieldImpact, ProjectileMotion, SurfaceBounce};
pub use movement::projectiles_movement_system;
pub use spawn::{ProjectileAssets, ProjectileMarker, spawn_ember_projectile, spawn_projectiles};
pub use spawning::{MuzzleCheck, ProjectileSpawnInfo, calculate_projectile_spawns};
pub use step::{ProjectileEnvironment, ProjectileFlightEvent, ProjectileStep, step_projectile};
pub use transform_sync::projectiles_transform_sync_system;

#[cfg(test)]
#[path = "tests/movement.rs"]
mod movement_tests;
