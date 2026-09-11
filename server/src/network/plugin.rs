use bevy::prelude::*;

use crate::schedule::ServerSet;

use super::{
    incoming::network_receive_system,
    outgoing::network_flush_system,
    snapshot::{
        network_broadcast_actor_moves_system, network_broadcast_player_moves_system, network_broadcast_snapshot_system,
    },
};

pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (
            network_receive_system.in_set(ServerSet::Ingress),
            network_broadcast_player_moves_system.in_set(ServerSet::Snapshot),
            network_broadcast_actor_moves_system.in_set(ServerSet::Snapshot),
            network_broadcast_snapshot_system.in_set(ServerSet::Snapshot),
        ),
    );
    app.add_systems(Last, network_flush_system);
}
