use bevy::audio::Volume;

use crate::constants::AUDIO_VOLUME_DB_MIN;

pub(crate) fn settings_volume(db: f32) -> Volume {
    if db <= AUDIO_VOLUME_DB_MIN {
        Volume::Linear(0.0)
    } else {
        Volume::Decibels(db)
    }
}
