use bevy::prelude::*;

use super::components::BumpFlashState;
use crate::{config::AssetSet, markers::*, resources::PlayerMap, systems::network::ServerReconciliation};
use common::{
    config::GameplayConfig,
    constants::{ALWAYS_PHASING, PHYSICS_EPSILON, PLAYER_GROUND_SNAP_DISTANCE, UPDATE_BROADCAST_INTERVAL},
    markers::{ActorMarker, PlayerMarker},
    physics::{
        CharacterMovePlan, CharacterVerticalMotion, CollisionWorld, overlaps_other_character, step_character_movement,
    },
    protocol::{CharacterMoveIntent, PlayerId, Position},
};

const BUMP_FLASH_DURATION: f32 = 0.08;
const BUMP_COLLISION_RELEASE_DELAY: f32 = 0.25;

pub(crate) type PlayerMovementQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static PlayerId,
        &'static mut Position,
        &'static CharacterMoveIntent,
        &'static mut CharacterVerticalMotion,
        Option<&'static mut BumpFlashState>,
        Option<&'static mut ServerReconciliation>,
        Has<LocalPlayerMarker>,
    ),
    (With<PlayerMarker>, Without<ActorMarker>),
>;

pub(crate) fn plan_player_moves(
    commands: &mut Commands,
    delta: f32,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
    players: &PlayerMap,
    query: &mut PlayerMovementQuery,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
    planned_moves: &mut Vec<CharacterMovePlan>,
) {
    let player_config = &gameplay_config.characters.player;
    let player_physics = player_config.physics();
    for (entity, player_id, mut client_pos, move_intent, mut motion, mut flash_state, mut recon_option, is_local) in
        query
    {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, bump_flash_ui);
        }

        // Derive horizontal velocity from input intent + speed power-up.
        let has_speed_power_up = players.0.get(player_id).is_some_and(|info| info.speed_power_up);
        let h_vel = move_intent.to_player_horizontal_velocity(player_config.speed, has_speed_power_up);
        let is_standing_still = h_vel.x.hypot(h_vel.z) < PHYSICS_EPSILON;

        // Calculate intended position from velocity (with server reconciliation if needed)
        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            // Slow idle correction reduces visible sliding when standing
            // players receive snapshot corrections. Moving characters hide
            // correction better.
            const IDLE_RECONCILIATION_TIME: f32 = 10.0;
            let run_correction_time: f32 = recon.rtt * 5.0; // Benchmark: RTT = 100ms equals 0.5s correction time

            let speed_ratio = (h_vel.x.hypot(h_vel.z) / player_config.speed).clamp(0.0, 1.0); // Ignore speed power-ups
            let correction_time_interval = IDLE_RECONCILIATION_TIME.lerp(run_correction_time, speed_ratio);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
            let total_delta = server_pos - Vec3::from(recon.client_pos);

            // X/Z reconciliation below nudges the input-derived target position.
            // Vertical movement is calculated inside `step_character_movement` from
            // vertical velocity, gravity, and ground/ceiling collision. If the local
            // player has drifted vertically beyond normal ground snap tolerance, trust
            // the server velocity instead of applying a separate smooth vertical
            // position offset outside physics.
            const VERTICAL_VELOCITY_RECONCILE_DISTANCE: f32 = PLAYER_GROUND_SNAP_DISTANCE;
            if is_local && total_delta.y.abs() >= VERTICAL_VELOCITY_RECONCILE_DISTANCE {
                motion.0 = recon.server_velocity.y;
            }

            // If the player got totally out of sync, we jump to the server position
            let out_of_sync_distance = if is_standing_still { 1.0 } else { 5.0 };
            if total_delta.x.abs() >= out_of_sync_distance
                || total_delta.y.abs() >= out_of_sync_distance
                || total_delta.z.abs() >= out_of_sync_distance
            {
                let player_name = players
                    .0
                    .get(player_id)
                    .map_or_else(|| format!("{player_id:?}"), |info| info.name.clone());
                warn!(
                    "{player_name} out of sync by x={:.2}, y={:.2}, z={:.2}; jumping to server position",
                    total_delta.x, total_delta.y, total_delta.z
                );
                *client_pos = recon.server_pos;
                motion.0 = recon.server_velocity.y;
                commands.entity(entity).remove::<ServerReconciliation>();
                planned_moves.push(CharacterMovePlan {
                    entity,
                    start: *client_pos,
                    target: *client_pos,
                    target_vertical_velocity: motion.0,
                    physics: player_physics,
                    blocked: false,
                });
                continue;
            }

            let dx = total_delta.x * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
            let dz = total_delta.z * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

            let new_x = h_vel.x.mul_add(delta, client_pos.x) + dx;
            let new_z = h_vel.z.mul_add(delta, client_pos.z) + dz;

            Position {
                x: new_x,
                y: client_pos.y, // Keep current vertical position for collision detection
                z: new_z,
            }
        } else {
            let new_x = h_vel.x.mul_add(delta, client_pos.x);
            let new_z = h_vel.z.mul_add(delta, client_pos.z);
            Position {
                x: new_x,
                y: client_pos.y, // Keep current vertical position for collision detection
                z: new_z,
            }
        };

        if let Some(collision_world) = collision_world {
            let has_phasing = ALWAYS_PHASING || players.0.get(player_id).is_some_and(|info| info.phasing_power_up);

            let step = step_character_movement(
                &client_pos,
                motion.0,
                collision_world,
                has_phasing,
                player_physics,
                target_pos.x,
                target_pos.z,
                delta,
            );
            target_pos = step.position;
            planned_moves.push(CharacterMovePlan {
                entity,
                start: *client_pos,
                target: target_pos,
                target_vertical_velocity: step.vertical_velocity,
                physics: player_physics,
                blocked: step.blocked,
            });
        } else {
            planned_moves.push(CharacterMovePlan {
                entity,
                start: *client_pos,
                target: target_pos,
                target_vertical_velocity: motion.0,
                physics: player_physics,
                blocked: false,
            });
        }
    }
}

