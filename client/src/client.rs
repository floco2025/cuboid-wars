use bevy::prelude::*;
use common::protocol::ServerMessage;
use std::collections::HashMap;

// ============================================================================
// Constants
// ============================================================================

const MAX_CHAT_HISTORY: usize = 100;

// ============================================================================
// Game Client State
// ============================================================================

#[derive(Debug)]
struct Player {
    name: String,
}

impl Player {
    const fn new(name: String) -> Self {
        Self { name }
    }
}

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
                for (id, name) in msg.logins {
                    if id != my_id {
                        lines.push(format!("{name} is here."));
                    }
                    self.add_player(id, Player::new(name));
                }
                lines.push("[Connected] You are now logged in.".to_string());
                self.add_chat_message(lines.join("\n"));
            }
            ServerMessage::Login(msg) => {
                let line = format!("{} joined.", msg.name);
                self.add_player(msg.id, Player::new(msg.name));
                self.add_chat_message(line);
            }
            ServerMessage::Logoff(msg) => {
                let name = self.get_player_name(msg.id);
                let line = if msg.graceful {
                    format!("{name} left.")
                } else {
                    format!("{name} disappeared.")
                };
                self.remove_player(msg.id);
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
                let old_name = self.get_player_name(msg.id);
                let line = format!("{} is now known as \"{}\".", old_name, msg.name);
                self.set_player_name(msg.id, msg.name);
                self.add_chat_message(line);
            }
        }
    }

    // ============================================================================
    // Private Helpers
    // ============================================================================

    fn add_player(&mut self, id: u32, player: Player) {
        self.players.insert(id, player);
    }

    fn remove_player(&mut self, id: u32) {
        self.players.remove(&id);
    }

    fn get_player_name(&self, id: u32) -> String {
        self.players
            .get(&id)
            .expect("player not found")
            .name
            .clone()
    }

    fn set_player_name(&mut self, id: u32, new_name: String) {
        let player = self.players.get_mut(&id).expect("player not found");
        player.name = new_name;
    }

    fn add_chat_message(&mut self, message: String) {
        self.chat_history.push(message);
        if self.chat_history.len() > MAX_CHAT_HISTORY {
            self.chat_history.remove(0);
        }
    }
}