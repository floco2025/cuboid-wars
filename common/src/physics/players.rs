use bevy_ecs::prelude::*;
use bevy_math::Vec3;
use rapier3d::{
    control::{CharacterAutostep, CharacterLength, KinematicCharacterController},
    parry::{
        query::{ShapeCastOptions, cast_shapes, intersection_test},
        shape::Cuboid,
    },
    prelude::{Pose, Vector},
};

use super::world::{CollisionWorld, ShapeCastHit};
use crate::{
    constants::{
        PHYSICS_EPSILON, PLAYER_DEPTH, PLAYER_FOOT_CLEARANCE, PLAYER_GRAVITY, PLAYER_GROUND_SNAP_DISTANCE,
        PLAYER_HEIGHT, PLAYER_JUMP_SPEED, PLAYER_STEP_HEIGHT, PLAYER_STEP_MIN_WIDTH, PLAYER_SUPPORT_PROBE_DEPTH,
        PLAYER_SUPPORT_PROBE_WIDTH, PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH,
    },
    protocol::Position,
};

const PLAYER_CONTACT_OFFSET: f32 = 0.01;
const PLAYER_BLOCKED_MOVEMENT_EPSILON: f32 = 0.01;
const PLAYER_AUTOSTEP_EPSILON: f32 = 0.01;

// Component attached to player entities tracking persistent gravity-axis motion.
// Player-controlled X/Z movement is derived from input each tick. Running on a
// ramp can add Y displacement for that frame, but it is not stored as velocity.
#[derive(Component, Default)]
pub struct PlayerVerticalMotion {
    pub vertical_velocity: f32,
}

impl PlayerVerticalMotion {
    pub fn apply_gravity(&mut self, delta: f32) {
        self.vertical_velocity -= PLAYER_GRAVITY * delta;
    }

