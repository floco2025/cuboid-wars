use crate::net::{ClientToServer, ServerToClient, per_client_network_io_task};
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::Incoming;
use rand::Rng;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, error, instrument, warn};

// ============================================================================
// Constants
// ============================================================================

const MIN_NAME_LENGTH: usize = 2;
const MAX_NAME_LENGTH: usize = 50;

// ============================================================================
// Game Server
// ============================================================================

#[derive(Debug, Clone, Copy)]
struct ConnectedClient;

#[derive(Debug)]
struct LoggedInClient {
    player: Player,
}

impl LoggedInClient {
    const fn new(_connected: ConnectedClient, player: Player) -> Self {
        Self { player }
    }
}

#[derive(Debug)]
enum ClientState {
    Connected(ConnectedClient),
    LoggedIn(LoggedInClient),
}

#[derive(Debug)]
struct Client {
    to_client: UnboundedSender<ServerToClient>,
    state: ClientState,
}

/// Game server that manages connected clients.
///
/// The server has business logic only with no I/O. I/O is handled by per-client tasks.
#[derive(Debug)]
pub struct GameServer {
    /// Map of client ID to client state
    clients: HashMap<u32, Client>,
    /// Counter for generating unique client IDs
    next_id: u32,
}

impl GameServer {
    // ============================================================================
    // Constructor
    // ============================================================================

    pub fn new() -> Self {
        Self {
            clients: HashMap::new(),
            next_id: 1,
        }
    }

    // ============================================================================
    // Helper functions
    // ============================================================================

    fn advance_id(&mut self) {
        loop {
            self.next_id = self.next_id.checked_add(1).unwrap_or(1);
            if !self.clients.contains_key(&self.next_id) {
                break;
            }
        }
    }

    fn add_client(&mut self, tx: UnboundedSender<ServerToClient>) -> u32 {
        let id = self.next_id;
        self.advance_id();
        self.clients.insert(
            id,
            Client {
                to_client: tx,
                state: ClientState::Connected(ConnectedClient),
            },
        );
        id
    }

    fn has_client(&self, id: u32) -> bool {
        self.clients.contains_key(&id)
    }

    fn remove_client(&mut self, id: u32) {
        let client = self
            .clients
            .remove(&id)
            .expect("remove_client called on non-existent client");
        // Ignore errors because the client task may already have terminated
        let _ = client.to_client.send(ServerToClient::Close);
    }

    fn login(&mut self, id: u32, name: String) -> Player {
        let mut client = self.clients.remove(&id).expect("login called on non-existent client");
        let ClientState::Connected(connected) = client.state else {
            panic!("login called on already logged-in client");
        };
        // Create new Player
        let mut rng = rand::thread_rng();
        let player = Player {
            name,
            pos: Position {
                x: rng.gen_range(-1000..=1000),
                y: rng.gen_range(-1000..=1000),
            },
        };
        // Consume the ConnectedClient to create LoggedInClient
        let logged_in = LoggedInClient::new(connected, player.clone());
        client.state = ClientState::LoggedIn(logged_in);
        self.clients.insert(id, client);
        player
    }

    fn is_logged_in(&self, id: u32) -> bool {
        let client = self
            .clients
            .get(&id)
            .expect("is_logged_in called on non-existent client");
        !matches!(client.state, ClientState::Connected(_))
    }

    fn get_players(&self) -> Vec<(u32, Player)> {
        self.clients
            .iter()
            .filter_map(|(id, client)| match &client.state {
                ClientState::LoggedIn(c) => Some((*id, c.player.clone())),
                ClientState::Connected(_) => None,
            })
            .collect()
    }

    fn get_logged_in_name(&self, id: u32) -> Option<String> {
        let client = self
            .clients
            .get(&id)
            .expect("get_logged_in_name called on non-existent client");
        match &client.state {
            ClientState::LoggedIn(c) => Some(c.player.name.clone()),
            ClientState::Connected(_) => None,
        }
    }

    fn change_name(&mut self, id: u32, name: String) {
        let client = self
            .clients
            .get_mut(&id)
            .expect("change_name called on non-existent client");
        let ClientState::LoggedIn(logged_in) = &mut client.state else {
            panic!("change_name called on non-logged-in client");
        };
        logged_in.player.name = name;
    }

    #[instrument(skip(self, msg))]
    fn send_to(&self, id: u32, msg: &ServerMessage) {
        let client = self.clients.get(&id).expect("send_to called on non-existent client");
        if let Err(e) = client.to_client.send(ServerToClient::Send(msg.clone())) {
            debug!("failed to send to client: {}", e);
        }
    }

    fn send_to_all(&self, msg: &ServerMessage) {
        for client in self.clients.values() {
            if matches!(client.state, ClientState::LoggedIn(_))
                && let Err(e) = client.to_client.send(ServerToClient::Send(msg.clone()))
            {
                debug!("failed to send to client: {}", e);
            }
        }
    }

