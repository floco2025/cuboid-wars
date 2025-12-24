use bevy::prelude::*;

use super::movement::{find_visible_moving_player, patrol_movement, pre_patrol_movement, target_movement};
use crate::{
    constants::*,
    net::ServerToClient,
    resources::{GridConfig, PlayerMap, SentryGrid, SentryMap, SentryMode},
    systems::network::broadcast_to_all,
};
use common::{
    collision::overlap_sentry_vs_player,
    constants::*,
    markers::{PlayerMarker, SentryMarker},
    protocol::*,
};

// ============================================================================
// Sentries Movement System
// ============================================================================

pub fn sentries_movement_system(
    time: Res<Time>,
    map_layout: Res<MapLayout>,
    grid_config: Res<GridConfig>,
    players: Res<PlayerMap>,
    mut sentries: ResMut<SentryMap>,
    mut sentry_grid_map: ResMut<SentryGrid>,
    mut param_set: ParamSet<(
        Query<(&SentryId, &mut Position, &mut Velocity, &mut FaceDirection), With<SentryMarker>>,
        Query<(&PlayerId, &Position, &Speed), With<PlayerMarker>>,
    )>,
) {
    let delta = time.delta_secs();
    let mut rng = rand::rng();

    // Use all_walls for sentry collision (sentries never go on roofs)
    let sentry_walls = &map_layout.lower_walls;

    // Collect player positions and speeds (excluding stunned players)
    let player_data: Vec<(PlayerId, Position, Speed)> = param_set
        .p1()
        .iter()
        .filter(|(player_id, _, _)| {
            // Filter out stunned players
            players.0.get(player_id).is_some_and(|info| info.stun_timer <= 0.0)
        })
        .map(|(player_id, position, speed)| (*player_id, *position, *speed))
        .collect();

    // First, collect all sentry data and player data we need
    let mut sentry_updates = Vec::new();
    for (sentry_id, sentry_pos, sentry_vel, face_dir) in param_set.p0().iter() {
        sentry_updates.push((*sentry_id, *sentry_pos, *sentry_vel, face_dir.0));
    }

    // Now process sentry updates
    for (sentry_id, mut sentry_pos, mut sentry_vel, mut face_dir) in sentry_updates {
        let Some(sentry_info) = sentries.0.get_mut(&sentry_id) else {
            continue;
        };

        // Handle mode transitions
        match sentry_info.mode {
            SentryMode::Patrol => {
                // Decrement cooldown timer
                sentry_info.mode_timer -= delta;

                // Always check for visible players
                if let Some(target_player_id) = find_visible_moving_player(&sentry_pos, &player_data, sentry_walls) {
                    let player_has_sentry_hunt = ALWAYS_SENTRY_HUNT
                        || players
                            .0
                            .get(&target_player_id)
                            .is_some_and(|info| info.sentry_hunt_power_up_timer > 0.0);

                    // Enter target mode if: player has sentry hunt (flee) OR cooldown expired (attack)
                    if player_has_sentry_hunt || sentry_info.mode_timer <= 0.0 {
                        sentry_info.mode = SentryMode::Target;
                        sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                        sentry_info.follow_target = Some(target_player_id);
                        // Remove from field map when leaving patrol mode
                        // Must clear both current AND destination cells (sentry occupies two cells while patrolling)
                        sentry_grid_map.clear_patrol_cells(&sentry_pos, &sentry_vel, sentry_id);
                    }
                }
            }
            SentryMode::Target => {
                // Check if we're fleeing from a player with sentry hunt power-up
                let is_fleeing = sentry_info
                    .follow_target
                    .and_then(|target_id| players.0.get(&target_id))
                    .is_some_and(|info| ALWAYS_SENTRY_HUNT || info.sentry_hunt_power_up_timer > 0.0);

                // Update target timer: only decrement when not fleeing
                if is_fleeing {
                    // If a sentry was attacking and is now fleeing, the timer has been decremented
                    // previously, so we reset it every time we are fleeing
                    sentry_info.mode_timer = SENTRY_TARGET_DURATION;
                } else {
                    sentry_info.mode_timer -= delta;
                }

                if sentry_info.mode_timer <= 0.0 {
                    // Target timer expired, switch to pre-patrol with cooldown
                    sentry_info.mode = SentryMode::PrePatrol;
                    sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION;
                    sentry_info.follow_target = None;
                } else {
                    // Check if target player still exists and is not stunned
                    if let Some(target_id) = sentry_info.follow_target {
                        let target_info = players.0.get(&target_id);
                        let target_valid = target_info.is_some_and(|info| info.logged_in && info.stun_timer <= 0.0);
                        let target_on_roof = player_data
                            .iter()
                            .find(|(id, _, _)| *id == target_id)
                            .is_some_and(|(_, pos, _)| pos.y >= ROOF_HEIGHT);

                        if !target_valid || target_on_roof {
                            // Target disconnected, stunned, or on a roof, switch to pre-patrol
                            sentry_info.mode = SentryMode::PrePatrol;
                            sentry_info.mode_timer = SENTRY_COOLDOWN_DURATION;
                            sentry_info.follow_target = None;
                        }
                    }
                }
            }
            SentryMode::PrePatrol => {
                // PrePatrol doesn't have a timer - it transitions when reaching grid center
                // The transition is handled in pre_patrol_movement
            }
        }

        // Execute movement based on current mode
        match sentry_info.mode {
            SentryMode::PrePatrol => {
                pre_patrol_movement(
                    &sentry_id,
                    &mut sentry_pos,
                    &mut sentry_vel,
                    &mut face_dir,
                    sentry_info,
                    &players,
                    &mut sentry_grid_map,
                    delta,
                );
            }
            SentryMode::Patrol => {
                patrol_movement(
                    &sentry_id,
                    &mut sentry_pos,
                    &mut sentry_vel,
                    &mut face_dir,
                    sentry_info,
                    &grid_config,
                    &players,
                    &mut sentry_grid_map,
                    delta,
                    &mut rng,
                );
            }
            SentryMode::Target => {
                if let Some(target_id) = sentry_info.follow_target {
                    target_movement(
                        &sentry_id,
                        &mut sentry_pos,
                        &mut sentry_vel,
                        target_id,
                        &player_data,
                        sentry_walls,
                        &map_layout.ramps,
                        &players,
                        delta,
                    );
                }
            }
        }

        // Write back the updated position, velocity, and face direction
        if let Ok((_, mut pos, mut vel, mut fd)) = param_set.p0().get_mut(sentry_info.entity) {
            *pos = sentry_pos;
            *vel = sentry_vel;
            fd.0 = face_dir;
        }
    }
}

