use bevy::prelude::*;
use common::physics::{
    CharacterMovePlan, PlayerMovementStep, PlayerStepResult, character_axis_separation, character_move_plans_intersect,
    step_player_movement, step_player_movement_blocked,
};

pub struct PlayerMove {
    pub step: PlayerStepResult,
    pub hits_character: bool,
}

// Both rendered and headless owners share this body-blocking policy: a move
// another body rejects is retried with the velocity into that body removed,
// as a wall contact leaves the rest, and stops outright if even that meets a
// body.
pub fn plan_player_move(
    entity: Entity,
    mut request: PlayerMovementStep<'_>,
    blockers: &[CharacterMovePlan],
) -> PlayerMove {
    let body = &request.gameplay_config.player;
    let mut stepped = step_player_movement(request);
    if request.stance.crouched && !stepped.stance.crouched {
        let standing = CharacterMovePlan::stationary(entity, stepped.start, 0.0, stepped.stance.physics(body));
        if overlapping_character(&standing, blockers).is_some() {
            request.intent.crouch = true;
            stepped = step_player_movement(request);
        }
    }
    let plan = |stepped: &PlayerStepResult| {
        CharacterMovePlan::from_movement_result(entity, stepped.start, stepped.movement, stepped.stance.physics(body))
    };
    let Some(other) = overlapping_character(&plan(&stepped), blockers) else {
        return PlayerMove {
            step: stepped,
            hits_character: false,
        };
    };
    let along = character_axis_separation(
        &other.start,
        other.physics,
        &stepped.start,
        request.stance.physics(body),
    )
    .with_y(0.0)
    .try_normalize();
    let slid = along.map(|along| step_player_movement_blocked(request, Some(along)));
    let step = match slid {
        Some(slid) if overlapping_character(&plan(&slid), blockers).is_none() => slid,
        _ => step_player_movement_blocked(request, None),
    };
    PlayerMove {
        step,
        hits_character: true,
    }
}

fn overlapping_character<'a>(
    candidate: &CharacterMovePlan,
    blockers: &'a [CharacterMovePlan],
) -> Option<&'a CharacterMovePlan> {
    blockers
        .iter()
        .find(|other| other.entity != candidate.entity && character_move_plans_intersect(candidate, other))
}

#[cfg(test)]
#[path = "tests/planning.rs"]
mod tests;
