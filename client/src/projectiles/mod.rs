mod audio;
mod collision;
mod movement;
mod spawn;
mod transform_sync;

pub use audio::LastBounceSound;
pub use movement::projectiles_movement_system;
pub use spawn::{ProjectileAssets, spawn_projectiles};
pub use transform_sync::projectiles_transform_sync_system;
