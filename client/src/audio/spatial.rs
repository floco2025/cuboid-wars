use bevy::{audio::Decodable, prelude::*};

use super::{AudioOcclusion, LowPassAudio};

// The source a spatial sound was spawned with, kept for as long as it plays.
#[derive(Component)]
pub(crate) struct SpatialSound<T: Asset>(Handle<T>);

// Occlusion muffles every spatial sound, so its player is exchanged for one
// over `LowPassAudio` the moment it is added, before Bevy can build a sink
// from the unfiltered source.
pub(super) fn intercept_spatial_sound<T: Asset + Decodable>(
    event: On<Add, AudioPlayer<T>>,
    sounds: Query<(&AudioPlayer<T>, &PlaybackSettings)>,
    mut commands: Commands,
) {
    let Ok((player, settings)) = sounds.get(event.entity) else {
        return;
    };
    if !settings.spatial {
        return;
    }
    commands
        .entity(event.entity)
        .remove::<AudioPlayer<T>>()
        .insert((SpatialSound(player.0.clone()), AudioOcclusion::default()));
}

pub(super) fn attach_low_pass_system<T: Asset + Decodable + Clone>(
    sounds: Query<(Entity, &SpatialSound<T>, &AudioOcclusion), Without<AudioPlayer<LowPassAudio<T>>>>,
    sources: Res<Assets<T>>,
    mut filtered: ResMut<Assets<LowPassAudio<T>>>,
    mut commands: Commands,
) {
    for (entity, sound, occlusion) in &sounds {
        let Some(source) = sources.get(&sound.0) else {
            continue;
        };
        let handle = filtered.add(LowPassAudio::new(source.clone(), occlusion.cutoff().clone()));
        commands.entity(entity).insert(AudioPlayer(handle));
    }
}

#[cfg(test)]
#[path = "tests/spatial.rs"]
mod tests;
