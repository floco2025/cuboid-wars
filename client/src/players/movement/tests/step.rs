use super::super::fixtures::*;
use common::protocol::Wall;

fn simulate(
    layout: MapLayout,
    start: Position,
    velocity: Vec3,
    intent: PlayerMoveIntent,
    stance: PlayerStance,
    ticks: usize,
) -> (Position, Vec3, PlayerStance) {
    let world = CollisionWorld::from_map_layout(&layout);
    let gameplay = gameplay_config();
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
            knockback_displacement: Vec3::ZERO,
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
    let gameplay = gameplay_config();
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
                        knockback_displacement: Vec3::ZERO,
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
    let crouched = PlayerStance { crouched: true };
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
    let gameplay = gameplay_config();
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
        knockback_displacement: Vec3::ZERO,
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
    let gameplay = gameplay_config();
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
            knockback_displacement: Vec3::X / 3.0,
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
            knockback_displacement: Vec3::ZERO,
            ..request
        });
        assert_eq!(after.horizontal_velocity, Vec3::ZERO);
        assert_eq!(after.movement.position.x, result.movement.position.x);
    }
}

#[test]
fn air_rates_are_independent_of_speed_pickups_and_leave_vertical_gravity_unchanged() {
    let world = CollisionWorld::from_map_layout(&MapLayout::default());
    let gameplay = gameplay_config();
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
                knockback_displacement: Vec3::ZERO,
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
    let gameplay = gameplay_config();
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
        knockback_displacement: Vec3::ZERO,
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

fn ladder_layout() -> MapLayout {
    MapLayout {
        floors: vec![ladder_front_base_floor(), ladder_back_landing_floor()],
        ladders: vec![test_ladder()],
        ..Default::default()
    }
}

// Air rates too weak to carry a body over the crest by air control alone.
fn weak_air_settings() -> MapSettings {
    let mut settings = map_settings();
    settings.movement.player.air_acceleration = 5.0;
    settings.movement.player.air_deceleration = 5.0;
    settings.movement.player.air_lateral_deceleration = 5.0;
    settings
}

// One intent held for a number of ticks; `jump` presses jump as the phase
// begins, from the support the previous tick left.
struct Phase {
    intent: PlayerMoveIntent,
    jump: bool,
    ticks: usize,
}

fn hold(intent: PlayerMoveIntent, ticks: usize) -> Phase {
    Phase {
        intent,
        jump: false,
        ticks,
    }
}

const TRACE_DELTA: f32 = 1.0 / 60.0;

fn trace(
    world: &CollisionWorld,
    settings: &MapSettings,
    start: Position,
    phases: &[Phase],
) -> Vec<(Position, CharacterSupport)> {
    let gameplay = gameplay_config();
    let carriers = Carriers::default();
    let portals = PortalSet::default();
    let mut pos = start;
    let mut velocity = Vec3::ZERO;
    let mut stance = PlayerStance::default();
    let mut knockback = KnockbackVelocity::default();
    let mut support = CharacterSupport::Airborne;
    let mut out = Vec::new();
    for phase in phases {
        if phase.jump {
            let jump = player_jump(
                support,
                phase.intent,
                velocity.y,
                world,
                gameplay.player.physics(),
                &settings.movement,
                false,
                &pos,
                &[],
            );
            match jump {
                Some(PlayerJump::Rise(vertical)) => velocity.y = vertical,
                Some(PlayerJump::Release(shove)) => knockback.0 += shove,
                None => {}
            }
        }
        for _ in 0..phase.ticks {
            let step = step_player_movement(PlayerMovementStep {
                start: pos,
                vertical_velocity: velocity.y,
                horizontal_velocity: velocity.with_y(0.0),
                stance,
                intent: phase.intent,
                has_speed: false,
                disabled: false,
                delta: TRACE_DELTA,
                has_low_gravity: false,
                held_keys: &[],
                open_fields: &[],
                knockback_displacement: knockback.step(TRACE_DELTA),
                collision_world: world,
                map_settings: settings,
                gameplay_config: &gameplay,
                portal_set: &portals,
                carriers: &carriers,
            });
            pos = step.movement.position;
            velocity = step.horizontal_velocity.with_y(step.movement.vertical_velocity);
            stance = step.stance;
            support = step.movement.support;
            knockback.decay(TRACE_DELTA, settings.movement.knockback.deceleration);
            out.push((pos, support));
        }
    }
    out
}

// Facing +Z into `test_ladder` from its front side, looking up.
const CLIMB: PlayerMoveIntent = PlayerMoveIntent {
    forward: 1.0,
    pitch: 1.0,
    ..PlayerMoveIntent::NONE
};
const LADDER_FOOT: Position = Position {
    x: 0.0,
    y: 0.0,
    z: -0.6,
};

#[test]
fn a_climber_steps_onto_the_landing_at_the_crest() {
    let world = CollisionWorld::from_map_layout(&ladder_layout());
    let trace = trace(&world, &weak_air_settings(), LADDER_FOOT, &[hold(CLIMB, 300)]);
    assert!(trace.iter().any(|(_, support)| *support == CharacterSupport::Ladder));
    assert!(
        trace.iter().any(|(pos, support)| {
            *support == CharacterSupport::Ground && pos.y > LEVEL_HEIGHT - 0.1 && pos.z > 0.3
        }),
        "never reached the landing: {:?}",
        trace.last()
    );
}

#[test]
fn a_climber_dismounts_sideways_at_ladder_speed() {
    let world = CollisionWorld::from_map_layout(&ladder_layout());
    let sideways = PlayerMoveIntent {
        sideways: 1.0,
        ..PlayerMoveIntent::NONE
    };
    let trace = trace(
        &world,
        &weak_air_settings(),
        LADDER_FOOT,
        &[hold(CLIMB, 20), hold(sideways, 30)],
    );
    assert_eq!(trace[19].1, CharacterSupport::Ladder);
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        pos.x.abs() > 0.6 && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

#[test]
fn a_jump_on_a_ladder_lets_go_without_rising() {
    let world = CollisionWorld::from_map_layout(&ladder_layout());
    let let_go = Phase {
        intent: PlayerMoveIntent::NONE,
        jump: true,
        ticks: 45,
    };
    let trace = trace(&world, &weak_air_settings(), LADDER_FOOT, &[hold(CLIMB, 20), let_go]);
    assert_eq!(trace[19].1, CharacterSupport::Ladder);
    // Letting go keeps only the climb velocity the body already carried.
    let settings = weak_air_settings();
    let ladder_speed = settings.movement.player.move_speed * settings.movement.player.move_speed_ladder;
    let coasting = ladder_speed * ladder_speed / (2.0 * settings.movement.gravity);
    let release_y = trace[19].0.y;
    let highest = trace[20..].iter().map(|(pos, _)| pos.y).fold(f32::MIN, f32::max);
    assert!(
        highest <= release_y + coasting + 1e-3,
        "rose from {release_y} to {highest}"
    );
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        world.ladder_volume_at(pos).is_none() && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

#[test]
fn a_forward_jump_lands_on_the_landing_behind_the_ladder() {
    let layout = MapLayout {
        floors: vec![ladder_front_base_floor(), ladder_back_landing_floor()],
        ladders: vec![test_ladder_two_storey()],
        ..Default::default()
    };
    let world = CollisionWorld::from_map_layout(&layout);
    let step_through = Phase {
        intent: PlayerMoveIntent::moving(0.0),
        jump: true,
        ticks: 60,
    };
    let trace = trace(
        &world,
        &weak_air_settings(),
        LADDER_FOOT,
        &[hold(CLIMB, 120), step_through],
    );
    let (release, support) = trace[119];
    assert!(
        support == CharacterSupport::Ladder && release.y > LEVEL_HEIGHT,
        "{release:?} {support:?}"
    );
    assert!(
        trace[120..].iter().any(|(pos, support)| {
            *support == CharacterSupport::Ground && pos.z > 0.0 && (pos.y - LEVEL_HEIGHT).abs() < 0.05
        }),
        "never landed behind the ladder: {:?}",
        trace.last()
    );
}

#[test]
fn a_jump_while_descending_still_lets_go() {
    let world = CollisionWorld::from_map_layout(&ladder_layout());
    let descend = PlayerMoveIntent {
        forward: -1.0,
        ..PlayerMoveIntent::NONE
    };
    let let_go = Phase {
        intent: descend,
        jump: true,
        ticks: 45,
    };
    // The climb velocity coasts out before the descent starts.
    let phases = [
        hold(CLIMB, 40),
        hold(PlayerMoveIntent::NONE, 15),
        hold(descend, 5),
        let_go,
    ];
    let trace = trace(&world, &weak_air_settings(), LADDER_FOOT, &phases);
    assert_eq!(trace[59].1, CharacterSupport::Ladder);
    let (pos, support) = trace.last().expect("trace is empty");
    assert!(
        world.ladder_volume_at(pos).is_none() && *support != CharacterSupport::Ladder,
        "{pos:?} {support:?}"
    );
}

// Walking off a sliding tile into its own floor portal: the tile's motion
// goes with the body as it sinks, so it keeps pace with the aperture, and
// comes off again at the crossing, since it is motion relative to the
// entry, not to the static exit.
#[test]
fn a_walk_into_a_sliding_tiles_portal_sinks_with_the_tile_and_leaves_its_speed_behind() {
    use crate::portals::{PlayerHopBody, player_hop};
    use common::protocol::{Portal, PortalEnd, PortalPairId};
    let (carrier, floor) = slider();
    let layout = MapLayout {
        carriers: vec![carrier],
        floors: vec![floor],
        ..Default::default()
    };
    let gameplay = gameplay_config();
    let settings = map_settings();
    let portal = |end, pos: Vec3, normal: Vec3, carrier| Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier,
    };
    // On the tile, a step short of the aperture, walking toward it.
    let mut pos = Position {
        x: 4.0 * 10.0 / 60.0,
        y: 0.0,
        z: 1.45,
    };
    let (mut vertical, mut horizontal, mut stance) = (0.0, Vec3::ZERO, PlayerStance::default());
    let mut sinking: Vec<f32> = Vec::new();
    for tick in 10..40u32 {
        let (world, carriers) = world_at(&layout, tick);
        let set = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::ZERO, Vec3::Y, TILE),
                portal(PortalEnd::B, Vec3::new(0.0, 8.0, 20.0), Vec3::NEG_Y, CarrierId::WORLD),
            ],
            &world,
            &carriers,
            gameplay.portals.size,
        );
        let step = step_player_movement(PlayerMovementStep {
            start: pos,
            vertical_velocity: vertical,
            horizontal_velocity: horizontal,
            stance,
            intent: PlayerMoveIntent {
                forward: -1.0,
                ..PlayerMoveIntent::NONE
            },
            has_speed: false,
            disabled: false,
            delta: 1.0 / 30.0,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            knockback_displacement: Vec3::ZERO,
            collision_world: &world,
            map_settings: &settings,
            gameplay_config: &gameplay,
            portal_set: &set,
            carriers: &carriers,
        });
        pos = step.movement.position;
        vertical = step.movement.vertical_velocity;
        horizontal = step.horizontal_velocity;
        stance = step.stance;
        if step.movement.support == CharacterSupport::Airborne {
            sinking.push(pos.x - carriers.pose(TILE).translation.x);
            assert!((horizontal.x - 2.0).abs() < 1e-3, "tick {tick}: {horizontal}");
        }
        if let Some(hop) = player_hop(
            &set,
            step.start.into(),
            pos.into(),
            &gameplay,
            &settings.movement,
            PlayerHopBody {
                stance,
                knockback: &KnockbackVelocity::default(),
                horizontal_velocity: &HorizontalVelocity(horizontal),
                vertical_velocity: vertical,
                carried: step.movement.carried(),
                yaw: 0.0,
            },
            1.0 / 30.0,
        ) {
            assert!(sinking.len() >= 2, "{sinking:?}");
            let drift = sinking
                .iter()
                .fold(0.0_f32, |worst, rel| worst.max((rel - sinking[0]).abs()));
            assert!(drift < 1e-3, "drifted across the aperture: {sinking:?}");
            let walk = settings.movement.player.move_speed;
            assert!(
                (hop.crossing.horizontal_velocity.length() - walk).abs() < 1e-3,
                "{hop:?}"
            );
            return;
        }
    }
    panic!("never crossed the tile's aperture: {pos:?}");
}

