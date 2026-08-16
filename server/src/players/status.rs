use bevy::prelude::*;

use super::{PlayerMap, UnlimitedMissiles};
use crate::network::broadcast_to_all;
use common::{config::GameplayConfig, protocol::ServerMessage};

// God mode: ammo refills every tick, so firing never runs dry. Missiles
// ride the snapshot (not `SPlayerStatus`), so this causes no extra
// broadcasts.
pub fn players_unlimited_missiles_system(
    mut players: ResMut<PlayerMap>,
    unlimited_missiles: Res<UnlimitedMissiles>,
    gameplay_config: Res<GameplayConfig>,
) {
    if !unlimited_missiles.0 {
        return;
    }
    for (_, player_info) in players.iter_mut() {
        player_info.missiles = gameplay_config.missiles.max_missiles;
    }
}

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
