use super::helpers::{
    overlap_aabb_vs_wall, ranges_overlap_1d, slide_along_axes, sweep_aabb_vs_wall, sweep_slab_interval,
};
use crate::{
    constants::{GHOST_SIZE, PLAYER_DEPTH, PLAYER_HEIGHT, PLAYER_WIDTH},
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
pub fn sweep_ghost_vs_ramp_footprint(start_pos: &Position, end_pos: &Position, ramp: &Ramp) -> bool {
    // Swept AABB against the ramp footprint expanded by ghost half extents.
    let half = GHOST_SIZE / 2.0;

    let min_x = ramp.x1.min(ramp.x2);
    let max_x = ramp.x1.max(ramp.x2);
    let min_z = ramp.z1.min(ramp.z2);
    let max_z = ramp.z1.max(ramp.z2);

    let center_x = f32::midpoint(min_x, max_x);
    let center_z = f32::midpoint(min_z, max_z);
    let half_x = (max_x - min_x) / 2.0 + half;
    let half_z = (max_z - min_z) / 2.0 + half;

    let dir_x = end_pos.x - start_pos.x;
    let dir_z = end_pos.z - start_pos.z;

    let local_x = start_pos.x - center_x;
    let local_z = start_pos.z - center_z;

    let mut t_min = 0.0_f32;
    let mut t_max = 1.0_f32;

    if let Some((new_min, new_max)) = sweep_slab_interval(local_x, dir_x, half_x, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    if let Some((new_min, new_max)) = sweep_slab_interval(local_z, dir_z, half_z, t_min, t_max) {
        t_min = new_min;
        t_max = new_max;
    } else {
        return false;
    }

    t_min <= t_max && t_max >= 0.0 && t_min <= 1.0
}

#[must_use]
pub fn overlap_ghost_vs_ramp_footprint(ghost_pos: &Position, ramp: &Ramp) -> bool {
    let half = GHOST_SIZE / 2.0;

    let g_min_x = ghost_pos.x - half;
    let g_max_x = ghost_pos.x + half;
    let g_min_z = ghost_pos.z - half;
    let g_max_z = ghost_pos.z + half;

    let r_min_x = ramp.x1.min(ramp.x2);
    let r_max_x = ramp.x1.max(ramp.x2);
    let r_min_z = ramp.z1.min(ramp.z2);
    let r_max_z = ramp.z1.max(ramp.z2);

    ranges_overlap_1d(g_min_x, g_max_x, r_min_x, r_max_x) && ranges_overlap_1d(g_min_z, g_max_z, r_min_z, r_max_z)
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

    if y_diff > f32::midpoint(PLAYER_HEIGHT, GHOST_SIZE) {
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
            walls.iter().any(|w| sweep_ghost_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_ghost_vs_ramp_footprint(current_pos, candidate, r))
        },
        |candidate| {
            walls.iter().any(|w| sweep_ghost_vs_wall(current_pos, candidate, w))
                || ramps
                    .iter()
                    .any(|r| sweep_ghost_vs_ramp_footprint(current_pos, candidate, r))
        },
    )
}
