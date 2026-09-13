use bevy::{
    audio::{Decodable, Volume},
    prelude::*,
};

use super::{AudioAnalysis, LoopAudio};

#[derive(Component)]
pub struct NormalizationGain(pub Volume);

pub(crate) fn audio_plugin(app: &mut App) {
    app.add_observer(normalize_sound::<AudioSource>)
        .add_observer(normalize_sound::<LoopAudio>);
}

fn normalize_sound<T: Asset + Decodable>(
    event: On<Add, AudioPlayer<T>>,
    mut sounds: Query<(&AudioPlayer<T>, &mut PlaybackSettings)>,
    asset_server: Res<AssetServer>,
    analysis: Res<AudioAnalysis>,
    mut commands: Commands,
) {
    let Ok((player, mut settings)) = sounds.get_mut(event.entity) else {
        return;
    };
    let gain = asset_server
        .get_path(player.0.id())
        .and_then(|path| path.path().to_str().map(|path| analysis.gain(path)))
        .unwrap_or(1.0);
    let gain = Volume::Linear(gain);
    settings.volume *= gain;
    commands.entity(event.entity).insert(NormalizationGain(gain));
}

#[cfg(test)]
#[path = "tests/normalization.rs"]
mod tests;
