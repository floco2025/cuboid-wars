use bevy::prelude::*;

use super::PlayerMap;
use crate::network::broadcast_to_all;
use common::protocol::{PlayerDeathEffect, PlayerId, PlayerMarker, Position, SPlayerDeath, ServerMessage};

pub(crate) fn enter_group_respawn(
    commands: &mut Commands,
    players: &mut PlayerMap,
    id: PlayerId,
    pos: Position,
) -> bool {
    if !players.group_respawn_active() {
        return false;
    }
    let Some(info) = players.get_mut(&id).filter(|info| info.connection.logged_in) else {
        return false;
    };
    let Some(entity) = info.entity() else {
        return false;
    };
    let victim_score = info.session.score;
    info.begin_group_respawn();
    commands.entity(entity).despawn();
    broadcast_to_all(
        players,
        ServerMessage::PlayerDeath(SPlayerDeath {
            id,
            pos,
            killer: None,
            victim_score,
            killer_score: None,
            effect: PlayerDeathEffect::GroupRespawn,
        }),
    );
    true
}

pub(crate) fn players_group_respawn_system(
    mut commands: Commands,
    mut players: ResMut<PlayerMap>,
    positions: Query<(&PlayerId, &Position), With<PlayerMarker>>,
) {
    if !players.group_respawn_active() {
        return;
    }
    for (id, pos) in &positions {
        enter_group_respawn(&mut commands, &mut players, *id, *pos);
    }
}
