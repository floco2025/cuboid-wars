use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterLength, KinematicCharacterController},
    geometry::Cuboid,
    parry::query::{ShapeCastOptions, cast_shapes, intersection_test},
    prelude::{Pose, Vector},
};

use super::world::CollisionWorld;
use crate::{
    constants::{
        PHYSICS_EPSILON, PLAYER_DEPTH, PLAYER_GRAVITY, PLAYER_HEIGHT, PLAYER_JUMP_SPEED, PLAYER_LANDING_EPSILON,
        PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH,
    },
    protocol::Position,
};

const PLAYER_CONTACT_OFFSET: f32 = 0.01;

// Component attached to player entities tracking 3D velocity for gravity and falling.
// Horizontal motion is still derived from `Speed` each tick; the vertical component
// here drives gravity/landing physics.
#[derive(Component, Default)]
pub struct PlayerMotion {
    pub velocity: Vec3,
}

impl PlayerMotion {
    pub fn apply_gravity(&mut self, delta: f32) {
        self.velocity.y -= PLAYER_GRAVITY * delta;
    }

    pub fn apply_terminal_velocity(&mut self) {
        if self.velocity.y < -PLAYER_TERMINAL_VELOCITY {
            self.velocity.y = -PLAYER_TERMINAL_VELOCITY;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerMotionStep {
    pub position: Position,
    pub vy: f32,
    pub hit_horizontal: bool,
}

#[must_use]
pub fn try_start_player_jump(
    motion: &mut PlayerMotion,
    collision_world: &CollisionWorld,
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    let ground_probe_pos = Position { x, y: pos.y, z };
    if motion.velocity.y > 0.0 || !is_player_grounded(collision_world, &ground_probe_pos) {
        return false;
    }

    motion.velocity.y = PLAYER_JUMP_SPEED;
    true
}

#[must_use]
pub fn step_player_motion(
    pos: &Position,
    motion: &PlayerMotion,
    collision_world: &CollisionWorld,
    has_phasing: bool,
    x: f32,
    z: f32,
    delta: f32,
) -> PlayerMotionStep {
    let mut next_motion = PlayerMotion {
        velocity: motion.velocity,
    };
    next_motion.apply_gravity(delta);
    next_motion.apply_terminal_velocity();

    let target = Position {
        x,
        y: next_motion.velocity.y.mul_add(delta, pos.y),
        z,
    };
    let desired_horizontal = Vector::new(target.x - pos.x, 0.0, target.z - pos.z);
    let desired_vertical = Vector::new(0.0, target.y - pos.y, 0.0);
    let character_shape = player_shape();
    let character_pos = player_pose(pos);
    let controller = player_controller();
    let mut hit_horizontal = false;
    let mut hit_ceiling = false;
    let horizontal_movement = collision_world.move_character(
        delta,
        &controller,
        &character_shape,
        &character_pos,
        desired_horizontal,
        has_phasing,
        false,
        |collision| {
            let normal = vec3(collision.hit.normal2);
            if normal.y.abs() <= 0.5 {
                hit_horizontal = true;
            }
        },
    );
    let after_horizontal = Position {
        x: pos.x + horizontal_movement.translation.x,
        y: pos.y + horizontal_movement.translation.y,
        z: pos.z + horizontal_movement.translation.z,
    };
    let vertical_pose = player_pose(&after_horizontal);
    let vertical_movement = collision_world.move_character(
        delta,
        &controller,
        &character_shape,
        &vertical_pose,
        desired_vertical,
        has_phasing,
        true,
        |collision| {
            let normal = vec3(collision.hit.normal2);
            if normal.y < -0.5 && desired_vertical.y > 0.0 {
                hit_ceiling = true;
            }
        },
    );
    let resolved = Position {
        x: after_horizontal.x + vertical_movement.translation.x,
        y: after_horizontal.y + vertical_movement.translation.y,
        z: after_horizontal.z + vertical_movement.translation.z,
    };
    let mut vy = next_motion.velocity.y;

    if vertical_movement.grounded && vy < 0.0
        || hit_ceiling && vy > 0.0
        || desired_vertical.y > 0.0 && vertical_movement.translation.y < desired_vertical.y - PHYSICS_EPSILON
    {
        vy = 0.0;
    }

    PlayerMotionStep {
        position: resolved,
        vy,
        hit_horizontal,
    }
}

fn is_player_grounded(collision_world: &CollisionWorld, pos: &Position) -> bool {
    let controller = player_controller();
    let shape = player_shape();
    let pose = player_pose(pos);
    collision_world
        .move_character(0.0, &controller, &shape, &pose, Vector::ZERO, false, true, |_| {})
        .grounded
}

fn player_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(PLAYER_CONTACT_OFFSET),
        min_slope_slide_angle: std::f32::consts::PI,
        snap_to_ground: Some(CharacterLength::Absolute(PLAYER_LANDING_EPSILON)),
        ..KinematicCharacterController::default()
    }
}

fn player_shape() -> Cuboid {
    Cuboid::new(Vector::new(PLAYER_WIDTH / 2.0, PLAYER_HEIGHT / 2.0, PLAYER_DEPTH / 2.0))
}

fn player_pose(pos: &Position) -> Pose {
    Pose::translation(pos.x, pos.y + PLAYER_HEIGHT / 2.0, pos.z)
}

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[must_use]
pub fn sweep_player_vs_player(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    let shape = player_shape();
    let velocity1 = Vector::new(end1.x - start1.x, end1.y - start1.y, end1.z - start1.z);
    let velocity2 = Vector::new(end2.x - start2.x, end2.y - start2.y, end2.z - start2.z);
    if intersection_test(&player_pose(start1), &shape, &player_pose(start2), &shape).is_ok_and(|overlaps| overlaps) {
        return true;
    }

    let options = ShapeCastOptions {
        max_time_of_impact: 1.0,
        ..ShapeCastOptions::default()
    };

    cast_shapes(
        &player_pose(start1),
        velocity1,
        &shape,
        &player_pose(start2),
        velocity2,
        &shape,
        options,
    )
    .is_ok_and(|hit| hit.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        constants::{FLOOR_THICKNESS, LEVEL_HEIGHT},
        map::ramp_surface_at,
        protocol::{Floor, MapLayout, Ramp, Wall},
    };

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

    #[test]
    fn player_hits_wall_cuboid_from_collision_world() {
        let wall = test_wall();
        let floor = lower_floor();
        let collision_world = collision_world_with(&[wall], &[floor], &[]);
        let pos = Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        };
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

        assert!(step.hit_horizontal);
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
        let motion = PlayerMotion::default();

        let first = step_player_motion(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
        let second = step_player_motion(
            &first.position,
            &motion,
            &collision_world,
            false,
            1.0,
            first.position.z,
            0.1,
        );

        assert!(first.hit_horizontal);
        assert!(second.hit_horizontal);
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
        let motion = PlayerMotion::default();

        let first = step_player_motion(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
        let second = step_player_motion(
            &first.position,
            &motion,
            &collision_world,
            false,
            1.0,
            first.position.z + 1.0,
            0.1,
        );

        assert!(second.hit_horizontal);
        assert!(second.position.x < 0.0);
        assert!(second.position.z > first.position.z);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

        assert!(step.hit_horizontal);
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
            width: 0.2,
            level: 0,
        };
        let floor = lower_floor();
        let collision_world = collision_world_with(&[wall], &[floor], &[]);
        let mut pos = Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        };
        let motion = PlayerMotion::default();

        for _ in 0..20 {
            let step = step_player_motion(&pos, &motion, &collision_world, false, 1.0, pos.z + 0.25, 0.1);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

        assert!(step.hit_horizontal);
        assert!(step.position.x > pos.x);
        assert!(step.position.z < 0.0);
    }

    #[test]
    fn phasing_player_ignores_wall_cuboid_from_collision_world() {
        let wall = test_wall();
        let collision_world = collision_world_with(&[wall], &[], &[]);
        let pos = Position {
            x: -1.0,
            y: 0.0,
            z: 0.0,
        };
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, true, 1.0, pos.z, 0.1);

        assert!(!step.hit_horizontal);
        assert_eq!(step.position.x, 1.0);
    }

