use super::*;
use crate::config::{KnockbackConfig, PlayerMovementConfig};
use std::collections::HashMap;

fn map_movement() -> MapMovementConfig {
    MapMovementConfig {
        player: PlayerMovementConfig {
            move_speed: 4.0,
            move_speed_power_up: 1.5,
            move_speed_ladder: 0.4,
            jump_speed: 12.0,
            ground_acceleration: 40.0,
            ground_deceleration: 16.0,
            ground_lateral_deceleration: 40.0,
            air_acceleration: 20.0,
            air_deceleration: 0.0,
            air_lateral_deceleration: 0.0,
        },
        actors: HashMap::new(),
        missile_speed: 16.0,
        projectile_speed: 90.0,
        gravity: 25.0,
        low_gravity: 5.0,
        knockback: KnockbackConfig {
            max_speed: 15.0,
            up_speed: 7.0,
            deceleration: 35.0,
        },
    }
}

#[test]
fn disabled_player_has_no_control_velocity() {
    let movement = map_movement();
    let intent = PlayerMoveIntent::moving(0.0);

    assert_eq!(player_control_velocity(intent, &movement, true, true), Vec3::ZERO);
}

#[test]
fn enabled_player_uses_configured_speed() {
    let movement = map_movement();
    let intent = PlayerMoveIntent::moving(0.0);

    assert_eq!(
        player_control_velocity(intent, &movement, false, false),
        Vec3::Z * movement.player.move_speed
    );
}

#[test]
fn slow_ground_acceleration_reaches_full_speed_independently_of_fast_stopping() {
    let mut cfg = map_movement().player;
    cfg.move_speed = 9.0;
    cfg.ground_acceleration = 9.0;
    for hz in [30, 60, 120] {
        for deceleration in [90.0, 900.0] {
            cfg.ground_deceleration = deceleration;
            let dt = 1.0 / hz as f32;
            let wish = Vec3::Z * cfg.move_speed;
            let mut velocity = Vec3::ZERO;
            for tick in 1..=hz * 2 {
                velocity = accelerate_player(velocity, Vec3::ZERO, wish, 0.0, true, &cfg, dt);
                let expected = (9.0 * tick as f32 / hz as f32).min(9.0);
                assert!((velocity.z - expected).abs() < 1e-4, "{hz} Hz: {velocity:?}");
            }
            for _ in 0..hz / 10 {
                velocity = accelerate_player(velocity, Vec3::ZERO, Vec3::ZERO, 0.0, true, &cfg, dt);
            }
            assert_eq!(velocity, Vec3::ZERO);
        }
    }
}

#[test]
fn turning_grip_removes_sideways_slip_without_changing_forward_acceleration() {
    let mut cfg = map_movement().player;
    cfg.move_speed = 9.0;
    cfg.ground_acceleration = 9.0;
    cfg.ground_deceleration = 90.0;
    for heading in 0..8 {
        let yaw = heading as f32 * std::f32::consts::FRAC_PI_4;
        let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let side = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        for grip in [9.0, 90.0] {
            cfg.ground_lateral_deceleration = grip;
            let turned = accelerate_player(side * 9.0, Vec3::ZERO, forward * 9.0, 0.0, true, &cfg, 0.1);
            assert!((turned.dot(forward) - 0.9).abs() < 1e-5);
            assert!((turned.dot(side) - (9.0 - grip * 0.1)).abs() < 1e-5);
        }
    }
}

#[test]
fn reversing_brakes_before_starting_slowly_in_the_new_direction() {
    let mut cfg = map_movement().player;
    cfg.ground_acceleration = 9.0;
    cfg.ground_deceleration = 90.0;
    let wish = Vec3::Z * 9.0;
    let start = Vec3::NEG_Z * 9.0;
    let stopped = accelerate_player(start, Vec3::ZERO, wish, 0.0, true, &cfg, 0.1);
    assert!(stopped.length() < 1e-5);
    let reversed = accelerate_player(start, Vec3::ZERO, wish, 0.0, true, &cfg, 0.15);
    assert!((reversed.z - 0.45).abs() < 1e-5);
    let mut stepped = start;
    for _ in 0..3 {
        stepped = accelerate_player(stepped, Vec3::ZERO, wish, 0.0, true, &cfg, 0.05);
    }
    assert!((stepped - reversed).length() < 1e-5);
}

