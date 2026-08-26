mod beam;
mod damage;
mod explosions;
mod plugin;
mod resources;

pub use beam::actors_beam_damage_system;
pub use damage::{
    DeathSource, apply_actor_projectile_hit, apply_player_beam_damage, apply_player_projectile_hit, award_actor_kill,
    kill_actor, kill_credit, kill_player,
};
pub use explosions::explosions_system;
pub use plugin::combat_plugin;
pub use resources::{PendingExplosion, PendingExplosions};
