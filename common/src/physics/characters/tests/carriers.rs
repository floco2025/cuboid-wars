use super::*;
use crate::{
    config::gameplay::load_test_gameplay,
    constants::TICK_SECS,
    map::Carriers,
    physics::{
        AirborneMomentum,
        characters::geometry::{character_center, character_shape},
    },
    protocol::{BarrierKindTable, Carrier, CarrierId, Floor},
};

const TILE: CarrierId = CarrierId(1);

// Slides from the origin four meters along +X in two seconds: 2 m/s, one
// fifteenth of a meter per tick. The tile is a slab centered on the
// carrier's origin.
fn slider() -> (Carrier, Floor) {
    (
        Carrier {
            parent: CarrierId::WORLD,
            level: 0,
            levels: 0,
            from: Position::default(),
            to: Position { x: 4.0, y: 0.0, z: 0.0 },
            travel_ticks: 60,
            pause_ticks: 0,
            phase_ticks: 0,
        },
        Floor {
            x1: -1.5,
            z1: -1.5,
            x2: 1.5,
            z2: 1.5,
            y: 0.0,
            thickness: FLOOR_THICKNESS,
            level: 0,
            carrier: TILE,
        },
    )
}

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

// The world one tick into the carrier's cycle: `previous` is the first end,
// `current` one tick along, and the colliders already sit at `current`.
fn carried_world(
    (carrier, floor): (Carrier, Floor),
    walls: &[Wall],
    floors: &[Floor],
    tick: u32,
) -> (CollisionWorld, Carriers) {
    let layout = MapLayout {
        walls: walls.to_vec(),
        floors: floors.iter().copied().chain([floor]).collect(),
        carriers: vec![carrier],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let mut carriers = Carriers::from_layout(&layout);
    carriers.advance(tick.wrapping_sub(1));
    carriers.advance(tick);
    world.set_carrier_poses(&carriers);
    (world, carriers)
}

fn ride(
    world: &CollisionWorld,
    carriers: &Carriers,
    start: Position,
    vertical_velocity: f32,
    control_velocity: Vec3,
    delta: f32,
) -> CharacterMovementResult {
    step_character_movement(
        CharacterStep {
            start,
            vertical_velocity,
            control_velocity,
            external_displacement: Vec3::ZERO,
            delta,
        },
        &CharacterEnvironment {
            collision_world: world,
            gravity: TEST_GRAVITY,
            passable_kinds: &[],
            ladder_climb_ratio: test_ladders(),
            physics: player_physics(),
            portals: None,
            carriers,
        },
    )
}

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
    character_center(Position::default(), physics).y + character_shape(physics).half_extents.y
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
        x: 0.9 - player_physics().collider.width / 2.0 - 0.05,
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
fn walking_off_the_tile_keeps_its_velocity() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let start = Position { x: 1.4, y: 0.0, z: 0.0 };
    let step = ride(&world, &carriers, start, 0.0, Vec3::X * player_speed(), 0.1);

    assert_eq!(
        step.support,
        CharacterSupport::Airborne,
        "still on it at {:?}",
        step.position
    );
    assert!((step.floor_velocity.x - 2.0).abs() < 1e-3);
    let mut momentum = AirborneMomentum::default();
    momentum.finish_step(&step);
    assert!(
        (momentum.0 - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3,
        "momentum {}",
        momentum.0
    );
}

#[test]
fn a_body_above_the_tolerance_is_not_carried() {
    let (world, carriers) = carried_world(slider(), &[], &[], 1);
    let above = Position { x: 0.0, y: 0.3, z: 0.0 };
    let step = ride(&world, &carriers, above, -1.0, Vec3::ZERO, TICK_SECS);

    assert!(step.position.x.abs() < 1e-6, "was carried to {:?}", step.position);
    assert_eq!(step.floor_velocity, Vec3::ZERO);
    assert_eq!(step.support, CharacterSupport::Ground, "the snap still lands it");
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
        x: 1.4 - player_physics().collider.width / 2.0 - 0.02,
        y: 0.0,
        z: 0.0,
    };
    let step = ride(&world, &carriers, start, 0.0, Vec3::X * player_speed(), TICK_SECS);

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

fn pushing_wall(axis: Vec3) -> Wall {
    let tangent = Vec3::new(-axis.z, 0.0, axis.x);
    Wall {
        x1: -tangent.x * 2.0,
        z1: -tangent.z * 2.0,
        x2: tangent.x * 2.0,
        z2: tangent.z * 2.0,
        width: 0.2,
        level: 0,
        y: 0.0,
        height: WALL_HEIGHT,
        carrier: TILE,
    }
}

fn push_ground() -> Floor {
    Floor {
        x1: -10.0,
        x2: 10.0,
        z1: -10.0,
        z2: 10.0,
        ..ground()
    }
}

#[test]
fn a_sliding_wall_pushes_a_body_on_static_ground_even_against_its_input() {
    for axis in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let half_extents = character_shape(player_physics()).half_extents;
        let half_width = half_extents.x * axis.x.abs() + half_extents.z * axis.z.abs();
        let (carrier, floor) = slider();
        let carrier = Carrier {
            to: Position::from(axis * 4.0),
            ..carrier
        };
        let floor = Floor { y: -10.0, ..floor };
        for control in [Vec3::ZERO, -axis * player_speed(), -axis * player_speed() * 2.0] {
            let mut pos = Position::from(axis * (0.1 + half_width + 0.01));
            for tick in 1..=45 {
                let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(axis)], &[push_ground()], tick);
                let step = ride(&world, &carriers, pos, 0.0, control, TICK_SECS);
                assert!(!step.crushed, "axis {axis}, control {control}, tick {tick}: {step:?}");
                let wall_plane = carriers.pose(TILE).translation.dot(axis);
                let separation = Vec3::from(step.position).dot(axis) - wall_plane;
                assert!(separation >= half_width + 0.09, "inside the wall: {step:?}");
                assert_eq!(step.support, CharacterSupport::Ground);
                assert_eq!(step.floor_velocity, Vec3::ZERO);
                pos = step.position;
            }
        }
    }
}

