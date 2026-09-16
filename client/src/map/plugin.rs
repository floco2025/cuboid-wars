use super::*;
use bevy::prelude::*;

use crate::{
    barriers::{
        PressurePlateModel, barriers_spawn_system, pressure_plates_animation_system, pressure_plates_attach_system,
        pressure_plates_spawn_system, pressure_plates_visibility_system,
    },
    bridges::bridges_spawn_system,
    fields::{
        CheckpointAssets, EraserAssets, FieldMeshes, FieldSurfaces, SharedCheckpoint, checkpoint_pennants_system,
        checkpoints_spawn_system, erasers_spawn_system, fields_fade_system,
    },
    schedule::ClientSet,
    vfx::{rain_audio_system, rain_particles_system, rain_smoothing_system},
};

// Map rendering systems are mostly one-shot or visibility/material
// maintenance driven by loaded assets and level focus. The set runs after
// `Presentation` and `Network` so grass burn reacts to this frame's scorch
// marks and server-delivered explosions.
pub fn map_plugin(app: &mut App) {
    app.init_resource::<FocusedMapLevel>()
        .init_resource::<GrassChunks>()
        .init_resource::<GrassSources>()
        .init_resource::<FieldMeshes>()
        .init_resource::<FieldSurfaces>()
        .init_resource::<SharedCheckpoint>()
        .init_resource::<EraserAssets>()
        .init_resource::<CheckpointAssets>()
        .init_resource::<PressurePlateModel>();
    app.add_systems(Startup, setup_grass_materials_system);
    app.add_systems(
        Update,
        (
            map_spawn_geometry_system,
            grass_sources_reset_system,
            grounds::grounds_spawn_system.after(grass_sources_reset_system),
            erasers_spawn_system,
            checkpoints_spawn_system,
            checkpoint_pennants_system.after(checkpoints_spawn_system),
            terrain_spawn_system.after(grass_sources_reset_system),
            (
                grass_streaming_system
                    .after(terrain_spawn_system)
                    .after(grounds::grounds_spawn_system),
                grass_chunk_finish_system.after(grass_streaming_system),
                grass_burn_system.after(grass_chunk_finish_system),
                weather_surfaces::weather_surfaces_system.after(grass_sources_reset_system),
            ),
            update_focused_map_level_system,
            map_level_focus_visibility_system
                .after(update_focused_map_level_system)
                .run_if(resource_changed::<FocusedMapLevel>),
            added_map_level_visibility_system
                .after(map_spawn_geometry_system)
                .after(erasers_spawn_system)
                .after(checkpoints_spawn_system)
                .after(terrain_spawn_system)
                .after(grass_streaming_system)
                .after(update_focused_map_level_system)
                .after(map_level_focus_visibility_system),
            map_wall_light_emissive_system,
            wall_light_flicker_system,
            barriers_spawn_system.after(update_focused_map_level_system),
            (
                pressure_plates_spawn_system,
                pressure_plates_attach_system,
                pressure_plates_visibility_system,
                pressure_plates_animation_system,
            )
                .chain(),
            bridges_spawn_system.after(update_focused_map_level_system),
            fields_fade_system,
        )
            .in_set(ClientSet::MapMaintenance),
    );
}

// The procedural celestial sky follows every scene camera while remaining
// world-aligned. Rain smoothing lives here too — the `Sky` set runs
// before `Presentation` so the shared particle clouds consume this frame's
// spawned drops.
pub fn sky_weather_plugin(app: &mut App) {
    app.init_resource::<celestial::SkyState>();
    app.add_systems(
        Update,
        (
            celestial::setup_sky_system,
            sky_probe::setup_sky_probe_system.after(ClientSet::Camera),
            sky_probe::share_sky_probe_system
                .after(ClientSet::Camera)
                .after(sky_probe::setup_sky_probe_system)
                .after(bevy::pbr::generate::generate_environment_map_light),
            sky_probe::refresh_sky_probe_system
                .after(sky_probe::setup_sky_probe_system)
                .after(celestial::celestial_state_system),
            celestial::attach_sky_to_cameras_system
                .after(celestial::setup_sky_system)
                .after(ClientSet::Camera),
            celestial::align_sky_domes_system
                .after(celestial::attach_sky_to_cameras_system)
                .after(ClientSet::Camera),
            rain_smoothing_system,
            celestial::celestial_state_system.after(rain_smoothing_system),
            (
                celestial::sky_material_system.after(celestial::setup_sky_system),
                celestial::celestial_lights_system,
                celestial::scene_ambient_system,
                celestial::distance_fog_system,
            )
                .after(celestial::celestial_state_system),
            rain_particles_system.after(rain_smoothing_system),
            rain_audio_system.after(rain_smoothing_system),
        )
            .in_set(ClientSet::Sky),
    );
}
