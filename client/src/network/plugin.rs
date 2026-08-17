use super::*;
use bevy::prelude::*;

use crate::schedule::ClientSet;

// Network consumes server messages and sends periodic ping requests.
pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (network_ping_system, network_process_server_messages_system).in_set(ClientSet::Network),
    );
}
