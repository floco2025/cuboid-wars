use bevy::prelude::*;
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

const MAX_CHAT_HISTORY: usize = 100;

// ============================================================================
// Game Client State
// ============================================================================

#[derive(Debug, Resource)]
pub struct ClientState {
    players: HashMap<u32, Player>,
    chat_history: Vec<String>,
}

impl ClientState {
    // ============================================================================
    // Constructor
    // ============================================================================

    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
            chat_history: Vec::new(),
        }
    }

    // ============================================================================
    // Public API
    // ============================================================================

    #[must_use]
    pub fn get_all_names(&self) -> Vec<String> {
        self.players.values().map(|p| p.name.clone()).collect()
    }

    #[must_use]
    pub fn chat_history(&self) -> &[String] {
        &self.chat_history
    }

    pub fn process_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Error(msg) => {
                self.add_chat_message(format!("[Server Error] {}", msg.message));
            }
            ServerMessage::Init(msg) => {
                let my_id = msg.id;
                let mut lines = Vec::new();
                for (id, player) in msg.players {
                    if id != my_id {
                        lines.push(format!("{} is here.", player.name));
                    }
                    self.add_player(id, player);
                }
                lines.push("[Connected] You are now logged in.".to_string());
                self.add_chat_message(lines.join("\n"));
            }
            ServerMessage::Login(msg) => {
                let name = msg.player.name.clone();
                self.add_player(msg.id, msg.player);
                self.add_chat_message(format!("{} joined.", name));
            }
            ServerMessage::Logoff(msg) => {
                let player = self.remove_player(msg.id);
                let line = if msg.graceful {
                    format!("{} left.", player.name)
                } else {
                    format!("{} disappeared.", player.name)
                };
                self.add_chat_message(line);
            }
            ServerMessage::Remove(msg) => {
                let name = self.get_player_name(msg.id);
                self.remove_player(msg.id);
                self.add_chat_message(format!("{name} was removed."));
            }
            ServerMessage::Say(msg) => {
                let name = self.get_player_name(msg.id);
                self.add_chat_message(format!("{}: {}", name, msg.text));
            }
            ServerMessage::Name(msg) => {
                let old_name = self.set_player_name(msg.id, msg.name.clone());
                self.add_chat_message(format!("{} is now known as \"{}\".", old_name, msg.name));
            }
        }
    }

    // ============================================================================
    // Private Helpers
    // ============================================================================

    fn add_player(&mut self, id: u32, player: Player) {
        self.players.insert(id, player);
    }

    fn remove_player(&mut self, id: u32) -> Player {
        self.players.remove(&id).expect("player not found")
    }

    fn get_player_name(&self, id: u32) -> String {
        self.players.get(&id).expect("player not found").name.clone()
    }

    fn set_player_name(&mut self, id: u32, new_name: String) -> String {
        let player = self.players.get_mut(&id).expect("player not found");
        std::mem::replace(&mut player.name, new_name)
    }

    fn add_chat_message(&mut self, message: String) {
        self.chat_history.push(message);
        if self.chat_history.len() > MAX_CHAT_HISTORY {
            self.chat_history.remove(0);
        }
    }
}
