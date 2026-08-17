use super::*;
use bevy::prelude::*;

use crate::{
    barriers::{
        barriers_pulsate_system, barriers_spawn_system, barriers_visibility_system, pressure_plates_spawn_system,
    },
    schedule::ClientSet,
    vfx::{rain_audio_system, rain_particles_system, rain_smoothing_system},
};

// Map rendering systems are mostly one-shot or visibility/material
// maintenance driven by loaded assets and level focus. The set runs after
// `Presentation` and `Network` so grass burn reacts to this frame's scorch
// marks and server-delivered explosions.
pub fn map_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            map_spawn_geometry_system,
            grass_spawn_system,
            grass_burn_system.after(grass_spawn_system),
            map_level_focus_visibility_system,
            map_wall_light_emissive_system,
            wall_light_flicker_system,
            barriers_spawn_system,
            barriers_pulsate_system,
            // After `map_level_focus_visibility_system` so the open-kind
            // override wins the per-frame race for barrier visibility.
            barriers_visibility_system.after(map_level_focus_visibility_system),
            pressure_plates_spawn_system,
        )
            .in_set(ClientSet::MapMaintenance),
    );
}

// Skybox setup, asset conversion, camera following, and ambient
// drift. Setup waits for `MapSettings` (from `SInit`) because the map
// decides which skybox to build; each setup system latches internally
// so it runs once. Rain smoothing lives here too — the `Sky` set runs
// before `Presentation` so the shared particle clouds consume this frame's
// spawned drops.
pub fn sky_weather_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            skybox::setup_skybox_from_cross_system.run_if(resource_exists::<common::protocol::MapSettings>),
            skybox::setup_sun_disc_system.run_if(resource_exists::<common::protocol::MapSettings>),
            cubemap::skybox_convert_cross_to_cubemap_system.run_if(resource_exists::<skybox::SkyboxCrossImage>),
            skybox::skybox_update_camera_system.run_if(resource_exists::<skybox::SkyboxCubemap>),
            skybox::skybox_rotate_system.run_if(resource_exists::<skybox::SkyboxCubemap>),
            // After the rotate step so the disc lands on the same frame's
            // sun direction; after camera sync would be ideal too, but a
            // one-frame positional lag at 400 m is invisible.
            skybox::sun_disc_system.after(skybox::skybox_rotate_system),
            // Rain: smooth the snapshot intensity first, then everything
            // that reads it. Dimming runs after the camera-insert system
            // so a freshly inserted Skybox is corrected the same frame.
            rain_smoothing_system,
            skybox::rain_dim_system
                .after(rain_smoothing_system)
                .after(skybox::skybox_update_camera_system),
            rain_particles_system.after(rain_smoothing_system),
            rain_audio_system.after(rain_smoothing_system),
        )
            .in_set(ClientSet::Sky),
    );
}
