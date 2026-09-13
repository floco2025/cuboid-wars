use bevy::{
    audio::{GlobalVolume, SpatialScale, Volume},
    prelude::*,
};
use common::{config::ActorGameplayConfig, protocol::ActorId};

use super::ActorAnimationVelocity;
use crate::{
    audio::{NormalizationGain, loop_sound_playback, settings_volume},
    config::{AudioConfig, ClientSettings, SoundDef},
    constants::{
        ACTOR_MOVEMENT_AUDIO_ATTACK_RATE, ACTOR_MOVEMENT_AUDIO_FLYING_PITCH, ACTOR_MOVEMENT_AUDIO_GROUND_PITCH,
        ACTOR_MOVEMENT_AUDIO_HOVER_GAIN, ACTOR_MOVEMENT_AUDIO_MIN_GAIN, ACTOR_MOVEMENT_AUDIO_PAUSE_GAIN,
        ACTOR_MOVEMENT_AUDIO_PITCH_VARIATION, ACTOR_MOVEMENT_AUDIO_REFERENCE_SPEED, ACTOR_MOVEMENT_AUDIO_RELEASE_RATE,
        ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED,
    },
};

#[derive(Component)]
pub(crate) struct ActorMovementAudio {
    configured_volume: Volume,
    flying: bool,
    pitch: f32,
    gain: f32,
}

pub(super) fn spawn_movement_audio(
    commands: &mut Commands,
    asset_server: &AssetServer,
    actor: Entity,
    id: ActorId,
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
                .with_volume(Volume::Linear(0.0))
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
        ),
        ActorMovementAudio {
            configured_volume: Volume::Decibels(movement_volume_db + sound.volume_db),
            flying: config.flies(),
            pitch: 1.0 + ((id.0 % 7) as f32 / 3.0 - 1.0) * ACTOR_MOVEMENT_AUDIO_PITCH_VARIATION,
            gain: 0.0,
        },
    ));
}

pub(crate) fn actor_movement_audio_system(
    time: Res<Time>,
    settings: Res<ClientSettings>,
    global_volume: Res<GlobalVolume>,
    actors: Query<&ActorAnimationVelocity>,
    mut sounds: Query<(
        &ChildOf,
        &mut ActorMovementAudio,
        &NormalizationGain,
        &mut PlaybackSettings,
        Option<&mut SpatialAudioSink>,
    )>,
) {
    let volume = settings_volume(settings.preferences.actor_movement_volume_db);
    for (parent, mut sound, normalization, mut playback, sink) in &mut sounds {
        let Ok(velocity) = actors.get(parent.parent()) else {
            continue;
        };
        let travel = velocity.0.length();
        let effort = (travel / ACTOR_MOVEMENT_AUDIO_REFERENCE_SPEED).clamp(0.0, 1.0);
        let target_gain = if sound.flying {
            ACTOR_MOVEMENT_AUDIO_HOVER_GAIN + (1.0 - ACTOR_MOVEMENT_AUDIO_HOVER_GAIN) * effort
        } else if travel > ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED {
            ACTOR_MOVEMENT_AUDIO_MIN_GAIN + (1.0 - ACTOR_MOVEMENT_AUDIO_MIN_GAIN) * effort
        } else {
            0.0
        };
        let (min_pitch, max_pitch) = if sound.flying {
            ACTOR_MOVEMENT_AUDIO_FLYING_PITCH
        } else {
            ACTOR_MOVEMENT_AUDIO_GROUND_PITCH
        };
        let target_speed = sound.pitch * min_pitch.lerp(max_pitch, effort);
        let fade_rate = if target_gain < sound.gain {
            ACTOR_MOVEMENT_AUDIO_RELEASE_RATE
        } else {
            ACTOR_MOVEMENT_AUDIO_ATTACK_RATE
        };
        sound.gain.smooth_nudge(&target_gain, fade_rate, time.delta_secs());
        playback.speed.smooth_nudge(&target_speed, fade_rate, time.delta_secs());
        if target_gain == 0.0 && sound.gain < ACTOR_MOVEMENT_AUDIO_PAUSE_GAIN {
            sound.gain = 0.0;
        }
        playback.volume = sound.configured_volume * normalization.0 * volume * Volume::Linear(sound.gain);
        playback.paused = sound.gain == 0.0;
        if let Some(mut sink) = sink {
            sink.set_volume(playback.volume * global_volume.volume);
            sink.set_speed(playback.speed);
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