#[test]
fn a_body_can_step_sideways_out_of_a_sliding_walls_path() {
    let (carrier, floor) = slider();
    let floor = Floor { y: -10.0, ..floor };
    let mut pos = Position {
        x: 0.61,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=45 {
        let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::Z * player_speed(), TICK_SECS);
        assert!(!step.crushed, "tick {tick}: {step:?}");
        pos = step.position;
    }
    assert!(pos.z > 3.0, "did not step clear: {pos:?}");
    assert!(pos.x < 2.5, "kept pushing after stepping clear: {pos:?}");
}

#[test]
fn a_sliding_raised_slab_pushes_a_body_beside_it() {
    let (carrier, floor) = slider();
    let floor = Floor { y: 1.5, ..floor };
    let mut pos = Position {
        x: floor.x2 + player_physics().collider.width / 2.0 + 0.01,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=45 {
        let (world, carriers) = carried_world((carrier, floor), &[], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(!step.crushed, "tick {tick}: {step:?}");
        assert!(step.position.x > pos.x + 0.05, "not pushed: {step:?}");
        assert!(step.position.y.abs() < 1e-3, "lifted onto the slab: {step:?}");
        pos = step.position;
    }
}

#[test]
fn a_body_can_board_a_low_slab_while_it_slides_towards_them() {
    let (carrier, floor) = slider();
    for height in [0.2, 0.4, 0.6] {
        let floor = Floor { y: height, ..floor };
        let mut pos = Position {
            x: floor.x2 + player_physics().collider.width / 2.0 + 0.01,
            y: 0.0,
            z: 0.0,
        };
        for tick in 1..=15 {
            let (world, carriers) = carried_world((carrier, floor), &[], &[push_ground()], tick);
            let step = ride(&world, &carriers, pos, 0.0, Vec3::NEG_X * 4.5, TICK_SECS);
            assert!(!step.crushed, "height {height}, tick {tick}: {step:?}");
            pos = step.position;
        }
        assert!(
            (pos.y - height).abs() < 1e-3,
            "not on the slab at height {height}: {pos:?}"
        );
    }
}

#[test]
fn a_sliding_wall_crushes_only_when_the_body_is_pinned_against_another_wall() {
    let (carrier, floor) = slider();
    let floor = Floor { y: -10.0, ..floor };
    let wall = pushing_wall(Vec3::X);
    let blocker = Wall {
        x1: 2.0,
        x2: 2.0,
        carrier: CarrierId::WORLD,
        ..wall
    };
    let mut pos = Position {
        x: 0.61,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=45 {
        let (world, carriers) = carried_world((carrier, floor), &[wall, blocker], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(step.position.x <= 1.41, "pushed through the static wall: {step:?}");
        if step.crushed {
            assert!(
                step.position.x >= 1.37,
                "crushed before reaching the static wall: {step:?}"
            );
            return;
        }
        pos = step.position;
    }
    panic!("not crushed between the two walls: {pos:?}");
}

#[test]
fn a_wall_does_not_drag_bystanders_when_moving_parallel_away_or_not_at_all() {
    let (carrier, floor) = slider();
    let floor = Floor { y: -10.0, ..floor };
    for travel in [Vec3::Z * 4.0, Vec3::NEG_X * 4.0, Vec3::ZERO] {
        let carrier = Carrier {
            to: Position::from(travel),
            ..carrier
        };
        let start = Position {
            x: 0.595,
            y: 0.0,
            z: 0.0,
        };
        let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], 1);
        let step = ride(&world, &carriers, start, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(!step.crushed, "travel {travel}: {step:?}");
        assert!(
            (Vec3::from(step.position) - Vec3::from(start)).length() < 1e-3,
            "dragged: {step:?}"
        );
    }
}

#[test]
fn a_very_slow_wall_still_pushes_instead_of_accumulating_penetration() {
    let (carrier, floor) = slider();
    let carrier = Carrier {
        travel_ticks: 40_000,
        ..carrier
    };
    let floor = Floor { y: -10.0, ..floor };
    let mut pos = Position {
        x: 0.61,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=600 {
        let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(!step.crushed, "tick {tick}: {step:?}");
        pos = step.position;
    }
    assert!(pos.x > 0.65, "not pushed: {pos:?}");
}

#[test]
fn a_diagonally_moving_wall_pushes_only_perpendicular_to_its_face() {
    let (carrier, floor) = slider();
    let carrier = Carrier {
        to: Position { x: 4.0, y: 1.0, z: 4.0 },
        ..carrier
    };
    let floor = Floor { y: -10.0, ..floor };
    let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], 1);
    let step = ride(
        &world,
        &carriers,
        Position {
            x: 0.61,
            y: 0.0,
            z: 0.0,
        },
        0.0,
        Vec3::ZERO,
        TICK_SECS,
    );
    assert!(!step.crushed, "{step:?}");
    assert!(step.position.x > 0.66, "not pushed: {step:?}");
    assert!(step.position.z.abs() < 1e-3, "dragged along the wall: {step:?}");
    assert!(step.position.y.abs() < 1e-3, "lifted by the wall: {step:?}");
    assert_eq!(step.floor_velocity, Vec3::ZERO);
}

#[test]
fn a_sliding_wall_pushes_actor_bodies_too() {
    let gameplay = load_test_gameplay().expect("test gameplay config invalid");
    let (carrier, floor) = slider();
    let floor = Floor { y: -10.0, ..floor };
    for (kind, actor) in &gameplay.actors {
        let physics = actor.physics();
        let mut pos = Position {
            x: 0.11 + physics.collider.width / 2.0,
            y: 0.0,
            z: 0.0,
        };
        for tick in 1..=45 {
            let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], tick);
            let step = step_character_movement(
                CharacterStep {
                    start: pos,
                    vertical_velocity: 0.0,
                    control_velocity: Vec3::NEG_X * 9.0,
                    external_displacement: Vec3::ZERO,
                    delta: TICK_SECS,
                },
                &CharacterEnvironment {
                    collision_world: &world,
                    gravity: TEST_GRAVITY,
                    passable_kinds: &[],
                    physics,
                    ladder_climb_ratio: test_ladders(),
                    portals: None,
                    carriers: &carriers,
                },
            );
            assert!(!step.crushed, "actor {kind}, tick {tick}: {step:?}");
            assert!(step.position.x > pos.x + 0.03, "actor {kind} not pushed: {step:?}");
            pos = step.position;
        }
    }
}
