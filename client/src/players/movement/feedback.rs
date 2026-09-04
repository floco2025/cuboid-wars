use bevy::{audio::Volume, prelude::*};

use crate::{
    audio::play_sound_with,
    config::{AssetSet, BumpAudioConfig},
    players::BumpFeedbackState,
};

// A hit spends the run-up; it only sounds when there was enough of one.
pub(super) fn bump(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio: &BumpAudioConfig,
    state: &mut Mut<BumpFeedbackState>,
    collided_with_wall: bool,
) {
    if let Some(volume) = audio.volume_for(state.run_up) {
        let sound_path = if collided_with_wall {
            asset_set.player_sound("bump_wall")
        } else {
            asset_set.player_sound("bump_player")
        };
        play_sound_with(
            commands,
            asset_server,
            sound_path,
            PlaybackSettings::DESPAWN.with_volume(Volume::Linear(volume)),
        );
    }
    state.run_up = 0.0;
}
