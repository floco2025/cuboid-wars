use serde::{Deserialize, Serialize};

// ============================================================================
// Client Messages
// ============================================================================

/// Client to Server: Login request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CLogin {
    pub name: String, // The login name
}

/// Client to Server: Graceful disconnect notification.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CLogoff {}

/// Client to Server: Name change request.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CName {
    pub name: String, // The new name
}

/// Client to Server: Chat message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CSay {
    pub text: String, // The chat message
}

/// Client to Server: Remove a participant.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CRemove {
    pub id: u32, // The id of the participant to remove
}

// ============================================================================
// Server Messages
// ============================================================================

/// Server to Client: Error message.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SError {
    pub message: String, // The error message to display
}

/// Server to Client: Initial server state after login.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SInit {
    pub id: u32,                    // The id that the server uses for the client
    pub logins: Vec<(u32, String)>, // All other participant ids and their names
}

/// Server to Client: Another participant connected.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SLogin {
    pub id: u32,      // The id for the new participant
    pub name: String, // The name of the new participant
}

/// Server to Client: A participant disconnected.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SLogoff {
    pub id: u32,        // The id of the participant who disconnected
    pub graceful: bool, // Graceful disconnect if true
}

/// Server to Client: Chat message from a participant.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SSay {
    pub id: u32,      // The id of the participant who sends the chat message
    pub text: String, // The chat message
}

/// Server to Client: Participant name change.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SName {
    pub id: u32,      // The id of the participant who changed their name
    pub name: String, // The new name
}

/// Server to Client: A participant was removed.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SRemove {
    pub id: u32, // The id of the participant who was removed
}

// ============================================================================
// Message Envelope
// ============================================================================

/// Wrapper for all messages sent between client and server
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ClientMessage {
    Login(CLogin),
    Logoff(CLogoff),
    Name(CName),
    Say(CSay),
    Remove(CRemove),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ServerMessage {
    Error(SError),
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
    Say(SSay),
    Name(SName),
    Remove(SRemove),
}
