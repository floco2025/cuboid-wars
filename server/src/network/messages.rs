use bevy::prelude::*;

use super::{
    admin::{AdminContext, handle_admin_message},
    broadcast::broadcast_to_others,
    feed::{FeedAudience, FeedEvent, emit_feed},
    incoming::{CharacterQueries, SharedWorld},
};
use crate::{
    actors::{ActorMap, PendingActorSpawns},
    config::FeedConfig,
    map::OpenBarrierKinds,
    missiles::{MissileMap, handle_missile_shot_message},
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap},
    projectiles::handle_shot_message,
    quests::QuestBoard,
};
use common::{
    config::GameplayConfig,
    constants::CHAT_MAX_CHARS,
    physics::{CharacterVerticalVelocity, CollisionWorld, player_jump_velocity},
    protocol::*,
};

// ============================================================================
// Message Dispatcher
// ============================================================================

// Dispatch messages from players who are already logged in to appropriate handlers.
pub fn dispatch_message(
    commands: &mut Commands,
    id: PlayerId,
    msg: ClientMessage,
    players: &mut PlayerMap,
    missiles: &mut MissileMap,
    time: &Res<Time>,
    world: &SharedWorld,
    queries: &CharacterQueries,
    actors: &ActorMap,
    open_barrier_kinds: &OpenBarrierKinds,
    pending_actor_spawns: &mut PendingActorSpawns,
    admin: &mut AdminContext,
    quest_board: &mut QuestBoard,
) {
    let entity = players.get(&id).and_then(PlayerInfo::entity);
    // Dead players have no entity. Drop their in-flight gameplay messages
    // but keep pings (RTT measurement through the respawn window), admin
    // commands, and chat (a dead player's console must still work).
    if players.get(&id).is_some_and(|info| info.is_dead())
        && !matches!(
            msg,
            ClientMessage::Ping(_) | ClientMessage::Admin(_) | ClientMessage::Chat(_)
        )
    {
        return;
    }

    match msg {
        ClientMessage::Login(_) => {
            warn!("{} sent login after already authenticated", players.describe(&id));
            if let Some(player) = players.get(&id) {
                // Terminate the connection to enforce a single-login flow
                let _ = player.connection.channel.send(ServerToClient::Close);
            }
        }
        ClientMessage::Move(msg) => {
            let Some(entity) = entity else {
                return;
            };
            trace!("{:?} input: {:?}", id, msg);
            handle_move_message(commands, entity, id, msg, &*players, queries);
        }
        ClientMessage::Jump(msg) => {
            let Some(entity) = entity else {
                return;
            };
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
        ClientMessage::Shot(msg) => {
            let Some(entity) = entity else {
                return;
            };
            debug!("{} shot", players.describe(&id));
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
            let Some(entity) = entity else {
                return;
            };
            debug!("{} missile shot at {:?}", players.describe(&id), msg.target);
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
            debug!("{} admin command: {:?}", players.describe(&id), msg.command);
            handle_admin_message(
                commands,
                players,
                actors,
                id,
                admin,
                &queries.player_data,
                &world.gameplay_config,
                &world.map_config,
                pending_actor_spawns,
                quest_board,
                &msg,
            );
        }
        ClientMessage::Chat(msg) => {
            handle_chat_message(id, &msg, players, &world.server_gameplay_config.feed);
        }
    }
}

// Chat lines are broadcast-amplified like player names: strip control
// characters (which also keeps chat single-line), cap the length, and drop
// anything empty.
fn sanitize_chat_text(raw: &str) -> Option<String> {
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).take(CHAT_MAX_CHARS).collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

fn handle_chat_message(id: PlayerId, msg: &CChat, players: &PlayerMap, feed: &FeedConfig) {
    let Some(text) = sanitize_chat_text(&msg.text) else {
        return;
    };
    emit_feed(
        players,
        feed,
        FeedAudience::Everyone,
        FeedEvent::Chat {
            name: players.display_name(&id),
            text,
        },
    );
}

// ============================================================================
// Message Handlers
// ============================================================================

// Handle the merged steady-state input message (movement intent + facing).
fn handle_move_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    msg: CMove,
    players: &PlayerMap,
    queries: &CharacterQueries,
) {
    // Untrusted boundary: drop input carrying non-finite values before they
    // reach movement trig and corrupt the authoritative position.
    if !msg.move_intent.is_finite() || !msg.face_yaw.is_finite() {
        return;
    }

    commands.entity(entity).insert((msg.move_intent, FaceYaw(msg.face_yaw)));

    // Get current movement state for reconciliation.
    if let (Ok((pos, _, _, _)), Ok(motion)) = (queries.player_data.get(entity), queries.player_motions.get(entity)) {
        // Broadcast the input update with position to all other logged-in players
        broadcast_to_others(
            players,
            id,
            ServerMessage::PlayerMove(SPlayerMove {
                id,
                movement: PlayerMovementState::new(*pos, msg.move_intent, motion.0, msg.face_yaw),
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

    let Ok((pos, move_intent, face_yaw, _)) = queries.player_data.get(entity) else {
        return;
    };
    let Ok(motion) = queries.player_motions.get(entity) else {
        return;
    };

    let Some(next_vertical_velocity) = player_jump_velocity(
        motion.0,
        collision_world,
        gameplay_config.player.physics(),
        gameplay_config.player.jump_speed,
        pos,
    ) else {
        return;
    };

    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(next_vertical_velocity));
    broadcast_to_others(
        players,
        id,
        ServerMessage::PlayerJump(SPlayerJump {
            id,
            movement: PlayerMovementState::new(*pos, *move_intent, next_vertical_velocity, face_yaw.0),
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
        let _ = player_info.connection.channel.send(ServerToClient::Send(pong_msg));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_text_is_sanitized() {
        assert_eq!(sanitize_chat_text("hello there"), Some("hello there".to_owned()));
        assert_eq!(sanitize_chat_text("he\nllo\u{7}"), Some("hello".to_owned()));
        assert_eq!(sanitize_chat_text("   \u{1b}  "), None);
        assert_eq!(sanitize_chat_text(""), None);
        let long = "x".repeat(400);
        assert_eq!(
            sanitize_chat_text(&long).expect("long chat survives truncated").len(),
            CHAT_MAX_CHARS
        );
    }
}
