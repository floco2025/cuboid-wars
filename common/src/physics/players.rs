use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use super::{
    helpers::{Collision, Cuboid, sweep_aabb_vs_aabb, sweep_point_vs_cuboid, wall_cuboid},
    world::{Axis, CollisionShape, CollisionSolid, CollisionWorld, Wedge},
};
use crate::{
    constants::{
        PHYSICS_EPSILON, PLAYER_DEPTH, PLAYER_GRAVITY, PLAYER_HEIGHT, PLAYER_JUMP_SPEED, PLAYER_LANDING_EPSILON,
        PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH,
    },
    protocol::{Position, Wall},
};

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
    if motion.velocity.y > 0.0 || collision_world.find_support(x, z, pos.y).is_none() {
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
    let mut resolved = sweep_player_through_solids(pos, &target, collision_world, has_phasing);
    let mut vy = next_motion.velocity.y;

    if resolved.hit_floor_top && vy < 0.0 || resolved.hit_ceiling && vy > 0.0 {
        vy = 0.0;
    }

    if vy <= 0.0
        && let Some(support_y) =
            collision_world.find_support(resolved.position.x, resolved.position.z, resolved.position.y)
    {
        resolved.position.y = support_y;
        vy = 0.0;
    }

    PlayerMotionStep {
        position: resolved.position,
        vy,
        hit_horizontal: resolved.hit_horizontal,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct PlayerSweepResult {
    position: Position,
    hit_horizontal: bool,
    hit_floor_top: bool,
    hit_ceiling: bool,
}

fn sweep_player_through_solids(
    start: &Position,
    target: &Position,
    collision_world: &CollisionWorld,
    has_phasing: bool,
) -> PlayerSweepResult {
    let mut current = *start;
    let mut remaining = Vec3::from(*target) - Vec3::from(*start);
    let mut hit_horizontal = false;
    let mut hit_floor_top = false;
    let mut hit_ceiling = false;

    for _ in 0..3 {
        if remaining.length_squared() <= PHYSICS_EPSILON * PHYSICS_EPSILON {
            break;
        }

        let next = Position::from(Vec3::from(current) + remaining);
        let Some(collision) = earliest_player_solid_collision(&current, &next, collision_world, has_phasing) else {
            current = next;
            break;
        };

        current = Position::from(Vec3::from(current) + remaining * collision.t + collision.normal * PHYSICS_EPSILON);

        if collision.normal.y > 0.5 {
            hit_floor_top = true;
        } else if collision.normal.y < -0.5 {
            hit_ceiling = true;
        } else {
            hit_horizontal = true;
        }

        let remaining_after_hit = remaining * (1.0 - collision.t);
        remaining = remaining_after_hit - collision.normal * remaining_after_hit.dot(collision.normal);
    }

    PlayerSweepResult {
        position: current,
        hit_horizontal,
        hit_floor_top,
        hit_ceiling,
    }
}

fn earliest_player_solid_collision(
    start: &Position,
    target: &Position,
    collision_world: &CollisionWorld,
    has_phasing: bool,
) -> Option<Collision> {
    collision_world
        .solids
        .iter()
        .filter(|solid| !(has_phasing && solid.phasing_passthrough))
        .filter_map(|solid| sweep_player_vs_solid(start, target, solid))
        .min_by(|a, b| a.t.total_cmp(&b.t))
}

fn sweep_player_vs_solid(start: &Position, target: &Position, solid: &CollisionSolid) -> Option<Collision> {
    match solid.shape {
        CollisionShape::Cuboid(cuboid) => {
            if is_player_on_cuboid_top(start, cuboid) {
                None
            } else {
                sweep_player_vs_cuboid(start, target, cuboid)
            }
        }
        CollisionShape::Wedge(wedge) => {
            if is_player_on_wedge_top(start, wedge) {
                None
            } else {
                sweep_player_vs_wedge(start, target, wedge)
            }
        }
    }
}

fn is_player_on_cuboid_top(pos: &Position, cuboid: Cuboid) -> bool {
    let cuboid_top = cuboid.center.y + cuboid.half_extents.y;

    (pos.y - cuboid_top).abs() <= PLAYER_LANDING_EPSILON
}

fn sweep_player_vs_cuboid(start: &Position, target: &Position, cuboid: Cuboid) -> Option<Collision> {
    let expanded = Cuboid {
        center: cuboid.center,
        half_extents: cuboid.half_extents + Vec3::new(PLAYER_WIDTH / 2.0, PLAYER_HEIGHT / 2.0, PLAYER_DEPTH / 2.0),
    };
    let start_center = Position {
        x: start.x,
        y: start.y + PLAYER_HEIGHT / 2.0,
        z: start.z,
    };
    let ray_dir = Vec3::new(target.x - start.x, target.y - start.y, target.z - start.z);
    sweep_point_vs_cuboid(&start_center, ray_dir, expanded)
}

fn is_player_on_wedge_top(pos: &Position, wedge: Wedge) -> bool {
    wedge
        .surface_y_at(pos.x, pos.z)
        .is_some_and(|surface_y| (pos.y - surface_y).abs() <= PLAYER_LANDING_EPSILON)
}

fn sweep_player_vs_wedge(start: &Position, target: &Position, wedge: Wedge) -> Option<Collision> {
    let player_half_extents = Vec3::new(PLAYER_WIDTH / 2.0, PLAYER_HEIGHT / 2.0, PLAYER_DEPTH / 2.0);
    let start_center = Vec3::new(start.x, start.y + PLAYER_HEIGHT / 2.0, start.z);
    let ray_dir = Vec3::new(target.x - start.x, target.y - start.y, target.z - start.z);

    sweep_point_vs_planes(start_center, ray_dir, player_half_extents, &wedge_planes(wedge))
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Plane {
    normal: Vec3,
    d: f32,
}

fn wedge_planes(wedge: Wedge) -> [Plane; 6] {
    let bounds = wedge.bounds;
    let footprint = bounds.footprint();

    [
        plane(Vec3::NEG_X, -footprint.min_x),
        plane(Vec3::X, footprint.max_x),
        plane(Vec3::NEG_Z, -footprint.min_z),
        plane(Vec3::Z, footprint.max_z),
        plane(Vec3::NEG_Y, -bounds.min_y()),
        wedge_top_plane(wedge),
    ]
}

fn wedge_top_plane(wedge: Wedge) -> Plane {
    let min_y = wedge.bounds.min_y();
    let max_y = wedge.bounds.max_y();
    let slope = if (wedge.high_at - wedge.low_at).abs() < PHYSICS_EPSILON {
        0.0
    } else {
        (max_y - min_y) / (wedge.high_at - wedge.low_at)
    };

    match wedge.slope_axis {
        Axis::X => plane(Vec3::new(-slope, 1.0, 0.0), min_y - slope * wedge.low_at),
        Axis::Z => plane(Vec3::new(0.0, 1.0, -slope), min_y - slope * wedge.low_at),
    }
}

fn plane(normal: Vec3, d: f32) -> Plane {
    let length = normal.length();
    if length <= PHYSICS_EPSILON {
        Plane { normal, d }
    } else {
        Plane {
            normal: normal / length,
            d: d / length,
        }
    }
}

fn sweep_point_vs_planes(start: Vec3, ray_dir: Vec3, half_extents: Vec3, planes: &[Plane]) -> Option<Collision> {
    let mut t_enter = 0.0_f32;
    let mut t_exit = 1.0_f32;
    let mut hit_normal = Vec3::ZERO;

    for plane in planes {
        let expanded_d = plane.d + half_extents.dot(plane.normal.abs());
        let dist = plane.normal.dot(start) - expanded_d;
        let denom = plane.normal.dot(ray_dir);

        if denom.abs() < PHYSICS_EPSILON {
            if dist > 0.0 {
                return None;
            }
            continue;
        }

        let t = -dist / denom;
        if denom < 0.0 {
            if t > t_enter {
                t_enter = t;
                hit_normal = plane.normal;
            }
        } else if t < t_exit {
            t_exit = t;
        }

        if t_enter > t_exit {
            return None;
        }
    }

    if t_exit < 0.0 || t_enter > 1.0 || hit_normal == Vec3::ZERO {
        return None;
    }

    Some(Collision {
        normal: hit_normal,
        t: t_enter.clamp(0.0, 1.0),
    })
}

#[must_use]
pub fn sweep_player_vs_player(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    sweep_aabb_vs_aabb(start1, end1, start2, end2, PLAYER_WIDTH, PLAYER_DEPTH, PLAYER_HEIGHT)
}

// Check if the player's AABB currently overlaps an axis-aligned wall.
#[must_use]
pub fn overlap_player_vs_wall(pos: &Position, wall: &Wall) -> bool {
    let wall = wall_cuboid(wall, 0.0);
    let player_center = Vec3::new(pos.x, pos.y + PLAYER_HEIGHT / 2.0, pos.z);
    let player_half_extents = Vec3::new(PLAYER_WIDTH / 2.0, PLAYER_HEIGHT / 2.0, PLAYER_DEPTH / 2.0);

    (player_center.x - wall.center.x).abs() <= player_half_extents.x + wall.half_extents.x
        && (player_center.y - wall.center.y).abs() <= player_half_extents.y + wall.half_extents.y
        && (player_center.z - wall.center.z).abs() <= player_half_extents.z + wall.half_extents.z
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
        let collision_world = collision_world_with(&[wall], &[], &[]);
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
        let collision_world = collision_world_with(&[wall], &[], &[]);
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
    fn upward_motion_into_floor_slab_side_is_horizontal_collision() {
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

        assert!(step.hit_horizontal);
        assert!(step.vy > 0.0);
        assert!(step.position.y > pos.y);
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
        assert!(step.position.y < floor.y);
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
    fn player_hits_floor_slab_side_when_moving_off_ramp_into_floor() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let collision_world = collision_world(&[floor], &[ramp]);
        let y = ramp_surface_at(&ramp, 2.0, 7.0);
        let pos = Position { x: 2.0, y, z: 7.0 };
        let motion = PlayerMotion::default();

        let step = step_player_motion(&pos, &motion, &collision_world, false, -1.0, pos.z, 0.1);

        assert!(step.hit_horizontal);
    }
}
