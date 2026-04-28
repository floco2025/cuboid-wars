use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use super::helpers::{
    overlap_aabb_vs_wall, slide_along_axes, sweep_aabb_vs_aabb, sweep_aabb_vs_wall, sweep_ramp_edges,
    sweep_ramp_high_cap, sweep_slab_interval,
};
use crate::{
    constants::{
        PHYSICS_EPSILON, PLAYER_DEPTH, PLAYER_GRAVITY, PLAYER_HEIGHT, PLAYER_JUMP_SPEED, PLAYER_LANDING_EPSILON,
        PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH, WALL_THICKNESS,
    },
    map::{find_support_floor, ramp_surface_at},
    protocol::{Floor, Position, Ramp, Wall},
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
pub struct PlayerVerticalStep {
    pub y: f32,
    pub vy: f32,
}

#[must_use]
pub fn try_start_player_jump(
    motion: &mut PlayerMotion,
    floors: &[Floor],
    ramps: &[Ramp],
    pos: &Position,
    x: f32,
    z: f32,
) -> bool {
    if motion.velocity.y > 0.0 || find_support_floor(floors, ramps, x, z, pos.y).is_none() {
        return false;
    }

    motion.velocity.y = PLAYER_JUMP_SPEED;
    true
}

#[must_use]
pub fn step_player_vertical_motion(
    pos: &Position,
    motion: &PlayerMotion,
    floors: &[Floor],
    ramps: &[Ramp],
    x: f32,
    z: f32,
    delta: f32,
) -> PlayerVerticalStep {
    let mut next_motion = PlayerMotion {
        velocity: motion.velocity,
    };
    next_motion.apply_gravity(delta);
    next_motion.apply_terminal_velocity();

    let raw_y = next_motion.velocity.y.mul_add(delta, pos.y);
    if next_motion.velocity.y > 0.0
        && let Some(ceiling_y) = earliest_floor_slab_collision_y(floors, pos.x, pos.z, x, z, pos.y, raw_y)
    {
        return PlayerVerticalStep { y: ceiling_y, vy: 0.0 };
    }

    let support = find_support_floor(floors, ramps, x, z, pos.y);
    if next_motion.velocity.y <= 0.0
        && let Some(support_y) = support
    {
        PlayerVerticalStep { y: support_y, vy: 0.0 }
    } else {
        PlayerVerticalStep {
            y: raw_y,
            vy: next_motion.velocity.y,
        }
    }
}

fn earliest_floor_slab_collision_y(
    floors: &[Floor],
    start_x: f32,
    start_z: f32,
    end_x: f32,
    end_z: f32,
    start_y: f32,
    end_y: f32,
) -> Option<f32> {
    floors
        .iter()
        .filter_map(|floor| sweep_upward_player_vs_floor_slab(start_x, start_z, end_x, end_z, start_y, end_y, floor))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, y)| y)
}

fn sweep_upward_player_vs_floor_slab(
    start_x: f32,
    start_z: f32,
    end_x: f32,
    end_z: f32,
    start_y: f32,
    end_y: f32,
    floor: &Floor,
) -> Option<(f32, f32)> {
    let floor_bottom = floor.y - floor.thickness;
    let floor_top = floor.y;
    if start_y >= floor_top - PHYSICS_EPSILON {
        return None;
    }

    let dy = end_y - start_y;
    if dy <= PHYSICS_EPSILON {
        return None;
    }

    let start_top = start_y + PLAYER_HEIGHT;
    let end_top = end_y + PLAYER_HEIGHT;
    if end_top < floor_bottom - PHYSICS_EPSILON {
        return None;
    }

    let vertical_enter_t = if start_top >= floor_bottom - PHYSICS_EPSILON {
        0.0
    } else {
        ((floor_bottom - start_top) / dy).clamp(0.0, 1.0)
    };
    let vertical_exit_t = if end_y <= floor_top + PHYSICS_EPSILON {
        1.0
    } else {
        ((floor_top - start_y) / dy).clamp(0.0, 1.0)
    };
    if vertical_enter_t > vertical_exit_t {
        return None;
    }

    let (xz_enter_t, xz_exit_t) = player_floor_footprint_overlap_interval(start_x, start_z, end_x, end_z, floor)?;
    let hit_t = vertical_enter_t.max(xz_enter_t);
    if hit_t > vertical_exit_t.min(xz_exit_t) {
        return None;
    }

    Some((hit_t, floor_bottom - PLAYER_HEIGHT - PHYSICS_EPSILON))
}

