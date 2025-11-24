use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use std::collections::HashMap;

// ============================================================================
// Game Client State
// ============================================================================

#[derive(Debug, Resource)]
pub struct ClientState {
    my_id: Option<u32>,
    players: HashMap<u32, Player>,
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
    pub fn my_id(&self) -> Option<u32> {
        self.my_id
    }

    #[must_use]
    pub fn players(&self) -> &HashMap<u32, Player> {
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
            ServerMessage::Logoff(_msg) => {
                // Player left - will be handled by sync system
            }
        }
    }

    // ============================================================================
    // Private Helpers
    // ============================================================================

    fn add_player(&mut self, id: u32, player: Player) {
        self.players.insert(id, player);
    }
}

