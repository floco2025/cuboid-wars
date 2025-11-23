#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

// ============================================================================
// Client Messages
// ============================================================================

/// Client to Server: Login request.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct CLogin {
    pub name: String, // The login name
}

/// Client to Server: Graceful disconnect notification.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct CLogoff {}

/// Client to Server: Name change request.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct CName {
    pub name: String, // The new name
}

/// Client to Server: Chat message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct CSay {
    pub text: String, // The chat message
}

/// Client to Server: Remove a participant.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct CRemove {
    pub id: u32, // The id of the participant to remove
}

// ============================================================================
// Server Messages
// ============================================================================

/// Server to Client: Error message.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SError {
    pub message: String, // The error message to display
}

/// Server to Client: Initial server state after login.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SInit {
    pub id: u32,                    // The id that the server uses for the client
    pub logins: Vec<(u32, String)>, // All other participant ids and their names
}

/// Server to Client: Another participant connected.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SLogin {
    pub id: u32,      // The id for the new participant
    pub name: String, // The name of the new participant
}

/// Server to Client: A participant disconnected.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SLogoff {
    pub id: u32,        // The id of the participant who disconnected
    pub graceful: bool, // Graceful disconnect if true
}

/// Server to Client: Chat message from a participant.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SSay {
    pub id: u32,      // The id of the participant who sends the chat message
    pub text: String, // The chat message
}

/// Server to Client: Participant name change.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SName {
    pub id: u32,      // The id of the participant who changed their name
    pub name: String, // The new name
}

/// Server to Client: A participant was removed.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub struct SRemove {
    pub id: u32, // The id of the participant who was removed
}

// ============================================================================
// Message Envelope
// ============================================================================

/// Wrapper for all messages sent between client and server
#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ClientMessage {
    Login(CLogin),
    Logoff(CLogoff),
    Name(CName),
    Say(CSay),
    Remove(CRemove),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bincode", derive(Encode, Decode))]
pub enum ServerMessage {
    Error(SError),
    Init(SInit),
    Login(SLogin),
    Logoff(SLogoff),
    Say(SSay),
    Name(SName),
    Remove(SRemove),
}
