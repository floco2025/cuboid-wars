use crate::{network::broadcast_to_all, resources::PlayerMap};
use common::protocol::{ActorId, ActorMoveIntent, ActorMovementState, Position, SActorMoveIntent, ServerMessage};

pub fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: ActorMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMoveIntent(SActorMoveIntent {
            id,
            movement: ActorMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}
