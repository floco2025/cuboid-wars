mod billboard;
mod health_bar;
mod name_label;
mod render_target;
mod spawn;

pub use billboard::floating_labels_billboard_system;
pub use health_bar::floating_health_bar_fill_system;
pub use name_label::{floating_label_scale_compensation_system, player_name_label_render_system};
pub use render_target::setup_label_texture;
pub use spawn::{LabelCamera, spawn_floating_health_bar, spawn_floating_player_label};
