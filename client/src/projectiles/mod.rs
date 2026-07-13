mod audio;
mod collision;
mod movement;
mod rendering;
mod spawn;

pub use audio::LastBounceSound;
pub use movement::projectiles_movement_system;
pub use rendering::projectiles_transform_sync_system;
pub use spawn::{ProjectileAssets, spawn_projectiles};
