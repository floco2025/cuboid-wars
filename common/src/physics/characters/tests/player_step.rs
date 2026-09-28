use super::fixtures::*;
use crate::{
    celestial::{CelestialMapSettings, LocalTime, Season},
    config::gameplay::load_test_gameplay,
    config::{KnockbackConfig, MapGeometryConfig, MapMovementConfig, PlayerMovementConfig},
    physics::{PlayerMovementStep, PortalSet, step_player_movement},
    protocol::{MapSettings, PlayerMoveIntent, PlayerStance, PortalMode},
};
use std::collections::HashMap;

pub(super) fn map_settings() -> MapSettings {
    MapSettings {
        grounds: None,
        celestial: CelestialMapSettings {
            latitude_degrees: 40.0,
            season: Season::Summer,
            north_yaw_degrees: 0.0,
            start_local_time: LocalTime::parse("09:00").expect("valid fixture time"),
            start_moon_phase: 0.25,
        },
        textures: Default::default(),

        geometry: MapGeometryConfig {
            grid_cell_size: 2.0,
            level_height: 2.0,
            floor_thickness: 0.2,
            wall_thickness: 0.2,
        },
        movement: MapMovementConfig {
            player: PlayerMovementConfig {
                move_speed: 4.375,
                move_speed_power_up: 1.5,
                jump_speed: 5.809475,
                ground_acceleration: 43.75,
                ground_deceleration: 17.5,
                ground_lateral_deceleration: 43.75,
                air_acceleration: 21.875,
                air_deceleration: 0.0,
                air_lateral_deceleration: 0.0,
            },
            actors: HashMap::new(),
            missile_speed: 20.0,
            projectile_speed: 30.0,
            gravity: 15.0,
            low_gravity: 5.0,
            ladder_climb_ratio: 0.6,
            knockback: KnockbackConfig {
                max_speed: 10.0,
                up_speed: 4.0,
                deceleration: 12.0,
            },
        },
        portals: PortalMode::Both,
        switches: Vec::new(),
        fields: Vec::new(),
    }
}

fn simulate(
    layout: MapLayout,
    start: Position,
    velocity: Vec3,
    intent: PlayerMoveIntent,
    stance: PlayerStance,
    ticks: usize,
) -> (Position, Vec3, PlayerStance) {
    let world = CollisionWorld::from_map_layout(&layout);
    let gameplay = load_test_gameplay().expect("gameplay");
    let settings = map_settings();
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    let mut pos = start;
    let mut velocity = velocity;
    let mut stance = stance;
    for _ in 0..ticks {
        let step = step_player_movement(PlayerMovementStep {
            start: pos,
            vertical_velocity: velocity.y,
            horizontal_velocity: velocity.with_y(0.0),
            stance,
            intent,
            has_speed: false,
            disabled: false,
            delta: 1.0 / 60.0,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            external_displacement: Vec3::ZERO,
            collision_world: &world,
            map_settings: &settings,
            gameplay_config: &gameplay,
            portal_set: &portals,
            carriers: &carriers,
        });
        pos = step.movement.position;
        velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
        stance = step.stance;
    }
    (pos, velocity, stance)
}

#[test]
fn jump_matches_reference_arc_and_retains_takeoff_motion() {
    let start = Position { x: 0.0, y: 0.0, z: 0.0 };
    let velocity = Vec3::new(4.375, 5.809475, 0.0);
    let (pos, v, _) = simulate(
        MapLayout::default(),
        start,
        velocity,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        23,
    );
    assert!((pos.y - 1.125).abs() < 0.01, "apex {pos:?}");
    assert!((pos.x - 4.375 * 23.0 / 60.0).abs() < 0.001);
    assert_eq!(v.x, velocity.x);
}

#[test]
fn a_released_jump_and_forward_held_jump_follow_the_same_arc() {
    let start = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let launch = Vec3::new(0.0, 5.809475, 4.375);
    let released = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        45,
    );
    let held = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::moving(0.0),
        PlayerStance::default(),
        45,
    );
    assert_eq!(released, held);
}

