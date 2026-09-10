mod hits;
mod plugin;
mod resources;
mod spawn;

pub use plugin::projectiles_plugin;
pub use resources::PendingProjectileHits;
pub(crate) use spawn::handle_projectile_shot_message;

#[cfg(test)]
mod hits_tests;
