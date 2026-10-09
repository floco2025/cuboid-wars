use super::{MissileImpact, MissileVelocity, RemoteMissileMotion, interpolate_remote_missiles_system};
use crate::{missiles::MissileMarker, network::SampleTiming};
use bevy::prelude::*;
use common::{math::angle_delta_radians, protocol::*};
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
