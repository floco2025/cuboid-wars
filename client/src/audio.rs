use bevy::{
    audio::{SpatialScale, Volume},
    prelude::*,
};

use crate::{config::AudioConfig, vfx::explosion_sound_speed};

// Flat one-shot feedback sound (UI clicks, own-player cues).
pub fn play_sound(commands: &mut Commands, asset_server: &AssetServer, asset_path: &str) {
    play_sound_with(commands, asset_server, asset_path, PlaybackSettings::DESPAWN);
}

// Flat sound with explicit playback settings (loops, volume tweaks).
// Returns the entity so loops can be despawned to stop.
pub fn play_sound_with(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    settings: PlaybackSettings,
) -> Entity {
    commands
        .spawn((AudioPlayer::new(asset_server.load(asset_path.to_owned())), settings))
        .id()
}

// Positional world one-shot: attenuates and pans with distance from `pos`
// (world meters compressed by `spatial_distance_scale`).
pub fn play_spatial_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    audio_config: &AudioConfig,
    pos: Vec3,
) {
    play_spatial_sound_with(
        commands,
        asset_server,
        asset_path,
        audio_config,
        PlaybackSettings::DESPAWN,
        pos,
    );
}

// Positional sound with explicit playback settings.
pub fn play_spatial_sound_with(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    audio_config: &AudioConfig,
    settings: PlaybackSettings,
    pos: Vec3,
) -> Entity {
    commands
        .spawn((
            AudioPlayer::new(asset_server.load(asset_path.to_owned())),
            settings
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
            Transform::from_translation(pos),
        ))
        .id()
}

// Explosion boom: positional, boosted by the explosion gain, pitched by
// blast size (bigger blast = deeper boom; `None` = unknown radius, normal
// pitch).
pub fn play_explosion_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    audio_config: &AudioConfig,
    pos: Vec3,
    blast_radius: Option<f32>,
) {
    play_spatial_sound_with(
        commands,
        asset_server,
        asset_path,
        audio_config,
        PlaybackSettings::DESPAWN
            .with_volume(Volume::Linear(audio_config.explosion_gain_multiplier))
            .with_speed(blast_radius.map_or(1.0, explosion_sound_speed)),
        pos,
    );
}
