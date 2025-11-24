use crate::net::{ClientToServer, ServerToClient, per_client_network_io_task};
#[allow(clippy::wildcard_imports)]
use common::protocol::*;
use quinn::Incoming;
use rand::Rng;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tracing::{debug, error, instrument, warn};

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
    clients: HashMap<PlayerId, Client>,
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
            if !self.clients.contains_key(&PlayerId(self.next_id)) {
                break;
            }
        }
    }

    fn add_client(&mut self, tx: UnboundedSender<ServerToClient>) -> PlayerId {
        let id = PlayerId(self.next_id);
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

    fn has_client(&self, id: PlayerId) -> bool {
        self.clients.contains_key(&id)
    }

    fn remove_client(&mut self, id: PlayerId) {
        let client = self
            .clients
            .remove(&id)
            .expect("remove_client called on non-existent client");
        // Ignore errors because the client task may already have terminated
        let _ = client.to_client.send(ServerToClient::Close);
    }

    fn login(&mut self, id: PlayerId) -> Player {
        let mut client = self.clients.remove(&id).expect("login called on non-existent client");
        let ClientState::Connected(connected) = client.state else {
            panic!("login called on already logged-in client");
        };
        // Create new Player with random position
        let mut rng = rand::thread_rng();
        let player = Player {
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

    fn is_logged_in(&self, id: PlayerId) -> bool {
        let client = self
            .clients
            .get(&id)
            .expect("is_logged_in called on non-existent client");
        !matches!(client.state, ClientState::Connected(_))
    }

    fn get_players(&self) -> Vec<(PlayerId, Player)> {
        self.clients
            .iter()
            .filter_map(|(id, client)| match &client.state {
                ClientState::LoggedIn(c) => Some((*id, c.player.clone())),
                ClientState::Connected(_) => None,
            })
            .collect()
    }

    #[instrument(skip(self, msg))]
    fn send_to(&self, id: PlayerId, msg: &ServerMessage) {
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

    fn send_to_all_except(&self, msg: &ServerMessage, exclude_id: PlayerId) {
        for (id, client) in &self.clients {
            if *id != exclude_id
                && matches!(client.state, ClientState::LoggedIn(_))
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
    pub async fn accept_client(&mut self, to_server: UnboundedSender<(PlayerId, ClientToServer)>, incoming: Incoming) {
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
        debug!(id = id.0, "accepted new client");

        // Spawn I/O task for this client
        let connection_clone = connection;
        tokio::spawn(async move {
            per_client_network_io_task(id, connection_clone, to_server, from_server).await;
        });
    }

    // ============================================================================
    // Handle client disconnects
    // ============================================================================

    pub fn disconnect_client(&mut self, id: PlayerId) {
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

    pub fn process_client_data(&mut self, id: PlayerId, msg: ClientMessage) {
        // Route to appropriate handler based on login status
        if self.is_logged_in(id) {
            self.process_client_message(id, msg);
        } else {
            self.process_client_login(id, msg);
        }
    }

    #[instrument(skip(self))]
    fn process_client_login(&mut self, id: PlayerId, msg: ClientMessage) {
        if let ClientMessage::Login(_login_msg) = msg {
            let other_players = self.get_players();
            let player = self.login(id);
            self.send_to(id, &ServerMessage::Init(SInit { id, player: player.clone(), other_players }));
            // Notify other clients (not the one who just logged in)
            self.send_to_all_except(&ServerMessage::Login(SLogin { id, player }), id);
        } else {
            // Protocol violation
            warn!("protocol violation: client sent non-login message before authenticating");
            self.remove_client(id);
        }
    }

    #[instrument(skip(self))]
    fn process_client_message(&mut self, id: PlayerId, msg: ClientMessage) {
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
        }
    }
}
