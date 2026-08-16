use bevy::prelude::*;

use crate::{audio::play_sound, config::AssetSet, players::BumpFeedbackState};

const BUMP_COLLISION_RELEASE_DELAY: f32 = 0.25;

pub(super) fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    state: &mut Mut<BumpFeedbackState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        let sound_path = if collided_with_wall {
            asset_set.player_sound("bump_wall")
        } else {
            asset_set.player_sound("bump_player")
        };

        play_sound(commands, asset_server, sound_path);
    }

    state.was_colliding = true;
    state.release_timer = BUMP_COLLISION_RELEASE_DELAY;
}

pub(super) fn release_collision_feedback_after_clear_frames(state: &mut Mut<BumpFeedbackState>, delta: f32) {
    if !state.was_colliding {
        return;
    }

    state.release_timer -= delta;
    if state.release_timer <= 0.0 {
        state.was_colliding = false;
    }
}
