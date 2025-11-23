#[cfg(feature = "json")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "bincode")]
use bincode::{Decode, Encode};

// Macro to reduce boilerplate for structs
macro_rules! message {
    ($(#[$meta:meta])* struct $name:ident $body:tt) => {
        $(#[$meta])*
        #[derive(Debug, Clone)]
        #[cfg_attr(feature = "json", derive(Serialize, Deserialize))]
        #[cfg_attr(feature = "bincode", derive(Encode, Decode))]
        pub struct $name $body
    };
}

// ============================================================================
// Client Messages
// ============================================================================

message! {
/// Client to Server: Login request.
struct CLogin {
    pub name: String, // The login name
}
}

message! {
/// Client to Server: Graceful disconnect notification.
struct CLogoff {}
}

message! {
/// Client to Server: Name change request.
struct CName {
    pub name: String, // The new name
}
}

message! {
/// Client to Server: Chat message.
struct CSay {
    pub text: String, // The chat message
}
}

message! {
/// Client to Server: Remove a participant.
struct CRemove {
    pub id: u32, // The id of the participant to remove
}
}

// ============================================================================
// Server Messages
// ============================================================================

message! {
/// Server to Client: Error message.
struct SError {
    pub message: String, // The error message to display
}
}

message! {
/// Server to Client: Initial server state after login.
struct SInit {
    pub id: u32,                    // The id that the server uses for the client
    pub logins: Vec<(u32, String)>, // All other participant ids and their names
}
}

message! {
/// Server to Client: Another participant connected.
struct SLogin {
    pub id: u32,      // The id for the new participant
    pub name: String, // The name of the new participant
}
}

message! {
/// Server to Client: A participant disconnected.
struct SLogoff {
    pub id: u32,        // The id of the participant who disconnected
    pub graceful: bool, // Graceful disconnect if true
}
}

message! {
/// Server to Client: Chat message from a participant.
struct SSay {
    pub id: u32,      // The id of the participant who sends the chat message
    pub text: String, // The chat message
}
}

message! {
/// Server to Client: Participant name change.
struct SName {
    pub id: u32,      // The id of the participant who changed their name
    pub name: String, // The new name
}
}

message! {
/// Server to Client: A participant was removed.
struct SRemove {
    pub id: u32, // The id of the participant who was removed
}
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
