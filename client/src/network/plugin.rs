use bevy::prelude::*;

use crate::schedule::ClientSet;
use common::physics::powered_bridges_sync_system;

use super::io::{network_flush_system, network_ping_system, network_receive_system};

pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            network_ping_system,
            network_receive_system,
            powered_bridges_sync_system.after(network_receive_system),
        )
            .in_set(ClientSet::Network),
    );
    app.add_systems(Last, network_flush_system);
}
