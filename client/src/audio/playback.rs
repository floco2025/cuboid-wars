use bevy::{
    audio::{Decodable, PlaybackMode, SpatialScale, Volume},
    prelude::*,
};

use super::LoopAudio;

use crate::{
    config::{AudioConfig, SoundDef},
    vfx::explosion_sound_speed,
};

// Flat one-shot feedback sound (UI clicks, own-player cues).
pub fn play_sound(commands: &mut Commands, asset_server: &AssetServer, sound: &SoundDef) {
    play_sound_with(commands, asset_server, sound, PlaybackSettings::DESPAWN);
}

// Flat sound with explicit playback settings (loops, volume tweaks).
// Returns the entity so loops can be despawned to stop.
pub fn play_sound_with(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sound: &SoundDef,
    settings: PlaybackSettings,
) -> Entity {
    commands.spawn(sound_playback(asset_server, sound, settings)).id()
}

// Positional world one-shot: attenuates and pans with distance from `pos`
// (world meters compressed by `spatial_distance_scale`).
pub fn play_spatial_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sound: &SoundDef,
    audio_config: &AudioConfig,
    pos: Vec3,
) {
    play_spatial_sound_with(
        commands,
        asset_server,
        sound,
        audio_config,
        PlaybackSettings::DESPAWN,
        pos,
    );
}

// Positional sound with explicit playback settings.
pub fn play_spatial_sound_with(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sound: &SoundDef,
    audio_config: &AudioConfig,
    settings: PlaybackSettings,
    pos: Vec3,
) -> Entity {
    commands
        .spawn((
            sound_playback(
                asset_server,
                sound,
                settings
                    .with_spatial(true)
                    .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
            ),
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
    sound: &SoundDef,
    audio_config: &AudioConfig,
    pos: Vec3,
    blast_radius: Option<f32>,
) {
    play_spatial_sound_with(
        commands,
        asset_server,
        sound,
        audio_config,
        explosion_playback_settings(audio_config, blast_radius),
        pos,
    );
}

pub(crate) fn explosion_playback_settings(audio_config: &AudioConfig, blast_radius: Option<f32>) -> PlaybackSettings {
    PlaybackSettings::DESPAWN
        .with_volume(Volume::Linear(audio_config.explosion_gain))
        .with_speed(blast_radius.map_or(1.0, explosion_sound_speed))
}

pub fn sound_playback(
    asset_server: &AssetServer,
    sound: &SoundDef,
    settings: PlaybackSettings,
) -> (AudioPlayer, PlaybackSettings) {
    sound_playback_for::<AudioSource>(asset_server, sound, settings)
}

pub(crate) fn loop_sound_playback(
    asset_server: &AssetServer,
    sound: &SoundDef,
    mut settings: PlaybackSettings,
) -> (AudioPlayer<LoopAudio>, PlaybackSettings) {
    // The decoder repeats directly; Bevy's looping wrapper would restore the broken span boundaries.
    settings.mode = PlaybackMode::Once;
    sound_playback_for::<LoopAudio>(asset_server, sound, settings)
}

fn sound_playback_for<T: Asset + Decodable>(
    asset_server: &AssetServer,
    sound: &SoundDef,
    mut settings: PlaybackSettings,
) -> (AudioPlayer<T>, PlaybackSettings) {
    settings.volume *= Volume::Decibels(sound.volume_db);
    (AudioPlayer(asset_server.load(sound.file.clone())), settings)
}
