use bevy::prelude::*;

use super::{
    admin::{AdminContext, handle_admin_message},
    broadcast::broadcast_to_others,
    incoming::{CharacterQueries, PlayerStateQuery, SharedWorld},
};
use crate::{
    actors::ActorMap,
    map::OpenBarrierKinds,
    missiles::{MissileMap, handle_missile_shot_message},
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
};
use common::{
    config::GameplayConfig,
    physics::{
        CharacterVerticalVelocity, CollisionWorld, ProjectileMarker, ProjectileMotion, calculate_projectile_spawns,
        try_start_player_jump,
    },
    protocol::*,
};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Dispatch messages from players who are already logged in to appropriate handlers.
pub fn dispatch_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut PlayerMap,
    missiles: &mut MissileMap,
    time: &Res<Time>,
    world: &SharedWorld,
    queries: &CharacterQueries,
    actors: &ActorMap,
    open_barrier_kinds: &OpenBarrierKinds,
    admin: &mut AdminContext,
) {
    // Dead players have a despawned entity; queueing entity-targeted
    // commands against the stale `entity` would panic when Bevy applies the
    // command buffer. Drop their in-flight gameplay messages but keep pings
    // (RTT measurement through the respawn window) and admin commands (a
    // dead admin's console must still work — neither touches the entity).
    if players.get(&id).is_some_and(|info| info.is_dead())
        && !matches!(msg, ClientMessage::Ping(_) | ClientMessage::Admin(_))
    {
        return;
    }

    match msg {
        ClientMessage::Login(_) => {
            warn!("{:?} sent login after already authenticated", id);
            if let Some(player) = players.get(&id) {
                // Terminate the connection to enforce a single-login flow
                let _ = player.channel.send(ServerToClient::Close);
            }
        }
        ClientMessage::PlayerMoveIntent(msg) => {
            trace!("{:?} move intent: {:?}", id, msg);
            handle_move_intent_message(commands, entity, id, msg, &*players, queries);
        }
        ClientMessage::Jump(msg) => {
            trace!("{:?} jump: {:?}", id, msg);
            handle_jump_message(
                commands,
                entity,
                id,
                &*players,
                queries,
                &world.collision_world,
                &world.gameplay_config,
            );
        }
        ClientMessage::Face(msg) => {
            trace!("{:?} face direction: {}", id, msg.dir);
            handle_face_message(commands, entity, id, msg, &*players);
        }
        ClientMessage::Shot(msg) => {
            debug!("{id:?} shot");
            handle_shot_message(
                commands,
                entity,
                id,
                msg,
                players,
                time,
                &queries.player_data,
                &world.collision_world,
                &world.gameplay_config,
                open_barrier_kinds,
            );
        }
        ClientMessage::MissileShot(msg) => {
            debug!("{id:?} missile shot at {:?}", msg.target);
            handle_missile_shot_message(
                commands,
                entity,
                id,
                &msg,
                players,
                missiles,
                &queries.player_data,
                actors,
                &queries.actor_data,
                &world.collision_world,
                &world.gameplay_config,
                &world.server_gameplay_config,
                open_barrier_kinds,
            );
        }
        ClientMessage::Ping(msg) => {
            handle_ping_message(id, msg, players);
        }
        ClientMessage::Admin(msg) => {
            debug!("{id:?} admin command: {:?}", msg.command);
            handle_admin_message(
                commands,
                players,
                id,
                admin,
                &queries.player_data,
                &world.gameplay_config,
                &msg,
            );
        }
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

// Handle move-input message.
fn handle_move_intent_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CPlayerMoveIntent,
    players: &PlayerMap,
    queries: &CharacterQueries,
) {
    // Untrusted boundary: drop a move intent carrying a non-finite direction
    // before it reaches movement trig and corrupts the authoritative position.
    if !msg.move_intent.is_finite() {
        return;
    }

    commands.entity(entity).insert(msg.move_intent);

    // Get current movement state for reconciliation.
    if let (Ok((pos, _, _, _)), Ok(motion)) = (queries.player_data.get(entity), queries.player_motions.get(entity)) {
        // Broadcast move-input update with position to all other logged-in players
        broadcast_to_others(
            players,
            id,
            ServerMessage::PlayerMoveIntent(SPlayerMoveIntent {
                id,
                movement: PlayerMovementState::new(*pos, msg.move_intent, motion.0),
            }),
        );
    }
}

fn handle_jump_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    players: &PlayerMap,
    queries: &CharacterQueries,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
) {
    if players.get(&id).is_some_and(PlayerInfo::is_stunned) {
        return;
    }

    let Ok((pos, move_intent, _, _)) = queries.player_data.get(entity) else {
        return;
    };
    let Ok(motion) = queries.player_motions.get(entity) else {
        return;
    };

    let mut next_vertical_velocity = motion.0;
    if !try_start_player_jump(
        &mut next_vertical_velocity,
        collision_world,
        gameplay_config.player.physics(),
        pos,
        pos.x,
        pos.z,
    ) {
        return;
    }

    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(next_vertical_velocity));
    broadcast_to_others(
        players,
        id,
        ServerMessage::Jump(SJump {
            id,
            movement: PlayerMovementState::new(*pos, *move_intent, next_vertical_velocity),
        }),
    );
}