// Released flight over the slider's floor portal, from `feet` at tick 10:
// per tick, the position relative to the tile, horizontal velocity, and
// vertical velocity after the step.
fn slider_portal_flight(feet: Position, vertical: f32, horizontal: Vec3, ticks: u32) -> Vec<(Vec3, Vec3, f32)> {
    use common::protocol::{Portal, PortalEnd, PortalPairId};
    let (carrier, floor) = slider();
    let layout = MapLayout {
        carriers: vec![carrier],
        floors: vec![floor],
        ..Default::default()
    };
    let gameplay = gameplay_config();
    let settings = map_settings();
    let portal = |end, pos: Vec3, normal: Vec3, carrier| Portal {
        pair: PortalPairId(1),
        end,
        pos: pos.into(),
        nx: normal.x,
        ny: normal.y,
        nz: normal.z,
        yaw: 0.0,
        carrier,
    };
    let (mut pos, mut vertical, mut horizontal, mut stance) = (feet, vertical, horizontal, PlayerStance::default());
    let mut trace = Vec::new();
    for tick in 10..10 + ticks {
        let (world, carriers) = world_at(&layout, tick);
        let set = PortalSet::rebuild(
            &[
                portal(PortalEnd::A, Vec3::new(0.0, 8.0, 20.0), Vec3::NEG_Y, CarrierId::WORLD),
                portal(PortalEnd::B, Vec3::ZERO, Vec3::Y, TILE),
            ],
            &world,
            &carriers,
            gameplay.portals.size,
        );
        let step = step_player_movement(PlayerMovementStep {
            start: pos,
            vertical_velocity: vertical,
            horizontal_velocity: horizontal,
            stance,
            intent: PlayerMoveIntent::NONE,
            has_speed: false,
            disabled: false,
            delta: 1.0 / 30.0,
            has_low_gravity: false,
            held_keys: &[],
            open_fields: &[],
            knockback_displacement: Vec3::ZERO,
            collision_world: &world,
            map_settings: &settings,
            gameplay_config: &gameplay,
            portal_set: &set,
            carriers: &carriers,
        });
        pos = step.movement.position;
        vertical = step.movement.vertical_velocity;
        horizontal = step.horizontal_velocity;
        stance = step.stance;
        assert_eq!(
            step.movement.support,
            CharacterSupport::Airborne,
            "tick {tick}: {pos:?}"
        );
        trace.push((Vec3::from(pos) - carriers.pose(TILE).translation, horizontal, vertical));
    }
    trace
}

