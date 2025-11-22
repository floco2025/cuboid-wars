use common::protocol::ServerMessage;
use std::collections::HashMap;

// ============================================================================
// Chat Client
// ============================================================================

#[derive(Debug)]
struct Client {
    name: String,
}

impl Client {
    const fn new(name: String) -> Self {
        Self { name }
    }
}

#[derive(Debug)]
pub struct ChatClient {
    clients: HashMap<u32, Client>,
}

impl ChatClient {
    // ============================================================================
    // Constructor
    // ============================================================================

    pub(crate) fn new() -> Self {
        Self {
            clients: HashMap::new(),
        }
    }

    // ============================================================================
    // Helper functions
    // ============================================================================

    fn add(&mut self, id: u32, participant: Client) {
        self.clients.insert(id, participant);
    }

    fn remove(&mut self, id: u32) {
        self.clients.remove(&id);
    }

    fn get_name(&self, id: u32) -> String {
        self.clients.get(&id).expect("participant not found").name.clone()
    }

    fn set_name(&mut self, id: u32, new_name: String) {
        let client = self.clients.get_mut(&id).expect("participant not found");
        client.name = new_name;
    }

    // ============================================================================
    // Public functions
    // ============================================================================

    #[must_use]
    pub fn get_all_names(&self) -> Vec<String> {
        self.clients.values().map(|p| p.name.clone()).collect()
    }

    #[must_use]
    pub fn get_id_by_name(&self, name: &str) -> Option<u32> {
        self.clients.iter().find(|(_, p)| p.name == name).map(|(id, _)| *id)
    }

    // ============================================================================
    // Process messages from the server
    // ============================================================================

    pub fn process_message(&mut self, msg: ServerMessage) {
        match msg {
            ServerMessage::Error(msg) => {
                eprintln!("[Server Error] {}", msg.message);
            }
            ServerMessage::Init(msg) => {
                let my_id = msg.id;
                for (id, name) in msg.logins {
                    if id != my_id {
                        println!("{name} is here.");
                    }
                    self.add(id, Client::new(name));
                }
                println!("[Connected] You are now logged in.");
            }
            ServerMessage::Login(msg) => {
                println!("{} joined.", msg.name);
                self.add(msg.id, Client::new(msg.name));
            }
            ServerMessage::Logoff(msg) => {
                let name = self.get_name(msg.id);
                if msg.graceful {
                    println!("{name} left.");
                } else {
                    println!("{name} disappeared.");
                }
                self.remove(msg.id);
            }
            ServerMessage::Remove(msg) => {
                let name = self.get_name(msg.id);
                println!("{name} was removed.");
                self.remove(msg.id);
            }
            ServerMessage::Say(msg) => {
                let name = self.get_name(msg.id);
                println!("{}: {}", name, msg.text);
            }
            ServerMessage::Name(msg) => {
                let old_name = self.get_name(msg.id);
                println!("{} is now known as \"{}\".", old_name, msg.name);
                self.set_name(msg.id, msg.name);
            }
        }
    }
}
