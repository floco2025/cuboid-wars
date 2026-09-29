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
    for (entity, _, position, mut previous_pos, .., is_local) in query.iter_mut() {
        if is_local && !planned_moves.iter().any(|planned_move| planned_move.entity == entity) {
            previous_pos.0 = *position;
        }
    }
    for planned_move in planned_moves {
        let Ok((
            _,
            _,
            mut client_pos,
            mut previous_pos,
            _,
            mut motion,
            mut feedback_state,
            _,
            mut momentum,
            mut animation_motion,
            mut stance,
            is_local,
        )) = query.get_mut(planned_move.entity)
        else {
            continue;
        };

        if !is_local {
            continue;
        }
        let result = planned_move.result;
        previous_pos.0 = planned_move.start;
        *client_pos = result.position;
        motion.0 = result.vertical_velocity;
        momentum.0 = planned_move.horizontal_velocity;
        *stance = planned_move.stance;
        animation_motion.record_step(
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
            },
        ));
        if let Some(state) = feedback_state.as_mut() {
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
