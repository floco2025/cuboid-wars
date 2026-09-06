use super::*;
use crate::{
    constants::TICK_SECS,
    map::Carriers,
    physics::AirborneMomentum,
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
