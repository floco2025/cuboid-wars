use bevy::{
    audio::{PlaybackMode, SpatialScale, Volume},
    prelude::*,
};

use crate::{
    config::AssetSet,
    constants::{
        PROJECTILE_BOUNCE_PREEMPT_LOUDNESS_RATIO, PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND,
        PROJECTILE_MIN_BOUNCE_SOUND_SPEED, SPATIAL_SOUND_SCALE,
    },
};

// Last projectile bounce sound: when it played and how loud it was at the
// listener. Rate limiting keeps its cadence, but a clearly-louder bounce
// (see `PROJECTILE_BOUNCE_PREEMPT_LOUDNESS_RATIO`) takes the slot early.
#[derive(Resource, Default)]
pub struct LastBounceSound {
    pub time: f32,
    pub loudness: f32,
}

// Estimated audibility of a world sound at the listener, mirroring rodio's
// attenuation law (`min(1/d², 1)` in spatial-scale-compressed units): full
// volume inside the knee, inverse-square falloff beyond it.
fn loudness_at_listener(pos: Vec3, listener_pos: Vec3) -> f32 {
    let compressed = pos.distance(listener_pos) * SPATIAL_SOUND_SCALE;
    (1.0 / (compressed * compressed)).min(1.0)
}

pub(super) fn play_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    settings: PlaybackSettings,
) {
    commands.spawn((AudioPlayer::new(asset_server.load(asset_path.to_owned())), settings));
}

// Positional world sound: attenuates and pans with distance, like the
// explosion sounds (same world-meter compression).
pub(super) fn play_spatial_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    settings: PlaybackSettings,
    pos: Vec3,
) {
    commands.spawn((
        AudioPlayer::new(asset_server.load(asset_path.to_owned())),
        settings
            .with_spatial(true)
            .with_spatial_scale(SpatialScale::new(SPATIAL_SOUND_SCALE)),
        Transform::from_translation(pos),
    ));
}

pub(super) fn play_barrier_impact_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    pos: Vec3,
) {
    play_spatial_sound(
        commands,
        asset_server,
        asset_set.player_sound("barrier_impact"),
        PlaybackSettings::DESPAWN,
        pos,
    );
}

pub(super) fn play_wall_bounce_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    speed_before: f32,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSound,
    pos: Vec3,
    listener_pos: Vec3,
) {
    if speed_before < PROJECTILE_MIN_BOUNCE_SOUND_SPEED {
        return;
    }
    let loudness = loudness_at_listener(pos, listener_pos);
    let min_interval = 1.0 / PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND;
    if current_time - last_bounce_sound.time < min_interval
        && loudness < last_bounce_sound.loudness * PROJECTILE_BOUNCE_PREEMPT_LOUDNESS_RATIO
    {
        return;
    }

    play_spatial_sound(
        commands,
        asset_server,
        asset_set.player_sound("hit_wall"),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(0.2),
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
        let knee = 1.0 / SPATIAL_SOUND_SCALE;
        assert_eq!(loudness_at_listener(Vec3::ZERO, listener), 1.0);
        assert_eq!(loudness_at_listener(Vec3::X * (knee * 0.5), listener), 1.0);
        let near = loudness_at_listener(Vec3::X * (knee * 2.0), listener);
        let far = loudness_at_listener(Vec3::X * (knee * 4.0), listener);
        assert!(near < 1.0);
        assert!(far < near);
    }
}