#[test]
fn a_stationary_jump_can_accelerate_to_movement_speed_and_coast_after_release() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![lower_floor()],
        ..Default::default()
    });
    let gameplay = load_test_gameplay().expect("gameplay");
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    let mut settings = map_settings();
    settings.movement.gravity = 25.0;
    settings.movement.player.jump_speed = 12.0;
    settings.movement.player.air_acceleration = 45.0;
    for speed in [6.0, 9.0] {
        settings.movement.player.move_speed = speed;
        for boosted in [false, true] {
            for yaw in [0.0, std::f32::consts::FRAC_PI_4] {
                let target_speed = speed
                    * if boosted {
                        settings.movement.player.move_speed_power_up
                    } else {
                        1.0
                    };
                let direction = Vec3::new(yaw.sin(), 0.0, yaw.cos());
                let mut pos = Position::default();
                let mut velocity = Vec3::Y * settings.movement.player.jump_speed;
                for tick in 0..28 {
                    // Jump vertically first, hold W while airborne, then release before landing.
                    let intent = if (6..26).contains(&tick) {
                        PlayerMoveIntent::moving(yaw)
                    } else {
                        PlayerMoveIntent::NONE
                    };
                    let before = pos;
                    let step = step_player_movement(PlayerMovementStep {
                        start: pos,
                        vertical_velocity: velocity.y,
                        horizontal_velocity: velocity.with_y(0.0),
                        stance: PlayerStance::default(),
                        intent,
                        has_speed: boosted,
                        disabled: false,
                        delta: 1.0 / 30.0,
                        has_low_gravity: false,
                        held_keys: &[],
                        open_fields: &[],
                        external_displacement: Vec3::ZERO,
                        collision_world: &world,
                        map_settings: &settings,
                        gameplay_config: &gameplay,
                        portal_set: &portals,
                        carriers: &carriers,
                    });
                    pos = step.movement.position;
                    velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
                    assert_eq!(step.movement.support, CharacterSupport::Airborne);
                    if tick < 6 {
                        assert_eq!(velocity.with_y(0.0), Vec3::ZERO);
                    } else if tick >= 26 {
                        let travelled = (Vec3::from(pos) - Vec3::from(before)).dot(direction);
                        assert!((travelled - target_speed / 30.0).abs() < 1e-4);
                    }
                }
                assert!((velocity.with_y(0.0) - direction * target_speed).length() < 1e-4);
                assert!(Vec3::from(pos).dot(direction) > target_speed * 0.3);
            }
        }
    }
}

#[test]
fn crouched_body_cannot_stand_inside_a_low_ceiling() {
    let mut ceiling = lower_floor();
    ceiling.y = 1.4;
    let layout = MapLayout {
        floors: vec![lower_floor(), ceiling],
        ..Default::default()
    };
    let start = Position::default();
    let crouched = PlayerStance {
        crouched: true,
        fraction: 1.0,
    };
    let (_, _, blocked) = simulate(layout, start, Vec3::ZERO, PlayerMoveIntent::NONE, crouched, 30);
    assert!(blocked.crouched);
    let (_, _, clear) = simulate(
        MapLayout {
            floors: vec![lower_floor()],
            ..Default::default()
        },
        start,
        Vec3::ZERO,
        PlayerMoveIntent::NONE,
        crouched,
        30,
    );
    assert!(!clear.crouched);
    assert_eq!(clear.fraction, 0.0);
}

#[test]
fn crouch_walk_uses_a_third_of_normal_speed_and_release_brakes_on_ground() {
    let floor = Floor {
        x1: -100.0,
        z1: -100.0,
        x2: 100.0,
        z2: 100.0,
        ..lower_floor()
    };
    let layout = MapLayout {
        floors: vec![floor],
        ..Default::default()
    };
    let input = PlayerMoveIntent {
        crouch: true,
        ..PlayerMoveIntent::moving(0.0)
    };
    let (pos, v, stance) = simulate(
        layout.clone(),
        Position::default(),
        Vec3::ZERO,
        input,
        PlayerStance::default(),
        60,
    );
    assert!(stance.crouched);
    assert!((v.z - 4.375 / 3.0).abs() < 0.001, "velocity {v:?}");
    let (_, v, _) = simulate(layout, pos, v, PlayerMoveIntent::NONE, stance, 60);
    assert!(v.with_y(0.0).length() < 1e-5);
}

#[test]
fn airborne_duck_preserves_body_centre_and_launch_velocity() {
    let start = Position {
        x: 0.0,
        y: 10.0,
        z: 0.0,
    };
    let launch = Vec3::new(4.0, -2.0, 3.0);
    let standing = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        1,
    );
    let ducked = simulate(
        MapLayout::default(),
        start,
        launch,
        PlayerMoveIntent {
            crouch: true,
            ..PlayerMoveIntent::NONE
        },
        PlayerStance::default(),
        1,
    );
    assert!((ducked.0.y - standing.0.y - 0.45).abs() < 1e-5);
    assert_eq!(ducked.1, standing.1);
    assert!(ducked.2.crouched);
    let unducked = simulate(
        MapLayout::default(),
        ducked.0,
        ducked.1,
        PlayerMoveIntent::NONE,
        ducked.2,
        1,
    );
    let continued = simulate(
        MapLayout::default(),
        standing.0,
        standing.1,
        PlayerMoveIntent::NONE,
        standing.2,
        1,
    );
    assert!((Vec3::from(unducked.0) - Vec3::from(continued.0)).length() < 1e-5);
}

