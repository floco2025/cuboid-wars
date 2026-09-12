mod contact_explosions;
mod geometry;
mod health;
mod movement;
mod plugin;
mod spawning;

pub use common::physics::knockback_decay_system;
pub(crate) use geometry::{character_overlaps_item, character_surface_distance};
pub use health::characters_health_regeneration_system;
pub(crate) use health::regenerate_health;
pub use movement::characters_movement_system;
pub use plugin::characters_plugin;
pub(crate) use spawning::{
    generate_ground_actor_spawn_position, generate_player_spawn_position, sample_clear_position, spawn_face_yaw,
};

pub(crate) use spawning::generate_flying_spawn_position;
