use bevy_ecs::prelude::*;
use bevy_math::Vec3;

use super::helpers::{
    overlap_aabb_vs_wall, slide_along_axes, sweep_aabb_vs_aabb, sweep_aabb_vs_wall, sweep_ramp_edges,
    sweep_ramp_high_cap,
};
use crate::{
    constants::{PLAYER_DEPTH, PLAYER_GRAVITY, PLAYER_HEIGHT, PLAYER_TERMINAL_VELOCITY, PLAYER_WIDTH, WALL_THICKNESS},
    map::height_on_ramp,
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

#[must_use]
pub fn sweep_player_vs_wall(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    sweep_aabb_vs_wall(start_pos, end_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

// Sweep the muzzle->spawn segment against a floor slab; returns true if it intersects.
#[must_use]
pub fn sweep_player_vs_floor(start: &Position, end: &Position, floor: &Floor, radius: f32) -> bool {
    let (min_x, max_x, min_z, max_z) = floor.bounds_xz();

    let start_inside = start.x >= min_x && start.x <= max_x && start.z >= min_z && start.z <= max_z;
    let end_inside = end.x >= min_x && end.x <= max_x && end.z >= min_z && end.z <= max_z;

    if !start_inside && !end_inside {
        return false;
    }

    let slab_bottom = floor.y - floor.thickness;
    let slab_top = floor.y;

    let seg_min_y = start.y.min(end.y);
    let seg_max_y = start.y.max(end.y);

    // If segment entirely above or below slab (with cushion), no hit
    if seg_min_y >= slab_top + radius {
        return false;
    }
    if seg_max_y <= slab_bottom - radius {
        return false;
    }

    true
}

#[must_use]
pub fn sweep_player_vs_ramp_edges(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
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

    // The high-cap is meant to block players at the bottom of a ramp from
    // walking into its tall face. Apply when the player's feet sit roughly at
    // the ramp's low edge — generalized from the old "y <= 0.1" check.
    let on_low_edge = (start_pos.y - ramp_y_low).abs() <= 0.2;

    sweep_ramp_edges(start_pos, end_pos, ramp, half_x, half_z, edge_half)
        || (on_low_edge && sweep_ramp_high_cap(start_pos, end_pos, ramp, half_x, half_z, edge_half))
}

#[must_use]
pub fn slide_player_along_obstacles(
    walls: &[Wall],
    ramps: &[Ramp],
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
                y: height_on_ramp(ramps, x, current_pos.z),
                z: current_pos.z,
            }
        },
        |dt| {
            let z = velocity_z.mul_add(dt, current_pos.z);
            Position {
                x: current_pos.x,
                y: height_on_ramp(ramps, current_pos.x, z),
                z,
            }
        },
        |candidate| {
            walls.iter().any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r))
        },
        |candidate| {
            walls.iter().any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r))
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
