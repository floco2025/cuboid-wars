use bevy::prelude::*;
use common::physics::{CharacterMovePlan, overlaps_other_character};

use super::{
    feedback::{release_collision_feedback_after_clear_frames, trigger_collision_feedback},
    types::PlayerMovementQuery,
};
use crate::{config::AssetSet, ui::BumpFlashMarker};

pub(crate) fn apply_player_moves(
    commands: &mut Commands,
    delta: f32,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    query: &mut PlayerMovementQuery,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashMarker>>,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, _, mut client_pos, _, mut motion, mut flash_state, _, is_local)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

        if let Some(state) = flash_state.as_mut()
            && state.flash_timer > 0.0
        {
            state.flash_timer -= delta;
            if state.flash_timer <= 0.0
                && is_local
                && let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next()
            {
                *visibility = Visibility::Hidden;
                bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
            }
        }

        let hits_character = overlaps_other_character(planned_move, planned_moves);

        if hits_character {
            client_pos.y = planned_move.target.y;
            motion.0 = planned_move.target_vertical_velocity;

            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(commands, asset_server, asset_set, bump_flash_ui, state, false);
            }
        } else {
            *client_pos = planned_move.target;
            motion.0 = planned_move.target_vertical_velocity;

            if let Some(state) = flash_state.as_mut() {
                if planned_move.blocked {
                    if is_local {
                        trigger_collision_feedback(commands, asset_server, asset_set, bump_flash_ui, state, true);
                    }
                } else {
                    release_collision_feedback_after_clear_frames(state, delta);
                }
            }
        }
    }
}
