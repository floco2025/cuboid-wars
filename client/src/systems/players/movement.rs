use bevy::prelude::*;

use super::components::BumpFlashState;
use crate::{config::AssetSet, markers::*, resources::PlayerMap, systems::network::ServerReconciliation};
use common::{
    constants::{
        ALWAYS_PHASING, PHYSICS_EPSILON, PLAYER_GROUND_SNAP_DISTANCE, PLAYER_SPEED, UPDATE_BROADCAST_INTERVAL,
    },
    physics::{
        CharacterVerticalMotion, CollisionWorld, PlannedCharacterMove, overlaps_other_character,
        step_character_movement,
    },
    protocol::{CharacterMoveIntent, PlayerId, Position},
};

// ============================================================================
// Helper Functions
// ============================================================================

const BUMP_FLASH_DURATION: f32 = 0.08;
const BUMP_COLLISION_RELEASE_DELAY: f32 = 0.25;

fn reconcile_vertical_motion_if_needed(
    motion: &mut CharacterVerticalMotion,
    recon: &ServerReconciliation,
    total_delta: Vec3,
    is_local: bool,
) {
    if !is_local || total_delta.y.abs() < PLAYER_GROUND_SNAP_DISTANCE {
        return;
    }

    motion.vertical_velocity = recon.server_velocity.y;
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

// ============================================================================
// Players Movement System
// ============================================================================

type MovementQuery<'w, 's> = Query<
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
>;

pub fn players_movement_system(
    mut commands: Commands,
    time: Res<Time>,
    asset_server: Res<AssetServer>,
    asset_set: Res<AssetSet>,
    collision_world: Option<Res<CollisionWorld>>,
    players: Res<PlayerMap>,
    mut query: MovementQuery,
    mut bump_flash_ui: Query<(&mut BackgroundColor, &mut Visibility), With<BumpFlashUIMarker>>,
) {
    let delta = time.delta_secs();

    // Pass 1: For each player, calculate intended position, then apply static-world collision.
    let mut planned_moves: Vec<PlannedCharacterMove> = Vec::new();

    for (entity, player_id, mut client_pos, move_intent, mut motion, mut flash_state, mut recon_option, is_local) in
        &mut query
    {
        if let Some(state) = flash_state.as_mut() {
            decay_flash_timer(state, delta, is_local, &mut bump_flash_ui);
        }

        // Derive horizontal velocity from input intent + speed power-up.
        let has_speed_power_up = players.0.get(player_id).is_some_and(|info| info.speed_power_up);
        let h_vel = move_intent.to_player_horizontal_velocity(has_speed_power_up);
        let is_standing_still = h_vel.x.hypot(h_vel.z) < PHYSICS_EPSILON;

        // Calculate intended position from velocity (with server reconciliation if needed)
        let mut target_pos = if let Some(recon) = recon_option.as_mut() {
            const IDLE_CORRECTION_TIME: f32 = 10.0; // Standing still: slow, smooth correction
            let run_correction_time: f32 = recon.rtt * 5.0; // Benchmark: RTT = 100ms equals 0.5s correction time

            let speed_ratio = (h_vel.x.hypot(h_vel.z) / PLAYER_SPEED).clamp(0.0, 1.0); // Ignore speed power-ups
            let correction_time_interval = IDLE_CORRECTION_TIME.lerp(run_correction_time, speed_ratio);
            let correction_factor = (UPDATE_BROADCAST_INTERVAL / correction_time_interval).clamp(0.0, 1.0);

            recon.timer += delta * correction_factor;
            if recon.timer >= UPDATE_BROADCAST_INTERVAL {
                commands.entity(entity).remove::<ServerReconciliation>();
            }

            let server_pos = Vec3::from(recon.server_pos) + recon.server_velocity * recon.rtt / 2.0;
            let total_delta = server_pos - Vec3::from(recon.client_pos);
            reconcile_vertical_motion_if_needed(&mut motion, recon, total_delta, is_local);

            // If the player got totally out of sync, we jump to the server position
            let out_of_sync_distance = if is_standing_still { 3.0 } else { 5.0 };
            if total_delta.x.abs() >= out_of_sync_distance
                || total_delta.y.abs() >= 1.0
                || total_delta.z.abs() >= out_of_sync_distance
            {
                let player_name = players
                    .0
                    .get(player_id)
                    .map_or_else(|| format!("{player_id:?}"), |info| info.name.clone());
                warn!("{player_name} out of sync, jumping to server position");
                *client_pos = recon.server_pos;
                motion.vertical_velocity = recon.server_velocity.y;
                commands.entity(entity).remove::<ServerReconciliation>();
                continue;
            }

            let dx = total_delta.x * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;
            let dz = total_delta.z * delta * correction_factor / UPDATE_BROADCAST_INTERVAL;

            let new_x = h_vel.x.mul_add(delta, client_pos.x) + dx;
            let new_z = h_vel.z.mul_add(delta, client_pos.z) + dz;

            Position {
                x: new_x,
                y: client_pos.y, // Keep current Y for collision detection
                z: new_z,
            }
        } else {
            let new_x = h_vel.x.mul_add(delta, client_pos.x);
            let new_z = h_vel.z.mul_add(delta, client_pos.z);
            Position {
                x: new_x,
                y: client_pos.y, // Keep current Y for collision detection
                z: new_z,
            }
        };

        if let Some(collision_world) = collision_world.as_ref() {
            let has_phasing = ALWAYS_PHASING || players.0.get(player_id).is_some_and(|info| info.phasing_power_up);

            let step = step_character_movement(
                &client_pos,
                &motion,
                collision_world,
                has_phasing,
                target_pos.x,
                target_pos.z,
                delta,
            );
            target_pos = step.position;
            planned_moves.push(PlannedCharacterMove {
                entity,
                start: *client_pos,
                target: target_pos,
                target_vertical_velocity: step.vertical_velocity,
                blocked: step.blocked,
            });
        } else {
            planned_moves.push(PlannedCharacterMove {
                entity,
                start: *client_pos,
                target: target_pos,
                target_vertical_velocity: motion.vertical_velocity,
                blocked: false,
            });
        }
    }

    // Pass 2: Check player-player collisions and apply final positions
    for planned_move in &planned_moves {
        let Ok((_, _, mut client_pos, _, mut motion, mut flash_state, _, is_local)) =
            query.get_mut(planned_move.entity)
        else {
            continue;
        };

        let hits_player = overlaps_other_character(planned_move, &planned_moves);

        // Apply final position and feedback
        if hits_player {
            client_pos.y = planned_move.target.y;
            motion.vertical_velocity = planned_move.target_vertical_velocity;

            if is_local && let Some(state) = flash_state.as_mut() {
                trigger_collision_feedback(
                    &mut commands,
                    &asset_server,
                    &asset_set,
                    &mut bump_flash_ui,
                    state,
                    false,
                );
            }
        } else {
            *client_pos = planned_move.target;
            motion.vertical_velocity = planned_move.target_vertical_velocity;

            if let Some(state) = flash_state.as_mut() {
                if planned_move.blocked {
                    if is_local {
                        trigger_collision_feedback(
                            &mut commands,
                            &asset_server,
                            &asset_set,
                            &mut bump_flash_ui,
                            state,
                            true,
                        );
                    }
                } else {
                    release_collision_feedback_after_clear_frames(state, delta);
                }
            }
        }
    }
}
