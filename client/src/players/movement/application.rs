use bevy::prelude::*;
use common::physics::{CharacterMovePlan, overlapping_character};

use super::{feedback::bump, planning::PlayerMovementQuery};
use crate::config::{AssetSet, AudioConfig};

// Below this horizontal speed a tick counts as standing still.
const STANDSTILL_SPEED: f32 = 0.5;

pub(crate) fn apply_player_moves(
    commands: &mut Commands,
    delta: f32,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    audio: &AudioConfig,
    query: &mut PlayerMovementQuery,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, _, mut client_pos, _, mut motion, mut feedback_state, _, _, _, is_local)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

        let hits_character = overlapping_character(planned_move, planned_moves).is_some();

        if hits_character {
            client_pos.y = planned_move.target.y;
            motion.0 = planned_move.target_vertical_velocity;

            if is_local && let Some(state) = feedback_state.as_mut() {
                bump(commands, asset_server, asset_set, &audio.bump, state, false);
            }
        } else {
            *client_pos = planned_move.target;
            motion.0 = planned_move.target_vertical_velocity;

            if is_local && let Some(state) = feedback_state.as_mut() {
                if planned_move.blocked {
                    bump(commands, asset_server, asset_set, &audio.bump, state, true);
                } else {
                    let moved = (planned_move.target.x - planned_move.start.x)
                        .hypot(planned_move.target.z - planned_move.start.z);
                    // Standing still ends the run-up, so a hop at a wall from
                    // beside it starts from nothing.
                    state.run_up = if moved > delta * STANDSTILL_SPEED {
                        state.run_up + moved
                    } else {
                        0.0
                    };
                }
            }
        }
    }
}
