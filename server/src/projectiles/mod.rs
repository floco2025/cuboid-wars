mod hits;
mod plugin;
mod resources;
mod spawn;

#[cfg(test)]
#[path = "tests/hits.rs"]
mod hits_tests;

pub use plugin::projectiles_plugin;
pub use resources::PendingProjectileHits;
pub(crate) use spawn::handle_projectile_shot_message;
