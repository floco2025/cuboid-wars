use bevy::prelude::*;
use common::physics::{CharacterMovePlan, overlaps_other_character};

use super::{
    feedback::{release_collision_feedback_after_clear_frames, trigger_collision_feedback},
    types::PlayerMovementQuery,
};
use crate::config::AssetSet;

pub(crate) fn apply_player_moves(
    commands: &mut Commands,
    delta: f32,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    query: &mut PlayerMovementQuery,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, _, mut client_pos, _, mut motion, mut feedback_state, _, is_local)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

        let hits_character = overlaps_other_character(planned_move, planned_moves);

        if hits_character {
            client_pos.y = planned_move.target.y;
            motion.0 = planned_move.target_vertical_velocity;

            if is_local && let Some(state) = feedback_state.as_mut() {
                trigger_collision_feedback(commands, asset_server, asset_set, state, false);
            }
        } else {
            *client_pos = planned_move.target;
            motion.0 = planned_move.target_vertical_velocity;

            if let Some(state) = feedback_state.as_mut() {
                if planned_move.blocked {
                    if is_local {
                        trigger_collision_feedback(commands, asset_server, asset_set, state, true);
                    }
                } else {
                    release_collision_feedback_after_clear_frames(state, delta);
                }
            }
        }
    }
}
