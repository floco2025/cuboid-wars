use super::*;
use common::protocol::PlayerId;

#[test]
fn roaming_route_uses_roam_speed() {
    let mut info = actor_info();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    info.mode = ActorMode::Roam;
    info.route = Some(route(target));

    let ActorDesire::Move { intent, target: actual } = desired_move(&info, &Position::default(), 2.0, 4.0) else {
        panic!("expected route movement");
    };
    assert_eq!(intent.speed(), Some(2.0));
    assert_eq!(actual, target);
}

#[test]
fn combat_route_uses_active_speed() {
    let mut info = actor_info();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };
    info.route = Some(route(target));

    let ActorDesire::Move { intent, .. } = desired_move(&info, &Position::default(), 2.0, 4.0) else {
        panic!("expected route movement");
    };
    assert_eq!(intent.speed(), Some(4.0));
}

#[test]
fn firing_actor_holds_and_faces_live_target() {
    let mut info = actor_info();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    info.beam = BeamState::Firing { remaining_secs: 1.0 };
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };

    let ActorDesire::HoldFacing { direction } = desired_move(&info, &Position::default(), 2.0, 4.0) else {
        panic!("expected firing hold");
    };
    assert_eq!(direction, direction_toward(&Position::default(), &target));
}
