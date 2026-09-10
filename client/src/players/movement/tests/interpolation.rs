use super::*;
use crate::{
    config::ClientSettings,
    network::{SampleBuffer, SampleTiming},
    players::PlayerMotionBundle,
    test_fixtures,
};
use bevy::ecs::system::RunSystemOnce;
use common::{
    config::{NetworkConfig, UpdateCadence},
    protocol::{Carrier, MapLayout, PlayerGeneration, PlayerMove},
};
use std::f32::consts::PI;

fn sample(seq: u32, x: f32, portal_crossing: u32) -> PlayerMove {
    PlayerMove {
        id: PlayerId(2),
        generation: PlayerGeneration(1),
        seq,
        portal_crossing,
        movement: PlayerMovementState::new(
            Position { x, ..default() },
            PlayerMoveIntent::Running { direction: 0.0 },
            0.0,
            0.0,
        ),
    }
}

fn immediate() -> SampleTiming {
    SampleTiming {
        delay_ticks: 0.0,
        interval_ticks: 1.0,
    }
}

fn timing(hz: u32) -> SampleTiming {
    let settings = ClientSettings::load_default().expect("client settings are invalid");
    SampleTiming::new(
        &settings.interpolation,
        &NetworkConfig {
            update_hz: hz,
            ..Default::default()
        },
    )
}

fn seeded(entry: PlayerMove, timing: SampleTiming) -> RemotePlayerMotion {
    RemotePlayerMotion(SampleBuffer::new(
        Some(entry.seq),
        PlayerSample {
            portal_crossing: entry.portal_crossing,
            movement: entry.movement,
        },
        timing,
    ))
}

fn shown(buffer: &mut RemotePlayerMotion, delta_ticks: f64) -> PlayerMovementState {
    buffer.advance(delta_ticks, &Carriers::default(), 1.0, 1.0 / 30.0).0
}

fn moving_platforms() -> Carriers {
    let platform = Carrier {
        parent: CarrierId::WORLD,
        level: 0,
        levels: 1,
        from: Position {
            x: 100.0,
            y: 0.0,
            z: 0.0,
        },
        to: Position {
            x: 116.0,
            y: 8.0,
            z: 0.0,
        },
        travel_ticks: 30,
        pause_ticks: 9,
        phase_ticks: 0,
    };
    Carriers::from_layout(&MapLayout {
        carriers: vec![
            platform,
            Carrier {
                parent: CarrierId(1),
                from: Position::default(),
                to: Position { x: 0.0, y: 0.0, z: 4.0 },
                phase_ticks: 5,
                ..platform
            },
        ],
        ..default()
    })
}

#[test]
fn delayed_riders_follow_platform_stops_reversals_and_nested_motion_between_ticks() {
    for carrier in [CarrierId(1), CarrierId(2)] {
        for hz in [10, 30] {
            let mut carriers = moving_platforms();
            let local = Position {
                x: 1.5,
                y: 0.01,
                z: -0.5,
            };
            let rider = |seq: u32| {
                let mut entry = sample(seq, local.x, 0);
                entry.movement.carrier = carrier;
                entry.movement.pos = local;
                entry.movement.support = CharacterSupport::Ground;
                entry
            };
            let mut buffer = seeded(rider(0), timing(hz));
            let mut cadence = UpdateCadence::new(hz, 30);
            for tick in 1..180 {
                carriers.advance(tick);
                if cadence.ready() && !(45..90).contains(&tick) {
                    buffer.push(rider(tick));
                }
                for frame in 0..4 {
                    let alpha = frame as f32 / 4.0;
                    let (movement, velocity) = buffer.advance(0.25, &carriers, alpha, 1.0 / 30.0);
                    let expected = carriers.pose_between(carrier, alpha).transform_position(&local);
                    assert!(
                        (Vec3::from(movement.pos) - Vec3::from(expected)).length() < 1e-5,
                        "rider separated from platform at tick {tick}, rate {hz}"
                    );
                    assert_eq!(velocity, Vec3::ZERO, "platform travel must not animate walking");
                }
            }
        }
    }
}

#[test]
fn a_rider_from_a_snapshot_follows_the_platform_before_the_first_movement_report() {
    let mut carriers = moving_platforms();
    let mut movement = sample(1, 2.0, 0).movement;
    movement.carrier = CarrierId(2);
    let mut buffer = RemotePlayerMotion::new(movement, timing(30));
    for tick in 1..70 {
        carriers.advance(tick);
        let shown = buffer.advance(1.0, &carriers, 0.5, 1.0 / 30.0).0;
        assert_eq!(
            shown.pos,
            carriers
                .pose_between(CarrierId(2), 0.5)
                .transform_position(&movement.pos)
        );
    }
}

#[test]
fn boarding_and_leaving_interpolate_world_positions_across_coordinate_frames() {
    let mut carriers = moving_platforms();
    carriers.advance(30);
    for (from, to) in [(CarrierId::WORLD, CarrierId(1)), (CarrierId(1), CarrierId::WORLD)] {
        let mut left = sample(1, 117.0, 0);
        let mut right = sample(4, 118.0, 0);
        left.movement.carrier = from;
        left.movement.pos = carriers.pose(from).inverse_transform_position(&left.movement.pos);
        right.movement.carrier = to;
        right.movement.pos = carriers.pose(to).inverse_transform_position(&right.movement.pos);
        let mut buffer = seeded(left, immediate());
        buffer.push(right);
        let mut previous = 117.0;
        let mut between = false;
        for _ in 0..12 {
            let x = buffer.advance(0.5, &carriers, 1.0, 1.0 / 30.0).0.pos.x;
            assert!(x >= previous && x <= 118.0);
            between |= x > 117.0 && x < 118.0;
            previous = x;
        }
        assert!(between, "boarding rendered no blend between the frames");
        assert_eq!(previous, 118.0);
    }
}

