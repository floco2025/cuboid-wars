use bevy::prelude::*;

use crate::resources::{FromAcceptChannel, PlayerInfo, PlayerMap};
use common::protocol::PlayerMarker;

// ============================================================================
// Accept Connections System
// ============================================================================

// Drain newly accepted connections into ECS entities and tracking state.
pub fn network_accept_connections_system(
    mut commands: Commands,
    mut from_accept: ResMut<FromAcceptChannel>,
    mut players: ResMut<PlayerMap>,
) {
    while let Ok((id, to_client)) = from_accept.try_recv() {
        debug!("{:?} connected", id);
        let entity = commands.spawn((PlayerMarker, id)).id();
        players.0.insert(id, PlayerInfo::new(entity, to_client));
    }
}
