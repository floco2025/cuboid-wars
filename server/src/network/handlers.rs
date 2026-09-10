use bevy::{ecs::system::SystemParam, prelude::*};

use super::feed::{FeedAudience, FeedEvent, emit_feed};
use crate::{
    config::{FeedConfig, ServerGameplayConfig},
    map::MapConfig,
    network::ServerToClient,
    players::{PlayerMap, PlayerStateQuery},
};
use common::{
    config::GameplayConfig, constants::CONSOLE_CHAT_MAX_CHARS, map::Carriers, physics::CollisionWorld, protocol::*,
};

// The world view and the character queries every ingress handler reads from.
// Handlers take the bundles rather than their fields, so a new resource does
// not ripple through every signature.
#[derive(SystemParam)]
pub(crate) struct SharedWorld<'w> {
    pub(crate) tick: Res<'w, ServerTick>,
    pub(crate) collision_world: Res<'w, CollisionWorld>,
    pub(crate) carriers: Res<'w, Carriers>,
    pub(crate) gameplay_config: Res<'w, GameplayConfig>,
    pub(crate) map_config: Res<'w, MapConfig>,
    pub(crate) server_gameplay_config: Res<'w, ServerGameplayConfig>,
    pub(crate) world_bootstrap: Res<'w, WorldBootstrap>,
}

#[derive(SystemParam)]
pub(crate) struct CharacterQueries<'w, 's> {
    pub(crate) player_data: PlayerStateQuery<'w, 's>,
}

pub(super) fn handle_ping_message(id: PlayerId, message: CPing, players: &PlayerMap, tick: ServerTick) {
    if let Some(player) = players.get(&id) {
        let pong = ServerMessage::Pong(SPong {
            tick: tick.0,
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
    let sanitized: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .take(CONSOLE_CHAT_MAX_CHARS)
        .collect();
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
            CONSOLE_CHAT_MAX_CHARS
        );
    }
}