fn player_floor_footprint_overlap_interval(
    start_x: f32,
    start_z: f32,
    end_x: f32,
    end_z: f32,
    floor: &Floor,
) -> Option<(f32, f32)> {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    let center_x = f32::midpoint(min_x, max_x);
    let center_z = f32::midpoint(min_z, max_z);
    let half_x = (max_x - min_x) / 2.0 + PLAYER_WIDTH / 2.0;
    let half_z = (max_z - min_z) / 2.0 + PLAYER_DEPTH / 2.0;
    let dx = end_x - start_x;
    let dz = end_z - start_z;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    let (new_min, new_max) = sweep_slab_interval(start_x - center_x, dx, half_x, t_min, t_max)?;
    t_min = new_min;
    t_max = new_max;

    let (new_min, new_max) = sweep_slab_interval(start_z - center_z, dz, half_z, t_min, t_max)?;
    Some((new_min, new_max))
}

#[must_use]
pub fn sweep_player_vs_wall(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    sweep_aabb_vs_wall(start_pos, end_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

#[must_use]
pub fn sweep_player_vs_ramp_edges(start_pos: &Position, end_pos: &Position, ramp: &Ramp, floors: &[Floor]) -> bool {
    // Skip ramps whose vertical extent doesn't overlap the player's body.
    // Without this, a player walking on level 0 collides with the side of a
    // ramp that sits on level 2.
    let player_y_low = start_pos.y.min(end_pos.y);
    let player_y_high = start_pos.y.max(end_pos.y) + PLAYER_HEIGHT;
    let ramp_y_low = ramp.y1.min(ramp.y2);
    let ramp_y_high = ramp.y1.max(ramp.y2);
    if player_y_high < ramp_y_low || player_y_low > ramp_y_high {
        return false;
    }

    let half_x = PLAYER_WIDTH / 2.0;
    let half_z = PLAYER_DEPTH / 2.0;
    let edge_half = WALL_THICKNESS / 2.0;

    let side_blocked = should_block_ramp_sides(start_pos, end_pos, ramp, floors)
        && sweep_ramp_edges(start_pos, end_pos, ramp, half_x, half_z, edge_half);

    // The high-cap is meant to block players at the bottom of a ramp from
    // walking into its tall face. Apply when the player's feet sit roughly at
    // the ramp's low edge, generalized from the old "y <= 0.1" check.
    let on_low_edge = (start_pos.y - ramp_y_low).abs() <= 0.2;
    let high_cap_blocked = on_low_edge && sweep_ramp_high_cap(start_pos, end_pos, ramp, half_x, half_z, edge_half);

    side_blocked || high_cap_blocked
}

fn should_block_ramp_sides(start_pos: &Position, end_pos: &Position, ramp: &Ramp, floors: &[Floor]) -> bool {
    let ramp_y_low = ramp.y1.min(ramp.y2);
    let ramp_y_high = ramp.y1.max(ramp.y2);

    if player_is_on_ramp_surface(start_pos, ramp) {
        return player_would_hit_floor_slab(end_pos, floors);
    }

    // Upper-floor players should be able to fall into the ramp opening instead
    // of colliding with invisible ramp side rails.
    if start_pos.y >= ramp_y_high - PLAYER_LANDING_EPSILON {
        return false;
    }

    // Preserve the old guard-rail behavior for lower-floor players beside the
    // ramp, so they slide along the side instead of entering through it.
    start_pos.y <= ramp_y_low + PLAYER_LANDING_EPSILON
}

fn player_is_on_ramp_surface(pos: &Position, ramp: &Ramp) -> bool {
    let (min_x, max_x, min_z, max_z) = ramp.bounds_xz();
    if pos.x < min_x || pos.x > max_x || pos.z < min_z || pos.z > max_z {
        return false;
    }

    (pos.y - ramp_surface_at(ramp, pos.x, pos.z)).abs() <= PLAYER_LANDING_EPSILON
}

fn player_would_hit_floor_slab(pos: &Position, floors: &[Floor]) -> bool {
    floors.iter().any(|floor| player_overlaps_floor_slab(pos, floor))
}

fn player_overlaps_floor_slab(pos: &Position, floor: &Floor) -> bool {
    let player_min_x = pos.x - PLAYER_WIDTH / 2.0;
    let player_max_x = pos.x + PLAYER_WIDTH / 2.0;
    let player_min_z = pos.z - PLAYER_DEPTH / 2.0;
    let player_max_z = pos.z + PLAYER_DEPTH / 2.0;
    let player_bottom = pos.y;
    let player_top = pos.y + PLAYER_HEIGHT;

    let floor_bottom = floor.y - floor.thickness;
    let floor_top = floor.y;
    if player_bottom >= floor_top - PHYSICS_EPSILON || player_top <= floor_bottom + PHYSICS_EPSILON {
        return false;
    }

    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();
    player_max_x >= min_x && player_min_x <= max_x && player_max_z >= min_z && player_min_z <= max_z
}

#[must_use]
pub fn slide_player_along_obstacles(
    walls: &[Wall],
    ramps: &[Ramp],
    floors: &[Floor],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    slide_along_axes(
        current_pos,
        velocity_x,
        velocity_z,
        delta,
        |dt| {
            let x = velocity_x.mul_add(dt, current_pos.x);
            Position {
                x,
                y: current_pos.y,
                z: current_pos.z,
            }
        },
        |dt| {
            let z = velocity_z.mul_add(dt, current_pos.z);
            Position {
                x: current_pos.x,
                y: current_pos.y,
                z,
            }
        },
        |candidate| {
            walls.iter().any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r, floors))
        },
        |candidate| {
            walls.iter().any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r, floors))
        },
    )
}