pub(crate) fn apply_player_moves(
    commands: &mut Commands,
    delta: f32,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    query: &mut PlayerMovementQuery,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
    planned_moves: &[CharacterMovePlan],
) {
    for planned_move in planned_moves {
        let Ok((_, _, mut client_pos, _, mut motion, mut flash_state, _, is_local)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

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

fn decay_flash_timer(
    state: &mut Mut<BumpFlashState>,
    delta: f32,
    is_local: bool,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
) {
    if state.flash_timer <= 0.0 {
        return;
    }

    state.flash_timer -= delta;
    if state.flash_timer <= 0.0
        && is_local
        && let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next()
    {
        *visibility = Visibility::Hidden;
        bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
    }
}

fn trigger_collision_feedback(
    commands: &mut Commands,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
    bump_flash_ui: &mut Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
    state: &mut Mut<BumpFlashState>,
    collided_with_wall: bool,
) {
    if !state.was_colliding {
        if let Some((mut bg_color, mut visibility)) = bump_flash_ui.iter_mut().next() {
            *visibility = Visibility::Visible;
            bg_color.0 = Color::srgba(1.0, 1.0, 1.0, 0.2);
        }

        let sound_path = if collided_with_wall {
            asset_set.sound("player_bumps_wall")
        } else {
            asset_set.sound("player_bumps_player")
        };

        commands.spawn((
            AudioPlayer::new(asset_server.load(sound_path.to_owned())),
            PlaybackSettings::DESPAWN,
        ));

        state.flash_timer = BUMP_FLASH_DURATION;
    }

    state.was_colliding = true;
    state.release_timer = BUMP_COLLISION_RELEASE_DELAY;
}

fn release_collision_feedback_after_clear_frames(state: &mut Mut<BumpFlashState>, delta: f32) {
    if !state.was_colliding {
        return;
    }

    state.release_timer -= delta;
    if state.release_timer <= 0.0 {
        state.was_colliding = false;
    }
}