    #[test]
    fn supported_player_can_start_jump() {
        let floor = lower_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let mut motion = PlayerMotion::default();

        assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));
        assert_eq!(motion.velocity.y, PLAYER_JUMP_SPEED);
    }

    #[test]
    fn airborne_player_cannot_start_jump() {
        let floor = lower_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
        let mut motion = PlayerMotion::default();

        assert!(!try_start_player_jump(
            &mut motion,
            &collision_world,
            &pos,
            pos.x,
            pos.z
        ));
        assert_eq!(motion.velocity.y, 0.0);
    }

    #[test]
    fn upward_jump_velocity_moves_player_above_support() {
        let floor = lower_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let mut motion = PlayerMotion::default();
        assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));

        let step = step_player_motion(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

        assert!(step.position.y > pos.y);
        assert!(step.vy > 0.0);
    }

    #[test]
    fn upward_motion_hits_floor_underside() {
        let floor = upper_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_motion(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

        assert_eq!(step.vy, 0.0);
        assert!(step.position.y <= floor.y - floor.thickness);
    }

    #[test]
    fn initial_ceiling_contact_does_not_cancel_horizontal_movement() {
        let floor = lower_floor();
        let ceiling = low_overhead_floor();
        let collision_world = collision_world(&[floor, ceiling], &[]);
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, 0.5, pos.z, 0.1);

        assert!(!step.hit_horizontal);
        assert!(step.position.x > pos.x);
        assert_eq!(step.position.y, floor.y);
        assert_eq!(step.vy, 0.0);
    }

    #[test]
    fn upward_motion_ignores_floor_underside_outside_footprint() {
        let floor = upper_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 5.0, y: 1.8, z: 0.0 };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_motion(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

        assert!(step.vy > 0.0);
        assert!(step.position.y > pos.y);
    }

    #[test]
    fn upward_motion_under_floor_edge_hits_ceiling_not_horizontal() {
        let floor = upper_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position {
            x: -5.0,
            y: 2.3,
            z: 0.0,
        };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_motion(&pos, &motion, &collision_world, false, -4.25, pos.z, 0.1);

        assert!(!step.hit_horizontal);
        assert_eq!(step.vy, 0.0);
        assert!(step.position.y <= floor.y - floor.thickness);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, -4.25, pos.z, 0.1);

        assert!(!step.hit_horizontal);
        assert!(step.position.x > pos.x);
        assert!(step.position.y >= floor.y);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

        assert!(!step.hit_horizontal);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

        assert!(step.hit_horizontal);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, pos.x, 0.25, 0.1);

        assert!(!step.hit_horizontal);
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
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, pos.x, 7.75, 0.1);

        assert!(!step.hit_horizontal);
        assert!(step.position.z < pos.z);
    }

    #[test]
    fn floor_slab_side_does_not_block_horizontal_movement_off_ramp() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let collision_world = collision_world(&[floor], &[ramp]);
        let y = ramp_surface_at(&ramp, 2.0, 7.0);
        let pos = Position { x: 2.0, y, z: 7.0 };
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

        assert!(!step.hit_horizontal);
        assert!(step.position.x < pos.x);
    }
}