#[test]
fn grounding_and_lowering_the_speed_target_brake_without_instantly_erasing_momentum() {
    let mut cfg = map_movement().player;
    cfg.move_speed = 9.0;
    cfg.ground_deceleration = 45.0;
    for (speed, target) in [(20.0, 9.0), (13.5, 9.0), (9.0, 3.0)] {
        let wish = Vec3::Z * target;
        let first = accelerate_player(Vec3::Z * speed, Vec3::ZERO, wish, 0.0, true, &cfg, 1.0 / 30.0);
        assert!((first.z - (speed - 1.5)).abs() < 1e-5);
        let mut velocity = first;
        for _ in 0..30 {
            velocity = accelerate_player(velocity, Vec3::ZERO, wish, 0.0, true, &cfg, 1.0 / 30.0);
        }
        assert_eq!(velocity, wish);
    }
}

#[test]
fn a_fast_forward_fling_is_unchanged_by_forward_input_or_release() {
    let cfg = map_movement().player;
    let launch = Vec3::Z * 20.0;
    for wish in [
        Vec3::ZERO,
        Vec3::Z * cfg.move_speed,
        Vec3::Z * cfg.move_speed * cfg.move_speed_power_up,
    ] {
        assert_eq!(
            accelerate_player(launch, Vec3::ZERO, wish, -10.0, false, &cfg, 1.0 / 30.0),
            launch
        );
    }
}

#[test]
fn air_strafing_gains_speed_but_fixed_sideways_input_saturates() {
    let cfg = map_movement().player;
    let mut velocity = Vec3::Z * 4.0;
    for _ in 0..120 {
        velocity = accelerate_player(velocity, Vec3::ZERO, Vec3::X * 4.0, -2.0, false, &cfg, 1.0 / 60.0);
    }
    assert!((velocity.x - cfg.move_speed).abs() < 1e-5);
    assert_eq!(velocity.z, 4.0);
    let before = velocity.length();
    let perpendicular = Vec3::new(velocity.z, 0.0, -velocity.x).normalize() * 4.0;
    velocity = accelerate_player(velocity, Vec3::ZERO, perpendicular, -2.0, false, &cfg, 1.0 / 60.0);
    assert!(velocity.length() > before);
}

#[test]
fn air_steering_and_braking_work_at_every_heading_above_normal_speed() {
    let mut cfg = map_movement().player;
    cfg.air_acceleration = 45.0;
    cfg.move_speed = 9.0;
    for heading in 0..8 {
        let yaw = heading as f32 * std::f32::consts::FRAC_PI_4;
        let forward = Vec3::new(yaw.sin(), 0.0, yaw.cos());
        let sideways = Vec3::new(yaw.cos(), 0.0, -yaw.sin());
        let launch = forward * 20.0;
        let strafed = accelerate_player(
            launch,
            Vec3::ZERO,
            sideways * cfg.move_speed,
            -1.0,
            false,
            &cfg,
            1.0 / 30.0,
        );
        assert!((strafed.dot(forward) - 20.0).abs() < 1e-5, "{yaw}: {strafed:?}");
        assert!((strafed.dot(sideways) - 1.5).abs() < 1e-5, "{yaw}: {strafed:?}");
        let braked = accelerate_player(
            launch,
            Vec3::ZERO,
            -forward * cfg.move_speed,
            -1.0,
            false,
            &cfg,
            1.0 / 30.0,
        );
        assert!((braked - forward * 18.5).length() < 1e-5, "{yaw}: {braked:?}");
    }
}

#[test]
fn upward_apex_control_is_weaker_and_input_normalization_has_no_diagonal_bonus() {
    let cfg = map_movement().player;
    let rising = accelerate_player(Vec3::ZERO, Vec3::ZERO, Vec3::X * 4.0, 1.0, false, &cfg, 0.01);
    let falling = accelerate_player(Vec3::ZERO, Vec3::ZERO, Vec3::X * 4.0, -1.0, false, &cfg, 0.01);
    assert!((falling.x - rising.x * 4.0).abs() < 1e-6);
    let input = PlayerMoveIntent {
        forward: 1.0,
        sideways: 1.0,
        ..PlayerMoveIntent::NONE
    };
    assert!((input.wish_velocity(4.0, false).length() - 4.0).abs() < 1e-6);
}

