mod render_target;
mod spawn;
mod systems;

pub use render_target::setup_label_texture;
pub use spawn::{LabelCamera, spawn_floating_health_bar, spawn_floating_player_label};
pub use systems::{
    floating_health_bar_fill_system, floating_label_scale_compensation_system, floating_labels_billboard_system,
    player_name_label_render_system,
};
