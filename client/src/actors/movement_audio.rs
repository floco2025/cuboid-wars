use bevy::{
    audio::{GlobalVolume, SpatialScale, Volume},
    prelude::*,
};
use common::{
    config::ActorGameplayConfig,
    physics::CharacterSupport,
    protocol::{ActorId, ActorMoveIntent},
};

use super::{ActorAnimationVelocity, wheel_animation::drive_speed};
use crate::{
    audio::{NormalizationGain, sound_playback},
    config::{AudioConfig, SoundDef},
    constants::{
        ACTOR_MOVEMENT_AUDIO_ATTACK_RATE, ACTOR_MOVEMENT_AUDIO_FLYING_PITCH, ACTOR_MOVEMENT_AUDIO_GROUND_PITCH,
        ACTOR_MOVEMENT_AUDIO_HOLD_SECS, ACTOR_MOVEMENT_AUDIO_HOVER_GAIN, ACTOR_MOVEMENT_AUDIO_MIN_GAIN,
        ACTOR_MOVEMENT_AUDIO_PAUSE_GAIN, ACTOR_MOVEMENT_AUDIO_PITCH_VARIATION, ACTOR_MOVEMENT_AUDIO_REFERENCE_SPEED,
        ACTOR_MOVEMENT_AUDIO_RELEASE_RATE, ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED,
    },
};

#[derive(Component)]
pub(crate) struct ActorMovementAudio {
    flying: bool,
    volume: Volume,
    pitch: f32,
    gain: f32,
    speed: f32,
    travel: f32,
    hold_secs: f32,
}

pub(super) fn spawn_movement_audio(
    commands: &mut Commands,
    asset_server: &AssetServer,
    actor: Entity,
    id: ActorId,
    config: &ActorGameplayConfig,
    sound: &SoundDef,
    audio_config: &AudioConfig,
) {
    if config.immovable {
        return;
    }
    commands.spawn((
        ChildOf(actor),
        Transform::from_xyz(0.0, config.physics().hitbox.center_y_offset(), 0.0),
        sound_playback(
            asset_server,
            sound,
            PlaybackSettings::LOOP
                .paused()
                .with_volume(Volume::Linear(0.0))
                .with_spatial(true)
                .with_spatial_scale(SpatialScale::new(audio_config.spatial_distance_scale)),
        ),
        ActorMovementAudio {
            flying: config.flies(),
            volume: Volume::Decibels(sound.volume_db),
            pitch: 1.0 + ((id.0 % 7) as f32 / 3.0 - 1.0) * ACTOR_MOVEMENT_AUDIO_PITCH_VARIATION,
            gain: 0.0,
            speed: 1.0,
            travel: 0.0,
            hold_secs: 0.0,
        },
    ));
}

fn movement_speed(intent: ActorMoveIntent, velocity: Vec3, support: CharacterSupport) -> f32 {
    match intent {
        ActorMoveIntent::Flying { velocity: control } => {
            let control = Vec3::from_array(control);
            velocity.dot(control.normalize_or_zero()).clamp(0.0, control.length())
        }
        ActorMoveIntent::Climbing { speed, .. } if support == CharacterSupport::Ladder => {
            velocity.y.abs().min(speed.abs())
        }
        ActorMoveIntent::ExitingLadder { .. } => drive_speed(intent, velocity, None),
        _ => drive_speed(intent, velocity, Some(support)),
    }
}

pub(crate) fn actor_movement_audio_system(
    time: Res<Time>,
    global_volume: Res<GlobalVolume>,
    actors: Query<(&ActorAnimationVelocity, &ActorMoveIntent, &CharacterSupport)>,
    mut sounds: Query<(
        &ChildOf,
        &mut ActorMovementAudio,
        &NormalizationGain,
        &mut PlaybackSettings,
        Option<&mut SpatialAudioSink>,
    )>,
) {
    for (parent, mut sound, normalization, mut playback, sink) in &mut sounds {
        let Ok((velocity, intent, support)) = actors.get(parent.parent()) else {
            continue;
        };
        let travel = movement_speed(*intent, velocity.0, *support);
        if travel > ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED {
            sound.travel = travel;
            sound.hold_secs = ACTOR_MOVEMENT_AUDIO_HOLD_SECS;
        } else {
            // Buffered travel can briefly vanish at a sample boundary or a turn.
            sound.hold_secs = (sound.hold_secs - time.delta_secs()).max(0.0);
            if sound.hold_secs == 0.0 {
                sound.travel = 0.0;
            }
        }
        let effort = (sound.travel / ACTOR_MOVEMENT_AUDIO_REFERENCE_SPEED).clamp(0.0, 1.0);
        let target_gain = if sound.flying {
            ACTOR_MOVEMENT_AUDIO_HOVER_GAIN + (1.0 - ACTOR_MOVEMENT_AUDIO_HOVER_GAIN) * effort
        } else if sound.travel > ACTOR_MOVEMENT_AUDIO_STANDSTILL_SPEED {
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
        sound.speed.smooth_nudge(&target_speed, fade_rate, time.delta_secs());
        if target_gain == 0.0 && sound.gain < ACTOR_MOVEMENT_AUDIO_PAUSE_GAIN {
            sound.gain = 0.0;
        }
        playback.volume = sound.volume * normalization.0 * Volume::Linear(sound.gain);
        playback.speed = sound.speed;
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
