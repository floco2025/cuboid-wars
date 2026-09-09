use bevy::prelude::*;

use super::PlayerMap;
use crate::network::broadcast_to_all;
use common::protocol::ServerMessage;

// System to count down player power-up and stun timers
pub fn players_status_timers_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in players.iter_mut() {
        // Only the power-up and stun fields can move here, so comparing them
        // avoids building (and cloning the held keys of) two full statuses.
        let before = (player_info.active_power_ups(), player_info.is_stunned());

        player_info.tick_timers(delta);

        if (player_info.active_power_ups(), player_info.is_stunned()) != before {
            status_messages.push(player_info.status(*player_id));
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