#[test]
fn a_portal_crossing_cuts_between_different_carrier_frames() {
    let mut carriers = moving_platforms();
    carriers.advance(30);
    let mut left = sample(1, 1.0, 0);
    left.movement.carrier = CarrierId(1);
    let mut right = sample(4, 2.0, 1);
    right.movement.carrier = CarrierId(2);
    let entrance = carriers.pose(CarrierId(1)).transform_position(&left.movement.pos);
    let exit = carriers.pose(CarrierId(2)).transform_position(&right.movement.pos);
    let mut buffer = seeded(left, immediate());
    buffer.push(right);
    let first = buffer.advance(0.5, &carriers, 1.0, 1.0 / 30.0).0.pos;
    assert_eq!(first, entrance);
    let mut last = first;
    for _ in 0..12 {
        last = buffer.advance(0.5, &carriers, 1.0, 1.0 / 30.0).0.pos;
        assert!(last == entrance || last == exit, "a crossing blended between its ends");
    }
    assert_eq!(last, exit);
}

#[test]
fn stopping_never_overshoots_or_settles_back_at_each_update_rate() {
    for hz in [30, 15, 10, 7, 1] {
        let mut buffer = seeded(sample(0, 0.0, 0), timing(hz));
        let mut cadence = UpdateCadence::new(hz, 30);
        cadence.ready();
        let mut previous = 0.0;
        for tick in 1..300 {
            if cadence.ready() {
                buffer.push(sample(tick, (tick as f32 / 30.0).min(2.0), 0));
            }
            for _ in 0..4 {
                let movement = shown(&mut buffer, 0.25);
                assert!(movement.pos.x >= previous, "backward motion at {hz} Hz, tick {tick}");
                assert!(movement.pos.x <= 2.0, "overshoot at {hz} Hz");
                previous = movement.pos.x;
            }
        }
        assert_eq!(previous, 2.0);
    }
}

#[test]
fn lost_crossing_report_still_cuts_at_the_next_sample() {
    let mut buffer = seeded(sample(1, 1.0, 0), immediate());
    buffer.push(sample(4, 100.0, 1));
    let mut last = 1.0;
    for _ in 0..16 {
        last = shown(&mut buffer, 0.25).pos.x;
        assert!(
            last == 1.0 || last == 100.0,
            "a lost crossing blended across the portal"
        );
    }
    assert_eq!(last, 100.0);
}

#[test]
fn sequences_wrap_and_repeated_or_late_reports_do_not_rewind_playback() {
    let mut buffer = seeded(sample(u32::MAX, 1.0, 0), immediate());
    assert!(buffer.push(sample(1, 3.0, 0)));
    assert!(!buffer.push(sample(u32::MAX, -10.0, 0)));
    assert!(!buffer.push(sample(1, -10.0, 0)));
    let mut previous = 1.0;
    for _ in 0..8 {
        let x = shown(&mut buffer, 0.5).pos.x;
        assert!(x >= previous);
        previous = x;
    }
    assert_eq!(previous, 3.0);
    assert!(buffer.push(sample(3, 5.0, 0)));
    for _ in 0..8 {
        let x = shown(&mut buffer, 0.5).pos.x;
        assert!(x >= previous && x <= 5.0);
        previous = x;
    }
    assert_eq!(previous, 5.0);
}

#[test]
fn landing_and_facing_follow_the_buffered_timeline() {
    let mut air = sample(1, 0.0, 0);
    air.movement.pos.y = 1.0;
    air.movement.face_yaw = PI - 0.1;
    air.movement.vertical_velocity = -2.0;
    air.movement.support = CharacterSupport::Airborne;
    let mut ground = sample(3, 2.0, 0);
    ground.movement.face_yaw = -PI + 0.1;
    ground.movement.support = CharacterSupport::Ground;
    let mut buffer = seeded(air, immediate());
    buffer.push(ground);
    let mut previous = shown(&mut buffer, 0.0);
    let mut blended = false;
    loop {
        let movement = shown(&mut buffer, 0.25);
        assert!(movement.pos.y <= previous.pos.y);
        assert!(movement.face_yaw.abs() > 3.0, "facing turned the long way round");
        if movement.support == CharacterSupport::Ground {
            break;
        }
        blended |= movement.pos.y > 0.0 && movement.pos.y < 1.0;
        previous = movement;
    }
    assert!(blended);
    let landed = shown(&mut buffer, 0.25);
    assert_eq!(landed.pos.y, 0.0);
    assert_eq!(landed.face_yaw, ground.movement.face_yaw);
}

#[test]
fn the_local_body_is_never_interpolated() {
    let mut world = World::new();
    world.insert_resource(Time::<()>::default());
    world.insert_resource(Time::<Fixed>::default());
    world.init_resource::<Carriers>();
    world.insert_resource(test_fixtures::map_settings());
    world.init_resource::<PlayerMap>();
    let start = sample(1, 0.0, 0).movement;
    let mut buffer = seeded(sample(1, 0.0, 0), immediate());
    buffer.push(sample(2, 10.0, 0));
    let entity = world
        .spawn((
            PlayerId(1),
            LocalPlayerMarker,
            start.pos,
            PlayerMotionBundle::from(&start),
            PlayerAnimationMotion::default(),
            buffer,
        ))
        .id();
    for _ in 0..30 {
        world
            .run_system_once(interpolate_remote_players_system)
            .expect("remote interpolation failed");
    }
    assert_eq!(world.get::<Position>(entity).expect("position missing").x, 0.0);
}
