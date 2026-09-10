use super::{MissileImpact, MissileVelocity, RemoteMissileMotion, interpolate_remote_missiles_system};
use crate::{
    config::ClientSettings,
    missiles::{MissileInfo, MissileMap, MissileMarker},
    network::SampleTiming,
};
use bevy::prelude::*;
use common::{
    config::{NetworkConfig, UpdateCadence},
    math::angle_delta_radians,
    protocol::*,
};
use std::{f32::consts::PI, time::Duration};

fn sample(x: f32) -> MissileMovementState {
    MissileMovementState::from_velocity(Position { x, ..default() }, Vec3::X * 20.0)
}

fn immediate() -> SampleTiming {
    SampleTiming {
        delay_ticks: 0.0,
        interval_ticks: 1.0,
    }
}

#[test]
fn repeated_snapshots_and_reordered_updates_cannot_restart_or_reverse_flight() {
    let mut start = sample(0.0);
    start.yaw = 3.0;
    let mut end = sample(2.0);
    end.yaw = -3.0;
    let mut motion = RemoteMissileMotion::new(u32::MAX - 1, start, immediate());
    assert!(motion.push(1, end));
    assert!(!motion.push(u32::MAX, sample(-50.0)));
    assert!(!motion.push(1, sample(50.0)));
    let mut previous = 0.0;
    let mut blended = false;
    for _ in 0..12 {
        let shown = motion.advance(0.5);
        assert!(shown.pos.x >= previous && shown.pos.x <= 2.0);
        assert!(shown.yaw.abs() >= 3.0 - 1e-5, "yaw turned the long way round");
        if shown.pos.x > 0.0 && shown.pos.x < 2.0 {
            blended = true;
            assert!(angle_delta_radians(shown.yaw, PI).abs() < 0.2);
        }
        previous = shown.pos.x;
    }
    assert!(blended);
    assert_eq!(motion.advance(1.0), end);
    assert_eq!(motion.advance(1000.0), end);
}

#[test]
fn configured_buffer_smooths_flight_and_holds_after_packet_gaps() {
    let settings = ClientSettings::load_default().expect("client config invalid");
    for hz in [7, 10, 30] {
        let mut motion = RemoteMissileMotion::new(
            0,
            sample(0.0),
            SampleTiming::new(
                &settings.interpolation,
                &NetworkConfig {
                    update_hz: hz,
                    ..Default::default()
                },
            ),
        );
        let mut cadence = UpdateCadence::new(hz, 30);
        cadence.ready();
        let mut previous = 0.0;
        for tick in 1..240 {
            if cadence.ready() && !(30..60).contains(&tick) {
                motion.push(tick, sample((tick as f32 / 30.0 * 20.0).min(60.0)));
            }
            for _ in 0..4 {
                let x = motion.advance(0.25).pos.x;
                assert!(
                    x >= previous - 1e-4 && x <= 60.0 + 1e-4,
                    "{hz} Hz tick {tick}: {previous} -> {x}"
                );
                previous = x;
            }
        }
        assert!((previous - 60.0).abs() < 1e-4);
    }
}

#[test]
fn a_reported_impact_is_flown_to_at_the_last_speed_and_then_marked() {
    let mut app = App::new();
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f64(1.0 / 30.0));
    app.insert_resource(time)
        .insert_resource(Time::<Fixed>::from_hz(30.0))
        .add_systems(Update, interpolate_remote_missiles_system);
    let movement = sample(8.0);
    let entity = app
        .world_mut()
        .spawn((
            MissileMarker,
            MissileId(1),
            movement.pos,
            MissileVelocity(movement.velocity()),
            RemoteMissileMotion::new(
                0,
                movement,
                SampleTiming {
                    delay_ticks: 2.0,
                    interval_ticks: 1.0,
                },
            ),
        ))
        .id();
    for _ in 0..120 {
        app.update();
    }
    assert_eq!(
        *app.world().get::<Position>(entity).expect("position missing"),
        movement.pos
    );
    let impact = Position {
        x: 12.0,
        ..movement.pos
    };
    app.world_mut()
        .get_mut::<RemoteMissileMotion>(entity)
        .expect("missile buffer missing")
        .detonate_at(impact, 1.0 / 30.0);
    let mut previous = 8.0;
    let mut frames = 0;
    while app.world().get::<MissileImpact>(entity).is_none() {
        app.update();
        let x = app.world().get::<Position>(entity).expect("position missing").x;
        assert!(x >= previous && x <= 12.0 + 1e-4);
        previous = x;
        frames += 1;
        assert!(frames <= 12, "the flight to the impact took too long");
    }
    assert!(frames >= 4, "the flight to the impact was cut short");
    assert!((previous - 12.0).abs() < 1e-4);
    assert_eq!(
        app.world()
            .get::<MissileImpact>(entity)
            .expect("impact marker missing")
            .0,
        impact
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

#[test]
fn a_flight_ended_here_consumes_its_own_detonation_cue_once() {
    let mut missiles = MissileMap::default();
    missiles.insert(
        MissileId(4),
        MissileInfo {
            entity: Entity::PLACEHOLDER,
            shooter: PlayerId(1),
            born_tick: 3,
            impact_pending: false,
        },
    );
    assert!(missiles.remove(&MissileId(4)).is_some());
    assert!(!missiles.retire(MissileId(4), 9));
    assert!(missiles.is_retired(&MissileId(4)));
    assert!(!missiles.retire(MissileId(4), 9));
}
