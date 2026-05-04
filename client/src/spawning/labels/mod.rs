mod floating;
mod texture;

pub use floating::{
    CharacterLabelMeshMarker, CharacterLabelTextMarker, LabelCamera, spawn_floating_actor_health_bar,
    spawn_floating_player_label,
};
pub use texture::setup_label_texture;