// ============================================================================
// Sentry-Player Collision System
// ============================================================================

// Check for sentry-player collisions and apply stun
pub fn sentry_player_collision_system(
    mut sentries: ResMut<SentryMap>,
    mut players: ResMut<PlayerMap>,
    sentry_query: Query<(&SentryId, &Position), With<SentryMarker>>,
    player_query: Query<(&PlayerId, &Position), With<PlayerMarker>>,
) {
    // Collect sentry positions
    let sentry_positions: Vec<(SentryId, Position)> = sentry_query.iter().map(|(id, pos)| (*id, *pos)).collect();

    // Collect player collisions first
    let mut player_hits: Vec<(PlayerId, SentryId)> = Vec::new();

    for (player_id, player_position) in &player_query {
        let Some(player_info) = players.0.get(player_id) else {
            continue;
        };

        // Skip if already stunned
        if player_info.stun_timer > 0.0 {
            continue;
        }

        // Skip if player has hunt power-up
        if ALWAYS_SENTRY_HUNT || player_info.sentry_hunt_power_up_timer > 0.0 {
            continue;
        }

        // Check collision with any sentry
        for (sentry_id, sentry_pos) in &sentry_positions {
            let Some(sentry_info) = sentries.0.get(sentry_id) else {
                continue;
            };

            if sentry_info.mode != SentryMode::Target {
                continue; // Skip stunning if sentry is not targeting
            }

            if sentry_info.follow_target != Some(*player_id) {
                continue; // Sentry is targeting someone else
            }

            if overlap_sentry_vs_player(sentry_pos, player_position) {
                player_hits.push((*player_id, *sentry_id));
                break; // Only one hit per frame
            }
        }
    }

    // Apply stun and broadcast
    for (player_id, sentry_id) in player_hits {
        let status_msg = if let Some(player_info) = players.0.get_mut(&player_id) {
            player_info.stun_timer = SENTRY_STUN_DURATION;
            player_info.hits -= SENTRY_HIT_PENALTY;

            // Send sentry hit message only to the hit player for sound effect
            let _ = player_info
                .channel
                .send(ServerToClient::Send(ServerMessage::SentryHit(SSentryHit {})));

            Some(player_info.status(player_id))
        } else {
            None
        };

        if let Some(status) = status_msg {
            broadcast_to_all(&players, ServerMessage::PlayerStatus(status));
        }

        // Put sentry into pre-patrol mode after hitting a player (will return to grid center)
        if let Some(sentry_info) = sentries.0.get_mut(&sentry_id) {
            sentry_info.mode = SentryMode::PrePatrol;
            sentry_info.mode_timer = 0.0;
        }
    }
}
