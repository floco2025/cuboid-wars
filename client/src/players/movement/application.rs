use bevy::prelude::*;
use common::physics::CharacterSupport;

use super::{
    feedback::bump,
    outcomes::LocalMovementStep,
    planning::{PlayerMove, PlayerMovementQuery},
};
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
    planned_moves: &[PlayerMove],
) {
    // A local body without a step this tick (dead) stays where it is, so the
    // render lerp collapses onto its position instead of replaying the last step.
    for mut player in query.iter_mut().filter(|player| player.is_local) {
        if !planned_moves
            .iter()
            .any(|planned_move| planned_move.entity == player.entity)
        {
            player.previous_position.0 = *player.position;
        }
    }
    for planned_move in planned_moves {
        let Ok(mut player) = query.get_mut(planned_move.entity) else {
            continue;
        };
        if !player.is_local {
            continue;
        }
        let result = planned_move.result;
        player.previous_position.0 = planned_move.start;
        *player.position = result.position;
        player.vertical_velocity.0 = result.vertical_velocity;
        player.horizontal_velocity.0 = planned_move.horizontal_velocity;
        *player.stance = planned_move.stance;
        player.animation.record_step(
            planned_move.start,
            &result,
            if result.support == CharacterSupport::Ladder {
                planned_move.control_velocity
            } else {
                planned_move.horizontal_velocity
            },
            planned_move.external_displacement,
            delta,
        );
        commands.entity(planned_move.entity).insert((
            result.grounding,
            LocalMovementStep {
                start: planned_move.start,
                crushed: result.crushed,
                impact_speed: result.impact_speed,
                carrier: result.carrier,
                support: result.support,
                carried: result.carried(),
            },
        ));
        if let Some(state) = player.feedback.as_mut() {
            if planned_move.hits_character || result.blocked {
                bump(
                    commands,
                    asset_server,
                    asset_set,
                    &audio.bump,
                    state,
                    !planned_move.hits_character,
                );
            } else {
                let moved = (result.position.x - planned_move.start.x).hypot(result.position.z - planned_move.start.z);
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
