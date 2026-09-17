use bevy::{
    audio::{Decodable, PlaybackMode},
    prelude::*,
};

use super::{AudioOcclusion, LoopSpan, LowPassAudio};

// The source a spatial sound was spawned with, kept for as long as it plays,
// and the loop it plays inside the filter.
#[derive(Component)]
pub(crate) struct SpatialSound<T: Asset> {
    source: Handle<T>,
    repeat: Option<LoopSpan>,
}

// Occlusion muffles every spatial sound, so its player is exchanged for one
// over `LowPassAudio` the moment it is added, before Bevy can build a sink
// from the unfiltered source. A loop moves into that source, and a playing
// sound is held paused until its first probe and volume have landed, since
// Bevy starts a sink at the unoccluded volume with the cutoff still open.
pub(super) fn intercept_spatial_sound<T: Asset + Decodable>(
    event: On<Add, AudioPlayer<T>>,
    mut sounds: Query<(&AudioPlayer<T>, &mut PlaybackSettings)>,
    mut commands: Commands,
) {
    let Ok((player, mut settings)) = sounds.get_mut(event.entity) else {
        return;
    };
    if !settings.spatial {
        return;
    }
    let repeat = matches!(settings.mode, PlaybackMode::Loop).then(|| LoopSpan {
        start_position: settings.start_position,
        duration: settings.duration,
    });
    if repeat.is_some() {
        settings.mode = PlaybackMode::Once;
        settings.start_position = None;
        settings.duration = None;
    }
    let occlusion = if settings.paused {
        AudioOcclusion::default()
    } else {
        settings.paused = true;
        AudioOcclusion::holding()
    };
    commands.entity(event.entity).remove::<AudioPlayer<T>>().insert((
        SpatialSound {
            source: player.0.clone(),
            repeat,
        },
        occlusion,
    ));
}

pub(super) fn attach_low_pass_system<T: Asset + Decodable + Clone>(
    sounds: Query<(Entity, &SpatialSound<T>, &AudioOcclusion), Without<AudioPlayer<LowPassAudio<T>>>>,
    sources: Res<Assets<T>>,
    mut filtered: ResMut<Assets<LowPassAudio<T>>>,
    mut commands: Commands,
) {
    for (entity, sound, occlusion) in &sounds {
        let Some(source) = sources.get(&sound.source) else {
            continue;
        };
        let handle = filtered.add(LowPassAudio::new(
            source.clone(),
            occlusion.cutoff().clone(),
            sound.repeat,
        ));
        commands.entity(entity).insert(AudioPlayer(handle));
    }
}

#[cfg(test)]
#[path = "tests/spatial.rs"]
mod tests;
