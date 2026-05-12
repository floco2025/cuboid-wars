use crate::{
    constants::{ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD, ACTOR_MOVE_INTENT_SEND_COOLDOWN},
    network::broadcast_to_all,
    resources::{ActorInfo, PlayerMap},
};
use common::{
    math::angle_delta_radians,
    protocol::{ActorId, ActorMoveIntent, ActorMovementState, Position, SActorMoveIntent, ServerMessage},
};

pub fn maybe_broadcast_actor_move_intent(
    players: &PlayerMap,
    id: ActorId,
    pos: Position,
    move_intent: ActorMoveIntent,
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

fn actor_move_intent_should_broadcast(
    last_broadcast: ActorMoveIntent,
    current: ActorMoveIntent,
    send_timer: f32,
) -> bool {
    match (current, last_broadcast) {
        (ActorMoveIntent::Idle, ActorMoveIntent::Idle) => false,
        (ActorMoveIntent::Idle, ActorMoveIntent::Moving { .. })
        | (ActorMoveIntent::Moving { .. }, ActorMoveIntent::Idle) => true,
        (
            ActorMoveIntent::Moving {
                direction: current_dir,
                speed: current_speed,
            },
            ActorMoveIntent::Moving {
                direction: last_dir,
                speed: last_speed,
            },
        ) => {
            (current_speed - last_speed).abs() > f32::EPSILON
                || send_timer >= ACTOR_MOVE_INTENT_SEND_COOLDOWN
                    && angle_delta_radians(current_dir, last_dir).abs()
                        >= ACTOR_MOVE_INTENT_DIR_CHANGE_THRESHOLD.to_radians()
        }
    }
}
