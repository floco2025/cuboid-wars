mod commit;
mod missiles;
mod movement;
mod shooting;
mod toggles;

pub use commit::commit_player_input_system;
pub use missiles::input_missile_system;
pub use movement::input_movement_system;
pub use shooting::input_shooting_system;
pub use toggles::{
    enter_borderless_fullscreen, input_camera_view_toggle_system, input_debug_colors_cycle_system,
    input_fullscreen_toggle_system, input_level_focus_toggle_system,
};

mod plugin;

pub use plugin::input_plugin;
