use bevy::{prelude::*, transform::TransformSystems};

use super::{
    LoopAudio,
    normalization::normalize_sound,
    occlusion::{OcclusionClock, audio_occlusion_system},
    spatial::{attach_low_pass_system, intercept_spatial_sound},
    volume::spatial_sink_volume_system,
};

pub(crate) fn audio_plugin(app: &mut App) {
    app.add_observer(normalize_sound::<AudioSource>)
        .add_observer(normalize_sound::<LoopAudio>)
        .add_observer(intercept_spatial_sound::<AudioSource>)
        .add_observer(intercept_spatial_sound::<LoopAudio>)
        .add_systems(
            PostUpdate,
            // Bevy builds sinks after transform propagation, so a loaded
            // source gets its filtered player the frame it is spawned.
            (
                attach_low_pass_system::<AudioSource>,
                attach_low_pass_system::<LoopAudio>,
            )
                .before(TransformSystems::Propagate),
        )
        // Bevy's playback set, which builds the sinks late in `PostUpdate`,
        // is private; `Last` is the first schedule that sees them.
        .add_systems(Last, spatial_sink_volume_system);
}

pub(crate) fn audio_occlusion_plugin(app: &mut App) {
    app.init_resource::<OcclusionClock>()
        .add_systems(Last, audio_occlusion_system.before(spatial_sink_volume_system));
}
