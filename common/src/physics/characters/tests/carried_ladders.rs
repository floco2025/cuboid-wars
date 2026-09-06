use super::*;
use crate::{
    constants::{LADDER_RAIL_INSET, LADDER_STANDOFF_CLEARANCE, TICK_SECS},
    protocol::{BarrierKindTable, Carrier, CarrierId},
};

const CARRIER: CarrierId = CarrierId(1);
const LANDING_Y: f32 = 2.4;
const CLIMB_SPEED: f32 = 5.0;

struct Climber {
    world: CollisionWorld,
    carriers: Carriers,
    physics: CharacterPhysicsConfig,
    position: Position,
    vertical_velocity: f32,
    momentum: AirborneMomentum,
    tick: u32,
}

impl Climber {
    fn new(travel: Vec3, phase_ticks: u32, height: f32) -> Self {
        let layout = MapLayout {
            carriers: vec![Carrier {
                parent: CarrierId::WORLD,
                level: 0,
                levels: 0,
                from: Position::default(),
                to: travel.into(),
                travel_ticks: 180,
                pause_ticks: 30,
                phase_ticks,
            }],
            ladders: vec![Ladder {
                height: LANDING_Y,
                carrier: CARRIER,
                ..test_ladder()
            }],
            floors: vec![
                Floor {
                    x1: -3.0,
                    x2: 3.0,
                    z1: -3.0,
                    z2: 3.0,
                    y: 0.0,
                    thickness: FLOOR_THICKNESS,
                    level: 0,
                    carrier: CARRIER,
                },
                Floor {
                    x1: -3.0,
                    x2: 3.0,
                    z1: -0.2,
                    z2: 3.0,
                    y: LANDING_Y,
                    thickness: FLOOR_THICKNESS,
                    level: 1,
                    carrier: CARRIER,
                },
            ],
            ..MapLayout::default()
        };
        let physics = player_physics();
        let carriers = Carriers::from_layout(&layout);
        let local = Vec3::new(
            0.0,
            height,
            -(LADDER_RAIL_INSET + physics.collider.depth / 2.0 + LADDER_STANDOFF_CLEARANCE),
        );
        Self {
            world: CollisionWorld::from_map_layout(&layout, &BarrierKindTable::default()),
            position: carriers.pose(CARRIER).transform_point(local).into(),
            carriers,
            physics,
            vertical_velocity: 0.0,
            momentum: AirborneMomentum::default(),
            tick: 0,
        }
    }

    fn local_position(&self) -> Vec3 {
        self.carriers
            .pose(CARRIER)
            .inverse_transform_point(self.position.into())
    }

    fn step(&mut self, control_velocity: Vec3) -> CharacterMovementResult {
        self.tick += 1;
        self.carriers.advance(self.tick);
        self.world.set_carrier_poses(&self.carriers);
        let result = step_character_movement(
            CharacterStep {
                start: self.position,
                vertical_velocity: self.vertical_velocity,
                control_velocity,
                external_displacement: self.momentum.step(TICK_SECS),
                delta: TICK_SECS,
            },
            &CharacterEnvironment {
                collision_world: &self.world,
                gravity: TEST_GRAVITY,
                passable_kinds: &[],
                physics: self.physics,
                ladder_climb_ratio: test_ladders(),
                portals: None,
                carriers: &self.carriers,
            },
        );
        self.position = result.position;
        self.vertical_velocity = result.vertical_velocity;
        self.momentum.finish_step(&result);
        result
    }
}

#[test]
fn climber_reaches_a_moving_landing_without_falling_or_crushing() {
    for travel in [Vec3::new(27.0, 0.0, -12.0), Vec3::new(-27.0, 6.0, 12.0)] {
        for phase in [30, 240] {
            let mut climber = Climber::new(travel, phase, 0.0);
            let mut landed = false;
            for _ in 0..70 {
                let before = climber.local_position();
                let result = climber.step(Vec3::Z * CLIMB_SPEED);
                let local = climber.local_position();
                assert!(
                    !result.crushed,
                    "crushed at {local:?}, travel {travel:?}, phase {phase}"
                );
                assert!(local.x.abs() < 0.01, "drifted off the ladder at {local:?}");
                if before.y < LANDING_Y {
                    assert!(local.y > before.y, "stopped climbing at {local:?}");
                }
                if result.support == CharacterSupport::Ground && local.y >= LANDING_Y - 0.01 {
                    landed = true;
                    break;
                }
            }
            assert!(landed, "landing not reached: {:?}", climber.local_position());
        }
    }
}

#[test]
fn idle_climber_rides_the_ladder_through_stops_and_reversals() {
    let mut climber = Climber::new(Vec3::new(27.0, 6.0, -12.0), 0, 1.0);
    let start = climber.local_position();
    for _ in 0..450 {
        let result = climber.step(Vec3::ZERO);
        let local = climber.local_position();
        assert_eq!(result.support, CharacterSupport::Ladder, "lost ladder at {local:?}");
        assert!(!result.crushed, "crushed at {local:?}");
        assert!(local.distance(start) < 0.01, "drifted from {start:?} to {local:?}");
        assert_eq!(result.vertical_velocity, 0.0);
    }
}

#[test]
fn descending_climber_moves_relative_to_the_ladder() {
    for phase in [30, 240] {
        let mut climber = Climber::new(Vec3::new(27.0, 6.0, -12.0), phase, 1.5);
        for _ in 0..10 {
            let before = climber.local_position();
            let result = climber.step(Vec3::NEG_Z * CLIMB_SPEED);
            let expected = before - Vec3::Y * CLIMB_SPEED * test_ladders() * TICK_SECS;
            assert_eq!(result.support, CharacterSupport::Ladder);
            assert!(!result.crushed);
            assert!(
                climber.local_position().distance(expected) < 0.01,
                "expected {expected:?}, got {:?}",
                climber.local_position()
            );
        }
    }
}

#[test]
fn jumping_climber_does_not_remain_attached_to_the_ladder() {
    let mut climber = Climber::new(Vec3::X * 27.0, 30, 1.0);
    let start = climber.position;
    climber.vertical_velocity = 12.0;
    let result = climber.step(Vec3::ZERO);
    assert_eq!(result.support, CharacterSupport::Airborne);
    assert!((result.position.x - start.x).abs() < 0.01);
    assert!(result.position.y > start.y);
    assert!(!result.crushed);
}

#[test]
fn leaving_a_moving_ladder_keeps_its_velocity() {
    let mut climber = Climber::new(Vec3::X * 27.0, 30, 1.0);
    climber.position.x += 0.4;
    let result = climber.step(Vec3::X * 6.0);
    assert_eq!(result.support, CharacterSupport::Airborne);
    assert!((climber.momentum.0.x - 4.5).abs() < 0.01);
    let local = climber.local_position();
    climber.step(Vec3::ZERO);
    assert!((climber.local_position().x - local.x).abs() < 0.01);
}

#[test]
fn body_behind_a_moving_ladder_is_not_carried_by_it() {
    let mut climber = Climber::new(Vec3::X * 27.0, 30, 0.7);
    climber.position.z = climber.carriers.pose(CARRIER).translation.z + 0.5;
    let start = climber.position;
    let result = climber.step(Vec3::ZERO);
    assert_eq!(result.support, CharacterSupport::Airborne);
    assert!((result.position.x - start.x).abs() < 0.01);
    assert_eq!(result.floor_velocity, Vec3::ZERO);
}
