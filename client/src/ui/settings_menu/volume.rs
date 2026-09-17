use bevy::{
    audio::{AudioSink, AudioSinkPlayback, GlobalVolume},
    prelude::*,
};

use crate::audio::sink_volume;

// A master volume change must be pushed onto every flat sound already playing
// (the rain loop); spatial sinks take it from `spatial_sink_volume_system`
// every frame.
pub(super) fn apply_global_volume_system(
    global_volume: Res<GlobalVolume>,
    mut sinks: Query<(&mut AudioSink, &PlaybackSettings)>,
) {
    for (mut sink, settings) in &mut sinks {
        sink.set_volume(sink_volume(settings, &global_volume));
    }
}
