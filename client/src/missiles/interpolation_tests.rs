use super::{MissileInfo, MissileMap, MissileVelocity, RemoteMissileMotion, missiles_transform_sync_system};
use crate::{characters::PreviousTickPosition, config::ClientSettings};
use bevy::prelude::*;
use common::config::NetworkConfig;
use common::{math::angle_delta_radians, protocol::*};
use std::{f32::consts::PI, time::Duration};

fn sample(x: f32) -> MissileMovementState {
    MissileMovementState::from_velocity(Position { x, ..default() }, Vec3::X * 20.0)
}

#[test]
fn repeated_snapshots_and_reordered_updates_cannot_restart_or_reverse_flight() {
    let mut start = sample(0.0);
    start.yaw = 3.0;
    let mut end = sample(2.0);
    end.yaw = -3.0;
    let mut motion = RemoteMissileMotion::new(u32::MAX - 1, start, 0.0);
    motion.push(1, end);
    motion.push(u32::MAX, sample(-50.0));
    motion.push(1, sample(50.0));
    let halfway = motion.advance(1.5);
    assert_eq!(halfway.pos.x, 1.0);
    assert!(angle_delta_radians(halfway.yaw, PI).abs() < 1e-5);
    assert_eq!(motion.advance(1.5), end);
    assert_eq!(motion.advance(1000.0), end);
}

#[test]
fn configured_buffer_smooths_flight_and_holds_after_packet_gaps() {
    let settings = ClientSettings::load_default().expect("client config invalid");
    for hz in [7, 10, 30] {
        let mut motion = RemoteMissileMotion::new(
            0,
            sample(0.0),
            settings.interpolation.delay_ticks(&NetworkConfig {
                update_hz: hz,
                ..Default::default()
            }),
        );
        let mut cadence = UpdateCadence::default();
        let mut previous = 0.0;
        for tick in 1..240 {
            if cadence.ready(hz, 30) && !(30..60).contains(&tick) {
                motion.push(tick, sample((tick as f32 / 30.0 * 20.0).min(60.0)));
            }
            for _ in 0..4 {
                let x = motion.advance(0.25).pos.x;
                assert!(x >= previous && x <= 60.0, "{hz} Hz tick {tick}: {previous} -> {x}");
                assert!(x - previous <= 20.0 / 120.0 + 1e-4);
                previous = x;
            }
        }
        assert_eq!(previous, 60.0);
    }
}

#[test]
fn observer_rendering_holds_the_latest_sample_despite_its_speed() {
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f64(1.0 / 60.0));
    app.insert_resource(time)
        .init_resource::<Time<Fixed>>()
        .init_resource::<MissileMap>()
        .add_systems(Update, missiles_transform_sync_system);
    let movement = sample(8.0);
    let entity = app
        .world_mut()
        .spawn((
            MissileMarker,
            MissileId(1),
            movement.pos,
            PreviousTickPosition(movement.pos),
            MissileVelocity(movement.velocity()),
            Transform::default(),
        ))
        .id();
    app.world_mut().resource_mut::<MissileMap>().entries.insert(
        MissileId(1),
        MissileInfo {
            entity,
            shooter: PlayerId(2),
            born_tick: 0,
            remote: Some(RemoteMissileMotion::new(0, movement, 0.0)),
        },
    );
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        movement.pos
    );
    assert_eq!(
        app.world()
            .get::<Transform>(entity)
            .expect("transform missing")
            .translation,
        Vec3::from(movement.pos)
    );
}

#[test]
fn detonated_missiles_stay_retired_until_snapshots_have_passed_their_death() {
    let mut missiles = MissileMap::default();
    assert!(missiles.retire(MissileId(1), u32::MAX));
    assert!(!missiles.retire(MissileId(1), u32::MAX));
    for tick in [u32::MAX - 1, u32::MAX] {
        missiles.discard_retired_before(tick);
        assert!(missiles.is_retired(&MissileId(1)));
    }
    missiles.discard_retired_before(0);
    assert!(!missiles.is_retired(&MissileId(1)));
}
