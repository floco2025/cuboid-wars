use super::helpers::{overlap_aabb_vs_wall, ranges_overlap_1d, slide_along_axes, sweep_aabb_vs_wall, sweep_ramp_edges};
use crate::{
    constants::{GHOST_SIZE, PLAYER_DEPTH, PLAYER_HEIGHT, PLAYER_WIDTH, RAMP_EDGE_WIDTH},
    protocol::{Position, Ramp, Wall},
};

#[must_use]
pub fn overlap_ghost_vs_wall(ghost_pos: &Position, wall: &Wall) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    overlap_aabb_vs_wall(ghost_pos, wall, ghost_half_size, ghost_half_size)
}

#[must_use]
pub fn sweep_ghost_vs_wall(start_pos: &Position, end_pos: &Position, wall: &Wall) -> bool {
    let ghost_half_size = GHOST_SIZE / 2.0;
    sweep_aabb_vs_wall(start_pos, end_pos, wall, ghost_half_size, ghost_half_size)
}

#[must_use]
pub fn sweep_ghost_vs_ramp_edges(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    sweep_ramp_edges(
        start_pos,
        end_pos,
        ramp,
        GHOST_SIZE / 2.0,
        GHOST_SIZE / 2.0,
        RAMP_EDGE_WIDTH / 2.0,
    )
}

#[must_use]
pub fn slide_ghost_along_obstacles(
    walls: &[Wall],
    ramps: &[Ramp],
    current_pos: &Position,
    velocity_x: f32,
    velocity_z: f32,
    delta: f32,
) -> Position {
    slide_ghost(current_pos, velocity_x, velocity_z, delta, walls, ramps)
}

#[must_use]
pub fn overlap_ghost_vs_player(ghost_pos: &Position, player_pos: &Position) -> bool {
    let player_center_y = player_pos.y + PLAYER_HEIGHT / 2.0;
    let ghost_center_y = GHOST_SIZE / 2.0;
    let y_diff = (player_center_y - ghost_center_y).abs();
    if y_diff > (PLAYER_HEIGHT + GHOST_SIZE) / 2.0 {
        return false;
    }

    let player_half_x = PLAYER_WIDTH / 2.0;
    let player_half_z = PLAYER_DEPTH / 2.0;
    let ghost_half = GHOST_SIZE / 2.0;

    let p_min_x = player_pos.x - player_half_x;
    let p_max_x = player_pos.x + player_half_x;
    let p_min_z = player_pos.z - player_half_z;
    let p_max_z = player_pos.z + player_half_z;

    let g_min_x = ghost_pos.x - ghost_half;
    let g_max_x = ghost_pos.x + ghost_half;
    let g_min_z = ghost_pos.z - ghost_half;
    let g_max_z = ghost_pos.z + ghost_half;

    ranges_overlap_1d(p_min_x, p_max_x, g_min_x, g_max_x) && ranges_overlap_1d(p_min_z, p_max_z, g_min_z, g_max_z)
}

// --- private helpers ---

fn slide_ghost(
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
        |dt| Position {
            x: velocity_x.mul_add(dt, current_pos.x),
            y: current_pos.y,
            z: current_pos.z,
        },
        |dt| Position {
            x: current_pos.x,
            y: current_pos.y,
            z: velocity_z.mul_add(dt, current_pos.z),
        },
        |candidate| {
            walls
                .iter()
                .any(|w| sweep_ghost_vs_wall(current_pos, candidate, w))
                || ramps.iter().any(|r| sweep_ghost_vs_ramp_edges(current_pos, candidate, r))
        },
        |candidate| {
            walls
                .iter()
                .any(|w| sweep_ghost_vs_wall(current_pos, candidate, w))
                || ramps.iter().any(|r| sweep_ghost_vs_ramp_edges(current_pos, candidate, r))
        },
    )
}
