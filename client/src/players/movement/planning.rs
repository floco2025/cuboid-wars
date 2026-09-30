use bevy::{ecs::query::QueryData, prelude::*};
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        CharacterMovePlan, CharacterMovementResult, CharacterVerticalVelocity, CollisionWorld, HorizontalVelocity,
        KnockbackVelocity, PortalSet, character_move_plans_intersect,
    },
    protocol::{
        ActorMarker, FieldId, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, PlayerStance, Position,
        PowerUpKind, SwitchState,
    },
};

use super::{PlayerMovementStep, step_player_movement, step_player_movement_blocked};
use crate::{
    characters::PreviousTickPosition,
    players::{BumpFeedbackState, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap},
};

pub struct PlayerMove {
    pub entity: Entity,
    pub start: Position,
    pub result: CharacterMovementResult,
    pub control_velocity: Vec3,
    pub external_displacement: Vec3,
    pub hits_character: bool,
    pub horizontal_velocity: Vec3,
    pub stance: PlayerStance,
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
    let mut blockers = actors.to_vec();
    blockers.extend(query.iter().filter(|player| !player.is_local).map(|player| {
        CharacterMovePlan::stationary(
            player.entity,
            *player.position,
            player.vertical_velocity.0,
            player.stance.physics(&gameplay_config.player),
        )
    }));
    let mut moves = Vec::new();
    for player in query.iter().filter(|player| player.is_local) {
        let info = players.get(player.id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[FieldId] = info.map_or(&[], |i| i.held_keys.as_slice());

        let external_displacement = player.knockback.step(delta);
        let request = PlayerMovementStep {
            start: *player.position,
            vertical_velocity: player.vertical_velocity.0,
            horizontal_velocity: player.horizontal_velocity.0,
            stance: *player.stance,
            intent: *player.intent,
            has_speed: has_speed_power_up,
            disabled: movement_disabled,
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
        moves.push(plan_player_move(player.entity, request, &blockers));
    }
    moves
}

// Both rendered and headless owners share this body-blocking policy.
pub fn plan_player_move(
    entity: Entity,
    mut request: PlayerMovementStep<'_>,
    blockers: &[CharacterMovePlan],
) -> PlayerMove {
    let mut stepped = step_player_movement(request);
    if request.stance.crouched && !stepped.stance.crouched {
        let standing = CharacterMovePlan::stationary(
            entity,
            stepped.start,
            0.0,
            stepped.stance.physics(&request.gameplay_config.player),
        );
        if overlapping_character(&standing, blockers).is_some() {
            request.intent.crouch = true;
            stepped = step_player_movement(request);
        }
    }
    let candidate = CharacterMovePlan::from_movement_result(
        entity,
        stepped.start,
        stepped.movement,
        stepped.stance.physics(&request.gameplay_config.player),
    );
    let hits_character = overlapping_character(&candidate, blockers).is_some();
    if hits_character {
        stepped = step_player_movement_blocked(request);
    }
    PlayerMove {
        entity,
        start: stepped.start,
        result: stepped.movement,
        control_velocity: stepped.control_velocity,
        external_displacement: stepped.external_displacement,
        horizontal_velocity: stepped.horizontal_velocity,
        stance: stepped.stance,
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

#[derive(QueryData)]
#[query_data(mutable)]
pub(crate) struct PlayerMovement {
    pub entity: Entity,
    pub id: &'static PlayerId,
    pub position: &'static mut Position,
    pub previous_position: &'static mut PreviousTickPosition,
    pub intent: &'static PlayerMoveIntent,
    pub vertical_velocity: &'static mut CharacterVerticalVelocity,
    pub feedback: Option<&'static mut BumpFeedbackState>,
    pub knockback: &'static KnockbackVelocity,
    pub horizontal_velocity: &'static mut HorizontalVelocity,
    pub animation: &'static mut PlayerAnimationMotion,
    pub stance: &'static mut PlayerStance,
    pub is_local: Has<LocalPlayerMarker>,
}

pub(crate) type PlayerMovementQuery<'w, 's> = Query<'w, 's, PlayerMovement, (With<PlayerMarker>, Without<ActorMarker>)>;

#[cfg(test)]
#[path = "tests/planning.rs"]
mod tests;
