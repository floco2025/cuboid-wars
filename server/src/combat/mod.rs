mod beam;
mod damage;
mod explosions;
mod resources;

pub use beam::actor_beam_damage_system;
pub use damage::{
    apply_actor_projectile_hit, apply_player_beam_damage, apply_player_projectile_hit, kill_actor, kill_player,
};
pub use explosions::explosions_system;
pub use resources::{PendingExplosion, PendingExplosions};
