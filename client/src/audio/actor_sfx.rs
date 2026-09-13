use bevy::{
    audio::{SpatialScale, Volume},
    prelude::*,
};

use super::sound_playback;
use crate::config::{AudioConfig, SoundDef};

pub(crate) fn actor_sfx_playback(
    asset_server: &AssetServer,
    sound: &SoundDef,
    volume_db: f32,
    mut settings: PlaybackSettings,
) -> (AudioPlayer, PlaybackSettings) {
    settings.volume *= Volume::Decibels(volume_db);
    sound_playback(asset_server, sound, settings)
}

pub(crate) fn play_actor_spatial_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    sound: &SoundDef,
    audio_config: &AudioConfig,
    volume_db: f32,
    settings: PlaybackSettings,
    pos: Vec3,
) {
    commands.spawn((
        actor_sfx_playback(
            asset_server,
            sound,
            volume_db,
            settings
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
        ),
        Transform::from_translation(pos),
    ));
}

#[cfg(test)]
#[path = "tests/actor_sfx.rs"]
mod tests;
