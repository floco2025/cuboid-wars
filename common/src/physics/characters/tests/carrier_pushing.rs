use super::*;
use crate::{
    config::gameplay::load_test_gameplay,
    constants::TICK_SECS,
    protocol::{Carrier, CarrierId},
};

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
        z1: -10.0,
        x2: 10.0,
        z2: 10.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
        carrier: CarrierId::WORLD,
    }
}

#[test]
fn a_sliding_wall_pushes_a_body_on_static_ground_even_against_its_input() {
    for axis in [Vec3::X, Vec3::NEG_X, Vec3::Z, Vec3::NEG_Z] {
        let body = player_physics().movement_collider;
        let half_extents = Vec3::new(body.radius(), body.height / 2.0, body.radius());
        let half_width = half_extents.x * axis.x.abs() + half_extents.z * axis.z.abs();
        let (carrier, floor) = slider();
        let carrier = Carrier {
            to: Position::from(axis * 4.0),
            ..carrier
        };
        let floor = Floor { y: -10.0, ..floor };
        for control in [Vec3::ZERO, -axis * TEST_PLAYER_SPEED, -axis * TEST_PLAYER_SPEED * 2.0] {
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
        x: 0.41,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=45 {
        let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::Z * TEST_PLAYER_SPEED, TICK_SECS);
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
        x: floor.x2 + player_physics().movement_collider.radius() + 0.01,
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
    for height in [0.1, 0.2] {
        let floor = Floor { y: height, ..floor };
        let mut pos = Position {
            x: floor.x2 + player_physics().movement_collider.radius() + 0.01,
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
        x: 0.41,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=45 {
        let (world, carriers) = carried_world((carrier, floor), &[wall, blocker], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(step.position.x <= 1.61, "pushed through the static wall: {step:?}");
        if step.crushed {
            assert!(
                step.position.x >= 1.57,
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
            x: 0.395,
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
        x: 0.41,
        y: 0.0,
        z: 0.0,
    };
    for tick in 1..=600 {
        let (world, carriers) = carried_world((carrier, floor), &[pushing_wall(Vec3::X)], &[push_ground()], tick);
        let step = ride(&world, &carriers, pos, 0.0, Vec3::ZERO, TICK_SECS);
        assert!(!step.crushed, "tick {tick}: {step:?}");
        pos = step.position;
    }
    assert!(pos.x > 0.45, "not pushed: {pos:?}");
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
            x: 0.41,
            y: 0.0,
            z: 0.0,
        },
        0.0,
        Vec3::ZERO,
        TICK_SECS,
    );
    assert!(!step.crushed, "{step:?}");
    assert!(step.position.x > 0.46, "not pushed: {step:?}");
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
            x: 0.11 + physics.movement_collider.radius(),
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
                &test_environment(&world, &carriers, physics, LadderMode::Automatic),
            );
            assert!(!step.crushed, "actor {kind}, tick {tick}: {step:?}");
            assert!(step.position.x > pos.x + 0.03, "actor {kind} not pushed: {step:?}");
            pos = step.position;
        }
    }
}
