use crate::constants::{
    PROJECTILE_IMPACT_MAX_SOUNDS_PER_SECOND, PROJECTILE_IMPACT_MIN_BOUNCE_SPEED,
    PROJECTILE_IMPACT_PREEMPTION_LOUDNESS_RATIO, PROJECTILE_IMPACT_WALL_BOUNCE_GAIN,
};
use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use crate::config::{AssetSet, AudioConfig};

// Last projectile bounce sound: when it played and how loud it was at the
// listener. Rate limiting keeps its cadence, but a clearly-louder bounce can
// take the slot early.
#[derive(Resource, Default)]
pub struct LastBounceSound {
    pub time: f32,
    pub loudness: f32,
}

// Estimated audibility of a world sound at the listener, mirroring rodio's
// attenuation law (`min(1/d², 1)` in spatial-scale-compressed units): full
// volume inside the knee, inverse-square falloff beyond it.
fn loudness_at_listener(pos: Vec3, listener_pos: Vec3, spatial_distance_scale: f32) -> f32 {
    let scaled_distance = pos.distance(listener_pos) * spatial_distance_scale;
    (1.0 / scaled_distance.powi(2)).min(1.0)
}

// The generic one-shot spawners live in `crate::audio`; this module keeps
// only the projectile-specific rate limiting.
pub(super) use crate::audio::{play_sound_with, play_spatial_sound_with};

pub(super) fn play_barrier_impact_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    pos: Vec3,
) {
    play_spatial_sound_with(
        commands,
        asset_server,
        asset_set.player_sound("barrier_impact"),
        audio_config,
        PlaybackSettings::DESPAWN,
        pos,
    );
}

pub(super) fn play_wall_bounce_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio_config: &AudioConfig,
    speed_before: f32,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSound,
    pos: Vec3,
    listener_pos: Vec3,
) {
    if speed_before < PROJECTILE_IMPACT_MIN_BOUNCE_SPEED {
        return;
    }
    let loudness = loudness_at_listener(pos, listener_pos, audio_config.spatial_distance_scale);
    let min_interval = 1.0 / PROJECTILE_IMPACT_MAX_SOUNDS_PER_SECOND;
    if current_time - last_bounce_sound.time < min_interval
        && loudness < last_bounce_sound.loudness * PROJECTILE_IMPACT_PREEMPTION_LOUDNESS_RATIO
    {
        return;
    }

    play_spatial_sound_with(
        commands,
        asset_server,
        asset_set.player_sound("hit_wall"),
        audio_config,
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(PROJECTILE_IMPACT_WALL_BOUNCE_GAIN),
            ..default()
        },
        pos,
    );
    last_bounce_sound.time = current_time;
    last_bounce_sound.loudness = loudness;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loudness_is_full_inside_knee_and_falls_off_beyond() {
        let listener = Vec3::ZERO;
        let scale = AudioConfig::default().spatial_distance_scale;
        let full_volume_radius = scale.recip();
        assert_eq!(loudness_at_listener(Vec3::ZERO, listener, scale), 1.0);
        assert_eq!(
            loudness_at_listener(Vec3::X * (full_volume_radius * 0.5), listener, scale),
            1.0
        );
        let near = loudness_at_listener(Vec3::X * (full_volume_radius * 2.0), listener, scale);
        let far = loudness_at_listener(Vec3::X * (full_volume_radius * 4.0), listener, scale);
        assert!(near < 1.0);
        assert!(far < near);
    }
}
