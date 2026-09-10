use bevy::prelude::*;
use common::{
    config::{GameplayConfig, NetworkConfig},
    protocol::{SMissileDetonated, ServerMessage, ServerTick},
};

use super::MissileMap;
use crate::{network::broadcast_to_all, players::PlayerMap, schedule::ticks_from_secs};

// Report lag a flying missile may accumulate past its lifetime before the server gives up on it.
const MISSILE_EXPIRY_MARGIN_SECS: f32 = 2.0;

// The owner flies the missile; one that never reports its detonation would otherwise ride every snapshot forever.
pub(super) fn missiles_expiry_system(
    tick: Res<ServerTick>,
    network: Res<NetworkConfig>,
    gameplay: Res<GameplayConfig>,
    players: Res<PlayerMap>,
    mut missiles: ResMut<MissileMap>,
) {
    let max_age = ticks_from_secs(
        gameplay.missiles.lifetime_secs + MISSILE_EXPIRY_MARGIN_SECS,
        network.server_hz,
    );
    for (id, pos) in missiles.take_expired(tick.0, max_age) {
        warn!("missile#{} expired without a detonation report", id.0);
        broadcast_to_all(
            &players,
            ServerMessage::MissileDetonated(SMissileDetonated { id, tick: tick.0, pos }),
        );
    }
}
