use super::helpers::{
    overlap_aabb_vs_wall, slide_along_axes, sweep_aabb_vs_aabb, sweep_aabb_vs_wall, sweep_ramp_edges, sweep_ramp_high_cap,
};
use crate::{
    constants::{PLAYER_DEPTH, PLAYER_HEIGHT, PLAYER_WIDTH, WALL_THICKNESS, ROOF_HEIGHT},
    protocol::{Position, Ramp, Roof, Wall},
    ramps::calculate_height_at_position,
};

#[must_use]
pub fn overlap_player_vs_wall(player_pos: &Position, wall: &Wall) -> bool {
    overlap_aabb_vs_wall(player_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

#[must_use]
pub fn sweep_player_vs_wall(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    sweep_aabb_vs_wall(start_pos, end_pos, wall, PLAYER_WIDTH / 2.0, PLAYER_DEPTH / 2.0)
}

// Sweep the muzzle->spawn segment against a roof slab; returns true if it intersects.
#[must_use]
pub fn sweep_player_vs_roof(start: &Position, end: &Position, roof: &Roof, radius: f32) -> bool {
    let min_x = roof.x1.min(roof.x2);
    let max_x = roof.x1.max(roof.x2);
    let min_z = roof.z1.min(roof.z2);
    let max_z = roof.z1.max(roof.z2);

    let start_inside = start.x >= min_x && start.x <= max_x && start.z >= min_z && start.z <= max_z;
    let end_inside = end.x >= min_x && end.x <= max_x && end.z >= min_z && end.z <= max_z;

    if !start_inside && !end_inside {
        return false;
    }

    let slab_bottom = ROOF_HEIGHT - roof.thickness;
    let slab_top = ROOF_HEIGHT;

    let seg_min_y = start.y.min(end.y);
    let seg_max_y = start.y.max(end.y);

    // Allow shooting over the roof if either endpoint is at/above the roof top (within radius cushion)
    if start.y >= slab_top - radius || end.y >= slab_top - radius {
        return false;
    }

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
    let half_x = PLAYER_WIDTH / 2.0;
    let half_z = PLAYER_DEPTH / 2.0;
    let edge_half = WALL_THICKNESS / 2.0;

    let on_ground = start_pos.y <= 0.1;

    sweep_ramp_edges(start_pos, end_pos, ramp, half_x, half_z, edge_half)
        || (on_ground && sweep_ramp_high_cap(start_pos, end_pos, ramp, half_x, half_z, edge_half))
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
    slide_player(current_pos, velocity_x, velocity_z, delta, walls, ramps)
}

#[must_use]
pub fn sweep_player_vs_player(start1: &Position, end1: &Position, start2: &Position, end2: &Position) -> bool {
    sweep_aabb_vs_aabb(
        start1,
        end1,
        start2,
        end2,
        PLAYER_WIDTH,
        PLAYER_DEPTH,
        PLAYER_HEIGHT,
    )
}

// --- private helpers ---
fn slide_player(
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
    walls: &[Wall],
    ramps: &[Ramp],
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
                y: calculate_height_at_position(ramps, x, current_pos.z),
                z: current_pos.z,
            }
        },
        |dt| {
            let z = velocity_z.mul_add(dt, current_pos.z);
            Position {
                x: current_pos.x,
                y: calculate_height_at_position(ramps, current_pos.x, z),
                z,
            }
        },
        |candidate| {
            walls
                .iter()
                .any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps.iter().any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r))
        },
        |candidate| {
            walls
                .iter()
                .any(|w| sweep_player_vs_wall(current_pos, candidate, w))
                || ramps.iter().any(|r| sweep_player_vs_ramp_edges(current_pos, candidate, r))
        },
    )
}
