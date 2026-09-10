use common::config::NetworkConfig;
use std::{f32::consts::PI, time::Duration};

use bevy::prelude::*;
use common::{
    map::Carriers,
    math::angle_delta_radians,
    physics::{CharacterSupport, CharacterVerticalVelocity},
    protocol::{ActorMoveIntent, ActorMovementState, Carrier, CarrierId, FaceYaw, MapLayout, Position, UpdateCadence},
};

use super::{ActorAnimationVelocity, RemoteActorMotion, actors_transform_sync_system};
use crate::config::ClientSettings;

fn sample(x: f32) -> ActorMovementState {
    ActorMovementState {
        pos: Position { x, ..default() },
        carrier: CarrierId::WORLD,
        move_intent: ActorMoveIntent::Moving {
            direction: 0.0,
            speed: 3.0,
        },
        vertical_velocity: 0.0,
        face_yaw: 0.0,
        support: CharacterSupport::Ground,
    }
}

#[test]
fn stops_and_packet_gaps_never_extrapolate_or_pull_an_actor_back() {
    let settings = ClientSettings::load_default().expect("client settings are invalid");
    for hz in [1, 7, 10, 15, 30] {
        let mut buffer = RemoteActorMotion::new(
            0,
            sample(0.0),
            settings.interpolation.delay_ticks(&NetworkConfig {
                update_hz: hz,
                ..Default::default()
            }),
        );
        let mut cadence = UpdateCadence::default();
        let mut previous = 0.0;
        for tick in 1..360 {
            if cadence.ready(hz, 30) && !(45..90).contains(&tick) {
                buffer.push(tick, sample((tick as f32 / 30.0).min(4.0)));
            }
            for _ in 0..4 {
                let (state, velocity) = buffer.advance(0.25, &Carriers::default(), 1.0, 1.0 / 30.0);
                assert!(state.pos.x >= previous, "backward motion at {hz} Hz, tick {tick}");
                assert!(state.pos.x <= 4.0, "overshoot at {hz} Hz");
                assert!(state.pos.x - previous <= 1.0 / 120.0 + 1e-5, "jump after a packet gap");
                assert!(velocity.x >= 0.0);
                previous = state.pos.x;
            }
        }
        assert_eq!(previous, 4.0);
        assert_eq!(
            buffer.advance(30.0, &Carriers::default(), 1.0, 1.0 / 30.0).1,
            Vec3::ZERO
        );
    }
}

#[test]
fn snapshot_and_movement_samples_share_ordering_across_tick_wraparound() {
    let mut start = sample(0.0);
    start.face_yaw = 3.0;
    start.support = CharacterSupport::Airborne;
    start.vertical_velocity = -6.0;
    let mut landed = sample(3.0);
    landed.face_yaw = -3.0;
    let mut buffer = RemoteActorMotion::new(u32::MAX - 1, start, 0.0);
    buffer.push(1, landed);
    buffer.push(u32::MAX, sample(-100.0));
    buffer.push(1, sample(100.0));
    let (middle, _) = buffer.advance(1.5, &Carriers::default(), 1.0, 1.0 / 30.0);
    assert_eq!(middle.pos.x, 1.5);
    assert_eq!(middle.vertical_velocity, -3.0);
    assert_eq!(middle.support, CharacterSupport::Airborne);
    assert!(angle_delta_radians(middle.face_yaw, PI).abs() < 1e-5);
    let (end, _) = buffer.advance(1.5, &Carriers::default(), 1.0, 1.0 / 30.0);
    assert_eq!(end, landed);
}

#[test]
fn rendering_holds_reported_bodies_despite_movement_intent_and_vertical_velocity() {
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f64(1.0 / 30.0));
    let mut app = App::new();
    app.insert_resource(time)
        .init_resource::<Time<Fixed>>()
        .init_resource::<Carriers>()
        .add_systems(Update, actors_transform_sync_system);
    let mut movement = sample(0.0);
    movement.vertical_velocity = -50.0;
    movement.support = CharacterSupport::Airborne;
    let entity = app
        .world_mut()
        .spawn((
            RemoteActorMotion::new(10, movement, 0.0),
            movement.pos,
            FaceYaw(movement.face_yaw),
            movement.move_intent,
            CharacterVerticalVelocity(movement.vertical_velocity),
            movement.support,
            ActorAnimationVelocity::default(),
            Transform::default(),
        ))
        .id();
    for _ in 0..120 {
        app.update();
        assert_eq!(
            *app.world().get::<Position>(entity).expect("actor position missing"),
            movement.pos
        );
        assert_eq!(
            app.world()
                .get::<ActorAnimationVelocity>(entity)
                .expect("actor velocity missing")
                .0,
            Vec3::ZERO
        );
    }
    let mut target = sample(3.0);
    target.support = CharacterSupport::Ladder;
    app.world_mut()
        .get_mut::<RemoteActorMotion>(entity)
        .expect("actor buffer missing")
        .push(40, target);
    for _ in 0..31 {
        app.update();
        let pos = *app.world().get::<Position>(entity).expect("actor position missing");
        assert_eq!(
            app.world()
                .get::<Transform>(entity)
                .expect("actor transform missing")
                .translation,
            Vec3::from(pos)
        );
        assert!(pos.x > 0.0 && pos.x <= 3.0);
    }
    assert_eq!(
        *app.world().get::<Position>(entity).expect("actor position missing"),
        target.pos
    );
    assert_eq!(
        *app.world()
            .get::<CharacterSupport>(entity)
            .expect("actor support missing"),
        CharacterSupport::Ladder
    );
}

#[test]
fn buffered_actors_follow_stopping_reversing_and_nested_platforms_without_wheel_travel() {
    let platform = Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 1,
        from: Position::default(),
        to: Position {
            x: 12.0,
            y: 4.0,
            z: 0.0,
        },
        travel_ticks: 30,
        pause_ticks: 9,
        phase_ticks: 0,
    };
    let layout = MapLayout {
        carriers: vec![
            platform,
            Carrier {
                parent: CarrierId(1),
                to: Position { x: 0.0, y: 0.0, z: 6.0 },
                phase_ticks: 5,
                ..platform
            },
        ],
        ..default()
    };
    let settings = ClientSettings::load_default().expect("client settings are invalid");
    for carrier in [CarrierId(1), CarrierId(2)] {
        for hz in [10, 30] {
            let mut carriers = Carriers::from_layout(&layout);
            let mut state = sample(2.0);
            state.carrier = carrier;
            let mut buffer = RemoteActorMotion::new(
                0,
                state,
                settings.interpolation.delay_ticks(&NetworkConfig {
                    update_hz: hz,
                    ..Default::default()
                }),
            );
            let mut cadence = UpdateCadence::default();
            for tick in 1..180 {
                carriers.advance(tick);
                if cadence.ready(hz, 30) && !(45..90).contains(&tick) {
                    buffer.push(tick, state);
                }
                for frame in 0..4 {
                    let alpha = frame as f32 / 4.0;
                    let (visible, velocity) = buffer.advance(0.25, &carriers, alpha, 1.0 / 30.0);
                    let expected = carriers.pose_between(carrier, alpha).transform_position(&state.pos);
                    assert!((Vec3::from(visible.pos) - Vec3::from(expected)).length() < 1e-5);
                    assert_eq!(velocity, Vec3::ZERO);
                }
            }
        }
    }
}