#[test]
fn glancing_wall_contact_keeps_tangential_velocity_and_removes_inward_velocity() {
    let wall = Wall {
        z1: -100.0,
        z2: 100.0,
        height: 100.0,
        ..test_wall()
    };
    let layout = MapLayout {
        walls: vec![wall],
        ..Default::default()
    };
    let (pos, v, _) = simulate(
        layout,
        Position {
            x: -1.0,
            y: 10.0,
            z: 0.0,
        },
        Vec3::new(8.0, 0.0, 4.0),
        PlayerMoveIntent::NONE,
        PlayerStance::default(),
        15,
    );
    assert!(pos.x < -0.39, "wall penetration: {pos:?}");
    assert!(v.x.abs() < 0.01, "inward velocity {v:?}");
    assert!((v.z - 4.0).abs() < 0.01, "lost slide velocity {v:?}");
}

#[test]
fn player_steps_over_a_low_stair_without_jumping() {
    let layout = MapLayout {
        floors: vec![
            Floor {
                x1: -5.0,
                x2: 0.0,
                z1: -5.0,
                z2: 5.0,
                ..lower_floor()
            },
            Floor {
                x1: 0.0,
                x2: 5.0,
                z1: -5.0,
                z2: 5.0,
                y: 0.15,
                thickness: 0.15,
                ..lower_floor()
            },
        ],
        ..Default::default()
    };
    let (pos, _, _) = simulate(
        layout,
        Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        },
        Vec3::ZERO,
        PlayerMoveIntent::moving(std::f32::consts::FRAC_PI_2),
        PlayerStance::default(),
        40,
    );
    assert!(pos.x > 1.0 && (pos.y - 0.15).abs() < 0.02, "stair {pos:?}");
}

#[test]
fn equipment_changes_affect_acceleration_and_gravity_without_erasing_air_velocity() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let gameplay = load_test_gameplay().expect("gameplay");
    let settings = map_settings();
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    let request = PlayerMovementStep {
        start: Position {
            x: 0.0,
            y: 20.0,
            z: 0.0,
        },
        vertical_velocity: -10.0,
        horizontal_velocity: Vec3::Z * 20.0,
        stance: PlayerStance::default(),
        intent: PlayerMoveIntent::moving(0.0),
        has_speed: true,
        disabled: false,
        delta: 1.0 / 30.0,
        has_low_gravity: true,
        held_keys: &[],
        open_fields: &[],
        external_displacement: Vec3::ZERO,
        collision_world: &world,
        map_settings: &settings,
        gameplay_config: &gameplay,
        portal_set: &portals,
        carriers: &carriers,
    };
    let boosted = step_player_movement(request);
    let erased = step_player_movement(PlayerMovementStep {
        has_speed: false,
        has_low_gravity: false,
        ..request
    });
    assert_eq!(boosted.horizontal_velocity, erased.horizontal_velocity);
    assert_eq!(erased.horizontal_velocity, Vec3::Z * 20.0);
    assert!((boosted.movement.vertical_velocity - (-10.0 - 5.0 / 30.0)).abs() < 1e-5);
    assert!((erased.movement.vertical_velocity - (-10.0 - 15.0 / 30.0)).abs() < 1e-5);
}

#[test]
fn passive_ground_and_air_braking_do_not_leave_reverse_velocity_after_a_blast() {
    let world = CollisionWorld::from_map_layout(&MapLayout {
        floors: vec![lower_floor()],
        ..Default::default()
    });
    let gameplay = load_test_gameplay().expect("gameplay");
    let mut settings = map_settings();
    settings.movement.player.air_deceleration = 30.0;
    settings.movement.player.air_lateral_deceleration = 40.0;
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    for height in [0.0, 10.0] {
        let request = PlayerMovementStep {
            start: Position {
                y: height,
                ..Position::default()
            },
            vertical_velocity: 0.0,
            horizontal_velocity: Vec3::ZERO,
            stance: PlayerStance::default(),
            intent: PlayerMoveIntent::NONE,
            has_speed: false,
            disabled: false,
            delta: 1.0 / 30.0,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            external_displacement: Vec3::X / 3.0,
            collision_world: &world,
            map_settings: &settings,
            gameplay_config: &gameplay,
            portal_set: &portals,
            carriers: &carriers,
        };
        let result = step_player_movement(request);
        assert!(result.movement.position.x > 0.3);
        assert_eq!(result.horizontal_velocity, Vec3::ZERO);
        let after = step_player_movement(PlayerMovementStep {
            start: result.movement.position,
            vertical_velocity: result.movement.vertical_velocity,
            external_displacement: Vec3::ZERO,
            ..request
        });
        assert_eq!(after.horizontal_velocity, Vec3::ZERO);
        assert_eq!(after.movement.position.x, result.movement.position.x);
    }
}

