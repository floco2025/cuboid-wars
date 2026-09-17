use bevy::{
    audio::{AudioSinkPlayback, GlobalVolume, SpatialAudioSink, Volume},
    prelude::*,
};

use super::AudioOcclusion;
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

// The one writer of a spatial sink's volume: its settings, the master volume,
// and its occlusion.
pub(super) fn spatial_sink_volume_system(
    global_volume: Res<GlobalVolume>,
    mut sinks: Query<(&mut SpatialAudioSink, &PlaybackSettings, &AudioOcclusion)>,
) {
    for (mut sink, playback, occlusion) in &mut sinks {
        sink.set_volume(sink_volume(playback, &global_volume) * Volume::Linear(occlusion.gain()));
    }
}

#[cfg(test)]
#[path = "tests/volume.rs"]
mod tests;
