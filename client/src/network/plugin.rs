use bevy::prelude::*;

use crate::schedule::ClientSet;

use super::io::{network_ping_system, network_receive_system};

pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (network_ping_system, network_receive_system).in_set(ClientSet::Network),
    );
}
