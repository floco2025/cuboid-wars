use super::*;
use crate::{
    constants::TICK_SECS,
    physics::AirborneMomentum,
    protocol::{Carrier, CarrierId},
};

fn lift() -> (Carrier, Floor) {
    let (carrier, floor) = slider();
    (
        Carrier {
            to: Position {
                x: 0.0,
                y: LEVEL_HEIGHT,
                z: 0.0,
            },
            levels: 1,
            ..carrier
        },
        floor,
    )
}

const SLIDE_PER_TICK: f32 = 4.0 / 60.0;

const RISE_PER_TICK: f32 = LEVEL_HEIGHT / 60.0;

#[test]
fn rider_slides_with_the_tile() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!(
        (step.position.x - SLIDE_PER_TICK).abs() < 1e-3,
        "moved to {:?}",
        step.position
    );
    assert!(step.position.y.abs() < 0.01, "left the surface: {:?}", step.position);
    assert_eq!(step.support, CharacterSupport::Ground);
    assert!(
        (step.floor_velocity.x - 2.0).abs() < 1e-3,
        "floor velocity {}",
        step.floor_velocity
    );
}

// The tile slides through a static floor at its own height: the rider's
// probe sees two surfaces a cast-noise apart, and the carried one carries.
#[test]
fn rider_keeps_riding_through_a_coincident_static_floor() {
    let (carrier, tile) = slider();
    let floor = Floor {
        x1: 1.0,
        z1: -1.5,
        x2: 5.0,
        z2: 1.5,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let mut pos = Position::default();
    let mut vertical_velocity = 0.0;
    for tick in 1..=60 {
        let (world, carriers) = carried_world((carrier, tile), &[], &[floor], tick);
        let step = ride(&world, &carriers, pos, vertical_velocity, Vec3::ZERO, TICK_SECS);
        pos = step.position;
        vertical_velocity = step.vertical_velocity;
        let surface = carriers.pose(TILE).translation;
        assert!(
            (pos.x - surface.x).abs() < 1e-3,
            "tick {tick}: rider at {:?}, tile at {surface}",
            pos
        );
        assert_eq!(step.support, CharacterSupport::Ground, "tick {tick}");
    }
}

#[test]
fn rider_rises_with_a_lift() {
    let (world, carriers) = carried_world(lift(), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!(
        (step.position.y - RISE_PER_TICK).abs() < 1e-3,
        "rose to {:?}",
        step.position
    );
    assert_eq!(step.support, CharacterSupport::Ground);
    assert_eq!(step.vertical_velocity, 0.0);
}

#[test]
fn rider_sinks_with_a_lift() {
    let (carrier, floor) = lift();
    let sinking = Carrier {
        phase_ticks: 60,
        ..carrier
    };
    let (world, carriers) = carried_world((sinking, floor), &[], &[], 1);
    let top = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    let step = ride(&world, &carriers, top, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(
        (step.position.y - (LEVEL_HEIGHT - RISE_PER_TICK)).abs() < 1e-3,
        "sank to {:?}",
        step.position
    );
    assert_eq!(step.support, CharacterSupport::Ground);
    // A carry, not a snap: the lift's descent is reported as floor velocity.
    assert!(
        (step.floor_velocity.y + RISE_PER_TICK / TICK_SECS).abs() < 1e-3,
        "floor velocity {}",
        step.floor_velocity
    );
    assert!(!step.crushed, "riding on top is not a crush");
}

// The top of the collision box above the feet.
fn head_height() -> f32 {
    let physics = player_physics();
    physics.movement_collider.height
}

fn ground() -> Floor {
    Floor {
        x1: -2.0,
        z1: -2.0,
        x2: 2.0,
        z2: 2.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

// A body on the ground under a descending lift can go neither down nor
// aside: once the slab is inside it, the step reports a crush.
#[test]
fn a_lift_descending_onto_a_standing_body_crushes_it() {
    let (carrier, floor) = lift();
    let sinking = Carrier {
        phase_ticks: 60,
        ..carrier
    };
    let head = head_height();
    let ticks_to = |slab_top: f32| ((LEVEL_HEIGHT - slab_top) / RISE_PER_TICK).round() as u32;

    let clear_tick = ticks_to(head + FLOOR_THICKNESS + 1.0);
    let (world, carriers) = carried_world((sinking, floor), &[], &[ground()], clear_tick);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);
    assert!(!step.crushed, "crushed with a meter of headroom");
    assert_eq!(step.support, CharacterSupport::Ground);

    let crushing_tick = ticks_to(head / 2.0);
    let (world, carriers) = carried_world((sinking, floor), &[], &[ground()], crushing_tick);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);
    assert!(step.crushed, "not crushed at {:?}", step.position);
}

// A rider carried up into a static ceiling stops at the ceiling while the
// slab keeps rising through its feet.
#[test]
fn a_lift_rising_into_a_ceiling_crushes_its_rider() {
    let (carrier, floor) = lift();
    let tick: u16 = 10;
    let feet = f32::from(tick - 1) * RISE_PER_TICK;
    let ceiling = Floor {
        y: feet + head_height() + 0.02 + FLOOR_THICKNESS,
        ..ground()
    };
    let (world, carriers) = carried_world((carrier, floor), &[], &[ceiling], u32::from(tick));
    let start = Position {
        x: 0.0,
        y: feet,
        z: 0.0,
    };

    let step = ride(&world, &carriers, start, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(step.crushed, "not crushed at {:?}", step.position);
}

#[test]
fn rider_walking_against_the_motion_moves_relative_to_the_tile() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::NEG_X, TICK_SECS);

    let expected = SLIDE_PER_TICK - TICK_SECS;
    assert!(
        (step.position.x - expected).abs() < 1e-3,
        "moved to {:?}",
        step.position
    );
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn rider_pushed_into_a_wall_is_blocked_and_left_behind() {
    let wall = Wall {
        x1: 1.0,
        z1: -2.0,
        x2: 1.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: CarrierId::WORLD,
    };
    let (world, carriers) = carried_world(slider(), &[wall], &[], 1);
    let start = Position {
        x: 0.9 - player_physics().movement_collider.radius() - 0.05,
        y: 0.0,
        z: 0.0,
    };
    let step = ride(&world, &carriers, start, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(step.blocked);
    assert!(
        step.position.x < start.x + SLIDE_PER_TICK - 0.005,
        "went to {:?}",
        step.position
    );
}

#[test]
fn walking_off_the_tile_keeps_its_velocity_at_different_tick_rates() {
    for hz in [30, 60] {
        let (mut carrier, floor) = slider();
        carrier.travel_ticks = 2 * hz;
        let mut position = Position { x: 1.4, y: 0.0, z: 0.0 };
        let step = (1..=hz)
            .find_map(|tick| {
                let (world, carriers) = carried_world((carrier, floor), &[], &[], tick);
                let step = ride(
                    &world,
                    &carriers,
                    position,
                    0.0,
                    Vec3::X * TEST_PLAYER_SPEED,
                    1.0 / hz as f32,
                );
                position = step.position;
                (step.support == CharacterSupport::Airborne).then_some(step)
            })
            .expect("player never left the moving tile");
        assert!((step.floor_velocity.x - 2.0).abs() < 1e-3);
        let mut momentum = AirborneMomentum::default();
        momentum.finish_step(&step);
        assert!(
            (momentum.0 - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3,
            "momentum {}",
            momentum.0
        );
    }
}

#[test]
fn a_body_above_the_tolerance_is_not_carried() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let above = Position { x: 0.0, y: 0.3, z: 0.0 };
    let step = ride(&world, &carriers, above, -1.0, Vec3::ZERO, TICK_SECS);

    assert!(step.position.x.abs() < 1e-6, "was carried to {:?}", step.position);
    assert_eq!(step.floor_velocity, Vec3::ZERO);
    assert_eq!(
        step.support,
        CharacterSupport::Airborne,
        "airborne bodies must land before snapping"
    );
}

#[test]
fn a_distant_lift_does_not_attach_an_airborne_body_to_a_slider() {
    let (slider, floor) = slider();
    for lift_travel in [-18.6, 18.6] {
        let layout = MapLayout {
            carriers: vec![
                slider,
                Carrier {
                    from: Position {
                        x: 100.0,
                        y: 0.0,
                        z: 0.0,
                    },
                    to: Position {
                        x: 100.0,
                        y: lift_travel,
                        z: 0.0,
                    },
                    ..slider
                },
            ],
            floors: vec![
                floor,
                Floor {
                    carrier: CarrierId(2),
                    ..floor
                },
            ],
            ..Default::default()
        };
        let (world, carriers) = world_at(&layout, 1);
        let start = Position {
            x: 0.0,
            y: 0.25,
            z: 0.0,
        };
        let step = ride(&world, &carriers, start, 8.0, Vec3::ZERO, TICK_SECS);

        assert!(step.position.x.abs() < 1e-6, "lift travel {lift_travel}: {step:?}");
        assert_eq!(step.floor_velocity, Vec3::ZERO);
        assert_eq!(step.support, CharacterSupport::Airborne);
    }
}

#[test]
fn a_distant_lift_does_not_let_a_ceiling_hide_a_riders_platform() {
    let (slider, floor) = slider();
    let layout = MapLayout {
        carriers: vec![
            slider,
            Carrier {
                from: Position {
                    x: 100.0,
                    y: 0.0,
                    z: 0.0,
                },
                to: Position {
                    x: 100.0,
                    y: 4.0,
                    z: 0.0,
                },
                travel_ticks: 1,
                ..slider
            },
        ],
        floors: vec![
            floor,
            Floor { y: 3.0, ..ground() },
            Floor {
                carrier: CarrierId(2),
                ..floor
            },
        ],
        ..Default::default()
    };
    let (world, carriers) = world_at(&layout, 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!((step.position.x - SLIDE_PER_TICK).abs() < 1e-3, "{step:?}");
    assert_eq!(step.support, CharacterSupport::Ground);
    assert!(!step.crushed);
}

#[test]
fn an_unsupported_carrier_does_not_hide_the_lift_a_body_rides() {
    let (slider, floor) = slider();
    let layout = MapLayout {
        carriers: vec![
            Carrier {
                to: Position {
                    x: 0.0,
                    y: -12.0,
                    z: 0.0,
                },
                ..slider
            },
            Carrier {
                from: Position {
                    x: 0.0,
                    y: -0.15,
                    z: 0.0,
                },
                to: Position {
                    x: 4.0,
                    y: -0.15,
                    z: 0.0,
                },
                ..slider
            },
        ],
        floors: vec![
            floor,
            Floor {
                carrier: CarrierId(2),
                ..floor
            },
        ],
        ..Default::default()
    };
    let (world, carriers) = world_at(&layout, 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!(step.position.x.abs() < 1e-6, "{step:?}");
    assert!(
        (step.floor_velocity - Vec3::new(0.0, -6.0, 0.0)).length() < 1e-3,
        "{step:?}"
    );
}

#[test]
fn a_static_floor_above_a_descending_lift_stops_the_ride() {
    let (carrier, floor) = lift();
    let sinking = Carrier {
        to: Position {
            x: 0.0,
            y: -6.0,
            z: 0.0,
        },
        ..carrier
    };
    let (world, carriers) = carried_world((sinking, floor), &[], &[ground()], 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!(step.position.y.abs() < 1e-3, "{step:?}");
    assert_eq!(step.floor_velocity, Vec3::ZERO);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn jumping_rider_takes_the_tile_velocity() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 12.0, Vec3::ZERO, TICK_SECS);

    assert_eq!(step.support, CharacterSupport::Airborne);
    assert!(
        (step.position.x - SLIDE_PER_TICK).abs() < 1e-3,
        "moved to {:?}",
        step.position
    );
    assert!((step.floor_velocity.x - 2.0).abs() < 1e-3);
    let mut momentum = AirborneMomentum::default();
    momentum.finish_step(&step);
    assert!((momentum.0.x - 2.0).abs() < 1e-3, "momentum {}", momentum.0);
}

// The ride tolerance and the controller's ground contact prediction cover
// the same height, so a body still carried at the start of a tick was on
// the ground at the end of the last one: a takeoff too slow to clear the
// tolerance stays grounded, and one that clears it is not carried again,
// so the tile's velocity is never taken twice.
#[test]
fn a_takeoff_takes_the_tile_velocity_at_most_once() {
    let (carrier, floor) = slider();
    // 2.2 m/s is carried twice: grounded after the first tick, airborne
    // after the second.
    for takeoff_speed in [0.5, 1.0, 1.4, 1.8, 2.2, 2.6, 3.0, 6.0, 12.0] {
        let mut pos = Position::default();
        let mut vertical_velocity = takeoff_speed;
        let mut momentum = AirborneMomentum::default();
        let mut took_off = false;
        for tick in 1..=4 {
            let (world, carriers) = carried_world((carrier, floor), &[], &[], tick);
            let step = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity,
                    control_velocity: Vec3::ZERO,
                    external_displacement: momentum.step(TICK_SECS),
                    delta: TICK_SECS,
                },
                &test_environment(&world, &carriers, player_physics(), LadderMode::Automatic),
            );
            pos = step.position;
            vertical_velocity = step.vertical_velocity;
            momentum.finish_step(&step);
            took_off |= step.support == CharacterSupport::Airborne;
            assert!(
                momentum.0.x >= -1e-3 && momentum.0.x <= 2.0 + 1e-3,
                "takeoff {takeoff_speed}, tick {tick}: momentum {}, {step:?}",
                momentum.0
            );
            if step.support == CharacterSupport::Airborne {
                assert!(
                    (momentum.0.x - 2.0).abs() < 1e-3,
                    "takeoff {takeoff_speed}, tick {tick}: momentum {}",
                    momentum.0
                );
            }
        }
        assert_eq!(took_off, takeoff_speed >= 2.2, "takeoff {takeoff_speed}");
    }
}

#[test]
fn jumping_off_a_lift_keeps_its_rise() {
    let (world, carriers) = carried_world(lift(), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 12.0, Vec3::ZERO, TICK_SECS);

    assert_eq!(step.support, CharacterSupport::Airborne);
    let lift_speed = RISE_PER_TICK / TICK_SECS;
    let expected = 12.0 - TEST_GRAVITY * TICK_SECS + lift_speed;
    assert!(
        (step.vertical_velocity - expected).abs() < 1e-3,
        "vertical velocity {} (expected {expected})",
        step.vertical_velocity
    );
}

#[test]
fn a_body_on_static_ground_beside_a_rising_lift_is_not_carried() {
    let ground = Floor {
        x1: 2.0,
        z1: -2.0,
        x2: 6.0,
        z2: 2.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    };
    let (world, carriers) = carried_world(lift(), &[], &[ground], 1);
    let beside = Position { x: 4.0, y: 0.0, z: 0.0 };
    let step = ride(&world, &carriers, beside, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(
        (Vec3::from(step.position) - Vec3::from(beside)).length() < 1e-3,
        "moved to {:?}",
        step.position
    );
    assert_eq!(step.floor_velocity, Vec3::ZERO);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn boarding_a_descending_lift_near_the_landing_does_not_crush() {
    let (carrier, floor) = lift();
    let carrier = Carrier {
        to: Position { x: 0.0, y: 4.0, z: 0.0 },
        travel_ticks: 120,
        pause_ticks: 120,
        phase_ticks: 240,
        ..carrier
    };
    for axis in [Vec3::X, Vec3::Z] {
        let landing = if axis == Vec3::X {
            Floor {
                x1: floor.x2 + 0.02,
                x2: 6.0,
                ..ground()
            }
        } else {
            Floor {
                z1: floor.z2 + 0.02,
                z2: 6.0,
                ..ground()
            }
        };
        for with_upper_landing in [false, true] {
            let mut floors = vec![landing];
            if with_upper_landing {
                floors.push(Floor {
                    y: 4.0,
                    level: 1,
                    ..landing
                });
            }
            for first_tick in [90, 100, 110, 112, 118, 120, 130] {
                let mut pos = Position::from(axis * (floor.x2 + 0.65));
                let mut velocity = 0.0;
                for tick in first_tick..first_tick + 60 {
                    let (world, carriers) = carried_world((carrier, floor), &[], &floors, tick);
                    let control = if Vec3::from(pos).dot(axis) > 0.25 {
                        -axis * 4.5
                    } else {
                        Vec3::ZERO
                    };
                    let step = ride(&world, &carriers, pos, velocity, control, TICK_SECS);
                    assert!(
                        !step.crushed,
                        "first tick {first_tick}, tick {tick}, axis {axis}, upper landing {with_upper_landing}, platform at {:?}, start {pos:?}, result {step:?}",
                        carriers.pose(TILE).translation
                    );
                    pos = step.position;
                    velocity = step.vertical_velocity;
                }
                assert!(Vec3::from(pos).dot(axis) < 0.3, "did not board: {pos:?}");
                assert!(pos.y.abs() < 1e-3, "not on the platform: {pos:?}");
                assert_eq!(velocity, 0.0);
            }
        }
    }
}

#[test]
fn a_floor_that_rose_through_the_feet_lifts_the_rider_out() {
    let (carrier, floor) = lift();
    // Six meters per second: 0.2 m per tick, deeper than the ground snap
    // starts above the feet.
    let fast = Carrier {
        travel_ticks: (LEVEL_HEIGHT / 0.2).round() as u32,
        ..carrier
    };
    let rise = LEVEL_HEIGHT / fast.travel_ticks as f32;
    let (world, carriers) = carried_world((fast, floor), &[], &[], 1);
    let step = ride(&world, &carriers, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

    assert!((step.position.y - rise).abs() < 1e-3, "rose to {:?}", step.position);
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn a_rider_beside_a_carriers_wall_is_carried_with_it() {
    let (carrier, floor) = slider();
    let wall = Wall {
        x1: 1.5,
        z1: -1.5,
        x2: 1.5,
        z2: 1.5,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: TILE,
    };
    let (world, carriers) = carried_world((carrier, floor), &[wall], &[], 1);
    let start = Position {
        x: 1.4 - player_physics().movement_collider.radius() - 0.02,
        y: 0.0,
        z: 0.0,
    };
    let step = ride(&world, &carriers, start, 0.0, Vec3::X * TEST_PLAYER_SPEED, TICK_SECS);

    // Carried with the tile and stopped by its wall, which moved too: the
    // body ends where the wall now lets it, not where the static wall would.
    assert!(step.blocked, "the moving wall did not block: {:?}", step.position);
    assert!(
        step.position.x > start.x + SLIDE_PER_TICK - 0.05,
        "was not carried with the wall: {:?}",
        step.position
    );
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn a_fast_slider_carries_a_rider_from_its_previous_edge() {
    let (carrier, floor) = slider();
    let carrier = Carrier {
        travel_ticks: 4,
        ..carrier
    };
    let (world, carriers) = carried_world((carrier, floor), &[], &[], 1);
    let start = Position {
        x: floor.x1 + 0.1,
        y: 0.0,
        z: 0.0,
    };
    let step = ride(&world, &carriers, start, 0.0, Vec3::ZERO, TICK_SECS);
    assert!(
        (step.position.x - start.x - 1.0).abs() < 0.01,
        "lost the rider: {step:?}"
    );
    assert_eq!(step.support, CharacterSupport::Ground);
    assert!(!step.crushed);
}