#[test]
fn air_rates_are_independent_of_speed_pickups_and_leave_vertical_gravity_unchanged() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let gameplay = load_test_gameplay().expect("gameplay");
    let mut settings = map_settings();
    settings.movement.player.air_acceleration = 20.0;
    settings.movement.player.air_deceleration = 30.0;
    settings.movement.player.air_lateral_deceleration = 40.0;
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    for has_speed in [false, true] {
        for has_low_gravity in [false, true] {
            let request = PlayerMovementStep {
                start: Position {
                    y: 10.0,
                    ..Position::default()
                },
                vertical_velocity: 0.0,
                horizontal_velocity: Vec3::Z * 9.0,
                stance: PlayerStance::default(),
                intent: PlayerMoveIntent::NONE,
                has_speed,
                disabled: false,
                delta: 0.1,
                has_low_gravity,
                held_keys: &[],
                open_fields: &[],
                external_displacement: Vec3::ZERO,
                collision_world: &world,
                map_settings: &settings,
                gameplay_config: &gameplay,
                portal_set: &portals,
                carriers: &carriers,
            };
            let released = step_player_movement(request);
            let turned = step_player_movement(PlayerMovementStep {
                intent: PlayerMoveIntent {
                    sideways: 1.0,
                    ..PlayerMoveIntent::NONE
                },
                ..request
            });
            assert!((released.horizontal_velocity - Vec3::Z * 6.0).length() < 1e-5);
            assert!((turned.horizontal_velocity - Vec3::new(2.0, 0.0, 5.0)).length() < 1e-5);
            assert_eq!(released.movement.vertical_velocity, turned.movement.vertical_velocity);
            assert_eq!(released.movement.position.y, turned.movement.position.y);
            let gravity = settings.gravity_for(has_low_gravity);
            assert!((released.movement.vertical_velocity + gravity * 0.1).abs() < 1e-5);
        }
    }
}

#[test]
fn a_rising_carrier_uses_ground_acceleration_and_inherits_velocity_once_on_jump() {
    let (mut carrier, floor) = slider();
    carrier.to = Position { x: 0.0, y: 4.0, z: 0.0 };
    let layout = MapLayout {
        carriers: vec![carrier],
        floors: vec![floor],
        ..Default::default()
    };
    let (world, carriers) = world_at(&layout, 10);
    let gameplay = load_test_gameplay().expect("gameplay");
    let settings = map_settings();
    let portals = PortalSet::default();
    let request = PlayerMovementStep {
        start: Position {
            x: 0.0,
            y: 4.0 * 9.0 / 60.0,
            z: 0.0,
        },
        vertical_velocity: 0.0,
        horizontal_velocity: Vec3::ZERO,
        stance: PlayerStance::default(),
        intent: PlayerMoveIntent::moving(0.0),
        has_speed: false,
        disabled: false,
        delta: 1.0 / 30.0,
        has_low_gravity: false,
        held_keys: &[],
        open_fields: &[],
        external_displacement: Vec3::ZERO,
        collision_world: &world,
        map_settings: &settings,
        gameplay_config: &gameplay,
        portal_set: &portals,
        carriers: &carriers,
    };
    let grounded = step_player_movement(request);
    assert_eq!(grounded.movement.support, CharacterSupport::Ground);
    assert!((grounded.horizontal_velocity.z - 4.375 / 3.0).abs() < 1e-4);
    let jumped = step_player_movement(PlayerMovementStep {
        vertical_velocity: settings.movement.player.jump_speed,
        ..request
    });
    assert!(
        (jumped.movement.vertical_velocity
            - (settings.movement.player.jump_speed + 2.0 - settings.movement.gravity / 30.0))
            .abs()
            < 1e-4
    );
    let (world, carriers) = world_at(&layout, 11);
    let next = step_player_movement(PlayerMovementStep {
        start: jumped.movement.position,
        vertical_velocity: jumped.movement.vertical_velocity,
        horizontal_velocity: jumped.horizontal_velocity,
        collision_world: &world,
        carriers: &carriers,
        ..request
    });
    assert!(
        (next.movement.vertical_velocity - (jumped.movement.vertical_velocity - settings.movement.gravity / 30.0))
            .abs()
            < 1e-4
    );
}
