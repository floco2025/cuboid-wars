use crate::{missiles::MissileMap, network::broadcast_to_all, players::PlayerMap};
use common::protocol::*;

pub(crate) fn handle_missile_shot_message(
    id: PlayerId,
    msg: CMissileShot,
    players: &mut PlayerMap,
    missiles: &mut MissileMap,
    tick: ServerTick,
) {
    if !msg.movement.is_finite() {
        return;
    }
    let Some(player) = players.get_mut(&id) else {
        return;
    };
    if player.is_dead() || player.session.generation != msg.generation || !player.try_start_missile() {
        return;
    }
    let missile_id = missiles.allocate();
    missiles.insert(
        missile_id,
        Missile {
            shooter: id,
            seq: 0,
            movement: msg.movement,
        },
        tick.0,
    );
    broadcast_to_all(
        players,
        ServerMessage::MissileLaunch(SMissileLaunch {
            id: missile_id,
            shooter: id,
            tick: tick.0,
            target: msg.target,
            movement: msg.movement,
        }),
    );
}
