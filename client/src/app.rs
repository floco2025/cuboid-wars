use bevy::prelude::*;

use crate::{
    actors::actors_transform_sync_system,
    cameras::setup_cameras_system,
    characters::{
        character_label_billboard_system, character_shadow_settings_system, characters_movement_system,
        characters_visual_turn_system, label_camera_visibility_system,
    },
    input::{
        input_camera_view_toggle_system, input_cursor_toggle_system, input_fullscreen_toggle_system,
        input_level_focus_toggle_system, input_movement_system, input_shooting_system,
    },
    items::items_animation_system,
    map::{
        map_level_focus_visibility_system, map_make_wall_lights_emissive_system, map_spawn_geometry_system,
        setup_world_geometry_system,
    },
    materials::generate_material_mipmaps_system,
    network::{network_echo_system, network_server_message_system},
    players::{
        local_player_camera_shake_system, local_player_camera_sync_system, local_player_cuboid_shake_system,
        local_player_rearview_sync_system, local_player_rearview_system, local_player_visibility_sync_system,
        players_transform_sync_system,
    },
    projectiles::{ProjectileAssets, projectiles_movement_system},
    skybox::{setup_skybox_from_cross, skybox_convert_cross_to_cubemap_system, skybox_update_camera_system},
    ui::{
        setup_ui_system, ui_fps_system, ui_health_bars_system, ui_player_list_system, ui_rtt_system,
        ui_stunned_blink_system, ui_toggle_crosshair_system,
    },
    vfx::explosion_effect_system,
};

pub struct ClientGamePlugin {
    pub texture_mipmaps_enabled: bool,
}

impl Plugin for ClientGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProjectileAssets>()
            .add_systems(
                Startup,
                (
                    setup_world_geometry_system,
                    setup_cameras_system,
                    setup_ui_system,
                    setup_skybox_from_cross.after(setup_world_geometry_system),
                ),
            )
            .add_systems(
                Update,
                (
                    input_movement_system.after(input_camera_view_toggle_system),
                    input_shooting_system.after(input_movement_system),
                    input_cursor_toggle_system,
                    input_camera_view_toggle_system,
                    input_level_focus_toggle_system,
                    input_fullscreen_toggle_system,
                ),
            )
            .add_systems(Update, (network_echo_system, network_server_message_system))
            .add_systems(
                Update,
                (
                    characters_movement_system,
                    players_transform_sync_system.after(characters_movement_system),
                    actors_transform_sync_system.after(characters_movement_system),
                    characters_visual_turn_system
                        .after(players_transform_sync_system)
                        .after(actors_transform_sync_system),
                    character_label_billboard_system,
                    label_camera_visibility_system,
                    character_shadow_settings_system,
                ),
            )
            .add_systems(
                Update,
                (
                    local_player_camera_shake_system,
                    local_player_cuboid_shake_system,
                    local_player_camera_sync_system
                        .after(input_movement_system)
                        .after(local_player_camera_shake_system),
                    local_player_rearview_sync_system.after(local_player_camera_sync_system),
                    local_player_rearview_system.after(local_player_rearview_sync_system),
                    local_player_visibility_sync_system.after(input_camera_view_toggle_system),
                ),
            )
            .add_systems(Update, projectiles_movement_system)
            .add_systems(Update, explosion_effect_system)
            .add_systems(Update, items_animation_system)
            .add_systems(
                Update,
                (
                    map_spawn_geometry_system,
                    map_level_focus_visibility_system,
                    map_make_wall_lights_emissive_system,
                ),
            )
            .add_systems(
                Update,
                (
                    ui_toggle_crosshair_system,
                    ui_player_list_system,
                    ui_health_bars_system.after(ui_player_list_system),
                    ui_stunned_blink_system,
                    ui_rtt_system,
                    ui_fps_system,
                ),
            )
            .add_systems(
                Update,
                (
                    skybox_convert_cross_to_cubemap_system.run_if(resource_exists::<crate::skybox::SkyboxCrossImage>),
                    skybox_update_camera_system.run_if(resource_exists::<crate::skybox::SkyboxCubemap>),
                ),
            );

        if self.texture_mipmaps_enabled {
            // Do not use bevy_mod_mipmap_generator::generate_mipmaps directly here.
            // It reacts to material events only once, while our materials often point
            // at image assets that are still loading. Our system retries until the
            // images exist, then calls the crate's mip generation function.
            app.add_systems(Update, generate_material_mipmaps_system);
        }
    }
}
