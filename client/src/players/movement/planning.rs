use bevy::prelude::*;
use common::{
    config::GameplayConfig,
    physics::{
        CharacterEnvironment, CharacterMovePlan, CharacterStep, CharacterVerticalVelocity, CollisionWorld,
        KnockbackVelocity, PortalSet, passable_barrier_kinds, player_control_velocity, step_character_movement,
    },
    protocol::{
        ActorMarker, BarrierKindId, MapSettings, PlayerId, PlayerMarker, PlayerMoveIntent, Position, PowerUpKind,
    },
};

use crate::{
    network::ServerReconciliation,
    players::{BumpFeedbackState, LocalPlayerMarker, PlayerMap},
};

use super::reconciliation::{PlayerReconciliationOutcome, decayed_snap_speed, reconcile_player};

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    map_settings: Option<&MapSettings>,
    gameplay_config: &GameplayConfig,
    players: &mut PlayerMap,
    open_barrier_kinds: &crate::barriers::OpenBarrierKinds,
    portal_set: &PortalSet,
    query: &mut PlayerMovementQuery,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_physics = gameplay_config.player.physics();
    let run_speed = gameplay_config.movement.player.run_speed;
    for (entity, player_id, mut client_pos, move_intent, mut motion, _, mut recon_option, knockback, _) in query {
        // Decay snap_speed each tick; new snapshot speed wins if larger.
        // Persisted on `PlayerInfo`. Deliberately fed by the SERVER velocity
        // (authoritative recent speed for the snap threshold), while the
        // correction window reads the predicted velocity — what the player
        // perceives right now.
        let current_server_speed = recon_option.as_ref().map_or(0.0, |r| r.server_velocity.xz().length());
        let snap_speed = match players.get_mut(player_id) {
            Some(info) => {
                info.snap_speed = decayed_snap_speed(info.snap_speed, current_server_speed, run_speed, delta);
                info.snap_speed
            }
            None => current_server_speed,
        };

        // Immutable lookup for the read-only fields; the mut borrow above ended with the `match`.
        let info = players.get(player_id);
        let has_speed_power_up = info.is_some_and(|i| i.power_up(PowerUpKind::Speed));
        let has_low_gravity = info.is_some_and(|i| i.power_up(PowerUpKind::LowGravity));
        let movement_disabled = info.is_some_and(|i| i.stunned);
        let held_keys: &[BarrierKindId] = info.map_or(&[], |i| i.held_keys.as_slice());
        let player_name = info.map(|i| i.name.as_str());

        let control_velocity =
            player_control_velocity(*move_intent, gameplay_config, has_speed_power_up, movement_disabled);

        let correction_displacement = match recon_option.as_mut() {
            Some(recon) => match reconcile_player(
                commands,
                entity,
                player_id,
                player_name,
                &mut client_pos,
                &mut motion,
                recon,
                control_velocity,
                delta,
                run_speed,
                snap_speed,
            ) {
                PlayerReconciliationOutcome::Displacement(displacement) => displacement,
                PlayerReconciliationOutcome::Snapped => {
                    planned_moves.push(CharacterMovePlan::stationary(
                        entity,
                        *client_pos,
                        motion.0,
                        player_physics,
                    ));
                    continue;
                }
            },
            None => Vec3::ZERO,
        };
        let external_displacement = correction_displacement + knockback.map_or(Vec3::ZERO, |k| k.step(delta));

        // `CollisionWorld` and `MapSettings` are both installed by the same
        // `SInit`, so they appear together.
        if let (Some(collision_world), Some(map_settings)) = (collision_world, map_settings) {
            // Same merge the server runs (`passable_barrier_kinds`) so
            // client-side prediction agrees with server-authoritative
            // movement about which barriers we pass through.
            let passable_kinds = passable_barrier_kinds(held_keys, &open_barrier_kinds.0);
            let step = step_character_movement(
                CharacterStep {
                    start: *client_pos,
                    vertical_velocity: motion.0,
                    control_velocity,
                    external_displacement,
                    delta,
                },
                &CharacterEnvironment {
                    collision_world,
                    gravity: map_settings.gravity_for(has_low_gravity),
                    passable_kinds: &passable_kinds,
                    physics: player_physics,
                    ladder_climb_ratio: gameplay_config.movement.ladder_climb_ratio,
                    portals: Some(portal_set),
                },
            );
            planned_moves.push(CharacterMovePlan::from_movement_result(
                entity,
                *client_pos,
                step,
                player_physics,
            ));
        } else {
            let target = Position {
                x: control_velocity.x.mul_add(delta, client_pos.x) + external_displacement.x,
                y: client_pos.y,
                z: control_velocity.z.mul_add(delta, client_pos.z) + external_displacement.z,
            };
            planned_moves.push(CharacterMovePlan::from_target(
                entity,
                *client_pos,
                target,
                motion.0,
                player_physics,
                false,
            ));
        }
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
        Option<&'static mut ServerReconciliation>,
        Option<&'static KnockbackVelocity>,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;
