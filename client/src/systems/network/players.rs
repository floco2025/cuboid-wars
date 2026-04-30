use bevy::prelude::*;
use std::collections::HashSet;

use super::components::ServerReconciliation;
use crate::{
    config::AssetSet,
    markers::MainCameraMarker,
    resources::{PlayerInfo, PlayerMap, RoundTripTime},
    spawning::{ProjectileAssets, spawn_player, spawn_projectiles},
    systems::{CameraShake, CuboidShake},
};
use common::{
    markers::PlayerMarker,
    physics::{CollisionWorld, PlayerVerticalMotion},
    protocol::*,
};

// ============================================================================
// Player Message Handlers
// ============================================================================

fn movement_velocity(movement: PlayerMovementState, has_speed_power_up: bool) -> Vec3 {
    let mut velocity = movement.move_input.to_velocity_for_player(has_speed_power_up);
    velocity.y = movement.vertical_velocity;
    velocity
}

// Handle player move-input update with server reconciliation.
pub fn handle_player_move_input_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    msg: SMoveInput,
) {
    trace!("{:?} move input: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        let server_velocity = movement_velocity(msg.movement, player.speed_power_up);

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                msg.movement.move_input, // Never the local player, so we can always overwrite intent
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.movement.pos,
                    server_velocity,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            commands.entity(player.entity).insert(msg.movement.move_input);
        }
    }
}

pub fn handle_player_jump_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    msg: SJump,
) {
    if let Some(player) = players.0.get(&msg.id)
        && let Ok((client_pos, _, _)) = player_data.get(player.entity)
    {
        let server_velocity = movement_velocity(msg.movement, player.speed_power_up);
        commands.entity(player.entity).insert((
            msg.movement.move_input,
            PlayerVerticalMotion {
                vertical_velocity: msg.movement.vertical_velocity,
            },
            ServerReconciliation {
                client_pos: *client_pos,
                server_pos: msg.movement.pos,
                server_velocity,
                timer: 0.0,
                rtt: rtt.rtt.as_secs_f32(),
            },
        ));
    }
}

// Handle player face direction update.
pub fn handle_player_face_message(commands: &mut Commands, players: &ResMut<PlayerMap>, msg: SFace) {
    trace!("{:?} face direction: {}", msg.id, msg.dir);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
    }
}

// Handle player shooting - spawn projectile(s) on client.
pub fn handle_player_shot_message(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    msg: SShot,
    collision_world: Option<&CollisionWorld>,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.0.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.face_dir));

        // Spawn projectile(s) based on player's multi-shot power-up status
        if let Ok((position, _, _)) = player_data.get(player.entity)
            && let Some(collision_world) = collision_world
        {
            spawn_projectiles(
                commands,
                projectile_assets,
                position,
                msg.face_dir,
                msg.face_pitch,
                player.multi_shot_power_up,
                collision_world,
                msg.id,
            );
        }
    }
}

// Handle player being hit - apply camera shake or cuboid shake.
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

// Handle a player's death + respawn: teleport their entity to the new position
// and zero out vertical velocity so they don't immediately resume falling.
pub fn handle_player_death_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    my_player_id: PlayerId,
    msg: SDeath,
) {
    if msg.id == my_player_id {
        info!("you died, respawning at {:?}", msg.respawn_pos);
    } else {
        debug!("{:?} died, respawning at {:?}", msg.id, msg.respawn_pos);
    }
    if let Some(player) = players.0.get(&msg.id) {
        commands
            .entity(player.entity)
            .insert((msg.respawn_pos, PlayerVerticalMotion::default()));
    }
}

// Handle player status update (power-ups, stun).
pub fn handle_player_status_message(
    commands: &mut Commands,
    players: &mut ResMut<PlayerMap>,
    msg: SPlayerStatus,
    my_player_id: PlayerId,
    asset_server: &AssetServer,
    asset_set: &AssetSet,
) {
    if let Some(player_info) = players.0.get_mut(&msg.id) {
        // Play power-up sound effect only for the local player
        if msg.id == my_player_id {
            // Don't play power-up sound effect if this message is due to a stun change
            if player_info.stunned == msg.stunned {
                // Only play power-up sound effect if it wasn't a downgrade
                let lost_power_up = player_info.speed_power_up && !msg.speed_power_up
                    || player_info.multi_shot_power_up && !msg.multi_shot_power_up
                    || player_info.phasing_power_up && !msg.phasing_power_up;

                if !lost_power_up {
                    commands.spawn((
                        AudioPlayer::new(asset_server.load(asset_set.sound("player_power_up").to_owned())),
                        PlaybackSettings::DESPAWN,
                    ));
                }
            }
        }

        player_info.speed_power_up = msg.speed_power_up;
        player_info.multi_shot_power_up = msg.multi_shot_power_up;
        player_info.phasing_power_up = msg.phasing_power_up;
        player_info.stunned = msg.stunned;
    }
}

// ============================================================================
// Player Synchronization Helper
// ============================================================================

// Synchronize players from bulk Update message - spawn/despawn/reconcile.
pub fn sync_players(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    images: &mut ResMut<Assets<Image>>,
    graphs: &mut ResMut<Assets<AnimationGraph>>,
    players: &mut ResMut<PlayerMap>,
    rtt: &ResMut<RoundTripTime>,
    player_data: &Query<(&Position, &MoveInput, &FaceDirection), With<PlayerMarker>>,
    camera_query: &Query<Entity, (With<Camera3d>, With<MainCameraMarker>)>,
    my_player_id: PlayerId,
    asset_server: &Res<AssetServer>,
    asset_set: &AssetSet,
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
        let entity = spawn_player(
            commands,
            asset_server,
            meshes,
            materials,
            images,
            graphs,
            asset_set,
            id.0,
            &player.name,
            &player.movement.pos,
            player.movement.move_input,
            player.face_dir,
            is_local,
        );
        commands.entity(entity).insert(PlayerVerticalMotion {
            vertical_velocity: player.movement.vertical_velocity,
        });

        if is_local && let Ok(camera_entity) = camera_query.single() {
            let camera_rotation = player.face_dir + std::f32::consts::PI;
            commands.entity(camera_entity).insert(
                Transform::from_xyz(player.movement.pos.x, 2.5, player.movement.pos.z + 3.0)
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
                stunned: player.stunned,
            },
        );
    }

    // Despawn players no longer present in the authoritative snapshot
    players.0.retain(|id, player| {
        if update_ids.contains(id) {
            true
        } else {
            commands.entity(player.entity).despawn();
            false
        }
    });

    // Update existing players with server state
    for (id, server_player) in server_players {
        if let Some(client_player) = players.0.get_mut(id) {
            if let Ok((client_pos, _, _)) = player_data.get(client_player.entity) {
                let server_velocity = movement_velocity(server_player.movement, server_player.speed_power_up);

                // The local player's input is always authoritative locally; don't overwrite
                // it from server updates.
                if *id != my_player_id {
                    commands
                        .entity(client_player.entity)
                        .insert(server_player.movement.move_input);
                }
                commands.entity(client_player.entity).insert(ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: server_player.movement.pos,
                    server_velocity,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                });
                if *id != my_player_id {
                    commands.entity(client_player.entity).insert(PlayerVerticalMotion {
                        vertical_velocity: server_player.movement.vertical_velocity,
                    });
                }
            }

            client_player.hits = server_player.hits;
            client_player.speed_power_up = server_player.speed_power_up;
            client_player.multi_shot_power_up = server_player.multi_shot_power_up;
            client_player.phasing_power_up = server_player.phasing_power_up;
        }
    }
}