#[must_use]
pub fn sweep_player_vs_player(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    sweep_aabb_vs_aabb(start1, end1, start2, end2, PLAYER_WIDTH, PLAYER_DEPTH, PLAYER_HEIGHT)
}

// Check if the player's AABB currently overlaps an axis-aligned wall.
#[must_use]
pub fn overlap_player_vs_wall(pos: &Position, wall: &Wall) -> bool {
    overlap_aabb_vs_wall(pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::{FLOOR_THICKNESS, LEVEL_HEIGHT};

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

    #[test]
    fn supported_player_can_start_jump() {
        let floor = lower_floor();
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let mut motion = PlayerMotion::default();

        assert!(try_start_player_jump(&mut motion, &[floor], &[], &pos, pos.x, pos.z));
        assert_eq!(motion.velocity.y, PLAYER_JUMP_SPEED);
    }

    #[test]
    fn airborne_player_cannot_start_jump() {
        let floor = lower_floor();
        let pos = Position { x: 0.0, y: 1.0, z: 0.0 };
        let mut motion = PlayerMotion::default();

        assert!(!try_start_player_jump(&mut motion, &[floor], &[], &pos, pos.x, pos.z));
        assert_eq!(motion.velocity.y, 0.0);
    }

    #[test]
    fn upward_jump_velocity_moves_player_above_support() {
        let floor = lower_floor();
        let pos = Position { x: 0.0, y: 0.0, z: 0.0 };
        let mut motion = PlayerMotion::default();
        assert!(try_start_player_jump(&mut motion, &[floor], &[], &pos, pos.x, pos.z));

        let step = step_player_vertical_motion(&pos, &motion, &[floor], &[], pos.x, pos.z, 0.1);

        assert!(step.y > pos.y);
        assert!(step.vy > 0.0);
    }

    #[test]
    fn upward_motion_hits_floor_underside() {
        let floor = upper_floor();
        let pos = Position { x: 0.0, y: 1.8, z: 0.0 };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_vertical_motion(&pos, &motion, &[floor], &[], pos.x, pos.z, 0.1);

        assert_eq!(step.vy, 0.0);
        assert!(step.y < floor.y - floor.thickness - PLAYER_HEIGHT);
    }

    #[test]
    fn upward_motion_ignores_floor_underside_outside_footprint() {
        let floor = upper_floor();
        let pos = Position { x: 5.0, y: 1.8, z: 0.0 };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_vertical_motion(&pos, &motion, &[floor], &[], pos.x, pos.z, 0.1);

        assert!(step.vy > 0.0);
        assert!(step.y > pos.y);
    }

    #[test]
    fn upward_motion_hits_floor_slab_when_entering_from_side() {
        let floor = upper_floor();
        let pos = Position {
            x: -5.0,
            y: 2.3,
            z: 0.0,
        };
        let motion = PlayerMotion {
            velocity: Vec3::new(0.0, PLAYER_JUMP_SPEED, 0.0),
        };

        let step = step_player_vertical_motion(&pos, &motion, &[floor], &[], -4.25, pos.z, 0.1);

        assert_eq!(step.vy, 0.0);
        assert!(step.y < floor.y - floor.thickness - PLAYER_HEIGHT);
    }

    #[test]
    fn lower_floor_player_slides_on_ramp_side() {
        let ramp = test_ramp();
        let start = Position {
            x: -1.0,
            y: 0.0,
            z: 4.0,
        };
        let end = Position { x: 1.0, y: 0.0, z: 4.0 };

        assert!(sweep_player_vs_ramp_edges(&start, &end, &ramp, &[]));
    }

    #[test]
    fn upper_floor_player_can_fall_into_ramp_side() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let start = Position {
            x: -1.0,
            y: LEVEL_HEIGHT,
            z: 4.0,
        };
        let end = Position {
            x: 1.0,
            y: LEVEL_HEIGHT,
            z: 4.0,
        };

        assert!(!sweep_player_vs_ramp_edges(&start, &end, &ramp, &[floor]));
    }

    #[test]
    fn ramp_player_can_fall_off_side_when_clear_of_upper_floor() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let y = ramp_surface_at(&ramp, 2.0, 1.0);
        let start = Position { x: 2.0, y, z: 1.0 };
        let end = Position { x: -1.0, y, z: 1.0 };

        assert!(!sweep_player_vs_ramp_edges(&start, &end, &ramp, &[floor]));
    }

    #[test]
    fn ramp_player_slides_when_side_exit_would_hit_upper_floor() {
        let ramp = test_ramp();
        let floor = upper_floor_west_of_ramp();
        let y = ramp_surface_at(&ramp, 2.0, 7.0);
        let start = Position { x: 2.0, y, z: 7.0 };
        let end = Position { x: -1.0, y, z: 7.0 };

        assert!(sweep_player_vs_ramp_edges(&start, &end, &ramp, &[floor]));
    }

    #[test]
    fn player_starting_inside_ramp_side_rail_can_escape() {
        let ramp = Ramp {
            x1: 0.0,
            y1: 0.0,
            z1: 4.0,
            x2: -4.0,
            y2: LEVEL_HEIGHT,
            z2: -4.0,
        };
        let start = Position {
            x: 0.12,
            y: 0.0,
            z: 1.97,
        };
        let end = Position {
            x: 0.24,
            y: 0.0,
            z: 2.25,
        };

        assert!(!sweep_player_vs_ramp_edges(&start, &end, &ramp, &[]));
    }
}
