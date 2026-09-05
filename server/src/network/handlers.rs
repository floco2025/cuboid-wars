use bevy::{ecs::system::SystemParam, prelude::*};

use super::feed::{FeedAudience, FeedEvent, emit_feed};
use crate::{
    actors::ActorStateQuery,
    config::{FeedConfig, ServerGameplayConfig},
    map::MapConfig,
    network::ServerToClient,
    players::{PlayerInfo, PlayerMap, PlayerStateQuery},
};
use common::{
    config::GameplayConfig,
    constants::CHAT_MAX_CHARS,
    map::MapGeometry,
    physics::{CharacterVerticalVelocity, CollisionWorld, player_jump_velocity},
    protocol::*,
};

#[derive(SystemParam)]
pub(super) struct SharedWorld<'w> {
    pub(super) map_layout: Res<'w, MapLayout>,
    pub(super) map_settings: Res<'w, MapSettings>,
    pub(super) map_geometry: Res<'w, MapGeometry>,
    pub(super) collision_world: Res<'w, CollisionWorld>,
    pub(super) gameplay_config: Res<'w, GameplayConfig>,
    pub(super) map_config: Res<'w, MapConfig>,
    pub(super) server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub(super) world_bootstrap: Res<'w, WorldBootstrap>,
}

#[derive(SystemParam)]
pub(super) struct CharacterQueries<'w, 's> {
    pub(super) player_data: PlayerStateQuery<'w, 's>,
    pub(super) player_motions: Query<'w, 's, &'static CharacterVerticalVelocity, With<PlayerMarker>>,
    pub(super) actor_data: ActorStateQuery<'w, 's>,
}

pub(super) fn handle_move_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    message: CMove,
    players: &mut PlayerMap,
) {
    // Reject non-finite input before it can corrupt authoritative movement.
    if !message.input.is_finite() {
        return;
    }
    let Some(info) = players.get_mut(&id) else {
        return;
    };
    if !sequence_is_newer(message.seq, info.session.last_move_seq) {
        debug!(
            "ignoring an outdated move from {:?} (seq {}, last {})",
            id, message.seq, info.session.last_move_seq
        );
        return;
    }
    info.session.last_move_seq = message.seq;
    // The intent is expressed on the side of the crossings the client's own
    // simulation has made; until this player has made the same ones, the
    // persisted intent, mapped through each hop, is the right one.
    if message.hops != info.session.hops {
        return;
    }
    let input = message.input;
    commands
        .entity(entity)
        .insert((input.move_intent, FaceYaw(input.face_yaw)));
}

pub(super) fn handle_jump_message(
    commands: &mut Commands,
    entity: Entity,
    id: PlayerId,
    players: &PlayerMap,
    queries: &CharacterQueries,
    collision_world: &CollisionWorld,
    gameplay_config: &GameplayConfig,
    map_settings: &MapSettings,
) {
    if players.get(&id).is_some_and(PlayerInfo::is_stunned) {
        return;
    }

    let Ok((pos, _, _, _)) = queries.player_data.get(entity) else {
        return;
    };
    let Ok(motion) = queries.player_motions.get(entity) else {
        return;
    };

    let Some(next_vertical_velocity) = player_jump_velocity(
        motion.0,
        collision_world,
        gameplay_config.player.physics(),
        map_settings.movement.player.jump_speed,
        pos,
    ) else {
        return;
    };

    commands
        .entity(entity)
        .insert(CharacterVerticalVelocity(next_vertical_velocity));
}

pub(super) fn handle_ping_message(id: PlayerId, message: CPing, players: &PlayerMap) {
    if let Some(player) = players.get(&id) {
        let pong = ServerMessage::Pong(SPong {
            timestamp_nanos: message.timestamp_nanos,
        });
        let _ = player.connection.channel.send(ServerToClient::Send(pong));
    }
}

pub(super) fn handle_chat_message(id: PlayerId, message: &CChat, players: &PlayerMap, feed: &FeedConfig) {
    let Some(text) = sanitize_chat_text(&message.text) else {
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

// Chat is broadcast-amplified, so keep malformed input bounded and single-line.
fn sanitize_chat_text(raw: &str) -> Option<String> {
    let sanitized: String = raw.chars().filter(|c| !c.is_control()).take(CHAT_MAX_CHARS).collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_owned())
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
            sanitize_chat_text(&long)
                .expect("long chat missing after truncation")
                .len(),
            CHAT_MAX_CHARS
        );
    }
}
