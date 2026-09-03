mod commit;
mod cursor;
mod missiles;
mod movement;
mod portals;
mod shooting;
mod toggles;
mod weapons;

pub use commit::{commit_player_input_system, send_client_snapshot_system};
pub use cursor::input_cursor_capture_system;
pub use missiles::input_missile_system;
pub use movement::{MAX_PITCH, input_movement_system};
pub use portals::input_portal_system;
pub use shooting::input_shooting_system;
pub use toggles::{
    enter_borderless_fullscreen, input_camera_view_toggle_system, input_debug_colors_cycle_system,
    input_fullscreen_toggle_system, input_level_focus_toggle_system,
};
pub use weapons::{WeaponMode, input_weapon_select_system};

mod plugin;

pub use plugin::input_plugin;
