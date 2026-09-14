use std::f32::consts::{FRAC_PI_2, PI};

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

    let ActorDesire::Move { intent, target: actual } = desired_move(
        &info,
        &Position::default(),
        &Position::default(),
        FRAC_PI_2,
        2.0,
        4.0,
        1.0 / 30.0,
    ) else {
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

    let ActorDesire::Move { intent, .. } = desired_move(
        &info,
        &Position::default(),
        &Position::default(),
        FRAC_PI_2,
        2.0,
        4.0,
        1.0 / 30.0,
    ) else {
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
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 1.0,
    };
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };

    let ActorDesire::HoldFacing { direction } = desired_move(
        &info,
        &Position::default(),
        &Position::default(),
        FRAC_PI_2,
        2.0,
        4.0,
        1.0 / 30.0,
    ) else {
        panic!("expected firing hold");
    };
    assert_eq!(direction, direction_toward(&Position::default(), &target));
}

#[test]
fn firing_actor_with_route_moves_at_active_speed() {
    let mut info = actor_info();
    let target = Position {
        x: 2.0,
        ..Position::default()
    };
    info.beam = BeamState::Firing {
        target: PlayerId(7),
        started_tick: 0,
        remaining_secs: 1.0,
    };
    info.mode = ActorMode::Engage {
        target: PlayerId(7),
        target_pos: target,
    };
    info.route = Some(route(target));

    let ActorDesire::Move { intent, .. } = desired_move(
        &info,
        &Position::default(),
        &Position::default(),
        FRAC_PI_2,
        2.0,
        4.0,
        1.0 / 30.0,
    ) else {
        panic!("expected route movement while firing");
    };
    assert_eq!(intent.speed(), Some(4.0));
}

#[test]
fn a_walk_behind_the_actor_turns_it_on_the_spot_before_it_moves() {
    let delta = 1.0 / 30.0;
    let mut heading = 0.0;
    let mut ticks_standing = 0;
    for tick in 0..30 {
        let intent = steer(
            ActorMoveIntent::Moving {
                direction: PI,
                speed: 4.0,
            },
            heading,
            delta,
        );
        let (Some(direction), Some(speed)) = (intent.direction(), intent.speed()) else {
            panic!("steering dropped the walk");
        };
        assert!(
            angle_delta_radians(direction, heading).abs() <= ACTOR_TURN_RATE * delta + 0.0001,
            "tick {tick} turned farther than the rate allows"
        );
        if speed == 0.0 {
            ticks_standing += 1;
        }
        heading = direction;
    }
    assert!(
        (3..15).contains(&ticks_standing),
        "a reversal starts with a pivot on the spot, then moves off while still turning"
    );
    assert!(
        angle_delta_radians(heading, PI).abs() < 0.0001,
        "the heading came all the way round"
    );
}

#[test]
fn a_slight_turn_keeps_nearly_all_of_the_speed() {
    let intent = steer(
        ActorMoveIntent::Moving {
            direction: 0.3,
            speed: 4.0,
        },
        0.0,
        1.0 / 30.0,
    );
    assert!(intent.speed().is_some_and(|speed| speed > 3.9));
}
