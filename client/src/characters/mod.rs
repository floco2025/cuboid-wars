mod animation;
mod components;
mod model;
mod movement;
mod resources;
mod spawn;
mod visual_turn;

pub use animation::{AnimationToPlay, character_animation_system};
pub use common::physics::knockback_decay_system;
pub use components::PreviousTickPosition;
pub use model::{CharacterModel, character_models_attach_system, load_character_model, model_transform};
pub use movement::{capture_previous_tick_position_system, characters_movement_system};
pub use resources::{BoundsMode, MaxHealth};
pub use spawn::{
    character_bounds_sync_system, grounding_debug_system, refresh_grounding_debug_system, spawn_character_bounds,
};
pub use visual_turn::characters_visual_turn_system;

mod plugin;

pub use plugin::{character_sync_plugin, prediction_plugin};