// A body coming up out of the tile's floor portal already moves with the
// tile; passing the top of the slab must not carry it a second time.
#[test]
fn rising_out_of_a_sliding_tiles_portal_is_not_carried_again() {
    let feet = Position {
        x: 10.0 * 4.0 / 60.0,
        y: -0.85,
        z: 0.0,
    };
    let trace = slider_portal_flight(feet, 6.0, Vec3::X * 2.0, 16);
    let (first, ..) = trace[0];
    let top = trace.iter().map(|(rel, ..)| rel.y).fold(f32::MIN, f32::max);
    assert!(
        top > 0.3 && trace.last().is_some_and(|(rel, ..)| rel.y < top),
        "{trace:?}"
    );
    for (rel, horizontal, _) in &trace {
        assert!((horizontal - Vec3::X * 2.0).length() < 1e-3, "{trace:?}");
        assert!((rel.x - first.x).abs() < 1e-3 && rel.z.abs() < 1e-3, "{trace:?}");
    }
}

// A body dropping into the tile's floor portal from the air never stood on
// the tile and takes none of its motion.
#[test]
fn dropping_into_a_sliding_tiles_portal_takes_none_of_its_motion() {
    let feet = Position {
        x: 10.0 * 4.0 / 60.0,
        y: 0.25,
        z: 0.0,
    };
    let trace = slider_portal_flight(feet, -1.0, Vec3::ZERO, 8);
    assert!(trace.last().is_some_and(|(rel, ..)| rel.y < -0.1), "{trace:?}");
    for (_, horizontal, _) in &trace {
        assert!(horizontal.length() < 1e-3, "{trace:?}");
    }
}
