use bevy::prelude::*;

use crate::resources::{FromAcceptChannel, PlayerInfo, PlayerMap};
use common::markers::PlayerMarker;

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
        players.0.insert(
            id,
            PlayerInfo {
                entity,
                logged_in: false,
                channel: to_client,
                hits: 0,
                name: String::new(),
                speed_power_up_timer: 0.0,
                multi_shot_power_up_timer: 0.0,
                phasing_power_up_timer: 0.0,
                sentry_hunt_power_up_timer: 0.0,
                stun_timer: 0.0,
                last_shot_time: f32::NEG_INFINITY,
            },
        );
    }
}
