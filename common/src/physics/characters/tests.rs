use super::*;
use crate::{
    constants::{FLOOR_THICKNESS, LEVEL_HEIGHT, PLAYER_SPEED, WALL_THICKNESS},
    map::ramp_surface_at,
    protocol::{Floor, MapLayout, Ramp, Wall},
};
use bevy_ecs::prelude::Entity;

fn test_ramp() -> Ramp {
    Ramp {
        x1: 0.0,
        y1: 0.0,
        z1: 0.0,
        x2: 4.0,
        y2: LEVEL_HEIGHT,
        z2: 8.0,
    }
}

fn upper_floor_west_of_ramp() -> Floor {
    Floor {
        x1: -4.0,
        z1: 0.0,
        x2: 0.0,
        z2: 8.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

fn lower_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: 0.0,
        thickness: FLOOR_THICKNESS,
        level: 0,
    }
}

fn upper_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: LEVEL_HEIGHT,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

fn low_overhead_floor() -> Floor {
    Floor {
        x1: -4.0,
        z1: -4.0,
        x2: 4.0,
        z2: 4.0,
        y: PLAYER_HEIGHT - 0.05,
        thickness: FLOOR_THICKNESS,
        level: 1,
    }
}

fn test_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: -2.0,
        x2: 0.0,
        z2: 2.0,
        width: 0.2,
        level: 0,
    }
}

fn horizontal_wall() -> Wall {
    Wall {
        x1: 0.0,
        z1: 0.0,
        x2: 4.0,
        z2: 0.0,
        width: 0.2,
        level: 0,
    }
}

fn upper_horizontal_wall() -> Wall {
    Wall {
        x1: 27.85,
        z1: 32.0,
        x2: 35.85,
        z2: 32.0,
        width: 0.3,
        level: 2,
    }
}

fn collision_world(floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    collision_world_with(&[], floors, ramps)
}

fn collision_world_with(walls: &[Wall], floors: &[Floor], ramps: &[Ramp]) -> CollisionWorld {
    CollisionWorld::from_map_layout(&MapLayout {
        walls: walls.to_vec(),
        ramps: ramps.to_vec(),
        floors: floors.to_vec(),
        wall_lights: vec![],
    })
}

fn test_entity(index: u32) -> Entity {
    Entity::from_raw_u32(index).expect("test entity index should be valid")
}

fn planned_move(entity: Entity, start: Position, target: Position) -> PlannedCharacterMove {
    PlannedCharacterMove {
        entity,
        start,
        target,
        target_vertical_velocity: 0.0,
        blocked: false,
    }
}

#[test]
fn overlapping_planned_characters_can_separate() {
    let first = planned_move(
        test_entity(1),
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position {
            x: -0.2,
            y: 0.0,
            z: 0.0,
        },
    );
    let second = planned_move(
        test_entity(2),
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 1.0, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(!overlaps_other_character(&first, &planned_moves));
    assert!(!overlaps_other_character(&second, &planned_moves));
}

#[test]
fn overlapping_planned_characters_cannot_move_deeper_together() {
    let first = planned_move(
        test_entity(1),
        Position { x: 0.0, y: 0.0, z: 0.0 },
        Position { x: 0.2, y: 0.0, z: 0.0 },
    );
    let second = planned_move(
        test_entity(2),
        Position { x: 0.8, y: 0.0, z: 0.0 },
        Position { x: 0.6, y: 0.0, z: 0.0 },
    );
    let planned_moves = [first, second];

    assert!(overlaps_other_character(&first, &planned_moves));
    assert!(overlaps_other_character(&second, &planned_moves));
}

#[test]
fn player_hits_wall_collider_from_collision_world() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
}

#[test]
fn repeated_wall_pressure_does_not_leak_through_wall() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let first = step_character_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
    let second = step_character_movement(
        &first.position,
        &motion,
        &collision_world,
        false,
        1.0,
        first.position.z,
        0.1,
    );

    assert!(first.blocked);
    assert!(second.blocked);
    assert!(second.position.x < 0.0);
}

