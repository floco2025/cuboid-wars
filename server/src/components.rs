#[allow(clippy::wildcard_imports)]
use bevy::prelude::*;
use tokio::sync::mpsc::UnboundedSender;

use crate::net::ServerToClient;

// ============================================================================
// Bevy Components
// ============================================================================

/// Network channel for sending messages to a specific client
#[derive(Component)]
pub struct NetworkChannel(pub UnboundedSender<ServerToClient>);

/// Marker component: client is logged in (authenticated)
#[derive(Component)]
pub struct LoggedIn;
