use crate::{
    combat::PendingExplosions,
    missiles::MissileMap,
    network::{broadcast_to_all, broadcast_to_others},
    players::PlayerMap,
};
use common::protocol::*;

pub(crate) fn handle_missile_moves(
    id: PlayerId,
    message: CMissileMoves,
    missiles: &mut MissileMap,
    players: &PlayerMap,
) {
    let mut moves = Vec::new();
    for update in message.moves {
        let Some(missile) = missiles.get_mut(&update.id) else {
            continue;
        };
        if missile.shooter != id || !sequence_is_newer(update.seq, missile.seq) || !update.movement.is_finite() {
            continue;
        }
        missile.seq = update.seq;
        missile.movement = update.movement;
        moves.push(update);
    }
    if !moves.is_empty() {
        broadcast_to_others(players, id, ServerMessage::MissileMoves(SMissileMoves { moves }));
    }
}

pub(crate) fn handle_missile_detonated(
    id: PlayerId,
    message: CMissileDetonated,
    missiles: &mut MissileMap,
    players: &PlayerMap,
    pending: &mut PendingExplosions,
    tick: ServerTick,
) {
    if missiles.get(&message.id).is_none_or(|missile| missile.shooter != id) || !message.pos.is_finite() {
        return;
    }
    missiles.remove(&message.id);
    pending.push_missile(id, message.pos, message.hits);
    broadcast_to_all(
        players,
        ServerMessage::MissileDetonated(SMissileDetonated {
            id: message.id,
            tick: tick.0,
            pos: message.pos,
        }),
    );
}
