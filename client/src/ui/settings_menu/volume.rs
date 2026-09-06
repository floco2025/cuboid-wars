use bevy::{
    audio::{AudioSink, AudioSinkPlayback, GlobalVolume, SpatialAudioSink},
    prelude::*,
};

// bevy applies `GlobalVolume` only when a sink is created, so a master
// volume change must be pushed onto everything already playing (the rain,
// beam, and firework loops). Same formula as sink creation.
pub(super) fn apply_global_volume_system(
    global_volume: Res<GlobalVolume>,
    mut sinks: Query<(&mut AudioSink, &PlaybackSettings)>,
    mut spatial_sinks: Query<(&mut SpatialAudioSink, &PlaybackSettings)>,
) {
    for (mut sink, settings) in &mut sinks {
        sink.set_volume(settings.volume * global_volume.volume);
    }
    for (mut sink, settings) in &mut spatial_sinks {
        sink.set_volume(settings.volume * global_volume.volume);
    }
}