#[test]
fn player_slides_along_wall_under_pressure() {
    let wall = test_wall();
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let first = step_character_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
    let second = step_character_movement(
        &first.position,
        &motion,
        &collision_world,
        false,
        1.0,
        first.position.z + 1.0,
        0.1,
    );

    assert!(second.blocked);
    assert!(second.position.x < 0.0);
    assert!(second.position.z > first.position.z);
}

#[test]
fn falling_player_pushing_into_wall_keeps_falling() {
    let wall = upper_horizontal_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: 30.391_533,
        y: 7.973_196,
        z: 31.539_902,
    };
    let motion = CharacterVerticalMotion {
        vertical_velocity: -PLAYER_TERMINAL_VELOCITY,
    };

    let step = step_character_movement(&pos, &motion, &collision_world, false, 30.394, 31.699, 0.0177);

    assert!(
        step.position.y < pos.y - 0.5,
        "expected falling to continue while sliding on wall, got {step:?}"
    );
    assert!(step.vertical_velocity < 0.0);
}

#[test]
fn diagonal_wall_hit_slides_in_same_step() {
    let wall = test_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
    assert!(step.position.z > 0.0);
}

#[test]
fn repeated_diagonal_wall_pressure_keeps_sliding() {
    let wall = Wall {
        x1: 0.0,
        z1: -100.0,
        x2: 0.0,
        z2: 100.0,
        width: WALL_THICKNESS,
        level: 0,
    };
    let floor = lower_floor();
    let collision_world = collision_world_with(&[wall], &[floor], &[]);
    let mut pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();
    let delta = 1.0 / 60.0;
    let velocity = Vec3::new(1.0, 0.0, 0.25).normalize() * PLAYER_SPEED;

    for _ in 0..120 {
        let step = step_character_movement(
            &pos,
            &motion,
            &collision_world,
            false,
            velocity.x.mul_add(delta, pos.x),
            velocity.z.mul_add(delta, pos.z),
            delta,
        );
        pos = step.position;
    }

    assert!(pos.x < 0.0);
    assert!(pos.z > 2.0);
}

#[test]
fn diagonal_wall_end_hit_slides_along_wall() {
    let wall = horizontal_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: -1.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

    assert!(step.blocked);
    assert!(step.position.x > pos.x);
    assert!(step.position.z < 0.0);
}

#[test]
fn phasing_player_ignores_wall_collider_from_collision_world() {
    let wall = test_wall();
    let collision_world = collision_world_with(&[wall], &[], &[]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, true, 1.0, pos.z, 0.1);

    assert!(!step.blocked);
    assert_eq!(step.position.x, 1.0);
}

#[test]
fn supported_player_can_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let mut motion = CharacterVerticalMotion::default();

    assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));
    assert_eq!(motion.vertical_velocity, PLAYER_JUMP_SPEED);
}

#[test]
fn airborne_player_cannot_start_jump() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
    let mut motion = CharacterVerticalMotion::default();

    assert!(!try_start_player_jump(
        &mut motion,
        &collision_world,
        &pos,
        pos.x,
        pos.z
    ));
    assert_eq!(motion.vertical_velocity, 0.0);
}

#[test]
fn upward_jump_velocity_moves_player_above_support() {
    let floor = lower_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let mut motion = CharacterVerticalMotion::default();
    assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));

    let step = step_character_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

    assert!(step.position.y > pos.y);
    assert!(step.vertical_velocity > 0.0);
}

#[test]
fn upward_motion_hits_floor_underside() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
    let motion = CharacterVerticalMotion {
        vertical_velocity: PLAYER_JUMP_SPEED,
    };

    let step = step_character_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

    assert_eq!(step.vertical_velocity, 0.0);
    assert!(step.position.y <= floor.y - floor.thickness);
}

