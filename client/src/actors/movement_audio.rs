use bevy::{
    audio::{SpatialScale, Volume},
    prelude::*,
};
use common::config::ActorGameplayConfig;

use super::ActorAnimationVelocity;
use crate::{
    audio::loop_sound_playback,
    config::{AudioConfig, SoundDef},
    constants::ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED,
};

#[derive(Component)]
pub(crate) struct ActorMovementAudioMarker;

pub(super) fn spawn_movement_audio(
    commands: &mut Commands,
    asset_server: &AssetServer,
    actor: Entity,
    config: &ActorGameplayConfig,
    sound: &SoundDef,
    audio_config: &AudioConfig,
    movement_volume_db: f32,
) {
    if config.immovable {
        return;
    }
    commands.spawn((
        ChildOf(actor),
        Transform::from_xyz(0.0, config.physics().hitbox.center_y_offset(), 0.0),
        loop_sound_playback(
            asset_server,
            sound,
            PlaybackSettings::ONCE
                .paused()
                .with_volume(Volume::Decibels(movement_volume_db))
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
        ),
        ActorMovementAudioMarker,
    ));
}

pub(crate) fn actor_movement_audio_system(
    actors: Query<&ActorAnimationVelocity>,
    mut sounds: Query<(&ChildOf, &mut PlaybackSettings, Option<&SpatialAudioSink>), With<ActorMovementAudioMarker>>,
) {
    for (parent, mut playback, sink) in &mut sounds {
        let Ok(velocity) = actors.get(parent.parent()) else {
            continue;
        };
        playback.paused = velocity.0.length_squared() <= ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED.powi(2);
        if let Some(sink) = sink {
            if playback.paused {
                sink.pause();
            } else {
                sink.play();
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/movement_audio.rs"]
mod tests;
