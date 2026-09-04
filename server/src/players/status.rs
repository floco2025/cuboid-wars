use bevy::prelude::*;

use super::PlayerMap;
use crate::network::broadcast_to_all;
use common::protocol::ServerMessage;

// System to count down player power-up and stun timers
pub fn players_status_timers_system(time: Res<Time>, mut players: ResMut<PlayerMap>) {
    let delta = time.delta_secs();

    let mut status_messages = Vec::new();

    for (player_id, player_info) in players.iter_mut() {
        let old_status = player_info.status(*player_id);

        player_info.tick_timers(delta);
        let new_status = player_info.status(*player_id);

        if old_status != new_status {
            status_messages.push(new_status);
        }
    }

    // Send status updates to all clients
    for msg in status_messages {
        broadcast_to_all(&players, ServerMessage::PlayerStatus(msg));
    }
}
