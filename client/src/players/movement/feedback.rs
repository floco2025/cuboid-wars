use bevy::prelude::*;

use crate::{config::AssetSet, players::BumpFlashState, ui::BumpFlashMarker};

const BUMP_FLASH_DURATION: f32 = 0.08;
const BUMP_COLLISION_RELEASE_DELAY: f32 = 0.25;

pub(super) fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashMarker>>,
    state: &mut Mut<BumpFlashState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Visible;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
        }

        let sound_path = if collided_with_wall {
            asset_set.player_sound("bump_wall")
        } else {
            asset_set.player_sound("bump_player")
        };

        commands.spawn((
            AudioPlayer::new(asset_server.load(sound_path.to_owned())),
            PlaybackSettings::DESPAWN,
        ));

        state.flash_timer = BUMP_FLASH_DURATION;
    }

    state.was_colliding = true;
    state.release_timer = BUMP_COLLISION_RELEASE_DELAY;
}

pub(super) fn release_collision_feedback_after_clear_frames(state: &mut Mut<BumpFlashState>, delta: f32) {
    if !state.was_colliding {
        return;
    }

    state.release_timer -= delta;
    if state.release_timer <= 0.0 {
        state.was_colliding = false;
    }
}
