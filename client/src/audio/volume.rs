use bevy::{
    audio::{GlobalVolume, Volume},
    prelude::PlaybackSettings,
};

use crate::constants::AUDIO_VOLUME_DB_MIN;

// Bevy folds the master volume into a sink only when it creates the sink, so
// every later volume write pushes the same product itself.
pub(crate) fn sink_volume(playback: &PlaybackSettings, global_volume: &GlobalVolume) -> Volume {
    playback.volume * global_volume.volume
}

pub(crate) fn settings_volume(db: f32) -> Volume {
    if db <= AUDIO_VOLUME_DB_MIN {
        Volume::Linear(0.0)
    } else {
        Volume::Decibels(db)
    }
}
