use bevy::prelude::*;

use crate::schedule::ClientSet;

use super::{incoming::network_receive_system, link::network_flush_system, ping::network_ping_system};

pub fn network_plugin(app: &mut App) {
    app.add_systems(
        Update,
        (network_ping_system, network_receive_system)
            .in_set(ClientSet::Network)
            .run_if(super::playback::live_gameplay),
    );
    app.add_systems(Last, network_flush_system.run_if(super::playback::live_gameplay));
}
