use super::*;
use crate::{
    constants::TICK_SECS,
    physics::AirborneMomentum,
    protocol::{BarrierKindTable, MovingFloor},
};

// Slides from the origin four meters along +X in two seconds: 2 m/s, one
// fifteenth of a meter per tick.
fn slider() -> MovingFloor {
    MovingFloor {
        x1: 0.0,
        y1: 0.0,
        z1: 0.0,
        x2: 4.0,
        y2: 0.0,
        z2: 0.0,
        half_x: 1.5,
        half_z: 1.5,
        thickness: FLOOR_THICKNESS,
        travel_ticks: 60,
        pause_ticks: 0,
        phase_ticks: 0,
        level: 0,
        levels: 0,
    }
}

fn lift() -> MovingFloor {
    MovingFloor {
        x2: 0.0,
        y2: LEVEL_HEIGHT,
        levels: 1,
        ..slider()
    }
}

const SLIDE_PER_TICK: f32 = 4.0 / 60.0;
const RISE_PER_TICK: f32 = LEVEL_HEIGHT / 60.0;

// The world one tick into the tile's cycle: `previous` is the first end,
// `current` one tick along, and the collider already sits at `current`.
fn moving_world(floor: MovingFloor, walls: &[Wall], tick: u32) -> (CollisionWorld, MovingFloors) {
    let layout = MapLayout {
        walls: walls.to_vec(),
        moving_floors: vec![floor],
        ..Default::default()
    };
    let mut world = CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default());
    let mut floors = MovingFloors::from_layout(&layout);
    floors.advance(tick.wrapping_sub(1));
    floors.advance(tick);
    world.set_moving_floor_centers(&floors.collider_centers());
    (world, floors)
}

fn ride(
    world: &CollisionWorld,
    floors: &MovingFloors,
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
            moving_floors: floors,
        },
    )
}

#[test]
fn rider_slides_with_the_tile() {
    let (world, floors) = moving_world(slider(), &[], 1);
    let step = ride(&world, &floors, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

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

#[test]
fn rider_rises_with_a_lift() {
    let (world, floors) = moving_world(lift(), &[], 1);
    let step = ride(&world, &floors, Position::default(), 0.0, Vec3::ZERO, TICK_SECS);

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
    let sinking = MovingFloor {
        phase_ticks: 60,
        ..lift()
    };
    let (world, floors) = moving_world(sinking, &[], 1);
    let top = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    let step = ride(&world, &floors, top, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(
        (step.position.y - (LEVEL_HEIGHT - RISE_PER_TICK)).abs() < 1e-3,
        "sank to {:?}",
        step.position
    );
    assert_eq!(step.support, CharacterSupport::Ground);
}

#[test]
fn rider_walking_against_the_motion_moves_relative_to_the_tile() {
    let (world, floors) = moving_world(slider(), &[], 1);
    let step = ride(&world, &floors, Position::default(), 0.0, Vec3::NEG_X, TICK_SECS);

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
    };
    let (world, floors) = moving_world(slider(), &[wall], 1);
    let start = Position {
        x: 0.9 - player_physics().collider.width / 2.0 - 0.05,
        y: 0.0,
        z: 0.0,
    };
    let step = ride(&world, &floors, start, 0.0, Vec3::ZERO, TICK_SECS);

    assert!(step.blocked);
    assert!(
        step.position.x < start.x + SLIDE_PER_TICK - 0.005,
        "went to {:?}",
        step.position
    );
}

#[test]
fn walking_off_the_tile_keeps_its_velocity() {
    let (world, floors) = moving_world(slider(), &[], 1);
    let start = Position { x: 1.4, y: 0.0, z: 0.0 };
    let step = ride(&world, &floors, start, 0.0, Vec3::X * player_speed(), 0.1);

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
    let (world, floors) = moving_world(slider(), &[], 1);
    let above = Position { x: 0.0, y: 0.3, z: 0.0 };
    let step = ride(&world, &floors, above, -1.0, Vec3::ZERO, TICK_SECS);

    assert!(step.position.x.abs() < 1e-6, "was carried to {:?}", step.position);
    assert_eq!(step.floor_velocity, Vec3::ZERO);
    assert_eq!(step.support, CharacterSupport::Ground, "the snap still lands it");
}

#[test]
fn jumping_rider_takes_the_tile_velocity() {
    let (world, floors) = moving_world(slider(), &[], 1);
    let step = ride(&world, &floors, Position::default(), 12.0, Vec3::ZERO, TICK_SECS);

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
    let (world, floors) = moving_world(lift(), &[], 1);
    let step = ride(&world, &floors, Position::default(), 12.0, Vec3::ZERO, TICK_SECS);

    assert_eq!(step.support, CharacterSupport::Airborne);
    let lift_speed = RISE_PER_TICK / TICK_SECS;
    let expected = 12.0 - TEST_GRAVITY * TICK_SECS + lift_speed;
    assert!(
        (step.vertical_velocity - expected).abs() < 1e-3,
        "vertical velocity {} (expected {expected})",
        step.vertical_velocity
    );
}
