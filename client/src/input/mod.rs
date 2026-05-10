mod movement;
mod shooting;
mod toggles;

pub use movement::input_movement_system;
pub use shooting::input_shooting_system;
pub use toggles::{
    input_camera_view_toggle_system, input_cursor_toggle_system, input_debug_colors_cycle_system,
    input_fullscreen_toggle_system, input_level_focus_toggle_system,
};
