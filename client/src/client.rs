use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use std::collections::HashMap;

// ============================================================================
// Game Client State
// ============================================================================

#[derive(Debug, Resource)]
pub struct ClientState {
    my_id: Option<PlayerId>,
    players: HashMap<PlayerId, Player>,
}

impl ClientState {
    // ============================================================================
    // Constructor
    // ============================================================================

    pub fn new() -> Self {
        Self {
            my_id: None,
            players: HashMap::new(),
        }
    }

    // ============================================================================
    // Public API
    // ============================================================================

    #[must_use]
    pub fn my_id(&self) -> Option<PlayerId> {
        self.my_id
    }

    #[must_use]
    pub fn players(&self) -> &HashMap<PlayerId, Player> {
        &self.players
    }

    pub fn process_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Init(msg) => {
                self.my_id = Some(msg.id);
                for (id, player) in msg.players {
                    self.add_player(id, player);
                }
            }
            ServerMessage::Login(msg) => {
                self.add_player(msg.id, msg.player);
            }
            ServerMessage::Logoff(msg) => {
                self.players.remove(&msg.id);
            }
        }
    }

    // ============================================================================
    // Private Helpers
    // ============================================================================

    fn add_player(&mut self, id: PlayerId, player: Player) {
        self.players.insert(id, player);
    }
}