// Handle face direction message.
fn handle_face_message(commands: &mut Commands, entity: Entity, id: PlayerId, msg: CFace, players: &PlayerMap) {
    if !msg.dir.is_finite() {
        return;
    }

    // Update the player's face direction
    commands.entity(entity).insert(FaceDirection(msg.dir));

    broadcast_to_others(players, id, ServerMessage::Face(SFace { id, dir: msg.dir }));
}

// Handle shot message.
fn handle_shot_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CShot,
    players: &mut PlayerMap,
    time: &Res<Time>,
    player_data: &PlayerStateQuery,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    open_barrier_kinds: &OpenBarrierKinds,
) {
    // Reject non-finite aim before it reaches projectile trig / authoritative
    // hit detection. Checked ahead of `try_start_shot` so a bad shot doesn't
    // burn the fire cooldown.
    if !(msg.face_dir.is_finite() && msg.face_pitch.is_finite()) {
        return;
    }

    let now = time.elapsed_secs();

    let Some(has_multi_shot) = players.get_mut(&id).and_then(|info| info.try_start_shot(now)) else {
        return;
    };

    commands.entity(entity).insert(FaceDirection(msg.face_dir));

    // Spawn projectile(s) on server for hit detection
    if let Ok((pos, _, _, _)) = player_data.get(entity) {
        let spawns = calculate_projectile_spawns(
            pos,
            msg.face_dir,
            msg.face_pitch,
            has_multi_shot,
            gameplay_config.player.eye_height(),
            collision_world,
            &open_barrier_kinds.0,
        );

        // Spawn each projectile
        for spawn_info in spawns {
            let proj_motion = ProjectileMotion::new(spawn_info.direction_yaw, spawn_info.direction_pitch);

            commands.spawn((
                ProjectileMarker,
                id, // Tag projectile with shooter's ID
                spawn_info.position,
                proj_motion,
            ));
        }
    }

    // Broadcast shot with face direction to all other logged-in players
    broadcast_to_others(
        players,
        id,
        ServerMessage::Shot(SShot {
            id,
            face_dir: msg.face_dir,
            face_pitch: msg.face_pitch,
        }),
    );
}

// Handle ping message — echo the timestamp back as a pong.
fn handle_ping_message(id: PlayerId, msg: CPing, players: &PlayerMap) {
    trace!("{:?} ping: {:?}", id, msg);
    if let Some(player_info) = players.get(&id) {
        let pong_msg = ServerMessage::Pong(SPong {
            timestamp_nanos: msg.timestamp_nanos,
        });
        let _ = player_info.channel.send(ServerToClient::Send(pong_msg));
    }
}
