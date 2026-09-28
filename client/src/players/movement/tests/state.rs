use super::*;

#[test]
fn movement_state_round_trips_through_its_components() {
    let state = PlayerMovementState {
        stance: common::protocol::PlayerStance {
            crouched: true,
            fraction: 1.0,
        },
        carrier: CarrierId::WORLD,
        pos: Position { x: 1.0, y: 2.0, z: 3.0 },
        move_intent: PlayerMoveIntent::moving(0.5),
        vertical_velocity: -4.0,
        face_yaw: 1.25,
        horizontal_velocity: [1.0, 0.0, -2.0],
        knockback: [0.5, 0.0, 0.25],
        support: CharacterSupport::Ladder,
    };
    let bundle = PlayerMotionBundle::from(&state);
    let rebuilt = player_movement_state(
        state.pos,
        bundle.move_intent,
        &bundle.face_yaw,
        &bundle.vertical_velocity,
        &bundle.horizontal_velocity,
        &bundle.knockback,
        bundle.support,
        bundle.stance,
    );
    assert_eq!(rebuilt.stance, state.stance);
    assert_eq!(rebuilt.pos, state.pos);
    assert_eq!(rebuilt.move_intent, state.move_intent);
    assert_eq!(rebuilt.vertical_velocity, state.vertical_velocity);
    assert_eq!(rebuilt.face_yaw, state.face_yaw);
    assert_eq!(rebuilt.horizontal_velocity, state.horizontal_velocity);
    assert_eq!(rebuilt.knockback, state.knockback);
    assert_eq!(rebuilt.support, state.support);
}