    // ============================================================================
    // Handle new client connections
    // ============================================================================

    #[instrument(skip(self, to_server, incoming))]
    pub async fn accept_client(&mut self, to_server: UnboundedSender<(u32, ClientToServer)>, incoming: Incoming) {
        // Await connection establishment
        let connection = match incoming.await {
            Ok(conn) => conn,
            Err(e) => {
                error!("failed to accept connection: {}", e);
                return;
            }
        };

        // Channel for sending from the server to a new client network IO task
        let (to_client, from_server) = unbounded_channel();

        // Add client to server
        let id = self.add_client(to_client);
        debug!(id, "accepted new client");

        // Spawn I/O task for this client
        let connection_clone = connection;
        tokio::spawn(async move {
            per_client_network_io_task(id, connection_clone, to_server, from_server).await;
        });
    }

    // ============================================================================
    // Handle client disconnects
    // ============================================================================

    pub fn disconnect_client(&mut self, id: u32) {
        // Don't do anything if the client has already been removed.
        if !self.has_client(id) {
            return;
        }

        let was_logged_in = self.is_logged_in(id);
        self.clients.remove(&id);

        // If the client was logged in, send non-graceful logoff messages to all other clients.
        if was_logged_in {
            self.send_to_all(&ServerMessage::Logoff(SLogoff { id, graceful: false }));
        }
    }

    // ============================================================================
    // Process messages from clients
    // ============================================================================

    pub fn process_client_data(&mut self, id: u32, msg: ClientMessage) {
        // Route to appropriate handler based on login status
        if self.is_logged_in(id) {
            self.process_client_message(id, msg);
        } else {
            self.process_client_login(id, msg);
        }
    }

    #[instrument(skip(self))]
    fn process_client_login(&mut self, id: u32, msg: ClientMessage) {
        if let ClientMessage::Login(login_msg) = msg {
            if let Err(err) = validate_name(&login_msg.name) {
                self.send_to(
                    id,
                    &ServerMessage::Error(SError {
                        message: format!("Login failed: {}", name_error_message(err)),
                    }),
                );
                self.remove_client(id);
                return;
            }

            let players = self.get_players();
            self.send_to(id, &ServerMessage::Init(SInit { id, players }));

            let player = self.login(id, login_msg.name.clone());
            self.send_to_all(&ServerMessage::Login(SLogin { id, player }));
        } else {
            // Protocol violation
            warn!("protocol violation: client sent non-login message before authenticating");
            self.remove_client(id);
        }
    }

    #[instrument(skip(self))]
    fn process_client_message(&mut self, id: u32, msg: ClientMessage) {
        match msg {
            ClientMessage::Login(_) => {
                // Protocol violation: already logged in
                warn!("protocol violation: client sent Login after already authenticated");
                self.remove_client(id);
            }
            ClientMessage::Logoff(_) => {
                // Client requested graceful disconnect, so we send graceful
                self.remove_client(id);

                // Send graceful logoff message to all other clients
                self.send_to_all(&ServerMessage::Logoff(SLogoff { id, graceful: true }));
            }
            ClientMessage::Say(say_msg) => {
                self.send_to_all(&ServerMessage::Say(SSay { id, text: say_msg.text }));
            }
            ClientMessage::Name(name_msg) => {
                if let Err(err) = validate_name(&name_msg.name) {
                    self.send_to(
                        id,
                        &ServerMessage::Error(SError {
                            message: format!("Name change failed: {}", name_error_message(err)),
                        }),
                    );
                    return;
                }

                self.change_name(id, name_msg.name.clone());

                self.send_to_all(&ServerMessage::Name(SName {
                    id,
                    name: name_msg.name,
                }));
            }
            ClientMessage::Remove(remove_msg) => {
                if self.get_logged_in_name(remove_msg.id).is_some() {
                    self.remove_client(remove_msg.id);
                    self.send_to_all(&ServerMessage::Remove(SRemove { id: remove_msg.id }));
                } else {
                    self.send_to(
                        id,
                        &ServerMessage::Error(SError {
                            message: format!("Cannot remove {}: not found", remove_msg.id),
                        }),
                    );
                }
            }
        }
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

#[derive(Clone, Copy)]
enum NameValidationError {
    TooShort,
    TooLong,
}

const fn validate_name(name: &str) -> Result<(), NameValidationError> {
    match name.len() {
        len if len < MIN_NAME_LENGTH => Err(NameValidationError::TooShort),
        len if len <= MAX_NAME_LENGTH => Ok(()),
        _ => Err(NameValidationError::TooLong),
    }
}

fn name_error_message(error: NameValidationError) -> String {
    match error {
        NameValidationError::TooShort => {
            format!("Name must be at least {MIN_NAME_LENGTH} characters")
        }
        NameValidationError::TooLong => {
            format!("Name too long (max {MAX_NAME_LENGTH} characters)")
        }
    }
}
