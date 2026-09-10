use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    map::Carriers,
    physics::{
        AirborneMomentum, CharacterMovePlan, CharacterVerticalVelocity, CollisionWorld, KnockbackVelocity, PortalSet,
        player_control_velocity,
    },
    protocol::{
        ActorMarker, BarrierKindId, MapSettings, PlateState, PlayerId, PlayerMarker, PlayerMoveIntent, Position,
        PowerUpKind,
    },
};

use super::{PlayerMovementStep, momentum_displacement, outcomes::LocalMovementStep, step_player_movement};
use crate::players::{BumpFeedbackState, LocalPlayerMarker, PlayerAnimationMotion, PlayerMap};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: &CollisionWorld,
    map_settings: &MapSettings,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    plates: &PlateState,
    portal_set: &PortalSet,
    carriers: &Carriers,
    local_dead: bool,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_physics = gameplay_config.player.physics();
    for (
        entity,
        player_id,
        client_pos,
        move_intent,
        motion,
        _,
        knockback,
        mut airborne_momentum,
        mut animation_motion,
        is_local,
    ) in query
    {
        if !is_local {
            planned_moves.push(CharacterMovePlan::stationary(
                entity,
                *client_pos,
                motion.0,
                player_physics,
            ));
            continue;
        }
        // A dead body stays where death left it; nothing collides with it.
        if local_dead {
            continue;
        }
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());

        let control_velocity = player_control_velocity(
            *move_intent,
            &map_settings.movement,
            has_speed_power_up,
            movement_disabled,
        );

        let external_displacement = momentum_displacement(Some(knockback), Some(&*airborne_momentum), delta);
        let step = step_player_movement(PlayerMovementStep {
            start: *client_pos,
            vertical_velocity: motion.0,
            control_velocity,
            delta,
            has_low_gravity,
            held_keys,
            open_kinds: &plates.open_barrier_kinds,
            knockback,
            airborne_momentum: &mut airborne_momentum,
            collision_world,
            map_settings,
            gameplay_config,
            portal_set,
            carriers,
        });
        commands.entity(entity).insert((
            step.grounding,
            LocalMovementStep {
                start: *client_pos,
                crushed: step.crushed,
                impact_speed: step.impact_speed,
                carrier: step.carrier,
                support: step.support,
            },
        ));
        animation_motion.record_step(*client_pos, &step, control_velocity, external_displacement, delta);
        planned_moves.push(CharacterMovePlan::from_movement_result(
            entity,
            *client_pos,
            step,
            player_physics,
        ));
    }
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
