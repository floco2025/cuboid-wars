use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, CollisionWorld,
        KnockbackVelocity, PortalSet, character_move_plans_intersect, player_control_velocity,
    },
    protocol::{
        ActorMarker, FieldId, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position, PowerUpKind, SwitchState,
    },
};

use super::{PlayerMovementStep, momentum_displacement, step_player_movement};
use crate::players::{BumpFeedbackState, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap};

pub struct PlayerMove {
    pub entity: Entity,
    pub start: Position,
    pub result: CharacterMovementResult,
    pub control_velocity: Vec3,
    pub external_displacement: Vec3,
    pub hits_character: bool,
}

pub(crate) fn plan_player_moves(
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    switch_state: &SwitchState,
    portal_set: &PortalSet,
    carriers: &Carriers,
    local_dead: bool,
    query: &mut PlayerMovementQuery,
    actors: &[CharacterMovePlan],
) -> Vec<PlayerMove> {
    if local_dead {
        return Vec::new();
    }
    let player_physics = gameplay_config.player.physics();
    let mut blockers = actors.to_vec();
    blockers.extend(
        query
            .iter()
            .filter(|(.., is_local)| !is_local)
            .map(|(entity, _, position, _, motion, ..)| {
                CharacterMovePlan::stationary(entity, *position, motion.0, player_physics)
            }),
    );
    let mut moves = Vec::new();
    for (entity, player_id, client_pos, move_intent, motion, _, knockback, airborne_momentum, _, is_local) in
        query.iter()
    {
        if !is_local {
            continue;
        }
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[FieldId] = info.map_or(&[], |i| i.held_keys.as_slice());

        let control_velocity = player_control_velocity(
            *move_intent,
            &map_settings.movement,
            has_speed_power_up,
            movement_disabled,
        );

        let external_displacement = momentum_displacement(Some(knockback), Some(airborne_momentum), delta);
        let request = PlayerMovementStep {
            start: *client_pos,
            vertical_velocity: motion.0,
            control_velocity,
            delta,
            has_low_gravity,
            held_keys,
            open_fields: &switch_state.open_fields,
            external_displacement,
            collision_world,
            map_settings,
            gameplay_config,
            portal_set,
            carriers,
        };
        moves.push(plan_player_move(entity, request, &blockers));
    }
    moves
}

// Both rendered and headless owners retry a body-blocked move with vertical
// travel only, so support and landing outcomes describe the accepted position.
pub fn plan_player_move(entity: Entity, request: PlayerMovementStep<'_>, blockers: &[CharacterMovePlan]) -> PlayerMove {
    let mut result = step_player_movement(request);
    let candidate = CharacterMovePlan::from_movement_result(
        entity,
        request.start,
        result,
        request.gameplay_config.player.physics(),
    );
    let hits_character = overlapping_character(&candidate, blockers).is_some();
    let mut control_velocity = request.control_velocity;
    let mut external_displacement = request.external_displacement;
    if hits_character {
        control_velocity = Vec3::ZERO;
        external_displacement *= Vec3::Y;
        result = step_player_movement(PlayerMovementStep {
            control_velocity,
            external_displacement,
            ..request
        });
    }
    PlayerMove {
        entity,
        start: request.start,
        result,
        control_velocity,
        external_displacement,
        hits_character,
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

pub(crate) type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static PlayerMoveIntent,
        &'static mut CharacterVerticalVelocity,
        Option<&'static mut BumpFeedbackState>,
        &'static KnockbackVelocity,
        &'static mut AirborneMomentum,
        &'static mut PlayerAnimationMotion,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

#[cfg(test)]
#[path = "tests/planning.rs"]
mod tests;
