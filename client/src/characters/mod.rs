mod animation;
mod components;
mod movement;
mod resources;
mod spawn;
mod visual_turn;

pub use animation::{AnimationToPlay, character_animation_system};
pub use common::physics::knockback_decay_system;
pub use components::PreviousTickPosition;
pub use movement::{capture_previous_tick_position_system, characters_movement_system};
pub use resources::MaxHealth;
pub use spawn::spawn_collider_box;
pub use visual_turn::characters_visual_turn_system;

mod plugin;

pub use plugin::{character_sync_plugin, prediction_plugin};
