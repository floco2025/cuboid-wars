use bevy::{
    audio::{PlaybackMode, Volume},
    prelude::*,
};

use crate::{
    config::AssetSet,
    constants::{PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND, PROJECTILE_MIN_BOUNCE_SOUND_SPEED},
};

// Last time a projectile bounce sound was played (for rate limiting).
#[derive(Resource, Default)]
pub struct LastBounceSoundTime(pub f32);

pub(super) fn play_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_path: &str,
    settings: PlaybackSettings,
) {
    commands.spawn((AudioPlayer::new(asset_server.load(asset_path.to_owned())), settings));
}

pub(super) fn play_wall_bounce_sound(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    speed_before: f32,
    current_time: f32,
    last_bounce_sound: &mut LastBounceSoundTime,
) {
    let min_interval = 1.0 / PROJECTILE_MAX_BOUNCE_SOUNDS_PER_SECOND;
    if speed_before < PROJECTILE_MIN_BOUNCE_SOUND_SPEED || current_time - last_bounce_sound.0 < min_interval {
        return;
    }

    play_sound(
        commands,
        asset_server,
        asset_set.player_sound("hit_wall"),
        PlaybackSettings {
            mode: PlaybackMode::Despawn,
            volume: Volume::Linear(0.2),
            ..default()
        },
    );
    last_bounce_sound.0 = current_time;
}