    pub fn apply_terminal_velocity(&mut self) {
        if self.vertical_velocity < -PLAYER_TERMINAL_VELOCITY {
            self.vertical_velocity = -PLAYER_TERMINAL_VELOCITY;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerMovementResult {
    pub position: Position,
    pub vertical_velocity: f32,
    // True when static-world collision materially blocked requested movement.
    // Side contacts that Rapier resolves by auto-stepping are not treated as blocked.
    pub blocked: bool,
}

// Represents a player's intended movement after static-world collision but before
// player-player collision.
#[derive(Copy, Clone)]
pub struct PlannedMove {
    pub entity: Entity,
    pub start: Position,
    pub target: Position,
    pub target_vertical_velocity: f32,
    pub blocked: bool,
}

// Check if a planned move would overlap with any other player's planned position.
#[must_use]
pub fn overlaps_other_player(candidate: &PlannedMove, planned_moves: &[PlannedMove]) -> bool {
    planned_moves.iter().any(|other| {
        other.entity != candidate.entity
            && player_paths_intersect(&candidate.start, &candidate.target, &other.start, &other.target)
    })
}

#[must_use]
pub fn try_start_player_jump(
    motion: &mut PlayerVerticalMotion,
    collision_world: &CollisionWorld,
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    let ground_probe_pos = Position { x, y: pos.y, z };
    if motion.vertical_velocity > 0.0 || !is_player_grounded(collision_world, &ground_probe_pos) {
        return false;
    }

    motion.vertical_velocity = PLAYER_JUMP_SPEED;
    true
}

#[must_use]
pub fn step_player_movement(
    pos: &Position,
    motion: &PlayerVerticalMotion,
    collision_world: &CollisionWorld,
    has_phasing: bool,
    x: f32,
    z: f32,
    delta: f32,
) -> PlayerMovementResult {
    let character_shape = player_shape();
    let character_pos = player_pose(pos);
    let support_shape = player_support_probe_shape();
    let current_ground = if motion.vertical_velocity <= 0.0 {
        player_ground_hit(collision_world, &support_shape, pos, has_phasing)
    } else {
        None
    };
    let mut next_motion = PlayerVerticalMotion {
        vertical_velocity: motion.vertical_velocity,
    };
    let can_follow_ground = next_motion.vertical_velocity <= 0.0;
    if current_ground.is_some() && can_follow_ground {
        next_motion.vertical_velocity = 0.0;
    } else {
        next_motion.apply_gravity(delta);
        next_motion.apply_terminal_velocity();
    }
    let controller = player_controller();

    let target = Position {
        x,
        y: next_motion.vertical_velocity.mul_add(delta, pos.y),
        z,
    };
    let input_move = Vector::new(target.x - pos.x, 0.0, target.z - pos.z);
    let gravity_axis_move = Vector::new(0.0, target.y - pos.y, 0.0);
    let supported_input_move = current_ground.map_or(input_move, |ground| {
        project_input_move_onto_support(input_move, ground.normal)
    });
    let requested_move = supported_input_move + gravity_axis_move;
    let mut saw_side_contact = false;
    let mut hit_ceiling = false;
    let movement = collision_world.move_character(
        delta,
        &controller,
        &character_shape,
        &character_pos,
        requested_move,
        has_phasing,
        true,
        |collision| {
            let normal = vec3(collision.hit.normal1);
            let is_side_contact = normal.y.abs() <= 0.5;
            let is_ceiling = normal.y < -0.5 && gravity_axis_move.y > 0.0;
            if is_side_contact {
                saw_side_contact = true;
            }
            if is_ceiling {
                hit_ceiling = true;
            }
        },
    );
    let mut resolved = Position {
        x: pos.x + movement.translation.x,
        y: pos.y + movement.translation.y,
        z: pos.z + movement.translation.z,
    };
    let resolved_ground = if can_follow_ground {
        player_ground_hit(collision_world, &support_shape, &resolved, has_phasing)
    } else {
        None
    };
    if let Some(ground) = resolved_ground {
        resolved.y -= ground.t - PLAYER_FOOT_CLEARANCE;
    }
    let mut vertical_velocity = next_motion.vertical_velocity;
    // Rapier reports a side contact while auto-stepping over slab/trim edges.
    // That is normal movement, not a wall hit, so don't expose it as blocked.
    let stepped_up = movement.grounded && movement.translation.y > requested_move.y + PLAYER_AUTOSTEP_EPSILON;
    let blocked =
        saw_side_contact && !stepped_up && movement_progress_was_blocked(supported_input_move, movement.translation);

    let grounded = resolved_ground.is_some();
    if grounded && vertical_velocity < 0.0
        || hit_ceiling && vertical_velocity > 0.0
        || gravity_axis_move.y > 0.0 && movement.translation.y < requested_move.y - PHYSICS_EPSILON
    {
        vertical_velocity = 0.0;
    }

    PlayerMovementResult {
        position: resolved,
        vertical_velocity,
        blocked,
    }
}

fn movement_progress_was_blocked(desired: Vector, actual: Vector) -> bool {
    let desired_xz = Vec3::new(desired.x, 0.0, desired.z);
    let desired_len = desired_xz.length();
    if desired_len <= PLAYER_BLOCKED_MOVEMENT_EPSILON {
        return false;
    }

    let actual_xz = Vec3::new(actual.x, 0.0, actual.z);
    let desired_dir = desired_xz / desired_len;
    let actual_along_desired = actual_xz.dot(desired_dir);
    actual_along_desired < desired_len - PLAYER_BLOCKED_MOVEMENT_EPSILON
}

fn is_player_grounded(collision_world: &CollisionWorld, pos: &Position) -> bool {
    let shape = player_support_probe_shape();
    player_ground_hit(collision_world, &shape, pos, false).is_some()
}

fn player_ground_hit(
    collision_world: &CollisionWorld,
    shape: &Cuboid,
    pos: &Position,
    has_phasing: bool,
) -> Option<ShapeCastHit> {
    let pose = player_support_probe_pose(pos);
    collision_world.ground_hit(
        shape,
        &pose,
        PLAYER_GROUND_SNAP_DISTANCE + PLAYER_FOOT_CLEARANCE,
        0.0,
        has_phasing,
    )
}

fn player_controller() -> KinematicCharacterController {
    KinematicCharacterController {
        offset: CharacterLength::Absolute(PLAYER_CONTACT_OFFSET),
        autostep: Some(CharacterAutostep {
            max_height: CharacterLength::Absolute(PLAYER_STEP_HEIGHT),
            min_width: CharacterLength::Absolute(PLAYER_STEP_MIN_WIDTH),
            include_dynamic_bodies: false,
        }),
        min_slope_slide_angle: std::f32::consts::FRAC_PI_3,
        snap_to_ground: None,
        ..KinematicCharacterController::default()
    }
}

fn player_shape() -> Cuboid {
    Cuboid::new(Vector::new(
        PLAYER_WIDTH / 2.0,
        player_collision_height() / 2.0,
        PLAYER_DEPTH / 2.0,
    ))
}

fn player_collision_height() -> f32 {
    // The logical foot position remains at `Position.y`; the collider starts a
    // little above it so floor-side faces at sole height don't block movement.
    PLAYER_HEIGHT - PLAYER_FOOT_CLEARANCE
}

fn player_support_probe_shape() -> Cuboid {
    Cuboid::new(Vector::new(
        PLAYER_SUPPORT_PROBE_WIDTH / 2.0,
        player_collision_height() / 2.0,
        PLAYER_SUPPORT_PROBE_DEPTH / 2.0,
    ))
}

fn project_input_move_onto_support(input_move: Vector, support_normal: Vec3) -> Vector {
    if input_move.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON {
        return input_move;
    }

    let input_move = Vec3::new(input_move.x, input_move.y, input_move.z);
    let tangent = input_move - support_normal * input_move.dot(support_normal);
    let Some(tangent_dir) = tangent.try_normalize() else {
        return Vector::new(input_move.x, input_move.y, input_move.z);
    };

    let surface_move = tangent_dir * input_move.length();
    Vector::new(surface_move.x, surface_move.y, surface_move.z)
}

fn player_pose(pos: &Position) -> Pose {
    Pose::translation(
        pos.x,
        pos.y + PLAYER_FOOT_CLEARANCE + player_collision_height() / 2.0,
        pos.z,
    )
}

fn player_support_probe_pose(pos: &Position) -> Pose {
    player_pose(pos)
}

fn vec3(v: Vector) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[must_use]
pub fn player_paths_intersect(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
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

#[must_use]
pub fn overlap_player_vs_item(player_pos: &Position, item_pos: &Position, collection_radius: f32) -> bool {
    let dx = player_pos.x - item_pos.x;
    let dy = player_pos.y - item_pos.y;
    let dz = player_pos.z - item_pos.z;
    let dist_sq = dx.mul_add(dx, dy.mul_add(dy, dz * dz));
    dist_sq <= collection_radius * collection_radius
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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let first = step_player_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
        let second = step_player_movement(
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
        let motion = PlayerVerticalMotion::default();

        let first = step_player_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);
        let second = step_player_movement(
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
        let motion = PlayerVerticalMotion {
            vertical_velocity: -PLAYER_TERMINAL_VELOCITY,
        };

        let step = step_player_movement(&pos, &motion, &collision_world, false, 30.394, 31.699, 0.0177);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        for _ in 0..20 {
            let step = step_player_movement(&pos, &motion, &collision_world, false, 1.0, pos.z + 0.25, 0.1);
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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, 1.0, 1.0, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, true, 1.0, pos.z, 0.1);

        assert!(!step.blocked);
        assert_eq!(step.position.x, 1.0);
    }

    #[test]
    fn supported_player_can_start_jump() {
        let floor = lower_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let mut motion = PlayerVerticalMotion::default();

        assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));
        assert_eq!(motion.vertical_velocity, PLAYER_JUMP_SPEED);
    }

    #[test]
    fn airborne_player_cannot_start_jump() {
        let floor = lower_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
        let mut motion = PlayerVerticalMotion::default();

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
        let mut motion = PlayerVerticalMotion::default();
        assert!(try_start_player_jump(&mut motion, &collision_world, &pos, pos.x, pos.z));

        let step = step_player_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

        assert!(step.position.y > pos.y);
        assert!(step.vertical_velocity > 0.0);
    }

    #[test]
    fn upward_motion_hits_floor_underside() {
        let floor = upper_floor();
        let collision_world = collision_world(&[floor], &[]);
        let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
        let motion = PlayerVerticalMotion {
            vertical_velocity: PLAYER_JUMP_SPEED,
        };

        let step = step_player_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

        assert_eq!(step.vertical_velocity, 0.0);
        assert!(step.position.y <= floor.y - floor.thickness);
    }

    #[test]
    fn initial_ceiling_contact_does_not_cancel_horizontal_movement() {
        let floor = lower_floor();
        let ceiling = low_overhead_floor();
        let collision_world = collision_world(&[floor, ceiling], &[]);
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, 0.5, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion {
            vertical_velocity: PLAYER_JUMP_SPEED,
        };

        let step = step_player_movement(&pos, &motion, &collision_world, false, pos.x, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion {
            vertical_velocity: PLAYER_JUMP_SPEED,
        };

        let step = step_player_movement(&pos, &motion, &collision_world, false, -4.25, pos.z, 0.1);

        assert!(step.blocked);
        assert!(step.vertical_velocity > 0.0);
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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, -3.75, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, 1.0, pos.z, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, pos.x, 0.25, 0.1);

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
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, pos.x, 7.75, 0.1);

        assert!(!step.blocked);
        assert!(step.position.z < pos.z);
    }

    #[test]
    fn floor_slab_side_blocks_movement_off_ramp_when_support_probe_leaves_ramp() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let collision_world = collision_world(&[floor], &[ramp]);
        let y = ramp_surface_at(&ramp, 2.0, 7.0);
        let pos = Position { x: 2.0, y, z: 7.0 };
        let motion = PlayerVerticalMotion::default();

        let step = step_player_movement(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

        assert!(step.blocked);
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
}
