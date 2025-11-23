use bevy::prelude::*;
use common::protocol::ServerMessage;
use std::collections::HashMap;

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
pub struct GameState {
    players: HashMap<u32, Player>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            players: HashMap::new(),
        }
    }

    fn add(&mut self, id: u32, player: Player) {
        self.players.insert(id, player);
    }

    fn remove(&mut self, id: u32) {
        self.players.remove(&id);
    }

    fn get_name(&self, id: u32) -> String {
        self.players
            .get(&id)
            .expect("player not found")
            .name
            .clone()
    }

    fn set_name(&mut self, id: u32, new_name: String) {
        let client = self.players.get_mut(&id).expect("player not found");
        client.name = new_name;
    }

    #[must_use]
    pub fn get_all_names(&self) -> Vec<String> {
        self.players.values().map(|p| p.name.clone()).collect()
    }

    pub fn process_message(&mut self, msg: ServerMessage) -> String {
        match msg {
            ServerMessage::Error(msg) => format!("[Server Error] {}", msg.message),
            ServerMessage::Init(msg) => {
                let my_id = msg.id;
                let mut lines = Vec::new();
                for (id, name) in msg.logins {
                    if id != my_id {
                        lines.push(format!("{name} is here."));
                    }
                    self.add(id, Player::new(name));
                }
                lines.push("[Connected] You are now logged in.".to_string());
                lines.join("\n")
            }
            ServerMessage::Login(msg) => {
                let line = format!("{} joined.", msg.name);
                self.add(msg.id, Player::new(msg.name));
                line
            }
            ServerMessage::Logoff(msg) => {
                let name = self.get_name(msg.id);
                let line = if msg.graceful {
                    format!("{name} left.")
                } else {
                    format!("{name} disappeared.")
                };
                self.remove(msg.id);
                line
            }
            ServerMessage::Remove(msg) => {
                let name = self.get_name(msg.id);
                self.remove(msg.id);
                format!("{name} was removed.")
            }
            ServerMessage::Say(msg) => {
                let name = self.get_name(msg.id);
                format!("{}: {}", name, msg.text)
            }
            ServerMessage::Name(msg) => {
                let old_name = self.get_name(msg.id);
                let line = format!("{} is now known as \"{}\".", old_name, msg.name);
                self.set_name(msg.id, msg.name);
                line
            }
        }
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
