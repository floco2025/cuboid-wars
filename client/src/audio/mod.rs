mod actor_sfx;
mod analysis;
mod looping;
mod normalization;
mod playback;
mod volume;

pub(crate) use actor_sfx::{actor_sfx_playback, play_actor_spatial_sound};
pub(crate) use analysis::AudioAnalysis;
pub(crate) use looping::{LoopAudio, LoopAudioLoader};
pub use normalization::NormalizationGain;
pub(crate) use normalization::audio_plugin;
pub(crate) use playback::{explosion_playback_settings, loop_sound_playback};
pub use playback::{
    play_explosion_sound, play_sound, play_sound_with, play_spatial_sound, play_spatial_sound_with, sound_playback,
};
pub(crate) use volume::settings_volume;