#[test]
fn initial_ceiling_contact_does_not_cancel_horizontal_movement() {
    let floor = lower_floor();
    let ceiling = low_overhead_floor();
    let collision_world = collision_world(&[floor, ceiling], &[]);
    let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, 0.5, pos.z, 0.1);

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!((step.position.y - floor.y).abs() < 0.001);
    assert_eq!(step.vertical_velocity, 0.0);
}

#[test]
fn upward_motion_ignores_floor_underside_outside_footprint() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position { x: 5.0, y: 1.8, z: 0.0 };
    let motion = CharacterVerticalMotion {
        vertical_velocity: PLAYER_JUMP_SPEED,
    };

    let step = step_character_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

    assert!(step.vertical_velocity > 0.0);
    assert!(step.position.y > pos.y);
}

#[test]
fn upward_motion_under_floor_edge_hits_floor_side() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position {
        x: -5.0,
        y: 2.3,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion {
        vertical_velocity: PLAYER_JUMP_SPEED,
    };

    let step = step_character_movement(&pos, &motion, &collision_world, false, -4.25, pos.z, 0.1);

    assert!(step.blocked);
    assert!(step.position.x > pos.x);
}

#[test]
fn player_on_floor_top_can_move_over_adjacent_floor_slab_edge() {
    let floor = upper_floor();
    let collision_world = collision_world(&[floor], &[]);
    let pos = Position {
        x: -5.0,
        y: floor.y,
        z: 0.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, -3.75, pos.z, 0.1);

    assert!(!step.blocked);
    assert!(step.position.x > pos.x);
    assert!(
        step.position.y >= floor.y - 0.01,
        "expected player to remain near floor top, got {step:?}"
    );
}

#[test]
fn player_walking_off_ramp_side_is_not_blocked_by_ramp_side() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: ramp_surface_at(&ramp, 2.0, 4.0),
        z: 4.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

    assert!(!step.blocked);
    assert!(step.position.x < pos.x);
}

#[test]
fn lower_floor_player_hits_wedge_side_from_collision_world() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: -1.0,
        y: 0.0,
        z: 4.0,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

    assert!(step.blocked);
    assert!(step.position.x < 0.0);
}

#[test]
fn lower_floor_player_can_enter_wedge_low_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: 0.0,
        z: -0.25,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, pos.x, 0.25, 0.1);

    assert!(!step.blocked);
    assert!(step.position.z > pos.z);
}

#[test]
fn upper_floor_player_can_enter_wedge_high_end() {
    let ramp = test_ramp();
    let collision_world = collision_world(&[], &[ramp]);
    let pos = Position {
        x: 2.0,
        y: LEVEL_HEIGHT,
        z: 8.25,
    };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, pos.x, 7.75, 0.1);

    assert!(!step.blocked);
    assert!(step.position.z < pos.z);
}

#[test]
fn low_obstacle_clearance_allows_movement_off_ramp_side() {
    let ramp = test_ramp();
    let floor = upper_floor_west_of_ramp();
    let collision_world = collision_world(&[floor], &[ramp]);
    let y = ramp_surface_at(&ramp, 2.0, 7.0);
    let pos = Position { x: 2.0, y, z: 7.0 };
    let motion = CharacterVerticalMotion::default();

    let step = step_character_movement(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

    assert!(!step.blocked);
    assert!(step.position.x < pos.x);
}

#[test]
fn item_overlap_uses_vertical_distance() {
    let player = Position {
        x: 0.0,
        y: LEVEL_HEIGHT,
        z: 0.0,
    };
    let item = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert!(!overlap_player_vs_item(&player, &item, 1.0));
}

#[test]
fn item_overlap_allows_same_level_collection() {
    let player = Position {
        x: 0.25,
        y: 0.0,
        z: 0.25,
    };
    let item = Position { x: 0.0, y: 0.0, z: 0.0 };

    assert!(overlap_player_vs_item(&player, &item, 1.0));
}
