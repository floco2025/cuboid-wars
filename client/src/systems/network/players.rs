use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    markers::MainCameraMarker,
    resources::{PlayerInfo, PlayerMap, RoundTripTime},
    spawning::{spawn_player, spawn_projectiles},
    systems::players::{CameraShake, CuboidShake},
};
use common::{constants::POWER_UP_SPEED_MULTIPLIER, markers::PlayerMarker, protocol::*};

// ============================================================================
// Player Message Handlers
// ============================================================================

/// Handle player speed update with server reconciliation.
pub fn handle_player_speed_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    msg: SSpeed,
) {
    trace!("{:?} speed: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        let multiplier = if player.speed_power_up { POWER_UP_SPEED_MULTIPLIER } else { 1.0 };
        let velocity = msg.speed.to_velocity().with_speed_multiplier(multiplier);

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                velocity, // Never the local player, so we can always insert velocity
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.pos,
                    server_vel: velocity,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            commands.entity(player.entity).insert(velocity);
        }
    }
}

/// Handle player face direction update.
pub fn handle_player_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: SFace) {
    trace!("{:?} face direction: {}", msg.id, msg.dir);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
    }
}

/// Handle player shooting - spawn projectile(s) on client.
pub fn handle_player_shot_message(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    msg: SShot,
    map_layout: Option<&MapLayout>,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((position, _)) = player_data.get(player.entity)
            && let Some(map_layout) = map_layout
        {
            spawn_projectiles(
                commands,
                meshes,
                materials,
                position,
                msg.face_dir,
                msg.face_pitch,
                player.multi_shot_power_up,
                map_layout.lower_walls.as_slice(),
                map_layout.ramps.as_slice(),
                map_layout.roofs.as_slice(),
                msg.id,
            );
        }
    }
}

/// Handle player being hit - apply camera shake or cuboid shake.
pub fn handle_player_hit_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    msg: SHit,
) {
    debug!("player {:?} was hit", msg.id);
    if msg.id == my_player_id {
        if let Ok(camera_entity) = camera_query.single() {
            commands.entity(camera_entity).insert(CameraShake {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                intensity: 3.0,
                dir_x: msg.hit_dir_x,
                dir_z: msg.hit_dir_z,
                offset_x: 0.0,
                offset_y: 0.0,
                offset_z: 0.0,
            });
        }
    } else if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(CuboidShake {
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            intensity: 0.3,
            dir_x: msg.hit_dir_x,
            dir_z: msg.hit_dir_z,
            offset_x: 0.0,
            offset_z: 0.0,
        });
    }
}

/// Handle player status update (power-ups, stun).
pub fn handle_player_status_message(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    msg: SPlayerStatus,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
) {
    if let Some(player_info) = players.0.get_mut(&msg.id) {
        // Play power-up sound effect only for the local player
        if msg.id == my_player_id {
            // Don't play power-up sound effect if this message is due to a stun change
            if player_info.stunned == msg.stunned {
                // Only play power-up sound effect if it wasn't a downgrade
                #[allow(clippy::nonminimal_bool)]
                if !(player_info.speed_power_up && !msg.speed_power_up
                    || player_info.multi_shot_power_up && !msg.multi_shot_power_up
                    || player_info.phasing_power_up && !msg.phasing_power_up)
                {
                    commands.spawn((
                        AudioPlayer::new(asset_server.load("sounds/player_powerup.wav")),
                        PlaybackSettings::DESPAWN,
                    ));
                }
            }
        }

        player_info.speed_power_up = msg.speed_power_up;
        player_info.multi_shot_power_up = msg.multi_shot_power_up;
        player_info.phasing_power_up = msg.phasing_power_up;
        player_info.sentry_hunt_power_up = msg.sentry_hunt_power_up;
        player_info.stunned = msg.stunned;
    }
}

// ============================================================================
// Player Synchronization Helper
// ============================================================================

/// Synchronize players from bulk Update message - spawn/despawn/reconcile.
pub fn sync_players(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    server_players: &[(PlayerId, Player)],
) {
    // Track which players the server knows about in this snapshot
    let update_ids: HashSet<PlayerId> = server_players.iter().map(|(id, _)| *id).collect();

    // Spawn any players that appear in the update but are missing locally
    for (id, player) in server_players {
        if players.0.contains_key(id) {
            continue;
        }

        let is_local = *id == my_player_id;
        debug!("spawning player {:?} from Update (is_local: {})", id, is_local);
        let multiplier = if player.speed_power_up { POWER_UP_SPEED_MULTIPLIER } else { 1.0 };
        let velocity = player.speed.to_velocity().with_speed_multiplier(multiplier);
        let entity = spawn_player(
            commands,
            asset_server,
            meshes,
            materials,
            images,
            graphs,
            id.0,
            &player.name,
            &player.pos,
            velocity,
            player.face_dir,
            is_local,
        );

        if is_local && let Ok(camera_entity) = camera_query.single() {
            let camera_rotation = player.face_dir + std::f32::consts::PI;
            commands.entity(camera_entity).insert(
                Transform::from_xyz(player.pos.x, 2.5, player.pos.z + 3.0)
                    .with_rotation(Quat::from_rotation_y(camera_rotation)),
            );
        }

        players.0.insert(
            *id,
            PlayerInfo {
                entity,
                hits: player.hits,
                name: player.name.clone(),
                speed_power_up: player.speed_power_up,
                multi_shot_power_up: player.multi_shot_power_up,
                phasing_power_up: player.phasing_power_up,
                sentry_hunt_power_up: player.sentry_hunt_power_up,
                stunned: player.stunned,
            },
        );
    }

    // Despawn players no longer present in the authoritative snapshot
    let stale_ids: Vec<PlayerId> = players
        .0
        .keys()
        .filter(|id| !update_ids.contains(id))
        .copied()
        .collect();

    for id in stale_ids {
        if let Some(player) = players.0.remove(&id) {
            commands.entity(player.entity).despawn();
        }
    }

    // Update existing players with server state
    for (id, server_player) in server_players {
        if let Some(client_player) = players.0.get_mut(id) {
            if let Ok((client_pos, _)) = player_data.get(client_player.entity) {
                let multiplier = if server_player.speed_power_up { POWER_UP_SPEED_MULTIPLIER } else { 1.0 };
                let server_vel = server_player.speed.to_velocity().with_speed_multiplier(multiplier);

                // The local player's velocity is always authoritive, so don't overwrite from
                // server updates
                if *id != my_player_id {
                    commands.entity(client_player.entity).insert(server_vel);
                }
                commands.entity(client_player.entity).insert(ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: server_player.pos,
                    server_vel,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                });
            }

            client_player.hits = server_player.hits;
            client_player.speed_power_up = server_player.speed_power_up;
            client_player.multi_shot_power_up = server_player.multi_shot_power_up;
            client_player.phasing_power_up = server_player.phasing_power_up;
        }
    }
}