#[test]
fn equal_ground_and_air_rates_have_equal_strength_independent_of_target_speed() {
    let mut cfg = map_movement().player;
    cfg.ground_acceleration = 20.0;
    cfg.air_acceleration = 20.0;
    cfg.ground_deceleration = 30.0;
    cfg.air_deceleration = 30.0;
    cfg.ground_lateral_deceleration = 40.0;
    cfg.air_lateral_deceleration = 40.0;
    for speed in [4.0, 9.0, 13.5] {
        cfg.move_speed = speed;
        for grounded in [true, false] {
            let wish = Vec3::Z * speed;
            let started = accelerate_player(Vec3::ZERO, Vec3::ZERO, wish, -2.0, grounded, &cfg, 0.05);
            assert!((started.z - 1.0).abs() < 1e-5);
            let released = accelerate_player(Vec3::Z * 9.0, Vec3::ZERO, Vec3::ZERO, -2.0, grounded, &cfg, 0.05);
            assert!((released.z - 7.5).abs() < 1e-5);
            let turned = accelerate_player(Vec3::X * 9.0, Vec3::ZERO, wish, -2.0, grounded, &cfg, 0.05);
            assert!((turned - Vec3::new(7.0, 0.0, 1.0)).length() < 1e-5);
        }
    }
}

#[test]
fn air_stopping_and_sideways_braking_are_independent_and_do_not_fight_acceleration() {
    let mut cfg = map_movement().player;
    cfg.move_speed = 9.0;
    cfg.air_acceleration = 20.0;
    for lateral in [0.0, 40.0] {
        cfg.air_lateral_deceleration = lateral;
        for stopping in [0.0, 30.0, 5000.0] {
            cfg.air_deceleration = stopping;
            let moved = accelerate_player(Vec3::X * 9.0, Vec3::ZERO, Vec3::Z * 9.0, -1.0, false, &cfg, 0.1);
            assert!((moved.z - 2.0).abs() < 1e-5);
            assert!((moved.x - (9.0 - lateral * 0.1)).abs() < 1e-5);
            let released = accelerate_player(Vec3::X * 9.0, Vec3::ZERO, Vec3::ZERO, -1.0, false, &cfg, 0.1);
            assert!((released.x - (9.0 - stopping * 0.1).max(0.0)).abs() < 1e-5);
        }
    }
}

#[test]
fn air_release_stops_exactly_and_reversal_does_not_restore_cancelled_speed() {
    let mut cfg = map_movement().player;
    cfg.move_speed = 9.0;
    cfg.air_deceleration = 30.0;
    let launch = Vec3::new(5.4, 0.0, 7.2);
    for hz in [30, 60, 120] {
        let mut velocity = launch;
        for _ in 0..hz / 2 {
            velocity = accelerate_player(velocity, Vec3::ZERO, Vec3::ZERO, -2.0, false, &cfg, 1.0 / hz as f32);
        }
        assert_eq!(velocity, Vec3::ZERO);
    }
    let braked = accelerate_player(Vec3::Z * 9.0, Vec3::ZERO, Vec3::NEG_Z * 9.0, -2.0, false, &cfg, 0.3);
    assert!(braked.length() < 1e-5);
    let reversed = accelerate_player(Vec3::Z * 9.0, Vec3::ZERO, Vec3::NEG_Z * 9.0, -2.0, false, &cfg, 0.4);
    assert!((reversed.z + 2.0).abs() < 1e-5);
    let released = accelerate_player(reversed, Vec3::ZERO, Vec3::ZERO, -2.0, false, &cfg, 0.2);
    assert_eq!(released, Vec3::ZERO);
}

#[test]
fn zero_air_rates_preserve_launch_velocity_under_every_input() {
    let mut cfg = map_movement().player;
    cfg.air_acceleration = 0.0;
    cfg.air_deceleration = 0.0;
    cfg.air_lateral_deceleration = 0.0;
    for launch in [Vec3::ZERO, Vec3::new(8.7, 0.0, -23.4)] {
        for wish in [
            Vec3::ZERO,
            Vec3::Z * 9.0,
            Vec3::NEG_Z * 9.0,
            Vec3::X * 9.0,
            Vec3::new(3.0, 0.0, 4.0),
        ] {
            assert_eq!(
                accelerate_player(launch, Vec3::ZERO, wish, 2.0, false, &cfg, 0.1),
                launch
            );
        }
    }
}

#[test]
fn air_acceleration_respects_blast_speed_without_passive_braking_storing_a_counter_impulse() {
    let mut cfg = map_movement().player;
    cfg.air_deceleration = 30.0;
    cfg.air_lateral_deceleration = 40.0;
    let blast = Vec3::Z * 20.0;
    for wish in [Vec3::ZERO, Vec3::Z * cfg.move_speed] {
        assert_eq!(
            accelerate_player(Vec3::ZERO, blast, wish, -2.0, false, &cfg, 0.1),
            Vec3::ZERO
        );
    }
    let strafed = accelerate_player(Vec3::ZERO, blast, Vec3::X * cfg.move_speed, -2.0, false, &cfg, 0.1);
    assert_eq!(strafed, Vec3::X * 2.0);
}
