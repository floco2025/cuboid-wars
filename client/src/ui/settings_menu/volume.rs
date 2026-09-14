use bevy::{
    audio::{AudioSink, AudioSinkPlayback, GlobalVolume, SpatialAudioSink},
    prelude::*,
};

use crate::audio::sink_volume;

// A master volume change must be pushed onto everything already playing
// (the rain, beam, and firework loops).
pub(super) fn apply_global_volume_system(
    global_volume: Res<GlobalVolume>,
    mut sinks: Query<(&mut AudioSink, &PlaybackSettings)>,
    mut spatial_sinks: Query<(&mut SpatialAudioSink, &PlaybackSettings)>,
) {
    for (mut sink, settings) in &mut sinks {
        sink.set_volume(sink_volume(settings, &global_volume));
    }
    for (mut sink, settings) in &mut spatial_sinks {
        sink.set_volume(sink_volume(settings, &global_volume));
    }
}
