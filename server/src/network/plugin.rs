use bevy::prelude::*;

use super::*;
use crate::schedule::ServerSet;

pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            network_process_client_messages_system.in_set(ServerSet::Ingress),
            network_broadcast_snapshot_system.in_set(ServerSet::Snapshot),
        ),
    );
}
