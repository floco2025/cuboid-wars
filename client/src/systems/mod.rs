mod actors;
mod animations;
mod cameras;
mod characters;
mod input;
mod items;
mod map;
mod network;
mod players;
mod projectiles;
mod skybox;
mod ui;
mod vfx;

pub(crate) fn visual_focus_level(y: f32) -> u8 {
    if y <= 0.0 {
        return 0;
    }
    (y / common::constants::LEVEL_HEIGHT).round().min(f32::from(u8::MAX)) as u8
}

pub use actors::actors_transform_sync_system;
pub use animations::{AnimationToPlay, character_animation_system};
pub use cameras::{MainCameraMarker, RearviewCameraMarker, setup_cameras_system};
pub use characters::{
    CharacterVisualTurnState, character_label_billboard_system, characters_movement_system,
    characters_visual_turn_system, label_camera_visibility_system,
};
pub use input::{
    input_camera_view_toggle_system, input_cursor_toggle_system, input_fullscreen_toggle_system,
    input_level_focus_toggle_system, input_movement_system, input_shooting_system,
};
pub use items::items_animation_system;
pub use map::{
    map_level_focus_visibility_system, map_make_wall_lights_emissive_system, map_spawn_geometry_system,
    setup_world_geometry_system,
};
pub use network::{AssetManagers, ServerReconciliation, network_echo_system, network_server_message_system};
pub use players::{
    BumpFlashState, CameraShake, CuboidShake, local_player_camera_shake_system, local_player_camera_sync_system,
    local_player_cuboid_shake_system, local_player_rearview_sync_system, local_player_rearview_system,
    local_player_visibility_sync_system, players_transform_sync_system,
};
pub use projectiles::projectiles_movement_system;
pub use skybox::{
    SkyboxCrossImage, SkyboxCubemap, setup_skybox_from_cross, skybox_convert_cross_to_cubemap_system,
    skybox_update_camera_system,
};
pub use ui::{
    BumpFlashMarker, CrosshairMarker, FpsMarker, PlayerEntryMarker, PlayerListMarker, RttMarker, setup_ui_system,
    ui_fps_system, ui_health_bars_system, ui_player_list_system, ui_rtt_system, ui_stunned_blink_system,
    ui_toggle_crosshair_system,
};
pub use vfx::explosion_effect_system;
