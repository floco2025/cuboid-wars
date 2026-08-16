mod animation;
mod assets;
mod particles;
mod scorch;
mod shards;
mod smoke;
mod spawn;

pub use animation::{explosion_lights_system, explosion_pulse_system};
pub(crate) use assets::with_white_vertex_colors;
pub use assets::{ExplosionAssets, ExplosionRadii, explosion_sound_speed};
pub use particles::{ExplosionVfxBudget, explosion_particles_system};
pub(crate) use scorch::ScorchOutline;
pub use scorch::scorch_marks_system;
pub use spawn::{ExplosionSpawnCtx, spawn_actor_explosion, spawn_missile_explosion, spawn_player_explosion};
