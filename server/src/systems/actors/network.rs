use crate::{
    constants::{ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD, ACTOR_MOVE_INTENT_SEND_COOLDOWN},
    resources::{ActorInfo, PlayerMap},
    systems::network::broadcast_to_all,
};
use common::{
    math::angle_delta_radians,
    protocol::{
        ActorId, CharacterMoveIntent, CharacterMovementState, Position, SActorMoveIntent, SActorTeleport, ServerMessage,
    },
};

pub fn maybe_broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: CharacterMoveIntent,
    vertical_velocity: f32,
    info: &mut ActorInfo,
) {
    if !actor_move_intent_should_broadcast(
        info.last_broadcast_move_intent,
        move_intent,
        info.move_intent_send_timer,
    ) {
        return;
    }

    broadcast_actor_move_intent(players, id, pos, move_intent, vertical_velocity);
    info.last_broadcast_move_intent = move_intent;
    info.move_intent_send_timer = 0.0;
}

pub fn broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: CharacterMoveIntent,
    vertical_velocity: f32,
) {
    broadcast_to_all(
        players,
        ServerMessage::ActorMoveIntent(SActorMoveIntent {
            id,
            movement: CharacterMovementState::new(pos, move_intent, vertical_velocity),
        }),
    );
}

pub fn broadcast_actor_teleport(players: &PlayerMap, id: ActorId, pos: Position, move_intent: CharacterMoveIntent) {
    broadcast_to_all(
        players,
        ServerMessage::ActorTeleport(SActorTeleport {
            id,
            movement: CharacterMovementState::new(pos, move_intent, 0.0),
        }),
    );
}

fn actor_move_intent_should_broadcast(
    last_broadcast: CharacterMoveIntent,
    current: CharacterMoveIntent,
    send_timer: f32,
) -> bool {
    let last_dir = last_broadcast.direction();
    let current_dir = current.direction();
    if last_dir.is_some() != current_dir.is_some() {
        return true;
    }

    match (current_dir, last_dir) {
        (Some(current), Some(last)) => {
            send_timer >= ACTOR_MOVE_INTENT_SEND_COOLDOWN
                && angle_delta_radians(current, last).abs() >= ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD.to_radians()
        }
        _ => false,
    }
}
