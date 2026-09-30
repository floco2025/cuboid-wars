use bevy::prelude::*;
use common::{
    config::CharacterGameplayConfig,
    constants::PLAYER_CROUCH_HULL_RATIO,
    physics::CharacterSupport,
    protocol::{PlayerMarker, PlayerStance},
};

use crate::constants::{PLAYER_CROUCH_BLEND_SECS, PLAYER_CROUCH_EYE_RATIO};

// How far the eye and the pose have followed the stance's hull, 0 standing
// to 1 crouched. The hull itself changes at once; this is presentation.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct CrouchBlend(pub f32);

impl CrouchBlend {
    // The blend at rest in `stance`.
    #[must_use]
    pub fn settled(stance: PlayerStance) -> Self {
        Self(if stance.crouched { 1.0 } else { 0.0 })
    }

    // A grounded body takes `PLAYER_CROUCH_BLEND_SECS` to follow its stance;
    // an airborne duck is instant, since the hull moved about its centre.
    pub fn advance(&mut self, stance: PlayerStance, support: CharacterSupport, delta: f32) {
        let target = Self::settled(stance).0;
        self.0 = if support == CharacterSupport::Ground {
            let step = delta / PLAYER_CROUCH_BLEND_SECS;
            self.0 + (target - self.0).clamp(-step, step)
        } else {
            target
        };
    }

    #[must_use]
    pub fn eye_height(self, config: &CharacterGameplayConfig) -> f32 {
        config.eye_height() * self.blend(PLAYER_CROUCH_EYE_RATIO)
    }

    // Vertical scale of the rendered model, following the hull through the blend.
    #[must_use]
    pub fn model_height_scale(self) -> f32 {
        self.blend(PLAYER_CROUCH_HULL_RATIO)
    }

    fn blend(self, crouched: f32) -> f32 {
        1.0 - self.0 * (1.0 - crouched)
    }
}

pub(crate) fn crouch_blend_system(
    time: Res<Time>,
    mut players: Query<(&PlayerStance, &CharacterSupport, &mut CrouchBlend), With<PlayerMarker>>,
) {
    for (stance, support, mut blend) in &mut players {
        blend.advance(*stance, *support, time.delta_secs());
    }
}

#[cfg(test)]
#[path = "tests/crouch.rs"]
mod tests;
