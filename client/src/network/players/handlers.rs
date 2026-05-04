use bevy::prelude::*;

use crate::{
    cameras::MainCameraMarker,
    config::AssetSet,
    network::{RoundTripTime, ServerReconciliation},
    players::PlayerMap,
    players::{CameraShake, CuboidShake},
    projectiles::{ProjectileAssets, spawn_projectiles},
};
use common::{
    config::GameplayConfig,
    physics::{CharacterVerticalVelocity, CollisionWorld},
    protocol::*,
};

// ============================================================================
// Player Message Handlers
// ============================================================================

pub(super) fn player_movement_velocity(
    movement: PlayerMovementState,
    walk_speed: f32,
    run_speed: f32,
    has_speed_power_up: bool,
) -> Vec3 {
    let mut velocity = movement
        .move_intent
        .to_horizontal_velocity(walk_speed, run_speed, has_speed_power_up);
    velocity.y = movement.vertical_velocity;
    velocity
}

// Handle player move-input update with server reconciliation.
pub fn handle_player_move_intent_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    gameplay_config: &GameplayConfig,
    msg: SPlayerMoveIntent,
) {
    trace!("{:?} move intent: {:?}", msg.id, msg);
    if let Some(player) = players.get(&msg.id) {
        let server_velocity = player_movement_velocity(
            msg.movement,
            gameplay_config.player.walk_speed,
            gameplay_config.player.run_speed,
            player.speed_power_up,
        );

        // Add server reconciliation if we have client position
        if let Ok((client_pos, _, _)) = player_data.get(player.entity) {
            commands.entity(player.entity).insert((
                msg.movement.move_intent, // Never the local player, so we can always overwrite intent
                ServerReconciliation {
                    client_pos: *client_pos,
                    server_pos: msg.movement.pos,
                    server_velocity,
                    timer: 0.0,
                    rtt: rtt.rtt.as_secs_f32(),
                },
            ));
        } else {
            commands.entity(player.entity).insert(msg.movement.move_intent);
        }
    }
}

pub fn handle_player_jump_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    rtt: &ResMut<RoundTripTime>,
    gameplay_config: &GameplayConfig,
    msg: SJump,
) {
    if let Some(player) = players.get(&msg.id)
        && let Ok((client_pos, _, _)) = player_data.get(player.entity)
    {
        let server_velocity = player_movement_velocity(
            msg.movement,
            gameplay_config.player.walk_speed,
            gameplay_config.player.run_speed,
            player.speed_power_up,
        );
        commands.entity(player.entity).insert((
            msg.movement.move_intent,
            CharacterVerticalVelocity(msg.movement.vertical_velocity),
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
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert(FaceDirection(msg.dir));
    }
}

// Handle player shooting - spawn projectile(s) on client.
pub fn handle_player_shot_message(
    commands: &mut Commands,
    projectile_assets: &ProjectileAssets,
    players: &ResMut<PlayerMap>,
    player_data: &Query<(&Position, &PlayerMoveIntent, &FaceDirection), With<PlayerMarker>>,
    msg: SShot,
    collision_world: Option<&CollisionWorld>,
    gameplay_config: &GameplayConfig,
) {
    trace!("{:?} shot: {:?}", msg.id, msg);
    if let Some(player) = players.get(&msg.id) {
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
                gameplay_config.player.eye_height(),
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
    } else if let Some(player) = players.get(&msg.id) {
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

// Handle an authoritative player teleport. This is not reconciliation; discard
// any pending correction and apply the movement state immediately.
pub fn handle_player_teleport_message(
    commands: &mut Commands,
    players: &ResMut<PlayerMap>,
    my_player_id: PlayerId,
    msg: SPlayerTeleport,
) {
    if msg.id == my_player_id {
        info!("you were teleported to {:?}", msg.movement.pos);
    } else {
        debug!("{:?} teleported to {:?}", msg.id, msg.movement.pos);
    }
    if let Some(player) = players.get(&msg.id) {
        commands.entity(player.entity).insert((
            msg.movement.pos,
            msg.movement.move_intent,
            CharacterVerticalVelocity(msg.movement.vertical_velocity),
        ));
        commands.entity(player.entity).remove::<ServerReconciliation>();
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
    if let Some(player_info) = players.get_mut(&msg.id) {
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
                        AudioPlayer::new(asset_server.load(asset_set.player_sound("collect_power_up").to_owned())),
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
